import os
import shutil
import subprocess

try:
    from SCons.Script import Import
    Import("env")
except Exception:
    env = None

# --- MCU to Rust target mapping ---
MCU_TO_RUST_TARGET = {
    "esp32":   "xtensa-esp32-espidf",
    "esp32s2": "xtensa-esp32s2-espidf",
    "esp32s3": "xtensa-esp32s3-espidf",
    "esp32c3": "riscv32imc-esp-espidf",
    "esp32c6": "riscv32imac-esp-espidf",
    "esp32h2": "riscv32imac-esp-espidf",
}

# --- Define paths ---
LIB_ROOT = os.path.dirname(os.path.abspath(__file__))
CRATE_NAME = "portable-xstate-c-cpp"
LIB_NAME = "portable_xstate_c_cpp"
STATIC_LIB_FILENAME = f"lib{LIB_NAME}.a"
CARGO_TOML = os.path.join(LIB_ROOT, CRATE_NAME, "Cargo.toml")
TARGET_DIR = os.path.join(LIB_ROOT, "target")
INCLUDE_DIR = os.path.join(LIB_ROOT, CRATE_NAME, "include")


def _detect_host_rust_target():
    try:
        out = subprocess.check_output(["rustc", "-vV"], text=True)
        for line in out.splitlines():
            if line.startswith("host:"):
                return line.split(":", 1)[1].strip()
    except Exception:
        pass
    return "native"


def resolve_target(platform, mcu):
    platform_norm = (platform or "").lower()
    mcu_norm = (mcu or "").lower()
    if platform_norm == "native" or mcu_norm in ("", "native"):
        return _detect_host_rust_target(), "cargo", False, True

    rust_target = MCU_TO_RUST_TARGET.get(mcu_norm)
    if not rust_target:
        return None, "cargo", False, False

    is_riscv = rust_target.startswith("riscv32")
    return rust_target, "cargo", is_riscv, False


def get_lib_release_dir(rust_dir, rust_target, is_native):
    if is_native or not rust_target:
        return os.path.join(rust_dir, "target", "release")
    return os.path.join(rust_dir, "target", rust_target, "release")


def build_cargo_cmd(cargo_to_use, rust_target, is_riscv, is_native, manifest_path=CARGO_TOML):
    cmd = [
        cargo_to_use,
        "build",
        "--manifest-path",
        manifest_path,
        "--release",
    ]
    if not is_native and rust_target:
        cmd.extend(["-Z", "build-std=std,panic_abort", "--target", rust_target])
    return cmd


def _source_esp_env(build_env):
    """Source ~/export-esp.sh to pick up the ESP Rust toolchain environment."""
    export_script = os.path.expanduser("~/export-esp.sh")
    if not os.path.exists(export_script):
        return

    print(f"Sourcing ESP Rust environment from {export_script}...")
    try:
        result = subprocess.run(
            ["bash", "-c", f". '{export_script}' > /dev/null 2>&1 && env"],
            capture_output=True, text=True, check=True,
            env=build_env
        )
        for line in result.stdout.splitlines():
            key, sep, value = line.partition("=")
            if sep:
                build_env[key] = value
    except subprocess.CalledProcessError as e:
        print(f"Warning: Failed to source {export_script}: {e}")


def _ensure_esp_toolchain_path(build_env):
    """Ensure the ESP Rust toolchain's cargo and rustc are first in PATH."""
    esp_bin = os.path.expanduser("~/.rustup/toolchains/esp/bin")
    if not os.path.isdir(esp_bin):
        return

    current_path = build_env.get("PATH", "")
    if esp_bin not in current_path.split(os.pathsep):
        build_env["PATH"] = esp_bin + os.pathsep + current_path
        print(f"Prepended ESP toolchain to PATH: {esp_bin}")


def build_rust_lib(mcu, rust_target, active_env=None, *args, **kwargs):
    active_env = active_env if active_env is not None else env
    is_native = kwargs.get("is_native", rust_target is None or mcu in ("", "native"))
    is_riscv = kwargs.get("is_riscv", bool(rust_target and rust_target.startswith("riscv32")))
    cargo_to_use = kwargs.get("cargo_to_use", "cargo")

    print(f"Building {CRATE_NAME} with Cargo for {mcu} ({rust_target})...")

    build_env = dict(active_env["ENV"]) if active_env is not None else dict(os.environ)

    if not shutil.which(cargo_to_use, path=build_env.get("PATH")):
        path_from_env = os.environ.get("PATH")
        if path_from_env:
            build_env["PATH"] = path_from_env

    if not is_native and rust_target is not None:
        _source_esp_env(build_env)
        _ensure_esp_toolchain_path(build_env)

    cargo_path = shutil.which(cargo_to_use, path=build_env.get("PATH"))
    if not cargo_path:
        print("Error: 'cargo' not found in PATH. "
              "Please ensure Rust and the ESP toolchain (espup) are installed.")
        if active_env is not None:
            active_env.Exit(1)
        return

    cmd = build_cargo_cmd(cargo_to_use, rust_target, is_riscv, is_native, manifest_path=CARGO_TOML)

    try:
        subprocess.check_call(cmd, env=build_env)
    except subprocess.CalledProcessError as e:
        print(f"Error: Cargo build failed with return code {e.returncode}")
        if active_env is not None:
            active_env.Exit(1)
        return

    lib_release_dir = get_lib_release_dir(LIB_ROOT, rust_target, is_native)
    lib_path = os.path.join(lib_release_dir, STATIC_LIB_FILENAME)

    if not os.path.exists(lib_path):
        print(f"Error: Expected static library not found at {lib_path}")
        if active_env is not None:
            active_env.Exit(1)
        return

    print(f"Rust library built successfully at: {lib_path}")


if env is not None:
    platform = env.get("PIOPLATFORM", "")
    if platform == "native":
        mcu = "native"
    else:
        mcu = env.BoardConfig().get("build.mcu", "esp32").lower()

    rust_target, cargo_to_use, is_riscv, is_native = resolve_target(platform, mcu)
    if is_native:
        print(f"Detected native platform. Building for host architecture ({rust_target}).")
    elif not rust_target:
        print(f"Error: Unsupported MCU '{mcu}' for Rust build. "
              f"Supported MCUs: {list(MCU_TO_RUST_TARGET.keys())}")
        env.Exit(1)
    else:
        print(f"Detected MCU: {mcu} -> Rust target: {rust_target}")

    if not env.GetOption("clean"):
        lib_target_dir = get_lib_release_dir(LIB_ROOT, rust_target, is_native)
        env.Append(
            LIBPATH=[lib_target_dir],
            LIBS=[LIB_NAME]
        )
        build_rust_lib(
            mcu,
            rust_target,
            active_env=env,
            cargo_to_use=cargo_to_use,
            is_riscv=is_riscv,
            is_native=is_native,
        )

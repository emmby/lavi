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
    "esp32":         "xtensa-esp32-espidf",
    "esp32s2":       "xtensa-esp32s2-espidf",
    "esp32s3":       "xtensa-esp32s3-espidf",
    "esp32c3":       "riscv32imc-esp-espidf",
    "esp32c6":       "riscv32imac-esp-espidf",
    "esp32h2":       "riscv32imac-esp-espidf",
    "cortex-m0plus": "thumbv6m-none-eabi",
    "cortex-m0+":    "thumbv6m-none-eabi",
    "cortex-m3":     "thumbv7m-none-eabi",
    "cortex-m4":     "thumbv7em-none-eabihf",
    "cortex-m7":     "thumbv7em-none-eabihf",
    "cortex-m33":    "thumbv8m.main-none-eabihf",
    "rp2040":        "thumbv6m-none-eabi",
    "rp2350":        "thumbv8m.main-none-eabihf",
    "ra4m1":         "thumbv7em-none-eabihf",
    "samd21g18a":    "thumbv6m-none-eabi",
    "samd51j19a":    "thumbv7em-none-eabihf",
    "at91sam3x8e":   "thumbv7m-none-eabi",
    "nrf52840":      "thumbv7em-none-eabihf",
    "imxrt1062":     "thumbv7em-none-eabihf",
}

MCU_TO_PREBUILT_DIR = {
    "cortex-m0+":  "cortex-m0plus",
    "rp2040":      "cortex-m0plus",
    "rp2350":      "cortex-m33",
    "ra4m1":       "cortex-m4",
    "samd21g18a":  "cortex-m0plus",
    "samd51j19a":  "cortex-m4",
    "at91sam3x8e": "cortex-m3",
    "nrf52840":    "cortex-m4",
    "imxrt1062":   "cortex-m7",
}

# --- Define paths ---
LIB_ROOT = os.path.dirname(os.path.abspath(__file__)) if "__file__" in globals() else os.getcwd()
CRATE_NAME = "lavi-c-cpp"
LIB_NAME = "lavi_c_cpp"
STATIC_LIB_FILENAME = f"lib{LIB_NAME}.a"

if os.path.basename(LIB_ROOT) == CRATE_NAME and os.path.isfile(os.path.join(LIB_ROOT, "Cargo.toml")):
    REPO_ROOT = os.path.abspath(os.path.join(LIB_ROOT, ".."))
    CARGO_TOML = os.path.join(LIB_ROOT, "Cargo.toml")
else:
    REPO_ROOT = LIB_ROOT
    CARGO_TOML = os.path.join(LIB_ROOT, CRATE_NAME, "Cargo.toml")

TARGET_DIR = os.path.join(REPO_ROOT, "target")


def get_include_dir(lib_root):
    src_dir = os.path.join(lib_root, "src")
    if os.path.isfile(os.path.join(src_dir, "lavi.h")):
        return src_dir
    inc_dir = os.path.join(lib_root, "include")
    if os.path.isfile(os.path.join(inc_dir, "lavi.h")):
        return inc_dir
    return os.path.join(lib_root, CRATE_NAME, "include")


INCLUDE_DIR = get_include_dir(LIB_ROOT)


SOFTFP_SUBDIR_MAP = {
    "cortex-m4":  "cortex-m4/fpv4-sp-d16-softfp",
    "cortex-m7":  "cortex-m7/fpv5-d16-softfp",
    "cortex-m33": "cortex-m33/fpv5-sp-d16-softfp",
}


def _detect_float_abi(active_env):
    if active_env is None:
        return ""
    try:
        board_cfg = active_env.BoardConfig()
        fabi = str(board_cfg.get("build.float-abi", "")).strip().lower()
        if fabi:
            return fabi
    except Exception:
        pass
    try:
        ccflags = active_env.get("CCFLAGS", [])
        flags_str = " ".join(str(x) for x in ccflags) if isinstance(ccflags, (list, tuple)) else str(ccflags)
        if "-mfloat-abi=softfp" in flags_str:
            return "softfp"
        if "-mfloat-abi=soft" in flags_str:
            return "soft"
        if "-mfloat-abi=hard" in flags_str:
            return "hard"
    except Exception:
        pass
    return ""


def get_prebuilt_lib_dir(lib_root, mcu, cpu="", float_abi=""):
    mcu_norm = (mcu or "").lower()
    cpu_norm = (cpu or "").lower()
    fabi_norm = (float_abi or "").lower()
    base = MCU_TO_PREBUILT_DIR.get(mcu_norm) or MCU_TO_PREBUILT_DIR.get(cpu_norm) or cpu_norm or mcu_norm
    if fabi_norm in ("soft", "softfp") and base in SOFTFP_SUBDIR_MAP:
        softfp_dir = os.path.join(lib_root, "src", SOFTFP_SUBDIR_MAP[base])
        if os.path.isfile(os.path.join(softfp_dir, STATIC_LIB_FILENAME)):
            return softfp_dir
    direct_dir = os.path.join(lib_root, "src", mcu_norm)
    if os.path.isfile(os.path.join(direct_dir, STATIC_LIB_FILENAME)):
        return direct_dir
    if base and base != mcu_norm:
        mapped_dir = os.path.join(lib_root, "src", base)
        if os.path.isfile(os.path.join(mapped_dir, STATIC_LIB_FILENAME)):
            return mapped_dir
    return direct_dir


def has_prebuilt_lib(lib_root, mcu, env_vars=None, cpu="", float_abi=""):
    mcu_norm = (mcu or "").lower()
    if mcu_norm in ("", "native"):
        return False
    check_env = env_vars if env_vars is not None else os.environ
    force_cargo = str(check_env.get("LAVI_FORCE_CARGO_BUILD", "")).strip().lower()
    if force_cargo in ("1", "true", "yes"):
        return False
    archive_path = os.path.join(
        get_prebuilt_lib_dir(lib_root, mcu_norm, cpu=cpu, float_abi=float_abi),
        STATIC_LIB_FILENAME,
    )
    return os.path.isfile(archive_path)


def _detect_host_rust_target():
    try:
        out = subprocess.check_output(["rustc", "-vV"], text=True)
        for line in out.splitlines():
            if line.startswith("host:"):
                return line.split(":", 1)[1].strip()
    except Exception:
        pass
    return "native"


def resolve_target(platform, mcu, cpu="", float_abi=""):
    platform_norm = (platform or "").lower()
    mcu_norm = (mcu or "").lower()
    cpu_norm = (cpu or "").lower()
    fabi_norm = (float_abi or "").lower()
    if platform_norm == "native" or mcu_norm in ("", "native"):
        return _detect_host_rust_target(), "cargo", False, True

    rust_target = MCU_TO_RUST_TARGET.get(mcu_norm) or MCU_TO_RUST_TARGET.get(cpu_norm)
    if not rust_target:
        return None, "cargo", False, False

    if fabi_norm in ("soft", "softfp") and rust_target.endswith("-none-eabihf"):
        rust_target = rust_target[:-2]

    is_riscv = rust_target.startswith("riscv32")
    return rust_target, "cargo", is_riscv, False


def get_lib_release_dir(rust_dir, rust_target, is_native):
    if is_native or not rust_target:
        return os.path.join(rust_dir, "target", "release")
    return os.path.join(rust_dir, "target", rust_target, "release")


def build_cargo_cmd(
    cargo_to_use,
    rust_target,
    is_riscv,
    is_native,
    manifest_path=CARGO_TOML,
    use_build_std=None,
):
    cmd = [
        cargo_to_use,
        "build",
        "--manifest-path",
        manifest_path,
        "--release",
    ]
    if not is_native and rust_target:
        is_thumb = rust_target.startswith("thumb")
        should_build_std = use_build_std if use_build_std is not None else (not is_thumb)
        if should_build_std:
            build_std = "core,alloc,panic_abort" if is_thumb else "std,panic_abort"
            cmd.extend(["-Z", f"build-std={build_std}", "--target", rust_target])
        else:
            cmd.extend(["--target", rust_target])
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
    except (subprocess.CalledProcessError, OSError) as e:
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

    if not shutil.which(cargo_to_use, path=build_env.get("PATH", "")):
        path_from_env = os.environ.get("PATH")
        if path_from_env:
            build_env["PATH"] = path_from_env

    if not is_native and rust_target is not None and not rust_target.startswith("thumb"):
        _source_esp_env(build_env)
        _ensure_esp_toolchain_path(build_env)

    cargo_path = shutil.which(cargo_to_use, path=build_env.get("PATH", ""))
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

    lib_release_dir = get_lib_release_dir(REPO_ROOT, rust_target, is_native)
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
        cpu = ""
        float_abi = ""
    else:
        board_cfg = env.BoardConfig()
        mcu = board_cfg.get("build.mcu", "esp32").lower()
        cpu = board_cfg.get("build.cpu", "").lower()
        float_abi = _detect_float_abi(env)

    rust_target, cargo_to_use, is_riscv, is_native = resolve_target(
        platform, mcu, cpu=cpu, float_abi=float_abi
    )
    if is_native:
        print(f"Detected native platform. Building for host architecture ({rust_target}).")
    elif not rust_target:
        print(f"Error: Unsupported MCU '{mcu}' (cpu='{cpu}') for Rust build. "
              f"Supported MCUs: {list(MCU_TO_RUST_TARGET.keys())}")
        env.Exit(1)
    else:
        print(f"Detected MCU: {mcu} -> Rust target: {rust_target}")

    if not env.GetOption("clean"):
        inc_dir = get_include_dir(LIB_ROOT)
        active_env_vars = dict(env["ENV"]) if "ENV" in env else None
        if not is_native and has_prebuilt_lib(
            LIB_ROOT, mcu, env_vars=active_env_vars, cpu=cpu, float_abi=float_abi
        ):
            prebuilt_dir = get_prebuilt_lib_dir(LIB_ROOT, mcu, cpu=cpu, float_abi=float_abi)
            print(f"Using precompiled {STATIC_LIB_FILENAME} from {prebuilt_dir}")
            env.Append(
                CPPPATH=[inc_dir],
                LIBPATH=[prebuilt_dir],
                LIBS=[LIB_NAME]
            )
        else:
            lib_target_dir = get_lib_release_dir(REPO_ROOT, rust_target, is_native)
            env.Append(
                CPPPATH=[inc_dir],
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

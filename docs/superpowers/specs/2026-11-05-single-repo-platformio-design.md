# Single-Repository PlatformIO Consolidation Design Spec

**Date:** 2026-11-05
**Status:** Draft

---

## 1. Intent and Why

Previously, PlatformIO support for `portable-xstate` lived in a separate wrapper repository (`emmby/portable-xstate-platformio`). That wrapper pinned a hardcoded upstream commit hash (`DEPENDENCY_REF`) in `build_rust.py`, cloned `portable-xstate` inside SCons during `pio run`, compiled `portable-xstate-c-cpp`, copied `portable_xstate.h` and `portable_xstate.hpp` into a duplicated `include/` directory, and relied on a cross-repository GitHub Actions dispatch workflow (`trigger-platformio-update.yml` -> `update-from-upstream.yml`) to keep `DEPENDENCY_REF` and the tracked headers in sync.

The original PlatformIO and Cargo limitations that motivated the two-repository split—PlatformIO Library Dependency Finder (LDF) header discovery before `extraScript`, duplicate symbol errors from compiling `.c`/`.cpp` test files in the repo, Cargo workspace `target/` paths, and `#![no_std]` workspace dependency resolution—are now cleanly solved via `library.json` configuration (`"includeDir": "portable-xstate-c-cpp/include"` and `"srcFilter": ["-<*>"]`) and the `#![no_std]` workspace structure.

Consolidating PlatformIO support directly into `portable-xstate` eliminates `DEPENDENCY_REF`, removes the nested `git clone` during PlatformIO builds, eliminates header duplication and cross-repo CI bot race conditions, and allows `portable-xstate-platformio` to be retired.

---

## 2. Goals, Non-Goals, and Anti-Goals

### Goals
- **Single-Repo PlatformIO Library**: Make `portable-xstate` directly installable as a PlatformIO library via `lib_deps = https://github.com/emmby/portable-xstate.git` for both ESP32 (`espressif32`) and host (`native`) PlatformIO environments.
- **Eliminate `DEPENDENCY_REF` and Build-Time Cloning**: Remove `DEPENDENCY_URL`, `DEPENDENCY_REF`, `DEPENDENCY_DIR`, `clone_dependency()`, and header copying from `build_rust.py`. Build `portable-xstate-c-cpp` directly in-place from the repository root.
- **Single Source of Truth for C/C++ Headers**: Point PlatformIO directly at `portable-xstate-c-cpp/include` via `library.json` without duplicating `portable_xstate.h` or `portable_xstate.hpp` into a root `include/` folder.
- **Migrate `stoplight-esp32` Example**: Move `examples/stoplight-esp32` (including `src/main.cpp`, `platformio.ini`, `wokwi.toml`, `wokwi-circuit.json`, and `.gitignore`) into `portable-xstate/examples/stoplight-esp32/`.
- **Integrate `build_rust.py` Unit Tests into `npm test`**: Place `test_build_rust.py` at `portable-xstate-c-cpp/tests/test_build_rust.py` and run it as part of `npm run test:c-cpp` (which is executed by `npm test` and `.github/workflows/ci.yml`).
- **Remove Cross-Repo Dispatch Workflow & Update Documentation**: Delete `.github/workflows/trigger-platformio-update.yml` and document PlatformIO setup and usage in `portable-xstate-c-cpp/README.md` and root `README.md`.

### Non-Goals
- Maintaining backward compatibility or forwarding shims in the retired `emmby/portable-xstate-platformio` repository.
- Running full ESP32 (`espup`) or Wokwi simulator builds inside the primary GitHub Actions `ci.yml` workflow on every push (unit testing `build_rust.py` via `test_build_rust.py` and building/testing the native C/C++ static library via `npm run test:c-cpp` covers CI).

### Anti-Goals
- **No Build-Time Git Operations**: `build_rust.py` must never invoke `git clone`, `git fetch`, or `git checkout`.
- **No Header Copying or Duplication**: `build_rust.py` must not copy headers into a secondary `include/` directory, and only `portable-xstate-c-cpp/include/` may contain `portable_xstate.h` and `portable_xstate.hpp`.
- **No C/C++ Test or Example Compilation by PlatformIO LDF**: PlatformIO must never compile `portable-xstate-c-cpp/tests/*.c*` or `examples/stoplight-esp32/src/*.cpp` when `portable-xstate` is consumed as a library dependency.

---

## 3. Constraints

- **PlatformIO Manifest Location**: `library.json` and `build_rust.py` must reside at the root of `portable-xstate` so PlatformIO recognizes the repository root when installed via Git URL or local symlink (`symlink://../..`).
- **Workspace Artifact Path**: Because `portable-xstate` is a Cargo workspace (`Cargo.toml` at the repository root), `cargo build --manifest-path <LIB_ROOT>/portable-xstate-c-cpp/Cargo.toml --release` places release artifacts in `<LIB_ROOT>/target/<rust_target>/release/libportable_xstate_c_cpp.a` (or `<LIB_ROOT>/target/release/libportable_xstate_c_cpp.a` when `is_native` is `True`).
- **SCons and Standalone Python Compatibility**: `build_rust.py` must run both inside PlatformIO's SCons runtime (`Import("env")`) and when imported directly by standard Python `unittest` (`python3 -m unittest portable-xstate-c-cpp/tests/test_build_rust.py`) where `SCons` is not installed.

---

## 4. Technical Design

### 4.1 Root `library.json`

Create [`library.json`](file:///Users/mike/.gemini/antigravity/worktrees/portable-xstate/evaluate_dependency_ref/library.json) at the root of `portable-xstate`:

```json
{
  "name": "portable-xstate",
  "version": "0.1.1",
  "description": "Portable XState runtime for C/C++ and PlatformIO, powered by Rust.",
  "keywords": [
    "xstate",
    "fsm",
    "state-machine",
    "statechart",
    "rust",
    "ffi"
  ],
  "repository": {
    "type": "git",
    "url": "https://github.com/emmby/portable-xstate.git"
  },
  "build": {
    "extraScript": "build_rust.py",
    "includeDir": "portable-xstate-c-cpp/include",
    "srcFilter": [
      "-<*>"
    ]
  },
  "export": {
    "include": [
      "Cargo.toml",
      "Cargo.lock",
      "build_rust.py",
      "portable-xstate-rust/**",
      "portable-xstate-c-cpp/Cargo.toml",
      "portable-xstate-c-cpp/src/**",
      "portable-xstate-c-cpp/include/**",
      "portable-xstate-wasm/Cargo.toml",
      "portable-xstate-wasm/src/**",
      "examples/stoplight-esp32/**"
    ]
  },
  "frameworks": [
    "*"
  ],
  "platforms": [
    "*"
  ]
}
```

Key details:
- `"includeDir": "portable-xstate-c-cpp/include"` directs PlatformIO's Library Dependency Finder (LDF) and compiler `-I` flags directly to the tracked headers in `portable-xstate-c-cpp/include/`.
- `"srcFilter": ["-<*>"]` prevents PlatformIO from compiling any `.c` or `.cpp` files in the repository (`portable-xstate-c-cpp/tests/` or `examples/`) as library sources.
- `"export.include"` includes all Cargo workspace member manifests (`portable-xstate-rust`, `portable-xstate-c-cpp`, `portable-xstate-wasm`) so Cargo workspace resolution succeeds even if packaged via `pio pkg pack`, while excluding `node_modules/`, `tests/`, `docs/`, and build artifacts.

### 4.2 Root `build_rust.py`

Create [`build_rust.py`](file:///Users/mike/.gemini/antigravity/worktrees/portable-xstate/evaluate_dependency_ref/build_rust.py) at the root of `portable-xstate`:

1. **Path Constants**:
   - `LIB_ROOT = os.getcwd()`
   - `CRATE_NAME = "portable-xstate-c-cpp"`
   - `LIB_NAME = "portable_xstate_c_cpp"`
   - `STATIC_LIB_FILENAME = f"lib{LIB_NAME}.a"`
   - `CARGO_TOML = os.path.join(LIB_ROOT, CRATE_NAME, "Cargo.toml")`
   - `TARGET_DIR = os.path.join(LIB_ROOT, "target")`
   - `INCLUDE_DIR = os.path.join(LIB_ROOT, CRATE_NAME, "include")`
   - Remove `DEPENDENCY_NAME`, `DEPENDENCY_URL`, `DEPENDENCY_REF`, `DEPENDENCY_DIR`, and `SRC_INCLUDE_DIR`.

2. **Target Resolution & Cargo Command Helpers** (preserved from `portable-xstate-platformio/build_rust.py`):
   - `MCU_TO_RUST_TARGET` mapping `esp32`, `esp32s2`, `esp32s3` to `xtensa-*-espidf` and `esp32c3`, `esp32c6`, `esp32h2` to `riscv32*-esp-espidf`.
   - `_detect_host_rust_target()` via `rustc -vV`.
   - `resolve_target(platform, mcu) -> tuple[str | None, str, bool, bool]`.
   - `get_lib_release_dir(rust_dir, rust_target, is_native) -> str` returning `<rust_dir>/target/release` when `is_native` is `True` and `<rust_dir>/target/<rust_target>/release` otherwise.
   - `build_cargo_cmd(cargo_to_use, rust_target, is_riscv, is_native, manifest_path=CARGO_TOML) -> list[str]`.

3. **In-Place `build_rust_lib(mcu, rust_target, active_env=None, *args, **kwargs)`**:
   - Does **not** call `clone_dependency()` (removed).
   - Prepares `build_env` from `active_env["ENV"]` (or `os.environ`), sources `~/export-esp.sh` and prepends `~/.rustup/toolchains/esp/bin` when `not is_native and rust_target is not None`.
   - Runs `subprocess.check_call(cmd, env=build_env)`.
   - Verifies that `os.path.join(get_lib_release_dir(LIB_ROOT, rust_target, is_native), STATIC_LIB_FILENAME)` exists on disk (calling `active_env.Exit(1)` on error).
   - Does **not** copy headers.

4. **SCons Hook (`if env is not None:`)**:
   - Detects `platform = env.get("PIOPLATFORM", "")` and `mcu` (`"native"` when `platform == "native"`, otherwise `env.BoardConfig().get("build.mcu", "esp32").lower()`).
   - Calls `resolve_target(platform, mcu)`.
   - When `not env.GetOption("clean")`, appends `LIBPATH=[get_lib_release_dir(LIB_ROOT, rust_target, is_native)]` and `LIBS=[LIB_NAME]` to `env`, and invokes `build_rust_lib(...)`.

### 4.3 Unit Tests (`portable-xstate-c-cpp/tests/test_build_rust.py` & `package.json`)

1. Create [`portable-xstate-c-cpp/tests/test_build_rust.py`](file:///Users/mike/.gemini/antigravity/worktrees/portable-xstate/evaluate_dependency_ref/portable-xstate-c-cpp/tests/test_build_rust.py):
   - Add the repository root to `sys.path` (`sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..")))`) so `import build_rust` works regardless of the current working directory.
   - Test:
     - `test_esp32_xtensa_targets` (`esp32`, `esp32s2`, `esp32s3`, case-insensitivity)
     - `test_esp32_riscv_targets` (`esp32c3`, `esp32c6`, `esp32h2`)
     - `test_unsupported_mcu_returns_none` (`stm32f4`)
     - `test_native_platform_target_resolution`
     - `test_get_lib_release_dir`
     - `test_build_cargo_cmd_native_vs_cross`
     - `test_build_rust_lib_invokes_cargo_in_place` (mocking `shutil.which`, `subprocess.check_call`, and `os.path.exists` to verify `build_rust_lib` runs Cargo against `CARGO_TOML` and checks the output archive without any git commands or header copying)
     - `test_library_json_configuration` (verifying `name == "portable-xstate"`, `frameworks == ["*"]`, `platforms == ["*"]`, `build.includeDir == "portable-xstate-c-cpp/include"`, `build.srcFilter == ["-<*>"]`, and that `portable_xstate.h` and `portable_xstate.hpp` exist in `build.includeDir`)
     - `test_c_cpp_crate_paths` (verifying `CRATE_NAME`, `LIB_NAME`, `STATIC_LIB_FILENAME`, `CARGO_TOML`, `INCLUDE_DIR`, and confirming `DEPENDENCY_REF` and `clone_dependency` do not exist on `build_rust`)

2. Update [`package.json`](file:///Users/mike/.gemini/antigravity/worktrees/portable-xstate/evaluate_dependency_ref/package.json):
   - Add `"test:c-cpp:pio": "python3 -m unittest portable-xstate-c-cpp/tests/test_build_rust.py"`
   - Update `"test:c-cpp"` to:
     ```json
     "test:c-cpp": "npm run build:c-cpp && npm run test:c-cpp:c && npm run test:c-cpp:cpp && npm run test:c-cpp:pio"
     ```

### 4.4 Migrate `examples/stoplight-esp32`

Migrate the ESP32 traffic light example from `portable-xstate-platformio/examples/stoplight-esp32` to [`examples/stoplight-esp32/`](file:///Users/mike/.gemini/antigravity/worktrees/portable-xstate/evaluate_dependency_ref/examples/stoplight-esp32):
- `examples/stoplight-esp32/.gitignore` (ignoring `.pio`)
- `examples/stoplight-esp32/platformio.ini`:
  ```ini
  [env:featheresp32]
  platform = espressif32
  board = featheresp32
  framework = arduino
  monitor_speed = 115200
  lib_deps =
      ; DO NOT CHANGE THIS TO A LOCAL RELATIVE PATH.
      ; This is intended as an example for developers.
      https://github.com/emmby/portable-xstate.git
  ```
- `examples/stoplight-esp32/src/main.cpp` (using `xstate::PortableXState`, `registerAction`, `start()`, `send("TIMER")`, and `matches(...)`)
- `examples/stoplight-esp32/wokwi.toml` and `examples/stoplight-esp32/wokwi-circuit.json`
- Update [`examples/README.md`](file:///Users/mike/.gemini/antigravity/worktrees/portable-xstate/evaluate_dependency_ref/examples/README.md) to mention `stoplight-esp32` alongside the browser examples.

### 4.5 CI Workflows & Documentation

1. **Delete `.github/workflows/trigger-platformio-update.yml`**:
   - No cross-repository dispatch is needed once PlatformIO builds directly from `portable-xstate`.
2. **Update [`portable-xstate-c-cpp/README.md`](file:///Users/mike/.gemini/antigravity/worktrees/portable-xstate/evaluate_dependency_ref/portable-xstate-c-cpp/README.md)**:
   - Add a **PlatformIO Integration** section covering:
     - Prerequisites (`espup` for ESP32 targets; host Rust toolchain for `platform = native`).
     - `platformio.ini` configuration (`lib_deps = https://github.com/emmby/portable-xstate.git`).
     - Pointer to [`examples/stoplight-esp32`](file:///Users/mike/.gemini/antigravity/worktrees/portable-xstate/evaluate_dependency_ref/examples/stoplight-esp32).
3. **Update [`README.md`](file:///Users/mike/.gemini/antigravity/worktrees/portable-xstate/evaluate_dependency_ref/README.md)**:
   - Mention PlatformIO support under Project Structure / `portable-xstate-c-cpp`.
4. **Update [`AGENTS.md`](file:///Users/mike/.gemini/antigravity/worktrees/portable-xstate/evaluate_dependency_ref/AGENTS.md)**:
   - Replace Section 3 (`Companion Repository Synchronization (portable-xstate-platformio)`) with in-repo PlatformIO guidelines (`build_rust.py`, `library.json`, `portable-xstate-c-cpp/tests/test_build_rust.py`, and `examples/stoplight-esp32`).

---

## 5. What Is Left to the Builder

- Exact formatting and prose of the PlatformIO documentation sections in `portable-xstate-c-cpp/README.md`, `README.md`, and `examples/README.md`.
- Any additional helper assertions in `portable-xstate-c-cpp/tests/test_build_rust.py` for edge-case environment variable handling (`PATH` fallback, `export-esp.sh` sourcing).

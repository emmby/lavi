# Single-Repository PlatformIO Consolidation Implementation Plan [flowos: 880e2c46-ac1f-4dcd-9bf1-789c2546938f]

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Consolidate PlatformIO library support (`library.json`, `build_rust.py`, `test_build_rust.py`, and `examples/stoplight-esp32`) directly into `portable-xstate`, eliminating `DEPENDENCY_REF`, build-time `git clone`, header copying, and the separate `portable-xstate-platformio` repository.

**Architecture:** Place `library.json` and `build_rust.py` at the root of `portable-xstate` so PlatformIO resolves C/C++ headers directly from `portable-xstate-c-cpp/include` (`"includeDir": "portable-xstate-c-cpp/include"`, `"srcFilter": ["-<*>"]`) and compiles `portable-xstate-c-cpp` in-place via Cargo into `<LIB_ROOT>/target/`. Integrate `portable-xstate-c-cpp/tests/test_build_rust.py` into `npm run test:c-cpp`, migrate `examples/stoplight-esp32`, and remove `.github/workflows/trigger-platformio-update.yml`.

**Tech Stack:** Python 3 (`unittest`, PlatformIO / SCons), Rust (`cargo`, `#![no_std]`), C11 / C++11 (`clang` / `clang++`), Node.js / npm scripts.

**Spec:** [`docs/superpowers/specs/2026-11-05-single-repo-platformio-design.md`](file:///Users/mike/.gemini/antigravity/worktrees/portable-xstate/evaluate_dependency_ref/docs/superpowers/specs/2026-11-05-single-repo-platformio-design.md)

## Global Constraints

- **No Build-Time Git Operations**: `build_rust.py` must never invoke `git clone`, `git fetch`, or `git checkout`, and must not define `DEPENDENCY_REF`, `DEPENDENCY_URL`, `DEPENDENCY_DIR`, or `clone_dependency`.
- **No Header Copying or Duplication**: `build_rust.py` must not copy headers into a root `include/` directory; `library.json` must point `"build.includeDir"` to `"portable-xstate-c-cpp/include"`.
- **No C/C++ Test or Example Compilation by PlatformIO LDF**: `library.json` must set `"build.srcFilter"` to `["-<*>"]`.
- **SCons and Standalone Python Compatibility**: `build_rust.py` must run inside PlatformIO's SCons runtime (`Import("env")`) and when imported directly by `python3 -m unittest portable-xstate-c-cpp/tests/test_build_rust.py` without `SCons` installed.

## Review Focus

- **Importing `build_rust` when `os.getcwd()` is not the repository root**: `LIB_ROOT` must use `os.path.dirname(os.path.abspath(__file__))` so `CARGO_TOML`, `TARGET_DIR`, and `INCLUDE_DIR` always resolve relative to the directory containing `build_rust.py` (tested in Task 1).
- **Missing `cargo` binary in PlatformIO GUI `ENV["PATH"]`**: `build_rust_lib` must fall back to `os.environ.get("PATH")` when `cargo` is missing from `active_env["ENV"]["PATH"]`, and call `active_env.Exit(1)` if `cargo` is still not found (tested in Task 1).
- **Cargo build failure or missing `libportable_xstate_c_cpp.a` artifact**: `build_rust_lib` must catch `subprocess.CalledProcessError` and check `os.path.exists(lib_path)`, calling `active_env.Exit(1)` cleanly in both cases (tested in Task 1).
- **ESP32 toolchain environment sourcing vs. `platform = native`**: Cross-compiled ESP32 builds must call `_source_esp_env` and `_ensure_esp_toolchain_path`, while `is_native=True` builds must skip them (tested in Task 1).
- **Cargo workspace member integrity under `library.json` `export.include`**: Every crate in root `Cargo.toml` `[workspace] members` (`portable-xstate-rust`, `portable-xstate-wasm`, `portable-xstate-c-cpp`) must be covered by `library.json` `"export.include"` so packaged archives pass Cargo workspace manifest checks (tested in Task 1).

---

## Phase 1: Single-Repository PlatformIO Consolidation [flowos: c0d6d633-44ed-438f-9735-a769e4d24696]

### Task 1: Add `library.json`, In-Place `build_rust.py`, and `test_build_rust.py` Suite [flowos: 2f998b2f-cac3-4e7a-8bd3-e8abab07fd27]

**Files:**
- Create: `portable-xstate-c-cpp/tests/test_build_rust.py`
- Create: `library.json`
- Create: `build_rust.py`
- Modify: `package.json:11-15`

**Interfaces:**
- Produces:
  - `build_rust.LIB_ROOT`, `build_rust.CRATE_NAME`, `build_rust.LIB_NAME`, `build_rust.STATIC_LIB_FILENAME`, `build_rust.CARGO_TOML`, `build_rust.TARGET_DIR`, `build_rust.INCLUDE_DIR`
  - `build_rust.resolve_target(platform: str, mcu: str) -> tuple[str | None, str, bool, bool]`
  - `build_rust.get_lib_release_dir(rust_dir: str, rust_target: str | None, is_native: bool) -> str`
  - `build_rust.build_cargo_cmd(cargo_to_use: str, rust_target: str | None, is_riscv: bool, is_native: bool, manifest_path: str = CARGO_TOML) -> list[str]`
  - `build_rust.build_rust_lib(mcu: str, rust_target: str | None, active_env=None, *args, **kwargs) -> None`
  - `npm run test:c-cpp:pio` in `package.json`

- [ ] **Step 1: Write the failing unit test suite in `portable-xstate-c-cpp/tests/test_build_rust.py`**

Create `portable-xstate-c-cpp/tests/test_build_rust.py`:

```python
"""Unit tests for in-repo PlatformIO build_rust.py and library.json."""

import json
import os
import subprocess
import sys
import unittest
from unittest.mock import MagicMock, patch

REPO_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
if REPO_ROOT not in sys.path:
    sys.path.insert(0, REPO_ROOT)

import build_rust  # noqa: E402


class TestBuildRust(unittest.TestCase):
    """Validate build_rust.py functions and library.json for in-repo ESP32 and native builds."""

    def test_esp32_xtensa_targets(self):
        for mcu, expected in [
            ("esp32", "xtensa-esp32-espidf"),
            ("esp32s2", "xtensa-esp32s2-espidf"),
            ("esp32s3", "xtensa-esp32s3-espidf"),
            ("ESP32S3", "xtensa-esp32s3-espidf"),
        ]:
            target, cargo, is_riscv, is_native = build_rust.resolve_target("espressif32", mcu)
            self.assertEqual(target, expected)
            self.assertEqual(cargo, "cargo")
            self.assertFalse(is_riscv)
            self.assertFalse(is_native)

    def test_esp32_riscv_targets(self):
        for mcu, expected in [
            ("esp32c3", "riscv32imc-esp-espidf"),
            ("esp32c6", "riscv32imac-esp-espidf"),
            ("esp32h2", "riscv32imac-esp-espidf"),
        ]:
            target, cargo, is_riscv, is_native = build_rust.resolve_target("espressif32", mcu)
            self.assertEqual(target, expected)
            self.assertEqual(cargo, "cargo")
            self.assertTrue(is_riscv)
            self.assertFalse(is_native)

    def test_unsupported_mcu_returns_none(self):
        target, cargo, is_riscv, is_native = build_rust.resolve_target("espressif32", "stm32f4")
        self.assertIsNone(target)
        self.assertFalse(is_riscv)
        self.assertFalse(is_native)

    @patch("subprocess.check_output", return_value="rustc 1.85.0\nhost: aarch64-apple-darwin\n")
    def test_native_platform_target_resolution(self, _mock_rustc):
        target, cargo, is_riscv, is_native = build_rust.resolve_target("native", "")
        self.assertEqual(target, "aarch64-apple-darwin")
        self.assertEqual(cargo, "cargo")
        self.assertFalse(is_riscv)
        self.assertTrue(is_native)

    def test_get_lib_release_dir(self):
        rust_dir = "/workspace/portable-xstate"
        self.assertEqual(
            build_rust.get_lib_release_dir(rust_dir, "aarch64-apple-darwin", True),
            "/workspace/portable-xstate/target/release",
        )
        self.assertEqual(
            build_rust.get_lib_release_dir(rust_dir, "xtensa-esp32s3-espidf", False),
            "/workspace/portable-xstate/target/xtensa-esp32s3-espidf/release",
        )

    def test_build_cargo_cmd_native_vs_cross(self):
        native_cmd = build_rust.build_cargo_cmd(
            "cargo", "aarch64-apple-darwin", False, True, manifest_path="/path/Cargo.toml"
        )
        self.assertEqual(
            native_cmd,
            ["cargo", "build", "--manifest-path", "/path/Cargo.toml", "--release"],
        )

        esp_cmd = build_rust.build_cargo_cmd(
            "cargo", "xtensa-esp32s3-espidf", False, False, manifest_path="/path/Cargo.toml"
        )
        self.assertEqual(
            esp_cmd,
            [
                "cargo",
                "build",
                "--manifest-path",
                "/path/Cargo.toml",
                "--release",
                "-Z",
                "build-std=std,panic_abort",
                "--target",
                "xtensa-esp32s3-espidf",
            ],
        )

    def test_c_cpp_crate_paths_and_no_dependency_ref(self):
        self.assertEqual(build_rust.LIB_ROOT, REPO_ROOT)
        self.assertEqual(build_rust.CRATE_NAME, "portable-xstate-c-cpp")
        self.assertEqual(build_rust.LIB_NAME, "portable_xstate_c_cpp")
        self.assertEqual(build_rust.STATIC_LIB_FILENAME, "libportable_xstate_c_cpp.a")
        self.assertEqual(
            build_rust.CARGO_TOML,
            os.path.join(REPO_ROOT, "portable-xstate-c-cpp", "Cargo.toml"),
        )
        self.assertEqual(
            build_rust.INCLUDE_DIR,
            os.path.join(REPO_ROOT, "portable-xstate-c-cpp", "include"),
        )
        self.assertTrue(os.path.isfile(build_rust.CARGO_TOML))
        self.assertFalse(hasattr(build_rust, "DEPENDENCY_REF"))
        self.assertFalse(hasattr(build_rust, "DEPENDENCY_URL"))
        self.assertFalse(hasattr(build_rust, "clone_dependency"))

    @patch("os.path.exists", return_value=True)
    @patch("subprocess.check_call")
    @patch("shutil.which", return_value="/usr/local/bin/cargo")
    @patch("build_rust._source_esp_env")
    @patch("build_rust._ensure_esp_toolchain_path")
    def test_build_rust_lib_invokes_cargo_in_place(
        self, mock_ensure_esp, mock_source_esp, _mock_which, mock_check_call, _mock_exists
    ):
        dummy_env = MagicMock()
        dummy_env.__getitem__.side_effect = lambda k: {"PATH": "/usr/bin"} if k == "ENV" else None

        # Native build skips ESP env helpers and runs cargo build in-place
        build_rust.build_rust_lib(
            "native", "aarch64-apple-darwin", active_env=dummy_env, is_native=True
        )
        mock_source_esp.assert_not_called()
        mock_ensure_esp.assert_not_called()
        mock_check_call.assert_called_once()
        cmd_called = mock_check_call.call_args[0][0]
        self.assertEqual(
            cmd_called,
            ["cargo", "build", "--manifest-path", build_rust.CARGO_TOML, "--release"],
        )
        dummy_env.Exit.assert_not_called()

        # Cross-compiled ESP32 build invokes ESP env helpers
        mock_check_call.reset_mock()
        build_rust.build_rust_lib(
            "esp32s3", "xtensa-esp32s3-espidf", active_env=dummy_env, is_native=False
        )
        mock_source_esp.assert_called_once()
        mock_ensure_esp.assert_called_once()
        mock_check_call.assert_called_once()
        dummy_env.Exit.assert_not_called()

    @patch("shutil.which", return_value=None)
    def test_build_rust_lib_exits_when_cargo_not_found(self, _mock_which):
        dummy_env = MagicMock()
        dummy_env.__getitem__.side_effect = lambda k: {"PATH": "/empty"} if k == "ENV" else None
        build_rust.build_rust_lib(
            "native", "aarch64-apple-darwin", active_env=dummy_env, is_native=True
        )
        dummy_env.Exit.assert_called_once_with(1)

    @patch("shutil.which", return_value="/usr/local/bin/cargo")
    @patch("subprocess.check_call", side_effect=subprocess.CalledProcessError(101, ["cargo"]))
    def test_build_rust_lib_exits_on_cargo_failure_or_missing_archive(
        self, mock_check_call, _mock_which
    ):
        dummy_env = MagicMock()
        dummy_env.__getitem__.side_effect = lambda k: {"PATH": "/usr/bin"} if k == "ENV" else None
        build_rust.build_rust_lib(
            "native", "aarch64-apple-darwin", active_env=dummy_env, is_native=True
        )
        dummy_env.Exit.assert_called_once_with(1)

        # Now succeed in cargo build, but fail the archive existence check
        dummy_env.Exit.reset_mock()
        mock_check_call.side_effect = None
        mock_check_call.return_value = 0
        with patch("os.path.exists", return_value=False):
            build_rust.build_rust_lib(
                "native", "aarch64-apple-darwin", active_env=dummy_env, is_native=True
            )
        dummy_env.Exit.assert_called_once_with(1)

    def test_library_json_configuration(self):
        lib_json_path = os.path.join(REPO_ROOT, "library.json")
        with open(lib_json_path, "r", encoding="utf-8") as f:
            manifest = json.load(f)
        self.assertEqual(manifest.get("name"), "portable-xstate")
        self.assertEqual(manifest.get("frameworks"), ["*"])
        self.assertEqual(manifest.get("platforms"), ["*"])
        build_cfg = manifest.get("build", {})
        self.assertEqual(build_cfg.get("extraScript"), "build_rust.py")
        self.assertEqual(build_cfg.get("includeDir"), "portable-xstate-c-cpp/include")
        self.assertEqual(build_cfg.get("srcFilter"), ["-<*>"])

        include_dir = os.path.join(REPO_ROOT, build_cfg["includeDir"])
        self.assertTrue(os.path.isfile(os.path.join(include_dir, "portable_xstate.h")))
        self.assertTrue(os.path.isfile(os.path.join(include_dir, "portable_xstate.hpp")))

        export_includes = manifest.get("export", {}).get("include", [])
        for member in ["portable-xstate-rust", "portable-xstate-c-cpp", "portable-xstate-wasm"]:
            self.assertTrue(
                any(member in pattern for pattern in export_includes),
                f"Workspace member {member} must be covered in library.json export.include",
            )


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 2: Run the unit test to verify it fails**

Run: `python3 -m unittest portable-xstate-c-cpp/tests/test_build_rust.py -v`
Expected: FAIL with `ModuleNotFoundError: No module named 'build_rust'`.

- [ ] **Step 3: Create `library.json` and `build_rust.py` at the repository root and update `package.json`**

1. Create `library.json` at the repository root with the exact JSON from Spec Section 4.1.
2. Create `build_rust.py` at the repository root:
   - Set `LIB_ROOT = os.path.dirname(os.path.abspath(__file__))`, `CRATE_NAME = "portable-xstate-c-cpp"`, `LIB_NAME = "portable_xstate_c_cpp"`, `STATIC_LIB_FILENAME = f"lib{LIB_NAME}.a"`, `CARGO_TOML = os.path.join(LIB_ROOT, CRATE_NAME, "Cargo.toml")`, `TARGET_DIR = os.path.join(LIB_ROOT, "target")`, `INCLUDE_DIR = os.path.join(LIB_ROOT, CRATE_NAME, "include")`.
   - Implement `_detect_host_rust_target()`, `resolve_target(platform, mcu)`, `get_lib_release_dir(rust_dir, rust_target, is_native)`, `build_cargo_cmd(cargo_to_use, rust_target, is_riscv, is_native, manifest_path=CARGO_TOML)`, `_source_esp_env(build_env)`, `_ensure_esp_toolchain_path(build_env)`, and `build_rust_lib(mcu, rust_target, active_env=None, *args, **kwargs)` per Spec Section 4.2 (building in-place against `LIB_ROOT` with no git cloning and no header copying).
3. In `package.json`, add `"test:c-cpp:pio": "python3 -m unittest portable-xstate-c-cpp/tests/test_build_rust.py"` and update `"test:c-cpp"` to `"npm run build:c-cpp && npm run test:c-cpp:c && npm run test:c-cpp:cpp && npm run test:c-cpp:pio"`.

- [ ] **Step 4: Run `npm run test:c-cpp` to verify C, C++, and PlatformIO build script tests pass**

Run: `npm run test:c-cpp`
Expected: PASS (`All C FFI tests passed!`, `All C++ FFI tests passed!`, and `Ran 9 tests ... OK`).

- [ ] **Step 5: Commit Task 1**

```bash
git add library.json build_rust.py package.json portable-xstate-c-cpp/tests/test_build_rust.py
git commit -m "feat(platformio): add in-repo library.json, build_rust.py, and unit tests"
```

---

### Task 2: Migrate `examples/stoplight-esp32`, Remove Cross-Repo Dispatch Workflow, and Update Documentation [flowos: edab9d30-daec-4e63-80f5-efa79af4b64e]

**Files:**
- Modify: `portable-xstate-c-cpp/tests/test_build_rust.py`
- Create: `examples/stoplight-esp32/.gitignore`
- Create: `examples/stoplight-esp32/platformio.ini`
- Create: `examples/stoplight-esp32/src/main.cpp`
- Create: `examples/stoplight-esp32/wokwi.toml`
- Create: `examples/stoplight-esp32/wokwi-circuit.json`
- Delete: `.github/workflows/trigger-platformio-update.yml`
- Modify: `portable-xstate-c-cpp/README.md`
- Modify: `README.md`
- Modify: `examples/README.md`
- Modify: `AGENTS.md`

**Interfaces:**
- Consumes: `REPO_ROOT` in `portable-xstate-c-cpp/tests/test_build_rust.py`
- Produces: Migrated `examples/stoplight-esp32` project, updated PlatformIO documentation, and retired `trigger-platformio-update.yml`

- [ ] **Step 1: Add failing test assertions in `portable-xstate-c-cpp/tests/test_build_rust.py` for the migrated example and removed workflow**

Add `test_stoplight_esp32_example_and_workflow_cleanup` to `TestBuildRust` in `portable-xstate-c-cpp/tests/test_build_rust.py`:

```python
    def test_stoplight_esp32_example_and_workflow_cleanup(self):
        pio_ini = os.path.join(REPO_ROOT, "examples", "stoplight-esp32", "platformio.ini")
        main_cpp = os.path.join(REPO_ROOT, "examples", "stoplight-esp32", "src", "main.cpp")
        self.assertTrue(os.path.isfile(pio_ini))
        self.assertTrue(os.path.isfile(main_cpp))
        with open(pio_ini, "r", encoding="utf-8") as f:
            ini_content = f.read()
        self.assertIn("https://github.com/emmby/portable-xstate.git", ini_content)
        self.assertNotIn("portable-xstate-platformio", ini_content)

        dispatch_workflow = os.path.join(
            REPO_ROOT, ".github", "workflows", "trigger-platformio-update.yml"
        )
        self.assertFalse(os.path.exists(dispatch_workflow))
```

- [ ] **Step 2: Run `npm run test:c-cpp:pio` to verify the new test fails**

Run: `npm run test:c-cpp:pio`
Expected: FAIL (`AssertionError: False is not true` for `examples/stoplight-esp32/platformio.ini`).

- [ ] **Step 3: Migrate `examples/stoplight-esp32`, delete `.github/workflows/trigger-platformio-update.yml`, and update docs**

1. Create `examples/stoplight-esp32/.gitignore` (`/.pio\n`), `examples/stoplight-esp32/platformio.ini` (pointing `lib_deps` to `https://github.com/emmby/portable-xstate.git` per Spec Section 4.4), `examples/stoplight-esp32/src/main.cpp`, `examples/stoplight-esp32/wokwi.toml`, and `examples/stoplight-esp32/wokwi-circuit.json` from `/Users/mike/GitHub/portable-xstate-platformio/examples/stoplight-esp32/`.
2. Remove `.github/workflows/trigger-platformio-update.yml` using `trash .github/workflows/trigger-platformio-update.yml`.
3. Update `portable-xstate-c-cpp/README.md`, `README.md`, and `examples/README.md` to document PlatformIO installation (`lib_deps = https://github.com/emmby/portable-xstate.git`) and the `examples/stoplight-esp32` project.
4. Update Section 3 of `AGENTS.md` to replace `Companion Repository Synchronization (portable-xstate-platformio)` with in-repo PlatformIO maintenance rules (`library.json`, `build_rust.py`, `portable-xstate-c-cpp/tests/test_build_rust.py`, and `examples/stoplight-esp32`).

- [ ] **Step 4: Run the full project build and test suite**

Run: `npm run build && npm test && cargo clippy --all-targets`
Expected: PASS across `test:native`, `test:wasm`, `test:e2e`, `test:c-cpp` (including `test:c-cpp:pio`), `test:unit`, and `cargo clippy` with zero warnings.

- [ ] **Step 5: Commit Task 2**

```bash
git add -A
git commit -m "feat(platformio): migrate stoplight-esp32 example, remove dispatch workflow, and update docs"
```

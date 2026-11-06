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
        rust_dir = "/workspace/lavi"
        self.assertEqual(
            build_rust.get_lib_release_dir(rust_dir, "aarch64-apple-darwin", True),
            "/workspace/lavi/target/release",
        )
        self.assertEqual(
            build_rust.get_lib_release_dir(rust_dir, "xtensa-esp32s3-espidf", False),
            "/workspace/lavi/target/xtensa-esp32s3-espidf/release",
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
        self.assertEqual(build_rust.CRATE_NAME, "lavi-c-cpp")
        self.assertEqual(build_rust.LIB_NAME, "lavi_c_cpp")
        self.assertEqual(build_rust.STATIC_LIB_FILENAME, "liblavi_c_cpp.a")
        self.assertEqual(
            build_rust.CARGO_TOML,
            os.path.join(REPO_ROOT, "lavi-c-cpp", "Cargo.toml"),
        )
        self.assertEqual(
            build_rust.INCLUDE_DIR,
            os.path.join(REPO_ROOT, "lavi-c-cpp", "include"),
        )
        self.assertTrue(os.path.isfile(build_rust.CARGO_TOML))
        self.assertFalse(hasattr(build_rust, "DEPENDENCY_REF"))
        self.assertFalse(hasattr(build_rust, "DEPENDENCY_URL"))
        self.assertFalse(hasattr(build_rust, "clone_dependency"))

    def test_scons_exec_without_dunder_file(self):
        """Simulate PlatformIO SCons SConscript execution where __file__ is not in globals()."""
        script_path = os.path.join(REPO_ROOT, "build_rust.py")
        with open(script_path, "r", encoding="utf-8") as f:
            source = f.read()

        scons_globals = {"__name__": "__main__"}
        with patch("os.getcwd", return_value=REPO_ROOT):
            exec(compile(source, script_path, "exec"), scons_globals)

        self.assertNotIn("__file__", scons_globals)
        self.assertEqual(scons_globals["LIB_ROOT"], REPO_ROOT)
        self.assertEqual(
            scons_globals["CARGO_TOML"],
            os.path.join(REPO_ROOT, "lavi-c-cpp", "Cargo.toml"),
        )

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

    @patch("os.path.exists", return_value=True)
    @patch("subprocess.check_call")
    def test_build_rust_lib_falls_back_to_os_environ_path(
        self, mock_check_call, _mock_exists
    ):
        dummy_env = MagicMock()
        # PlatformIO GUI env with missing PATH key in ENV dict
        dummy_env.__getitem__.side_effect = lambda k: {} if k == "ENV" else None

        def fake_which(cmd, path=None):
            if path is None:
                # Python's shutil.which(cmd, path=None) defaults to os.environ["PATH"]
                return "/custom/cargo/bin/cargo"
            if path and "/custom/cargo/bin" in path:
                return "/custom/cargo/bin/cargo"
            return None

        with patch("shutil.which", side_effect=fake_which), patch.dict(
            os.environ, {"PATH": "/custom/cargo/bin"}, clear=False
        ):
            build_rust.build_rust_lib(
                "native", "aarch64-apple-darwin", active_env=dummy_env, is_native=True
            )
        dummy_env.Exit.assert_not_called()
        mock_check_call.assert_called_once()
        passed_env = mock_check_call.call_args[1]["env"]
        self.assertEqual(passed_env.get("PATH"), "/custom/cargo/bin")

    @patch("os.path.exists", return_value=True)
    @patch("subprocess.run", side_effect=FileNotFoundError(2, "No such file or directory: 'bash'"))
    def test_source_esp_env_handles_missing_bash_oserror(self, _mock_run, _mock_exists):
        build_env = {"PATH": "/usr/bin"}
        build_rust._source_esp_env(build_env)
        self.assertEqual(build_env, {"PATH": "/usr/bin"})

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
        self.assertEqual(manifest.get("name"), "lavi")
        self.assertEqual(manifest.get("frameworks"), ["*"])
        self.assertEqual(manifest.get("platforms"), ["*"])
        build_cfg = manifest.get("build", {})
        self.assertEqual(build_cfg.get("extraScript"), "build_rust.py")
        self.assertEqual(build_cfg.get("includeDir"), "lavi-c-cpp/include")
        self.assertEqual(build_cfg.get("srcFilter"), ["-<*>"])

        include_dir = os.path.join(REPO_ROOT, build_cfg["includeDir"])
        self.assertTrue(os.path.isfile(os.path.join(include_dir, "lavi.h")))
        self.assertTrue(os.path.isfile(os.path.join(include_dir, "lavi.hpp")))

        export_includes = manifest.get("export", {}).get("include", [])
        for member in ["lavi", "lavi-c-cpp", "lavi-wasm"]:
            self.assertTrue(
                any(member in pattern for pattern in export_includes),
                f"Workspace member {member} must be covered in library.json export.include",
            )

    def test_stoplight_esp32_example_and_workflow_cleanup(self):
        pio_ini = os.path.join(REPO_ROOT, "examples", "stoplight-esp32", "platformio.ini")
        main_cpp = os.path.join(REPO_ROOT, "examples", "stoplight-esp32", "src", "main.cpp")
        self.assertTrue(os.path.isfile(pio_ini))
        self.assertTrue(os.path.isfile(main_cpp))
        with open(pio_ini, "r", encoding="utf-8") as f:
            ini_content = f.read()
        self.assertIn("https://github.com/emmby/lavi.git", ini_content)
        self.assertNotIn("portable-xstate-platformio", ini_content)

        dispatch_workflow = os.path.join(
            REPO_ROOT, ".github", "workflows", "trigger-platformio-update.yml"
        )
        self.assertFalse(os.path.exists(dispatch_workflow))


if __name__ == "__main__":
    unittest.main()

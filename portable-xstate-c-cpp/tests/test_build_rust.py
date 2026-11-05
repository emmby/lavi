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

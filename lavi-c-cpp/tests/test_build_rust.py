"""Unit tests for in-repo PlatformIO build_rust.py and library.json."""

import json
import os
import subprocess
import sys
import unittest
from unittest.mock import MagicMock, patch

REPO_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
CRATE_DIR = os.path.join(REPO_ROOT, "lavi-c-cpp")
if CRATE_DIR not in sys.path:
    sys.path.insert(0, CRATE_DIR)

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

    def test_arm_cortex_m_targets(self):
        for mcu, cpu, float_abi, expected in [
            ("cortex-m0plus", "", "", "thumbv6m-none-eabi"),
            ("cortex-m3", "", "", "thumbv7m-none-eabi"),
            ("cortex-m4", "", "", "thumbv7em-none-eabihf"),
            ("cortex-m4", "", "hard", "thumbv7em-none-eabihf"),
            ("cortex-m4", "", "softfp", "thumbv7em-none-eabi"),
            ("cortex-m7", "", "", "thumbv7em-none-eabihf"),
            ("cortex-m7", "", "softfp", "thumbv7em-none-eabi"),
            ("cortex-m33", "", "", "thumbv8m.main-none-eabihf"),
            ("cortex-m33", "", "softfp", "thumbv8m.main-none-eabi"),
            ("rp2040", "cortex-m0plus", "", "thumbv6m-none-eabi"),
            ("rp2350", "cortex-m33", "", "thumbv8m.main-none-eabihf"),
            ("ra4m1", "cortex-m4", "hard", "thumbv7em-none-eabihf"),
            ("nrf52840", "cortex-m4", "softfp", "thumbv7em-none-eabi"),
            ("stm32f407vg", "cortex-m4", "", "thumbv7em-none-eabihf"),
            ("imxrt1062", "cortex-m7", "", "thumbv7em-none-eabihf"),
        ]:
            target, cargo, is_riscv, is_native = build_rust.resolve_target(
                "arm", mcu, cpu=cpu, float_abi=float_abi
            )
            self.assertEqual(target, expected)
            self.assertEqual(cargo, "cargo")
            self.assertFalse(is_riscv)
            self.assertFalse(is_native)

    def test_unsupported_mcu_returns_none(self):
        target, cargo, is_riscv, is_native = build_rust.resolve_target("atmelavr", "atmega328p")
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

        arm_stable_cmd = build_rust.build_cargo_cmd(
            "cargo", "thumbv7em-none-eabihf", False, False, manifest_path="/path/Cargo.toml"
        )
        self.assertEqual(
            arm_stable_cmd,
            [
                "cargo",
                "build",
                "--manifest-path",
                "/path/Cargo.toml",
                "--release",
                "--target",
                "thumbv7em-none-eabihf",
            ],
        )

        arm_build_std_cmd = build_rust.build_cargo_cmd(
            "cargo",
            "thumbv7em-none-eabihf",
            False,
            False,
            manifest_path="/path/Cargo.toml",
            use_build_std=True,
        )
        self.assertEqual(
            arm_build_std_cmd,
            [
                "cargo",
                "build",
                "--manifest-path",
                "/path/Cargo.toml",
                "--release",
                "-Z",
                "build-std=core,alloc,panic_abort",
                "--target",
                "thumbv7em-none-eabihf",
            ],
        )

    def test_c_cpp_crate_paths_and_no_dependency_ref(self):
        self.assertEqual(build_rust.LIB_ROOT, CRATE_DIR)
        self.assertEqual(build_rust.REPO_ROOT, REPO_ROOT)
        self.assertEqual(build_rust.CRATE_NAME, "lavi-c-cpp")
        self.assertEqual(build_rust.LIB_NAME, "lavi_c_cpp")
        self.assertEqual(build_rust.STATIC_LIB_FILENAME, "liblavi_c_cpp.a")
        self.assertEqual(
            build_rust.CARGO_TOML,
            os.path.join(CRATE_DIR, "Cargo.toml"),
        )
        self.assertEqual(
            build_rust.INCLUDE_DIR,
            os.path.join(CRATE_DIR, "include"),
        )
        self.assertTrue(os.path.isfile(build_rust.CARGO_TOML))
        self.assertFalse(os.path.exists(os.path.join(REPO_ROOT, "library.json")))
        self.assertFalse(os.path.exists(os.path.join(REPO_ROOT, "build_rust.py")))
        self.assertFalse(hasattr(build_rust, "DEPENDENCY_REF"))
        self.assertFalse(hasattr(build_rust, "DEPENDENCY_URL"))
        self.assertFalse(hasattr(build_rust, "clone_dependency"))

    def test_scons_exec_without_dunder_file(self):
        """Simulate PlatformIO SCons SConscript execution where __file__ is not in globals()."""
        script_path = os.path.join(CRATE_DIR, "build_rust.py")
        with open(script_path, "r", encoding="utf-8") as f:
            source = f.read()

        scons_globals = {"__name__": "__main__"}
        with patch("os.getcwd", return_value=CRATE_DIR):
            exec(compile(source, script_path, "exec"), scons_globals)

        self.assertNotIn("__file__", scons_globals)
        self.assertEqual(scons_globals["LIB_ROOT"], CRATE_DIR)
        self.assertEqual(
            scons_globals["CARGO_TOML"],
            os.path.join(CRATE_DIR, "Cargo.toml"),
        )

    def test_prebuilt_lib_detection_and_fast_path(self):
        """Validate prebuilt src/<mcu>/liblavi_c_cpp.a detection and SCons fast-path."""
        import tempfile

        tmp_base = os.path.join(REPO_ROOT, ".tmp")
        os.makedirs(tmp_base, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=tmp_base) as pkg_root:
            mcu_dir = os.path.join(pkg_root, "src", "esp32s3")
            os.makedirs(mcu_dir, exist_ok=True)
            inc_dir = os.path.join(pkg_root, "src")
            with open(os.path.join(inc_dir, "lavi.h"), "w", encoding="utf-8") as f:
                f.write("// header\n")
            archive_path = os.path.join(mcu_dir, "liblavi_c_cpp.a")
            with open(archive_path, "wb") as f:
                f.write(b"!<arch>\n")

            self.assertEqual(build_rust.get_prebuilt_lib_dir(pkg_root, "esp32s3"), mcu_dir)
            self.assertTrue(build_rust.has_prebuilt_lib(pkg_root, "esp32s3"))
            self.assertFalse(build_rust.has_prebuilt_lib(pkg_root, "esp32c3"))
            self.assertFalse(build_rust.has_prebuilt_lib(pkg_root, "native"))
            self.assertEqual(build_rust.get_include_dir(pkg_root), inc_dir)

            # LAVI_FORCE_CARGO_BUILD=1 disables prebuilt fast-path
            self.assertFalse(
                build_rust.has_prebuilt_lib(
                    pkg_root, "esp32s3", env_vars={"LAVI_FORCE_CARGO_BUILD": "1"}
                )
            )

            # Simulate SCons execution with prebuilt library present (must NOT invoke Cargo)
            script_path = os.path.join(CRATE_DIR, "build_rust.py")
            with open(script_path, "r", encoding="utf-8") as f:
                source = f.read()

            dummy_env = MagicMock()
            dummy_env.get.side_effect = lambda k, default="": "espressif32" if k == "PIOPLATFORM" else default
            dummy_env.BoardConfig.return_value = {"build.mcu": "esp32s3"}
            dummy_env.GetOption.return_value = False
            dummy_env.__contains__.side_effect = lambda k: k == "ENV"
            dummy_env.__getitem__.side_effect = lambda k: {"PATH": "/usr/bin"} if k == "ENV" else None

            scons_globals = {"__name__": "__main__"}
            with patch.dict(sys.modules, {"SCons": MagicMock(), "SCons.Script": MagicMock()}):
                sys.modules["SCons.Script"].Import.side_effect = (
                    lambda name: scons_globals.__setitem__(name, dummy_env)
                )
                with patch("os.getcwd", return_value=pkg_root), patch(
                    "subprocess.check_call"
                ) as mock_cargo:
                    exec(compile(source, script_path, "exec"), scons_globals)
                    mock_cargo.assert_not_called()
                    dummy_env.Append.assert_called_once_with(
                        CPPPATH=[inc_dir],
                        LIBPATH=[mcu_dir],
                        LIBS=["lavi_c_cpp"],
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
        lib_json_path = os.path.join(CRATE_DIR, "library.json")
        with open(lib_json_path, "r", encoding="utf-8") as f:
            manifest = json.load(f)
        self.assertEqual(manifest.get("name"), "lavi")
        self.assertEqual(manifest.get("license"), "MIT")
        authors = manifest.get("authors", [])
        self.assertTrue(len(authors) >= 1)
        self.assertEqual(authors[0].get("name"), "Mike Burton")
        self.assertTrue(authors[0].get("maintainer"))
        self.assertEqual(manifest.get("frameworks"), ["*"])
        self.assertEqual(manifest.get("platforms"), ["*"])
        build_cfg = manifest.get("build", {})
        self.assertEqual(build_cfg.get("extraScript"), "build_rust.py")
        self.assertEqual(build_cfg.get("srcFilter"), ["-<*>"])

        include_dir = os.path.join(CRATE_DIR, "include")
        self.assertTrue(os.path.isfile(os.path.join(include_dir, "lavi.h")))
        self.assertTrue(os.path.isfile(os.path.join(include_dir, "lavi.hpp")))

        export_includes = manifest.get("export", {}).get("include", [])
        self.assertFalse(
            any("lavi-wasm" in pattern for pattern in export_includes),
            "library.json export.include must not include lavi-wasm",
        )

    def test_library_properties_configuration(self):
        props_path = os.path.join(CRATE_DIR, "library.properties")
        self.assertTrue(os.path.isfile(props_path))
        props = {}
        with open(props_path, "r", encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if not line or line.startswith("#"):
                    continue
                if "=" in line:
                    k, v = line.split("=", 1)
                    props[k.strip()] = v.strip()

        self.assertEqual(props.get("name"), "lavi")
        self.assertEqual(props.get("version"), "0.1.0")
        self.assertEqual(props.get("precompiled"), "full")
        self.assertEqual(props.get("dot_a_linkage"), "true")
        self.assertEqual(props.get("ldflags"), "-llavi_c_cpp")
        self.assertEqual(props.get("includes"), "lavi.h,lavi.hpp")
        archs = [a.strip() for a in props.get("architectures", "").split(",") if a.strip()]
        for expected_mcu in [
            "esp32",
            "esp32s2",
            "esp32s3",
            "esp32c3",
            "esp32c6",
            "esp32h2",
            "cortex-m0plus",
            "cortex-m3",
            "cortex-m4",
            "cortex-m7",
            "cortex-m33",
        ]:
            self.assertIn(expected_mcu, archs)

    def test_esp32_example_and_workflow_cleanup(self):
        pio_ini = os.path.join(REPO_ROOT, "examples", "esp32", "platformio.ini")
        main_cpp = os.path.join(REPO_ROOT, "examples", "esp32", "src", "main.cpp")
        arduino_ino = os.path.join(REPO_ROOT, "examples", "esp32", "Stoplight", "Stoplight.ino")
        old_dir = os.path.join(REPO_ROOT, "examples", "stoplight-esp32")
        self.assertTrue(os.path.isfile(pio_ini))
        self.assertTrue(os.path.isfile(main_cpp))
        self.assertTrue(os.path.isfile(arduino_ino))
        self.assertFalse(os.path.exists(old_dir))
        with open(pio_ini, "r", encoding="utf-8") as f:
            ini_content = f.read()
        self.assertIn("lavi-embedded.zip", ini_content)
        self.assertNotIn("portable-xstate-platformio", ini_content)

        dispatch_workflow = os.path.join(
            REPO_ROOT, ".github", "workflows", "trigger-platformio-update.yml"
        )
        self.assertFalse(os.path.exists(dispatch_workflow))


if __name__ == "__main__":
    unittest.main()

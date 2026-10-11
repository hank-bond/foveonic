"""Check native bundle resources, compiler icon metadata, and launch argument preservation."""

import importlib.util
from pathlib import Path
import plistlib
import tempfile
import unittest
from unittest.mock import patch

CRATE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("build_macos_app", CRATE / "build-macos-app.py")
APP = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(APP)


class BundleTests(unittest.TestCase):
    def test_bundle_uses_compiler_metadata_and_preserves_executable_mode(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / "foveonic-gui"
            binary.write_bytes(b"test executable")
            binary.chmod(0o755)
            compiled = root / "compiled"
            compiled.mkdir()
            for name in ["Assets.car", "Foveonic.icns"]:
                (compiled / name).write_bytes(name.encode())
            icon_metadata = {"CFBundleIconFile": "Foveonic", "CFBundleIconName": "Foveonic"}
            with (compiled / "partial.plist").open("wb") as target:
                plistlib.dump(icon_metadata, target)
            bundle = APP.assemble_bundle(binary, compiled, "0.0.1")
            contents = bundle / "Contents"
            executable = contents / "MacOS/foveonic-gui"
            self.assertEqual(executable.read_bytes(), binary.read_bytes())
            self.assertEqual(executable.stat().st_mode & 0o777, 0o755)
            with (contents / "Info.plist").open("rb") as source:
                metadata = plistlib.load(source)
            for key, value in icon_metadata.items():
                self.assertEqual(metadata[key], value)
            self.assertEqual(metadata["CFBundleIdentifier"], "org.foveonic.foveonic")
            self.assertEqual(metadata["CFBundleExecutable"], executable.name)
            self.assertEqual(metadata["LSMinimumSystemVersion"], "26.0")
            self.assertNotIn("NSRequiresAquaSystemAppearance", metadata)
            self.assertEqual(sorted(p.name for p in (contents / "Resources").iterdir()),
                             ["Assets.car", "Foveonic.icns"])
            # A rebuild replaces stale resources instead of keeping an old icon path.
            stale = contents / "Resources/old-icon.png"
            stale.write_bytes(b"old")
            APP.assemble_bundle(binary, compiled, "0.0.2")
            self.assertFalse(stale.exists())

    def test_input_path_is_absolute_without_changing_camera_overrides(self):
        arguments = ["--wb", "Shade", "--mode", "Standard", "relative folder/image.X3F"]
        result = APP.launch_arguments(arguments)
        self.assertEqual(result[:4], arguments[:4])
        self.assertEqual(result[4], str(Path(arguments[4]).resolve()))
        self.assertEqual(APP.launch_arguments(["--help"]), ["--help"])
        self.assertEqual(APP.launch_arguments([]), [])

    def test_cargo_report_controls_artifact_location(self):
        report = '{"reason":"compiler-artifact","target":{"name":"foveonic-gui"},"executable":"/custom-target/release/foveonic-gui"}\n'
        with patch.object(APP.subprocess, "run") as run:
            run.return_value.stdout = report
            self.assertEqual(APP.build_binary(), Path("/custom-target/release/foveonic-gui"))
            self.assertIn("--release", run.call_args.args[0])
            self.assertEqual(APP.build_binary(debug=True), Path("/custom-target/release/foveonic-gui"))
            self.assertNotIn("--release", run.call_args.args[0])


if __name__ == "__main__":
    unittest.main()

"""Build and launch a local macOS app bundle whose native icon follows system appearance settings."""

import argparse
import json
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import tempfile

CRATE = Path(__file__).resolve().parent
ROOT = CRATE.parent.parent


def build_binary(debug=False):
    """Use Cargo's reported artifact path, including custom target directories."""
    command = ["cargo", "build", "-p", "foveonic-gui", "--message-format=json-render-diagnostics"]
    if not debug:
        command.append("--release")
    result = subprocess.run(command, cwd=ROOT, check=True, stdout=subprocess.PIPE, text=True)
    for line in result.stdout.splitlines():
        message = json.loads(line)
        if (message.get("reason") == "compiler-artifact"
                and message.get("target", {}).get("name") == "foveonic-gui"
                and message.get("executable")):
            return Path(message["executable"])
    raise RuntimeError("Cargo did not report the GUI executable")


def assemble_bundle(binary, compiled, version):
    """Package only the executable and native icon resources beside the Cargo artifact."""
    bundle = binary.parent / "Foveonic.app"
    contents = bundle / "Contents"
    if bundle.exists():
        shutil.rmtree(bundle)
    (contents / "MacOS").mkdir(parents=True)
    (contents / "Resources").mkdir()
    shutil.copy2(binary, contents / "MacOS/foveonic-gui")
    for name in ["Assets.car", "Foveonic.icns"]:
        shutil.copy2(compiled / name, contents / "Resources" / name)
    with (compiled / "partial.plist").open("rb") as source:
        icon_metadata = plistlib.load(source)
    metadata = {
        "CFBundleExecutable": "foveonic-gui",
        "CFBundleIdentifier": "org.foveonic.foveonic",
        "CFBundleName": "Foveonic",
        "CFBundleDisplayName": "Foveonic",
        "CFBundlePackageType": "APPL",
        "CFBundleShortVersionString": version,
        "CFBundleVersion": version,
        "LSMinimumSystemVersion": "26.0",
        "NSHighResolutionCapable": True,
        "NSSupportsAutomaticGraphicsSwitching": True,
    }
    # actool owns the icon names; Assets.car supplies automatic light/dark
    # appearances, and its companion ICNS provides the static fallback.
    metadata.update(icon_metadata)
    with (contents / "Info.plist").open("wb") as target:
        plistlib.dump(metadata, target)
    return bundle


def launch_arguments(arguments):
    """Resolve the input path before LaunchServices changes the working directory."""
    result = list(arguments)
    skip_value = False
    for i, argument in enumerate(result):
        if skip_value:
            skip_value = False
        elif argument in ["--wb", "--mode"]:
            skip_value = True
        elif not argument.startswith("-"):
            result[i] = str(Path(argument).expanduser().resolve())
            break
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--debug", action="store_true", help="build the debug profile instead of release")
    parser.add_argument("--build-only", action="store_true", help="create the bundle without opening it")
    parser.add_argument("arguments", nargs=argparse.REMAINDER, help="GUI arguments after --")
    args = parser.parse_args()
    if sys.platform != "darwin":
        parser.error("this command requires macOS and Xcode 26 or later")
    with tempfile.TemporaryDirectory(prefix="foveonic-app-icon-") as directory:
        compiled = Path(directory)
        subprocess.run([
            sys.executable, str(CRATE / "assets/generate-icon.py"),
            "--compile-dir", str(compiled),
        ], check=True)
        binary = build_binary(args.debug)
        cargo_metadata = subprocess.run([
            "cargo", "metadata", "--no-deps", "--format-version", "1",
        ], cwd=ROOT, check=True, stdout=subprocess.PIPE, text=True)
        version = next(package["version"] for package in json.loads(cargo_metadata.stdout)["packages"]
                       if package["name"] == "foveonic-gui")
        bundle = assemble_bundle(binary, compiled, version)
    print(bundle, flush=True)
    if not args.build_only:
        arguments = args.arguments
        if arguments[:1] == ["--"]:
            arguments = arguments[1:]
        subprocess.run(["open", "-n", str(bundle), "--args", *launch_arguments(arguments)], check=True)


if __name__ == "__main__":
    main()

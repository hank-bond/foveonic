---
summary: "Install the tools needed to build Foveonic from a fresh macOS or Windows machine."
kind: how-to
---

# Build from scratch

Foveonic is built and tested on Apple Silicon macOS and Windows x64.

Every build needs Rust 1.88 or newer, Git, a C/C++ compiler and CMake. The JPEG XL dependency (`jxl-sys`) compiles C++ with CMake during the build.

## macOS

1. Install Xcode 26+ and select its command-line tools: `sudo xcode-select -s /Applications/Xcode.app`.
2. Install [Homebrew](https://brew.sh), then `brew install cmake`.
3. Install Rust with [rustup](https://rustup.rs). On Apple Silicon, use the native `aarch64-apple-darwin` toolchain. An x86_64 toolchain runs under Rosetta and distorts performance measurements.
4. From the repository root, build the app with `cargo xtask bundle`. It is created at `target/release/Foveonic.app`.

## Windows

1. Install Visual Studio 2022 or its Build Tools with the C++ desktop workload, including the C++ Clang tools (`clang-cl`) and CMake.
2. Install Rust with [rustup](https://rustup.rs) using the default `x86_64-pc-windows-msvc` toolchain.
3. From a developer shell at the repository root, run `cargo build --locked --release -p foveonic-gui`.
4. Place `dxcompiler.dll` and `dxil.dll` from the pinned [DirectX shader compiler](https://github.com/microsoft/DirectXShaderCompiler/releases/tag/v1.9.2609) beside `foveonic-gui.exe`. `python scripts/setup_tests.py --install` downloads and verifies the pinned archive.

## Tests

Running the maintained tests needs additional tools, including Python 3.11+ and ExifTool. `python3 scripts/setup_tests.py` reports what is missing. On macOS, `--install` installs missing Homebrew packages. See the [test inventory](./testing-coverage.md) for setup details.

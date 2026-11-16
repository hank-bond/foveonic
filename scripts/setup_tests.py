"""Prepare shared test tools, Python dependencies and reproducible example images."""

import argparse
import hashlib
import io
import json
from dataclasses import dataclass
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1]
PYTHON_ENV = ROOT / '.local/test-python'
PYTHON_REQUIREMENTS = ROOT / 'scripts/test-requirements.txt'
DXC_URL = 'https://github.com/microsoft/DirectXShaderCompiler/releases/download/v1.9.2609/dxc_2026_09_29.zip'
DXC_SHA256 = 'ad31b1fc8443175d204f77a611fdb3ef2ec42759bdc2f1167368de24a4a7e7f1'
DXC_CACHE = ROOT / '.local/test-dependencies/dxc_2026_09_29.zip'
DXC_FILES = ('dxcompiler.dll', 'dxil.dll')


@dataclass(frozen=True)
class Tool:
    name: str
    command: tuple
    formula: str = ''
    component: str = ''


TOOLS = (
    Tool('Rust 1.88+', ('rustc', '--version')),
    Tool('Cargo', ('cargo', '--version')),
    Tool('rustfmt', ('cargo', 'fmt', '--version'), component='rustfmt'),
    Tool('Clippy', ('cargo', 'clippy', '--version'), component='clippy'),
    Tool('C/C++ compiler', ('cc', '--version')),
    Tool('CMake', ('cmake', '--version'), formula='cmake'),
    Tool('ExifTool', ('exiftool', '-ver'), formula='exiftool'),
)
GITHUB = Tool('GitHub CLI', ('gh', '--version'), formula='gh')


def version(tool):
    if not shutil.which(tool.command[0]):
        return None
    result = subprocess.run(tool.command, cwd=ROOT, capture_output=True, text=True, check=False)
    if result.returncode:
        return None
    text = result.stdout.strip().splitlines()
    if not text:
        return None
    if tool.command[0] == 'rustc':
        match = re.search(r'rustc (\d+)\.(\d+)', text[0])
        if not match or tuple(map(int, match.groups())) < (1, 88):
            return None
    return text[0]


def check_tools(install=False, images=False, offline=False):
    tools = tuple(Tool(tool.name, ('clang-cl', '--version'))
                  if platform.system() == 'Windows' and tool.name == 'C/C++ compiler' else tool
                  for tool in TOOLS)
    tools = (*tools, GITHUB) if images and not offline else tools
    found = {tool: version(tool) for tool in tools}
    missing = [tool for tool in tools if found[tool] is None]
    if install and missing:
        # Bootstrap Rust, Xcode and package managers deliberately outside this
        # helper. Installing these changes the machine's toolchain selection.
        unsupported = [tool.name for tool in missing if not (tool.formula or tool.component)]
        if unsupported:
            raise ValueError('Install prerequisites first: ' + ', '.join(unsupported)
                             + '. See docs/testing-coverage.md#test-environment-setup')
        formulas = [tool.formula for tool in missing if tool.formula]
        components = [tool.component for tool in missing if tool.component]
        if formulas and (platform.system() != 'Darwin' or not shutil.which('brew')):
            raise ValueError('Automatic package installation requires macOS and Homebrew. '
                             'Install these tools manually: ' + ', '.join(formulas))
        if components and not shutil.which('rustup'):
            raise ValueError('Install rustfmt and Clippy for the selected Rust toolchain manually.')
        if formulas:
            subprocess.run(['brew', 'install', *formulas], check=True)
        if components:
            subprocess.run(['rustup', 'component', 'add', *components], cwd=ROOT, check=True)
        found = {tool: version(tool) for tool in tools}
        missing = [tool for tool in tools if found[tool] is None]
    for tool, text in found.items():
        print(f'{tool.name}: {text or "MISSING or incompatible"}')
    if missing:
        raise ValueError('Missing test requirements: ' + ', '.join(tool.name for tool in missing)
                         + '. Run with --install on macOS, or see '
                         'docs/testing-coverage.md#test-environment-setup')


def prepare_images(source_dir, offline):
    flags = ['--offline'] if offline else []
    subprocess.run(['cargo', 'run', '--locked', '--package', 'xtask', *flags, '--',
                    'examples', 'sync', '--source-dir', str(source_dir), *flags],
                   cwd=ROOT, check=True)
    subprocess.run([sys.executable, str(ROOT / 'scripts/profiling/fetch_samples.py'),
                    '--directory', str(source_dir), *flags], cwd=ROOT, check=True)


def cargo_target_directory():
    """Ask Cargo for the configured target root without fetching dependencies."""
    result = subprocess.run(['cargo', 'metadata', '--locked', '--offline', '--no-deps',
                             '--format-version', '1'], cwd=ROOT, capture_output=True,
                            text=True, check=True)
    return Path(json.loads(result.stdout)['target_directory'])


def prepare_dxc(install=False, archive=DXC_CACHE, target=None):
    """Verify the pinned compiler beside Windows test and application binaries."""
    if platform.system() != 'Windows':
        return
    if platform.machine().lower() not in ('amd64', 'x86_64'):
        raise ValueError('The pinned DirectX shader compiler requires Windows x64.')
    archive = Path(archive)
    if not archive.is_file():
        if not install:
            raise ValueError('Missing pinned DirectX shader compiler archive. Run scripts/setup_tests.py --install')
        archive.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=archive.parent) as temporary:
            download = Path(temporary) / 'dxc.zip'
            with download.open('wb') as output, urllib.request.urlopen(DXC_URL, timeout=60) as response:
                shutil.copyfileobj(response, output)
            if hashlib.sha256(download.read_bytes()).hexdigest() != DXC_SHA256:
                raise ValueError('DirectX shader compiler archive checksum mismatch')
            # Do not replace a file created by another preparation process.
            destination = archive.open('xb')
            try:
                with destination, download.open('rb') as source:
                    shutil.copyfileobj(source, destination)
            except Exception:
                archive.unlink(missing_ok=True)
                raise
    contents = archive.read_bytes()
    if hashlib.sha256(contents).hexdigest() != DXC_SHA256:
        raise ValueError('DirectX shader compiler archive checksum mismatch')
    try:
        with zipfile.ZipFile(io.BytesIO(contents)) as package:
            files = {name: package.read(f'bin/x64/{name}') for name in DXC_FILES}
    except (KeyError, zipfile.BadZipFile) as error:
        raise ValueError('DirectX shader compiler archive is incomplete or invalid') from error
    target = Path(target) if target is not None else cargo_target_directory()
    destinations = [target / profile / suffix / name
                    for profile in ('debug', 'release') for suffix in ('', 'deps') for name in DXC_FILES]
    # Validate every existing destination before making any changes. Never
    # overwrite a different compiler in a shared/custom target directory.
    for path in destinations:
        if path.exists() and (not path.is_file() or path.read_bytes() != files[path.name]):
            raise ValueError(f'Existing DirectX shader compiler differs from the pinned archive: {path}')
    missing = [path for path in destinations if not path.is_file()]
    if missing and not install:
        raise ValueError('Missing staged DirectX shader compiler files. Run scripts/setup_tests.py --install')
    for path in missing:
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open('xb') as output:
            output.write(files[path.name])
    print('DirectX shader compiler: pinned archive and debug/release application and test paths verified')


def prepare_python(install=False, directory=PYTHON_ENV):
    """Reuse one repository environment for every Python test suite."""
    python = directory / ('Scripts/python.exe' if sys.platform == 'win32' else 'bin/python')
    if not python.is_file():
        if not install:
            raise ValueError('Missing shared Python test environment. Run scripts/setup_tests.py --install')
        subprocess.run([sys.executable, '-m', 'venv', str(directory)], cwd=ROOT, check=True)
    required = dict(line.strip().split('==', 1) for line in PYTHON_REQUIREMENTS.read_text().splitlines()
                    if line.strip() and not line.lstrip().startswith('#'))
    command = [str(python), '-c',
               'import json, sys; from importlib.metadata import version; '
               'sys.exit(not all(version(k) == v for k, v in json.loads(sys.argv[1]).items()))',
               json.dumps(required)]
    def ready():
        return subprocess.run(command, cwd=ROOT, capture_output=True, text=True, check=False).returncode == 0
    if not ready():
        if not install:
            raise ValueError('Missing or incompatible Python test dependencies: '
                             + ', '.join(required) + '. Run scripts/setup_tests.py --install')
        subprocess.run([str(python), '-m', 'pip', 'install', '-r', str(PYTHON_REQUIREMENTS)],
                       cwd=ROOT, check=True)
        if not ready():
            raise ValueError('Python test dependencies remain unavailable after installation')
    print('Shared Python test interpreter: ' + str(python))
    print('Python test dependencies: ' + ', '.join(f'{name}=={value}' for name, value in required.items()))
    return python


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--install', action='store_true', help='prepare missing tools and shared Python test dependencies')
    parser.add_argument('--images', action='store_true', help='sync examples and verify pinned external samples')
    parser.add_argument('--offline', action='store_true', help='verify cached images without downloads')
    parser.add_argument('--source-dir', type=Path, default=ROOT / '.local/example-photographs')
    args = parser.parse_args()
    if args.install and args.offline:
        parser.error('--install cannot be combined with --offline')
    if sys.version_info < (3, 11):
        parser.error('Python 3.11 or later is required by the shared test dependencies')
    try:
        check_tools(args.install, args.images, args.offline)
        prepare_python(args.install)
        prepare_dxc(args.install)
        if args.images:
            prepare_images(args.source_dir.resolve(), args.offline)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, str(error) + '\n')
    print('Test tools ready. GPU and graphical-session availability are checked by the selected tests.')


if __name__ == '__main__':
    main()

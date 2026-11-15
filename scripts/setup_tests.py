"""Prepare shared test tools, Python dependencies and reproducible example images."""

import argparse
import json
from dataclasses import dataclass
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
PYTHON_ENV = ROOT / '.local/test-python'
PYTHON_REQUIREMENTS = ROOT / 'scripts/test-requirements.txt'


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
    tools = (*TOOLS, GITHUB) if images and not offline else TOOLS
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
        if args.images:
            prepare_images(args.source_dir.resolve(), args.offline)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, str(error) + '\n')
    print('Test tools ready. GPU and graphical-session availability are checked by the selected tests.')


if __name__ == '__main__':
    main()

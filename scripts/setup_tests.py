"""Check test tools, explicitly install missing macOS dependencies, and prepare example images."""

import argparse
from dataclasses import dataclass
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--install', action='store_true', help='install only missing supported tools')
    parser.add_argument('--images', action='store_true', help='sync examples and verify pinned external samples')
    parser.add_argument('--offline', action='store_true', help='verify cached images without downloads')
    parser.add_argument('--source-dir', type=Path, default=ROOT / '.local/example-photographs')
    args = parser.parse_args()
    if args.install and args.offline:
        parser.error('--install cannot be combined with --offline')
    try:
        check_tools(args.install, args.images, args.offline)
        if args.images:
            prepare_images(args.source_dir.resolve(), args.offline)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, str(error) + '\n')
    print('Test tools ready. GPU and graphical-session availability are checked by the selected tests.')


if __name__ == '__main__':
    main()

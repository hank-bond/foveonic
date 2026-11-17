"""Fingerprint compatible CI dependencies and export the actual checkout identity."""

import hashlib
import json
import os
from pathlib import Path
import platform
import re
import subprocess

ENVIRONMENT = ('ImageOS', 'ImageVersion', 'MACOSX_DEPLOYMENT_TARGET', 'CC', 'CXX',
               'CFLAGS', 'CXXFLAGS', 'LDFLAGS', 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS',
               'CARGO_BUILD_TARGET', 'CARGO_INCREMENTAL')
NATIVE_TOOLS = {
    'Darwin': [['xcodebuild', '-version'], ['xcrun', '--sdk', 'macosx', '--show-sdk-version'],
               ['clang', '--version'], ['cmake', '--version']],
    'Windows': [['clang-cl', '--version'], ['cmake', '--version']],
}


def fingerprint(platform_identity, tool_versions, environment, helper_sha256):
    """Hash explicit compatibility inputs without including the source revision."""
    identity = {'platform': platform_identity, 'tools': tool_versions,
                'helper_sha256': helper_sha256,
                'environment': {name: environment.get(name, '') for name in ENVIRONMENT}}
    value = hashlib.sha256(json.dumps(identity, sort_keys=True).encode()).hexdigest()
    return value, identity


def main():
    system = platform.system()
    if system not in NATIVE_TOOLS:
        raise ValueError('build cache fingerprint requires Darwin or Windows')
    commands = [['rustc', '-vV'], *NATIVE_TOOLS[system]]
    tools = {' '.join(command): subprocess.check_output(command, text=True).strip()
             for command in commands}
    revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
    if re.fullmatch('[0-9a-f]{40}', revision) is None:
        raise ValueError('git did not return a full source revision')
    helper_sha = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
    value, identity = fingerprint(platform.platform(), tools, os.environ, helper_sha)
    with open(os.environ['GITHUB_OUTPUT'], 'a') as output:
        output.write('fingerprint=' + value + '\nrevision=' + revision + '\n')
    with open(os.environ['GITHUB_ENV'], 'a') as environment:
        environment.write('FOVEONIC_BUILD_REVISION=' + revision + '\n')
    print(json.dumps(identity, indent=2))


if __name__ == '__main__':
    main()

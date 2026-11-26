"""Reconcile ignored Rust test capabilities and print pinned, serial libtest commands without running tests."""

import argparse
import json
import os
from pathlib import Path
import re
import subprocess

from file_hash import digest

HERE = Path(__file__).resolve().parent
MANIFEST = HERE / 'rust_test_inventory.json'
RESOURCES = {'gpu', 'gpu_sdk', 'exiftool', 'gpu_exiftool'}


def _simple(value):
    return isinstance(value, str) and re.fullmatch(r'[A-Za-z0-9_-]+', value) is not None


def validate_inventory(document):
    """Reject ambiguous or incomplete declarations before discovering binaries or inputs."""
    if document.get('schema_version') != 1:
        raise ValueError('unsupported Rust inventory schema')
    suites, identities, groups = {}, set(), set()
    for suite in document['suites']:
        key = (suite['kind'], suite['target'])
        if (not _simple(suite['id']) or suite['id'] in suites
                or key in [s['key'] for s in suites.values()]):
            raise ValueError('inventory suite identities must be unique')
        suites[suite['id']] = {'key': key, 'tests': set()}
    for group in document['groups']:
        if (not _simple(group['id']) or group['id'] in groups
                or group['suite'] not in suites or group['resource'] not in RESOURCES
                or group['environment'] not in document['environments']
                or not isinstance(group['one_test_per_process'], bool) or not group['tests']):
            raise ValueError('invalid required test group')
        groups.add(group['id'])
        for name in group['tests']:
            identity = (group['suite'], name)
            if not isinstance(name, str) or not name or identity in identities:
                raise ValueError('test classification must be unique and explicit')
            identities.add(identity)
            suites[group['suite']]['tests'].add(name)
    for row in document['excluded']:
        identity = (row['suite'], row['test'])
        if (row['suite'] not in suites or not isinstance(row['test'], str) or not row['test']
                or identity in identities or not row['reason'].strip()
                or not row['requirements'].strip() or not row['source'].strip()):
            raise ValueError('excluded tests need unique names, reasons and explicit dependencies')
        identities.add(identity)
        suites[row['suite']]['tests'].add(row['test'])
    if not identities:
        raise ValueError('inventory is empty')
    for names in document['input_sets'].values():
        if not names or len(names) != len(set(names)) or any(Path(n).name != n for n in names):
            raise ValueError('fixture sets need unique source basenames')
    for values in document['environments'].values():
        for variable, binding in values.items():
            if re.fullmatch(r'[A-Z][A-Z0-9_]*', variable) is None or len(binding) != 1:
                raise ValueError('invalid environment binding')
            if 'directory' in binding:
                if binding['directory'] not in document['input_sets']:
                    raise ValueError('unknown fixture set')
            elif 'value' in binding:
                if not isinstance(binding['value'], str) or not binding['value']:
                    raise ValueError('literal environment values must be nonempty strings')
            elif 'file' not in binding or binding['file'] not in document['input_sets']['standard']:
                raise ValueError('unknown fixture file')
    return suites


LIBRARY_KINDS = {'lib', 'rlib', 'staticlib', 'cdylib', 'dylib'}


def compiler_artifacts(text):
    """Read Cargo's JSON messages, including compiler-reported test executable paths."""
    artifacts, build_finished = {}, 0
    for line in text.splitlines():
        if not line.strip():
            continue
        row = json.loads(line)
        if row.get('reason') == 'build-finished':
            build_finished += 1
            if build_finished != 1 or row.get('success') is not True:
                raise ValueError('compiler JSON requires exactly one successful build-finished record')
        if (row.get('reason') != 'compiler-artifact' or not row.get('executable')
                or row.get('profile', {}).get('test') is not True):
            continue
        if build_finished:
            raise ValueError('test artifact appears after build-finished')
        target = row['target']
        kinds = set(target['kind'])
        # A library also built for linking, such as a static library, still runs one lib test.
        if len(kinds) > 1 and kinds <= LIBRARY_KINDS:
            kinds = {'lib'}
        if len(kinds) != 1:
            raise ValueError('ambiguous test target kind')
        key = (kinds.pop(), target['name'])
        artifact = {'kind': key[0], 'target': key[1], 'package_id': row['package_id'],
                    'executable': str(Path(row['executable']).resolve(strict=True))}
        if key in artifacts and artifacts[key] != artifact:
            raise ValueError(f'ambiguous compiler test target: {key}')
        artifacts[key] = artifact
    if build_finished != 1:
        raise ValueError('compiler JSON requires exactly one successful build-finished record')
    if not artifacts:
        raise ValueError('compiler JSON contains no test executables')
    return list(artifacts.values())


def ignored_names(text):
    """Parse only libtest's listing format; no test body is invoked."""
    names, summary = [], None
    for line in text.splitlines():
        if line.endswith(': test'):
            names.append(line[:-6])
        elif line.strip():
            match = re.fullmatch(r'(\d+) tests?, (\d+) benchmarks?', line)
            if match is None or summary is not None:
                raise ValueError(f'unrecognized ignored-test listing: {line}')
            summary = tuple(map(int, match.groups()))
    if len(names) != len(set(names)):
        raise ValueError('duplicate ignored test name')
    if summary != (len(names), 0):
        raise ValueError('ignored-test listing is incomplete or contains unclassified benchmarks')
    return names


def discover(artifacts, runner=subprocess.run):
    """Launch test binaries in listing mode only, retaining binary and list identities."""
    found = []
    for artifact in artifacts:
        command = [artifact['executable'], '--ignored', '--list']
        output = runner(command, capture_output=True, text=True, check=True, timeout=30)
        found.append({**artifact, 'binary_sha256': digest(artifact['executable']),
                      'list_command': command, 'ignored': ignored_names(output.stdout)})
    return found


def reconcile(document, discovered):
    suites = validate_inventory(document)
    by_key = {}
    for row in discovered:
        key = (row['kind'], row['target'])
        if key in by_key:
            raise ValueError(f'duplicate discovered test target: {key}')
        if len(row['ignored']) != len(set(row['ignored'])):
            raise ValueError(f'duplicate discovered test: {key}')
        by_key[key] = row
    declared = {s['key'] for s in suites.values()}
    for key, row in by_key.items():
        if key not in declared and row['ignored']:
            raise ValueError(f'unclassified ignored test target: {key}')
    result = {}
    for identity, suite in suites.items():
        if suite['key'] not in by_key:
            raise ValueError(f'missing compiler test target: {identity}')
        row = by_key[suite['key']]
        actual = set(row['ignored'])
        unknown, missing = actual - suite['tests'], suite['tests'] - actual
        if unknown or missing:
            raise ValueError(f'{identity}: unclassified={sorted(unknown)}; missing={sorted(missing)}')
        result[identity] = row
    return result


def input_hashes(document, definitions, external):
    """Reuse maintained identities, adding only the retained audit's extra originals."""
    hashes = dict(definitions['sample_hashes'])
    for row in external['files']:
        if row['file'] in hashes:
            raise ValueError('duplicate shared input identity')
        hashes[row['file']] = row['sha256']
    for name, row in document['additional_inputs'].items():
        if name in hashes:
            raise ValueError('inventory duplicates a shared input identity')
        hashes[name] = row['sha256']
    required = {name for names in document['input_sets'].values() for name in names}
    if any(name not in hashes or re.fullmatch(r'[0-9a-f]{64}', hashes[name]) is None for name in required):
        raise ValueError('fixture set has no valid pinned input identity')
    return hashes


def verify_inputs(document, hashes, directories):
    """Check disposable fixture copies without modifying them or staging new files."""
    receipts = {}
    for identity, expected in document['input_sets'].items():
        directory = Path(directories[identity]).resolve(strict=True)
        actual = {p.name for p in directory.iterdir() if p.suffix.lower() == '.x3f'}
        if actual != set(expected):
            raise ValueError(f'{identity}: fixture membership differs: missing={sorted(set(expected)-actual)}, extra={sorted(actual-set(expected))}')
        files = []
        for name in expected:
            path = directory / name
            if not path.is_file() or path.is_symlink() or path.stat().st_nlink != 1:
                raise ValueError(f'{path}: tests require disposable full file copies')
            checksum = digest(path)
            if checksum != hashes[name]:
                raise ValueError(f'{path}: pinned fixture SHA-256 differs')
            extra = document['additional_inputs'].get(name)
            if extra is not None and path.stat().st_size != extra['bytes']:
                raise ValueError(f'{path}: pinned fixture byte length differs')
            files.append({'file': name, 'sha256': checksum, 'bytes': path.stat().st_size})
        receipts[identity] = {'directory': str(directory), 'files': files}
    return receipts


def command_plan(document, discovered, fixtures, environ=None):
    """Print ordinary serial libtest groups; this module never executes these commands."""
    matched = reconcile(document, discovered)
    rules = document['unset_environment']
    unset = sorted(set(rules['names']) | {name for name in (os.environ if environ is None else environ)
                   if name.startswith(tuple(rules['prefixes'])) or name.endswith(tuple(rules['suffixes']))})
    commands = []
    for group in document['groups']:
        artifact = matched[group['suite']]
        environment = {}
        for name, binding in document['environments'][group['environment']].items():
            if 'directory' in binding:
                environment[name] = fixtures[binding['directory']]['directory']
            elif 'file' in binding:
                environment[name] = str(Path(fixtures['standard']['directory']) / binding['file'])
            else:
                environment[name] = binding['value']
        selections = [[name] for name in group['tests']] if group['one_test_per_process'] else [group['tests']]
        for index, names in enumerate(selections):
            arguments = [artifact['executable'], *names, '--ignored', '--exact', '--test-threads=1']
            if group['resource'] in ('exiftool', 'gpu_exiftool'):
                arguments.append('--nocapture')
            commands.append({'group': group['id'], 'partition': index, 'resource': group['resource'],
                             'serial_per_adapter': True, 'suite': group['suite'],
                             'binary_sha256': artifact['binary_sha256'], 'tests': names,
                             'argv': arguments,
                             'unset_environment': unset, 'environment': environment,
                             'environment_order': 'unset first, then apply environment; preserve other host settings and pinned runtime DLL paths'})
    return {'schema_version': 1, 'classification_complete': True, 'test_execution_performed': False,
            'scope': document['scope'], 'fixture_policy': document['fixture_policy'],
            'counts': {'required': sum(len(g['tests']) for g in document['groups']),
                       'excluded': len(document['excluded']), 'commands': len(commands)},
            'discovered': discovered, 'fixtures': fixtures, 'commands': commands,
            'excluded': document['excluded']}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--compiler-json', type=Path, required=True,
                        help='stdout from cargo test --workspace --no-run --message-format=json; never builds here')
    parser.add_argument('--manifest', type=Path, default=MANIFEST)
    parser.add_argument('--fixture-dir', type=Path, required=True, help='disposable full-copy directory with the declared original set')
    parser.add_argument('--merrill-fixture-dir', type=Path, required=True, help='disposable Merrill-HI-only fixture directory')
    parser.add_argument('--output', type=Path, help='new JSON command-plan file; default stdout')
    args = parser.parse_args()
    try:
        document = json.loads(args.manifest.read_text())
        validate_inventory(document)
        found = discover(compiler_artifacts(args.compiler_json.read_text()))
        reconcile(document, found)
        hashes = input_hashes(document, json.loads((HERE / 'test_inputs.json').read_text()),
                              json.loads((HERE / 'external_raw_samples.json').read_text()))
        fixtures = verify_inputs(document, hashes, {'standard': args.fixture_dir,
                                                   'merrill_high_only': args.merrill_fixture_dir})
        report = command_plan(document, found, fixtures)
        report['inputs'] = {name: {'path': str(path.resolve()), 'sha256': digest(path)} for name, path in
                            [('inventory', args.manifest), ('compiler_json', args.compiler_json),
                             ('input_definitions', HERE / 'test_inputs.json'),
                             ('external_samples', HERE / 'external_raw_samples.json')]}
        if args.output is None:
            print(json.dumps(report, indent=2))
        else:
            with args.output.open('x') as stream:
                stream.write(json.dumps(report, indent=2) + '\n')
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
        parser.exit(1, str(error) + '\n')


if __name__ == '__main__':
    main()

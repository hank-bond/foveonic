"""Compare experimental polling intervals through existing background-window application scenarios."""

import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

from file_hash import digest
from wide import build_provenance, inputs

HERE = Path(__file__).resolve().parent


def run(args):
    binary = args.binary.resolve(strict=True)
    ordinary = args.ordinary_binary.resolve(strict=True)
    provenance = build_provenance(binary, args.build_provenance)
    sources = inputs(args.source_dir, HERE / 'wide_inputs.json')
    if binary.name != 'foveonic-gui' or args.trials < 1:
        raise ValueError('use a frozen foveonic-gui and positive trials')
    directory = args.directory.resolve()
    directory.mkdir(parents=True, exist_ok=False)
    (directory / 'wide_native.py').write_bytes(Path(__file__).read_bytes())
    cases = [{'source': row['source'], 'settings': {'exposure': -0.15 if index % 2 == 0 else 0.15,
                                                  'exposure_auto': False}}
             for index, row in enumerate(sources)]
    case_path = directory / 'cases.json'
    case_path.write_text(json.dumps(cases, indent=2) + '\n')
    result = {'kind': 'wide-poll-native-context', 'build_provenance': provenance,
              'ordinary_binary_sha256': digest(ordinary), 'inputs': sources,
              'paired_count': args.paired_count, 'rows': [], 'passed': False,
              'limits': ['Application execution remains serial; only polling interval changes.',
                         'Command-driven scenarios in background windows, without physical pointer injection.',
                         'These endpoints do not measure physical screen presentation.',
                         'Each scenario uses a disposable owned temporary-cache directory; cache policy is unchanged.',
                         'Fresh image verification is requested for journey and background checks, separately from warm timing.']}
    def execute(name, poll, scenario, count, verify):
        command = [sys.executable, str(HERE / 'scenarios.py'), '--binary', str(binary), '--cases', str(case_path),
                   '--directory', str(directory / name), '--scenario', scenario, '--count', str(count),
                   '--quality', 'quality', '--mode', 'off', '--trials', '1', '--repeats', '5', '--background',
                   '--max-memory-mb', '4096', '--timeout', '120', '--step-timeout', '30']
        if verify:
            command.extend(['--compare-images', str(ordinary)])
        env = {key: value for key, value in os.environ.items() if not key.startswith('WIDE_')}
        env['WIDE_POLL_US'] = str(poll)
        row = {'name': name, 'poll_us': poll, 'scenario': scenario, 'count': count,
               'command': command, 'isolated_disposable_disk_cache': True, 'passed': False}
        result['rows'].append(row)
        # The ordinary decoded cache lives under TMPDIR, independently of HOME.
        # Unique diagnostic namespaces otherwise accumulate across these trials.
        with tempfile.TemporaryDirectory(prefix=name + '-', dir=directory) as temporary:
            env['TMPDIR'] = temporary
            with (directory / (name + '.log')).open('x') as log:
                child = subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT)
        row['returncode'] = child.returncode
        report = directory / name / 'regression.json'
        if report.exists():
            data = json.loads(report.read_text())
            row.update(passed=child.returncode == 0 and data['passed'], timing_valid=data['timing_valid'],
                       workload_key=data['workload_key'], group_elapsed_ms=data['group_elapsed_ms'])
            peers = [prior for prior in result['rows'][:-1]
                     if prior['scenario'] == scenario and prior['count'] == count and prior.get('passed')]
            if any(prior['workload_key'] != row['workload_key'] for prior in peers):
                row.update(timing_valid=False, timing_failure='native workload differs from its paired control')
        (directory / 'summary.json').write_text(json.dumps(result, indent=2) + '\n')
        print(f'{name}: {"passed" if row["passed"] else "FAILED"}', flush=True)
        if not row['passed']:
            raise RuntimeError(f'{name}: native scenario failed; remaining GUI runs stopped for review')
    try:
        for index in range(args.trials):
            for poll in ((1000, 100) if index % 2 == 0 else (100, 1000)):
                execute(f'warm-{args.paired_count}-{index:02}-{poll}', poll, 'warm-edit', args.paired_count, False)
        for poll in (100, 1000):
            execute(f'warm-eight-{poll}', poll, 'warm-edit', 8, False)
            execute(f'journey-{poll}', poll, 'selection-journey', 2, True)
            execute(f'background-{poll}', poll, 'background-edit', 2, True)
        result['passed'] = all(row['passed'] for row in result['rows'])
    except (OSError, ValueError, KeyError, RuntimeError) as error:
        result['failure'] = str(error)
        raise
    finally:
        result['completed_utc'] = datetime.now(timezone.utc).isoformat()
        (directory / 'summary.json').write_text(json.dumps(result, indent=2) + '\n')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--build-provenance', type=Path, required=True)
    parser.add_argument('--ordinary-binary', type=Path, required=True)
    parser.add_argument('--source-dir', type=Path, required=True)
    parser.add_argument('--directory', type=Path, required=True)
    parser.add_argument('--trials', type=int, default=3)
    parser.add_argument('--paired-count', type=int, choices=[2, 8], default=2)
    args = parser.parse_args()
    try:
        result = run(args)
    except (OSError, ValueError, KeyError, RuntimeError) as error:
        parser.exit(1, str(error) + '\n')
    if not result['passed']:
        raise SystemExit(1)


if __name__ == '__main__':
    main()

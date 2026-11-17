"""Run the bounded wide-rendering investigation serially and preserve each independent experiment's evidence."""

import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
import subprocess
import sys
import time

from file_hash import digest
from wide import build_provenance

HERE = Path(__file__).resolve().parent


def run(args):
    binary = args.binary.resolve(strict=True)
    ordinary = args.ordinary_binary.resolve(strict=True)
    provenance = args.build_provenance.resolve(strict=True)
    build_provenance(binary, provenance)
    directory = args.directory.resolve()
    directory.mkdir(parents=True, exist_ok=False)
    (directory / 'wide_suite.py').write_bytes(Path(__file__).read_bytes())
    result = {'kind': 'wide-rendering-investigation', 'probe_binary_sha256': digest(binary),
              'ordinary_binary_sha256': digest(ordinary), 'supplement': args.supplement,
              'stages': [], 'passed': False,
              'scope': 'Engine experiment. Passing denotes correctness; inspect clean timing counts separately.'}
    def execute(name, script, options):
        command = [sys.executable, str(HERE / script), *options, '--directory', str(directory / name)]
        row = {'name': name, 'command': command, 'started_utc': datetime.now(timezone.utc).isoformat(), 'passed': False}
        result['stages'].append(row)
        started = time.monotonic()
        with (directory / (name + '.log')).open('x') as log:
            child = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT)
        row.update(returncode=child.returncode, wall_seconds=time.monotonic() - started)
        summary = directory / name / 'summary.json'
        if summary.exists():
            data = json.loads(summary.read_text())
            row.update(passed=child.returncode == 0 and data['passed'], summary=str(summary))
            if data.get('failure'):
                row['failure'] = data['failure']
        (directory / 'summary.json').write_text(json.dumps(result, indent=2) + '\n')
        print(f'{name}: {"passed" if row["passed"] else "FAILED"}; {row["wall_seconds"]:.1f} seconds', flush=True)
        return row['passed']
    common = ['--binary', str(binary), '--build-provenance', str(provenance),
              '--source-dir', str(args.source_dir.resolve()), '--trials', str(args.trials)]
    if args.allow_contended_observations:
        common.append('--allow-contended-observations')
    def replay(name, report):
        return execute(name, 'wide_replay.py', ['--binary', str(ordinary), '--report', str(report)])
    try:
        if not execute('smoke', 'wide.py', [*common, '--counts', '2', '--modes', 'serial:1,grouped:2', '--trials', '1']):
            raise ValueError('probe smoke gate failed')
        for edit in ('exposure', 'noise'):
            if not replay(f'smoke-replay-{edit}', directory / 'smoke' / f'{edit}-2-control/report.json'):
                raise ValueError('probe differs from independently frozen ordinary rendering')
        experiments = [
            ('warm', []),
            ('cold', ['--counts', '8', '--cold-graph']),
            ('depth2', ['--counts', '8', '--modes', 'serial:1,grouped:1', '--steps-per-photo', '2']),
            ('depth4', ['--counts', '8', '--modes', 'serial:1,grouped:1', '--steps-per-photo', '4']),
            ('cold-depth2', ['--counts', '8', '--modes', 'serial:1,grouped:1', '--steps-per-photo', '2', '--cold-graph']),
            ('poll100', ['--counts', '8', '--modes', 'serial:1,grouped:2', '--poll-us', '100']),
            ('poll0', ['--counts', '8', '--modes', 'serial:1,grouped:2', '--poll-us', '0']),
            ('scale4', ['--counts', '2,8', '--scale-divisor', '4']),
            ('scale2', ['--counts', '2', '--scale-divisor', '2']),
            ('mixed', ['--counts', '8', '--edits', 'exposure', '--manifest', str(HERE / 'wide_mixed_inputs.json')]),
        ]
        if args.supplement:
            experiments = [
                ('matched-depths', ['--counts', '8', '--compare-depths']),
                ('matched-depths-cold', ['--counts', '8', '--compare-depths', '--cold-graph']),
                ('deferred-finish', ['--counts', '8', '--defer-finished']),
                ('matched-polls', ['--counts', '8', '--compare-polls']),
                ('matched-depths-scale4', ['--counts', '2', '--compare-depths', '--scale-divisor', '4']),
            ]
        for name, options in experiments:
            execute(name, 'wide.py', [*common, *options])
        # Construct the stress contract explicitly rather than forwarding timing-only flags.
        stress = ['--binary', str(binary), '--build-provenance', str(provenance),
                  '--source-dir', str(args.source_dir.resolve())]
        if args.allow_contended_observations:
            stress.append('--allow-contended-observations')
        if not args.supplement:
            execute('stress', 'wide_stress.py', stress)
        replays = [
            ('replay-warm-noise', directory / 'warm/noise-8-control/report.json'),
            ('replay-mixed', directory / 'mixed/exposure-8-control/report.json'),
            ('replay-scale2', directory / 'scale2/exposure-2-control/report.json'),
            ('replay-scale4', directory / 'scale4/noise-2-control/report.json'),
            ('replay-cold', directory / 'cold/noise-8-control/report.json'),
        ]
        if args.supplement:
            replays = [
                ('replay-depths-noise', directory / 'matched-depths/noise-8-control/report.json'),
                ('replay-depths-cold', directory / 'matched-depths-cold/exposure-8-control/report.json'),
                ('replay-depths-scale4', directory / 'matched-depths-scale4/noise-2-control/report.json'),
            ]
        for name, report in replays:
            if report.exists():
                replay(name, report)
        result['passed'] = all(row['passed'] for row in result['stages'])
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
    parser.add_argument('--trials', type=int, default=7)
    parser.add_argument('--allow-contended-observations', action='store_true')
    parser.add_argument('--supplement', action='store_true', help='run directly rotated depth/poll and deferred-finish controls')
    args = parser.parse_args()
    try:
        result = run(args)
    except (OSError, ValueError, KeyError, RuntimeError) as error:
        parser.exit(1, str(error) + '\n')
    if not result['passed']:
        raise SystemExit(1)


if __name__ == '__main__':
    main()

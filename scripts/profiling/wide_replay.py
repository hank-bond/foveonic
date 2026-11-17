"""Compare probe outputs with an independently frozen ordinary renderer using exact snapshots and geometry."""

import argparse
import json
from pathlib import Path
import subprocess

from file_hash import digest
from image_replay import events
from process_memory import run_captured
from results import IMAGE_GEOMETRY_FIELDS, compare_images
from watch_workloads import running_workloads


def run(binary, report_path, directory, max_memory_mib=4096):
    binary, report_path = binary.resolve(strict=True), report_path.resolve(strict=True)
    report = json.loads(report_path.read_text())
    count = len(report['sources'])
    if not count or any(len(report[field]) != count for field in
                        ('source_sha256', 'pixels_sha256', 'geometry', 'final_settings_snapshots', 'effective_working_scale_divisors')):
        raise ValueError('probe output identities are incomplete')
    directory = directory.resolve()
    directory.mkdir(parents=True, exist_ok=False)
    (directory / 'wide_replay.py').write_bytes(Path(__file__).read_bytes())
    result = {'kind': 'wide-probe-independent-replay', 'binary_sha256': digest(binary),
              'probe_report_sha256': digest(report_path), 'passed': False, 'rows': [],
              'timing_validity': 'correctness only; concurrent workloads permitted and recorded'}
    try:
        for index, source in enumerate(report['sources']):
            row = {'index': index, 'source': source, 'passed': False,
                   'concurrent_workloads': running_workloads()}
            result['rows'].append(row)
            if digest(Path(source)) != report['source_sha256'][index]:
                raise ValueError('source differs from probe input')
            retained = report_path.parent / f'{index}.f32'
            if digest(retained) != report['pixels_sha256'][index]:
                raise ValueError('retained probe pixels differ from the recorded output hash')
            snapshot = report['final_settings_snapshots'][index]
            snapshot_path = directory / f'{index}.settings.json'
            snapshot_path.write_text(json.dumps(snapshot, indent=2) + '\n')
            samples = directory / f'{index}-samples'
            command = [str(binary), source, str(directory / f'{index}-encoded'), '--warm', '0',
                       '--settings-snapshot', str(snapshot_path), '--goldens', str(samples)]
            child = run_captured(command, limit_bytes=max_memory_mib * 1024 * 1024,
                                 samples=directory / f'{index}.memory.jsonl')
            if child.returncode:
                raise ValueError(f'ordinary renderer exited {child.returncode}; inspect {index}.memory.stderr')
            observed = events(child.stdout)
            if (observed['session']['settings'] != snapshot['settings']
                    or observed['session']['processing_quality'] != snapshot['processing_quality']
                    or observed['processing']['effective_scale_divisor'] != report['effective_working_scale_divisors'][index]):
                raise ValueError('ordinary renderer used different settings, policy or working scale')
            geometry = {field: observed['validation'][field] for field in IMAGE_GEOMETRY_FIELDS}
            expected_geometry = report['geometry'][index]['validation']
            row['comparison'] = compare_images(source, 'probe-replay', retained,
                                                expected_geometry, samples / (Path(source).stem + '.f32'), geometry)
            row['passed'] = row['comparison']['outcome'] == 'passed'
            if not row['passed']:
                raise ValueError(f'photo {index}: probe differs from ordinary rendering')
            print(f'{index}: exact ordinary-render replay', flush=True)
        result['passed'] = True
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.TimeoutExpired) as error:
        result['failure'] = str(error)
        if result['rows'] and not result['rows'][-1]['passed']:
            result['rows'][-1]['failure'] = str(error)
        raise
    finally:
        (directory / 'summary.json').write_text(json.dumps(result, indent=2) + '\n')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True, help='independent frozen profile_pipeline')
    parser.add_argument('--report', type=Path, required=True, help='probe control report with retained pixels')
    parser.add_argument('--directory', type=Path, required=True)
    parser.add_argument('--max-memory-mib', type=int, default=4096)
    args = parser.parse_args()
    try:
        run(args.binary, args.report, args.directory, args.max_memory_mib)
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.TimeoutExpired) as error:
        parser.exit(1, str(error) + '\n')


if __name__ == '__main__':
    main()

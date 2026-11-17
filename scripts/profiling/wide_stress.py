"""Check wide-probe memory observations and independent settlement after submitted-work interruption."""

import argparse
import json
from pathlib import Path
import subprocess

import wide
from file_hash import digest


def check_interruption(report, directory, expected, require_unfinished_peer=True):
    """Derive unchanged-peer outcomes independently of the scheduler's result labels."""
    injection = report['injection']
    count = len(expected)
    if not report['injection_peer_submitted'] or report['injected_ms'] is None or report['group_fence_ms'] is None:
        raise ValueError('interruption did not observe peer submission and a subsequent drained window')
    live_peers = max((peers for index, peers in report['probe']['unfinished_submitted_peers_at_interruption']
                      if index == 0), default=0)
    if require_unfinished_peer and not live_peers:
        raise ValueError('interruption had no independently advanced unfinished peer')
    if report['group_fence_ms'] < report['injected_ms']:
        raise ValueError('interruption drain precedes injection')
    completed = [index for index, _ in report['completion_ms']]
    wanted = list(range(count)) if injection == 'replace' else list(range(1, count))
    if sorted(completed) != wanted:
        raise ValueError('interruption delivered an obsolete photo or failed to settle a peer')
    for index in range(1, count):
        pixels = directory / f'{index}.f32'
        actual = {'sha256': digest(pixels), 'bytes': pixels.stat().st_size,
                  'geometry': report['geometry'][index], 'settings': report['final_settings_snapshots'][index]}
        wide.check_images([expected[index]], [actual])
    if injection == 'replace':
        if report['replacement_exact'] is not True:
            raise ValueError('replacement differs from fresh serial verification')
        markers = [report[field] for field in ('injected_ms', 'group_fence_ms',
                                               'replacement_step_observed_ms', 'replacement_ms')]
        if any(value is None for value in markers) or markers != sorted(markers):
            raise ValueError('replacement lifecycle observations are missing or out of order')
        later_peers = [time for index, time in report['completion_ms'] if index != 0 and time > markers[0]]
        if not later_peers or markers[-1] >= max(later_peers):
            raise ValueError('replacement only completed after all remaining peers')
    else:
        if (directory / '0.f32').exists():
            raise ValueError('obsolete photo retained a delivered output')
        expected_error = 'render superseded' if injection == 'cancel' else 'injected observer failure'
        if report['outcomes'][0] != expected_error:
            raise ValueError('unexpected interrupted-photo outcome')
    return {'peer_outputs_exact': count - 1, 'replacement_exact': report['replacement_exact'],
            'unfinished_advanced_peers_at_injection': live_peers,
            'drain_after_injection_ms': report['group_fence_ms'] - report['injected_ms'],
            'replacement_after_injection_ms': (None if injection != 'replace' else
                                               report['replacement_ms'] - report['injected_ms'])}


def run(args):
    binary = args.binary.resolve(strict=True)
    injections = args.injections.split(',')
    if (binary.name != 'profile_wide' or args.repeats < 1 or args.max_memory_mib <= 0
            or len(set(injections)) != len(injections)
            or any(injection not in ('cancel', 'failure', 'replace') for injection in injections)):
        raise ValueError('use a frozen profile_wide binary and positive repeats')
    provenance = wide.build_provenance(binary, args.build_provenance)
    sources = wide.inputs(args.source_dir, args.manifest)
    if not 2 <= len(sources) <= 8:
        raise ValueError('stress inputs must contain two through eight photographs')
    directory = args.directory.resolve()
    directory.mkdir(parents=True, exist_ok=False)
    (directory / 'wide_stress.py').write_bytes(Path(__file__).read_bytes())
    (directory / 'wide.py').write_bytes(Path(wide.__file__).read_bytes())
    result = {'binary_sha256': digest(binary), 'build_provenance': provenance,
              'inputs': sources, 'cold_graph': args.cold_graph,
              'defer_finished': args.defer_finished,
              'require_unfinished_peer': args.require_unfinished_peer, 'rows': [], 'passed': False,
              'limits': ['Engine-only experiment; no coordinator, UI handoff or total memory admission.',
                         'Memory observations are separate from timing trials.',
                         'The probe prioritizes replacement advancement. Already encoded peer work may finish earlier.',
                         'The completion check requires replacement before the last remaining peer; it does not prove exclusive service.',
                         'Injection follows a submitted step; ordinary waits have completed in interleaved mode.',
                         'Replacement-step observation is after step return, not the time of queue submission.']}
    destination = directory / 'summary.json'
    def execute(name, edit, mode, width, memory=False, injection='', window=64):
        row = {'name': name, 'edit': edit, 'mode': mode, 'width': width,
               'memory_observation': memory, 'injection': injection, 'window_mib': window, 'passed': False}
        result['rows'].append(row)
        report, metadata = wide.trial(binary, sources, directory / name, edit, mode, width,
                                      args.max_memory_mib * 1024 * 1024, args.allow_contended_observations,
                                      memory=memory, injection=injection, window_mib=window, cold_graph=args.cold_graph,
                                      flush_on_finish=False if args.defer_finished and mode == 'grouped' else None)
        row.update(metadata)
        row['logical_gpu_boundary_peak_bytes'] = report['logical_gpu_boundary_peak_bytes']
        row['probe'] = report['probe']
        return row, report
    try:
        controls = {}
        for edit in (('exposure',) if args.skip_memory else ('exposure', 'noise')):
            row, report = execute(f'{edit}-control', edit, 'serial', 1)
            controls[edit] = wide.image_identity(directory / row['name'], report)
            row['passed'] = True
            if args.skip_memory:
                continue
            for mode, width in (('interleaved', 1), ('grouped', 1), ('grouped', 2), ('grouped', 4)):
                name = f'{edit}-memory-{mode}-{width}'
                row, report = execute(name, edit, mode, width, memory=True)
                wide.check_images(controls[edit], wide.image_identity(directory / name, report))
                row['passed'] = True
                destination.write_text(json.dumps(result, indent=2) + '\n')
        for repeat in range(args.repeats):
            for injection in injections:
                for mode, width, window in (('interleaved', 2, 64), ('grouped', 2, 64),
                                             ('grouped', 4, 64), ('grouped', 2, 1)):
                    name = f'{injection}-{repeat:02}-{mode}-{width}-window{window}'
                    row, report = execute(name, 'exposure', mode, width, injection=injection, window=window)
                    row.update(check_interruption(report, directory / name, controls['exposure'], args.require_unfinished_peer))
                    row['passed'] = True
                    destination.write_text(json.dumps(result, indent=2) + '\n')
                    print(f'{name}: peers exact; drain {row["drain_after_injection_ms"]:.3f} ms; '
                          f'replacement {row["replacement_after_injection_ms"]}', flush=True)
        result['passed'] = True
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.TimeoutExpired) as error:
        result['failure'] = str(error)
        if result['rows'] and not result['rows'][-1]['passed']:
            result['rows'][-1]['failure'] = str(error)
        raise
    finally:
        destination.write_text(json.dumps(result, indent=2) + '\n')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--build-provenance', type=Path, required=True)
    parser.add_argument('--source-dir', type=Path, required=True)
    parser.add_argument('--directory', type=Path, required=True)
    parser.add_argument('--manifest', type=Path, default=Path(__file__).with_name('wide_inputs.json'))
    parser.add_argument('--repeats', type=int, default=3)
    parser.add_argument('--max-memory-mib', type=int, default=4096)
    parser.add_argument('--allow-contended-observations', action='store_true')
    parser.add_argument('--cold-graph', action='store_true')
    parser.add_argument('--skip-memory', action='store_true', help='run interruption evidence without separate memory samples')
    parser.add_argument('--injections', default='cancel,failure,replace')
    parser.add_argument('--require-unfinished-peer', action='store_true', help='reject injection without a live advanced peer cursor')
    parser.add_argument('--defer-finished', action='store_true', help='group normal photo finishes until the round drains')
    args = parser.parse_args()
    try:
        run(args)
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.TimeoutExpired) as error:
        parser.exit(1, str(error) + '\n')


if __name__ == '__main__':
    main()

"""Compare opt-in wide-rendering mechanisms with pinned inputs and separately classified timing evidence."""

import argparse
import json
import os
from pathlib import Path
import resource
import subprocess
import sys

from file_hash import digest
from native import source_tree_hash, workload_monitor
from process_memory import run_captured
from trace_analysis import distribution
from watch_workloads import concurrent_workloads, require_quiet, running_workloads

MODES = (('serial', 1), ('interleaved', 2), ('grouped', 2), ('grouped', 4))


def parse_modes(value):
    modes = tuple((mode, int(width)) for mode, width in (item.split(':') for item in value.split(',')))
    if (not modes or len(set(modes)) != len(modes)
            or any(mode not in ('serial', 'interleaved', 'grouped') or not 1 <= width <= 4
                   or (mode == 'serial' and width != 1) for mode, width in modes)):
        raise ValueError('use unique serial:1, interleaved:WIDTH or grouped:WIDTH modes with widths 1..4')
    return modes


def inputs(source_dir, manifest):
    """Resolve only pinned basenames, preserving the declared photograph order."""
    rows = json.loads(manifest.read_text())['files']
    result = []
    for row in rows:
        name = row['file']
        if Path(name).name != name or name in ('', '.', '..'):
            raise ValueError('wide input names must be basenames')
        source = (source_dir / name).resolve(strict=True)
        actual = digest(source)
        if actual != row['sha256']:
            raise ValueError(f'{name}: pinned input hash differs')
        result.append({'source': str(source), 'sha256': actual})
    if len({row['source'] for row in result}) != len(result):
        raise ValueError('wide inputs must name distinct originals')
    return result


def image_identity(directory, report):
    """Read full output samples outside timing, paired with their effective recipes and geometry."""
    images = []
    for index, geometry in enumerate(report['geometry']):
        pixels = directory / f'{index}.f32'
        checksum = digest(pixels)
        if checksum != report['pixels_sha256'][index]:
            raise ValueError('retained pixels differ from the probe report hash')
        images.append({'sha256': checksum, 'bytes': pixels.stat().st_size,
                       'geometry': geometry, 'settings': report['final_settings_snapshots'][index]})
    if len(images) != len(report['sources']):
        raise ValueError('output count differs from requested photographs')
    return images


def check_images(expected, actual):
    if len(expected) != len(actual):
        raise ValueError('wide output count differs from serial control')
    for index, (left, right) in enumerate(zip(expected, actual)):
        if left != right:
            raise ValueError(f'photo {index}: samples, geometry or effective settings differ from serial control')


def check_workload(report, sources, edit, mode, width, poll_us, steps_per_photo=1, cold_graph=False):
    if (report['mode'] != mode or report['width'] != width or report['edit'] != edit
            or report['completion_poll_interval_us'] != poll_us or report['profile_mode'] != 'off'
            or report['steps_per_photo'] != steps_per_photo or report['cold_graph'] != cold_graph):
        raise ValueError('renderer reported a different workload or profiling mode')
    if ([str(Path(path).resolve()) for path in report['sources']] != [row['source'] for row in sources]
            or report['source_sha256'] != [row['sha256'] for row in sources]):
        raise ValueError('renderer source identities differ from pinned inputs')


def build_provenance(binary, path):
    record = json.loads(path.read_text())
    if record['binary_sha256'] != digest(binary):
        raise ValueError('frozen binary differs from its build provenance')
    for field in ('git_revision', 'source_tree_sha256', 'build_command', 'rustc_vv'):
        if not record.get(field):
            raise ValueError(f'build provenance is missing {field}')
    return record


def summarize(rows):
    """Keep warmups, contested observations and clean timing distributions distinguishable."""
    groups = {}
    for row in rows:
        if row['warmup'] or row.get('control') or not row.get('passed'):
            continue
        key = (row['edit'], row['count'], row['mode'], row['width'], row['steps_per_photo'], row['cold_graph'], row['poll_us'])
        groups.setdefault(key, []).append(row)
    result = []
    for (edit, count, mode, width, steps, cold, poll), samples in groups.items():
        clean = [row for row in samples if row['contention']['uncontended']]
        entry = {'edit': edit, 'count': count, 'mode': mode, 'width': width,
                 'steps_per_photo': steps, 'cold_graph': cold,
                 'poll_us': poll,
                 'trials': len(samples), 'uncontended_trials': len(clean)}
        for label, values in [('all_observations', samples), ('uncontended', clean)]:
            entry[label] = {field: distribution([row[field] for row in values])
                            for field in ('group_ms', 'first_ms', 'active_ms', 'physical_peak_bytes', 'process_cpu_ms')}
        result.append(entry)
    return result


def trial(binary, sources, directory, edit, mode, width, limit_bytes, allow_contended, memory=False, injection='', poll_us=1000, window_mib=64, steps_per_photo=1, cold_graph=False, scale_divisor=None, flush_on_finish=None):
    before = running_workloads()
    if not allow_contended:
        require_quiet()
    monitor = directory.parent / (directory.name + '.workloads.jsonl')
    memory_samples = directory.parent / (directory.name + '.memory.jsonl')
    env = {key: value for key, value in os.environ.items() if not key.startswith('WIDE_')}
    steps_per_photo = steps_per_photo if mode == 'grouped' else 1
    env.update(WIDE_EDIT=edit, WIDE_PROFILE='off', WIDE_POLL_US=str(poll_us), WIDE_WINDOW_MIB=str(window_mib),
               WIDE_STEPS_PER_PHOTO=str(steps_per_photo))
    if cold_graph:
        env['WIDE_COLD_GRAPH'] = '1'
    if scale_divisor is not None:
        env['WIDE_SCALE_DIVISOR'] = str(scale_divisor)
    if flush_on_finish is not None:
        env['WIDE_FLUSH_ON_FINISH'] = '1' if flush_on_finish else '0'
    if memory:
        env['WIDE_MEMORY'] = '1'
    if injection:
        env['WIDE_INJECT'] = injection
    command = [str(binary), str(directory), mode, str(width), *[row['source'] for row in sources]]
    pids = []
    with workload_monitor(monitor):
        usage_before = resource.getrusage(resource.RUSAGE_CHILDREN)
        child = run_captured(command, limit_bytes=limit_bytes, samples=memory_samples,
                             env=env, on_started=pids.append)
        usage_after = resource.getrusage(resource.RUSAGE_CHILDREN)
    if child.returncode:
        raise RuntimeError(f'{directory.name}: renderer exited {child.returncode}; inspect {memory_samples.with_suffix(".stderr")}')
    report = json.loads((directory / 'report.json').read_text())
    check_workload(report, sources, edit, mode, width, poll_us, steps_per_photo, cold_graph)
    if (report['window_bytes'] != window_mib * 1024 * 1024 or report['injection'] != injection
            or report['scale_override'] != scale_divisor):
        raise ValueError('renderer reported a different completion window or injection')
    if flush_on_finish is not None and report['flush_on_finish'] != flush_on_finish:
        raise ValueError('renderer reported a different completion-flush policy')
    times = report['completion_ms']
    if not injection and sorted(index for index, _ in times) != list(range(len(sources))):
        raise ValueError('renderer must complete each requested photograph exactly once')
    contention = concurrent_workloads(monitor, pids)
    # A short competing process could end before the monitor's next sample.
    if before:
        contention['uncontended'] = False
        contention['before'] = before
    footprint = [json.loads(line)['physical_footprint_bytes'] for line in memory_samples.read_text().splitlines()]
    return report, {'command': command, 'contention': contention,
                    'admitted_width': report['admitted_width'],
                    'steps_per_photo': steps_per_photo, 'cold_graph': cold_graph,
                    'effective_working_scale_divisors': report['effective_working_scale_divisors'],
                    'poll_us': poll_us,
                    'process_cpu_ms': 1000 * (usage_after.ru_utime + usage_after.ru_stime
                                              - usage_before.ru_utime - usage_before.ru_stime),
                    'physical_peak_bytes': max(footprint, default=0),
                    'group_ms': report['elapsed_ms'],
                    'first_ms': min((time for _, time in times), default=None),
                    'active_ms': next((time for index, time in times if index == 0), None)}


def run(args):
    binary = args.binary.resolve(strict=True)
    if binary.name != 'profile_wide':
        raise ValueError('freeze the executable with basename profile_wide so workload monitoring recognizes it')
    provenance = build_provenance(binary, args.build_provenance)
    directory = args.directory.resolve()
    resolved = inputs(args.source_dir, args.manifest)
    counts = [int(value) for value in args.counts.split(',')]
    edits = args.edits.split(',')
    modes = parse_modes(args.modes)
    depths = (('serial', 1, 1), ('grouped', 1, 1), ('grouped', 1, 2), ('grouped', 1, 4),
                 ('grouped', 2, 1), ('grouped', 4, 1)) if args.compare_depths else tuple(
                     (mode, width, args.steps_per_photo if mode == 'grouped' else 1) for mode, width in modes)
    schedules = tuple((*entry, args.poll_us) for entry in depths)
    if args.compare_polls:
        schedules = tuple((mode, width, 1, poll) for mode, width in (('serial', 1), ('grouped', 2))
                          for poll in (1000, 100, 0))
    if (args.trials < 1 or args.max_memory_mib <= 0 or not 0 <= args.poll_us <= 10000
            or not counts or len(set(counts)) != len(counts)
            or args.steps_per_photo not in (1, 2, 4)
            or (args.scale_divisor is not None and (args.scale_divisor < 1
                                                   or args.scale_divisor & (args.scale_divisor - 1)))
            or any(count < 1 or count > min(8, len(resolved)) for count in counts)):
        raise ValueError('positive trials/memory and supported photograph counts required')
    if len(set(edits)) != len(edits) or any(edit not in ('exposure', 'noise') for edit in edits):
        raise ValueError('edits must be exposure or noise')
    directory.mkdir(parents=True, exist_ok=False)
    (directory / 'wide-driver.py').write_bytes(Path(__file__).read_bytes())
    result = {'schema_version': 1, 'binary': str(binary), 'binary_sha256': digest(binary),
              'build_provenance': provenance, 'build_provenance_sha256': digest(args.build_provenance),
              'runner_source_tree_sha256': source_tree_hash(Path(__file__).resolve().parents[2]),
              'runner_sha256': digest(Path(__file__)), 'input_manifest_sha256': digest(args.manifest),
              'inputs': resolved, 'poll_us': args.poll_us, 'steps_per_photo': args.steps_per_photo,
              'cold_graph': args.cold_graph, 'scale_divisor': args.scale_divisor,
              'modes': modes, 'schedules': schedules, 'defer_finished': args.defer_finished,
              'trials': [], 'passed': False,
              'limits': ['Engine-only reduced sessions; no GUI latency or memory-budget admission claim.',
                         'Preparation outside warm-edit timing; recipe preparation and display completion inside.',
                         'Cold-graph mode skips the warm downstream render and its textures; source opening and sealing stay outside timing.',
                         'Physical footprint includes setup and verification; logical boundary samples omit within-node peaks.',
                         'Process CPU includes preparation, warmup and readback, and is not the timed render interval.',
                         'Contested timings are observational and excluded from uncontended distributions.']}
    destination = directory / 'summary.json'
    try:
        for edit in edits:
            for count in counts:
                selected = resolved[:count]
                control_dir = directory / f'{edit}-{count}-control'
                row = {'edit': edit, 'count': count, 'mode': 'serial', 'width': 1,
                       'control': True, 'warmup': True, 'passed': False}
                result['trials'].append(row)
                control, metadata = trial(binary, selected, control_dir, edit, 'serial', 1,
                                          args.max_memory_mib * 1024 * 1024, args.allow_contended_observations,
                                          poll_us=args.poll_us, cold_graph=args.cold_graph, scale_divisor=args.scale_divisor)
                expected = image_identity(control_dir, control)
                row.update(passed=True, **metadata)
                for iteration in range(args.trials + 1):
                    offset = iteration % len(schedules)
                    for mode, width, steps, poll in schedules[offset:] + schedules[:offset]:
                        name = f'{edit}-{count}-{iteration:02}-{mode}-{width}'
                        if args.compare_depths:
                            name += f'-depth{steps}'
                        if args.compare_polls:
                            name += f'-poll{poll}'
                        target = directory / name
                        row = {'edit': edit, 'count': count, 'mode': mode, 'width': width,
                               'iteration': iteration, 'warmup': iteration == 0, 'passed': False}
                        result['trials'].append(row)
                        report, metadata = trial(binary, selected, target, edit, mode, width,
                                                 args.max_memory_mib * 1024 * 1024, args.allow_contended_observations,
                                                 poll_us=poll, steps_per_photo=steps,
                                                 cold_graph=args.cold_graph, scale_divisor=args.scale_divisor,
                                                 flush_on_finish=False if args.defer_finished and mode == 'grouped' else None)
                        row.update(metadata)
                        actual = image_identity(target, report)
                        check_images(expected, actual)
                        row.update(passed=True, images=actual)
                        if not args.keep_pixels:
                            for index in range(len(actual)):
                                (target / f'{index}.f32').unlink()
                        result['summary'] = summarize(result['trials'])
                        destination.write_text(json.dumps(result, indent=2) + '\n')
                        print(f'{name}: exact; first {row["first_ms"]:.3f}, group {row["group_ms"]:.3f} ms; '
                              f'{"uncontended" if row["contention"]["uncontended"] else "contested"}', flush=True)
        result['passed'] = True
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.TimeoutExpired) as error:
        result['failure'] = str(error)
        if result['trials'] and not result['trials'][-1]['passed']:
            result['trials'][-1]['failure'] = str(error)
        raise
    finally:
        result['summary'] = summarize(result['trials'])
        destination.write_text(json.dumps(result, indent=2) + '\n')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--source-dir', type=Path, required=True)
    parser.add_argument('--directory', type=Path, required=True)
    parser.add_argument('--build-provenance', type=Path, required=True, help='record from the frozen probe build')
    parser.add_argument('--manifest', type=Path, default=Path(__file__).with_name('wide_inputs.json'))
    parser.add_argument('--trials', type=int, default=7)
    parser.add_argument('--counts', default='1,2,8')
    parser.add_argument('--edits', default='exposure,noise')
    parser.add_argument('--max-memory-mib', type=int, default=4096)
    parser.add_argument('--poll-us', type=int, default=1000, help='experimental completion polling interval; zero spins')
    parser.add_argument('--modes', default=','.join(f'{mode}:{width}' for mode, width in MODES))
    parser.add_argument('--steps-per-photo', type=int, default=1, help='required operations per photo in grouped mode')
    parser.add_argument('--cold-graph', action='store_true', help='render downstream graph without its initial warm render')
    parser.add_argument('--scale-divisor', type=int, help='explicit power-of-two working scale instead of camera Thumb')
    comparison = parser.add_mutually_exclusive_group()
    comparison.add_argument('--compare-depths', action='store_true', help='rotate serial, grouped depth1/2/4 and width2/4 directly')
    comparison.add_argument('--compare-polls', action='store_true', help='rotate serial/grouped at 1000/100/0 microseconds directly')
    parser.add_argument('--defer-finished', action='store_true', help='group normal photo finishes until the round drains')
    parser.add_argument('--allow-contended-observations', action='store_true')
    parser.add_argument('--keep-pixels', action='store_true')
    args = parser.parse_args()
    try:
        run(args)
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.TimeoutExpired) as error:
        parser.exit(1, str(error) + '\n')


if __name__ == '__main__':
    main()

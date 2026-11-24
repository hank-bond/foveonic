"""Record a render run from any revision and compare two runs by case for timing and pixel changes."""

import argparse
import copy
import json
import math
import os
from pathlib import Path
import platform
import subprocess
import sys
import time

from file_hash import digest
from native import recording_environment, workload_monitor
from process_memory import run_captured
from render_matrix import expand_cases, render_arguments

ROOT = Path(__file__).resolve().parents[2]
RENDERER = Path('target/release/examples') / ('profile_pipeline.exe' if os.name == 'nt' else 'profile_pipeline')
# Peak memory is sampled, so smaller changes are not reported.
MEMORY_CHANGE_PERCENT = 10
# What a case asked for. Two runs compare a case when these agree.
REQUESTED = ('source_sha256', 'resolution', 'processing_quality', 'options', 'settings_snapshot')
from results import IMAGE_GEOMETRY_FIELDS, render_result
from trace_analysis import distribution
from watch_workloads import concurrent_workloads, require_quiet


def git(*arguments, cwd=ROOT):
    return subprocess.run(['git', *arguments], cwd=cwd, check=True, capture_output=True, text=True).stdout.strip()


def build(revision, *, root=ROOT, offline=False):
    """Build the renderer of `revision`, or of the working tree, and name the source it came from.

    A commit is exported to its own directory under `.local/runs/builds` and built once.
    """
    cargo = ['cargo', 'build', '--release', '--locked', '-p', 'foveonic', '--example', 'profile_pipeline',
             *(['--offline'] if offline else [])]
    if revision == 'worktree':
        source = root
        name = git('rev-parse', 'HEAD', cwd=root) + ('-dirty' if git('status', '--porcelain', cwd=root) else '')
    else:
        name = git('rev-parse', '--verify', revision + '^{commit}', cwd=root)
        source = root / '.local/runs/builds' / name
        if not (source / 'Cargo.toml').exists():
            source.mkdir(parents=True, exist_ok=True)
            archive = subprocess.run(['git', 'archive', name], cwd=root, check=True, capture_output=True).stdout
            subprocess.run(['tar', '-x', '-C', str(source)], input=archive, check=True)
    if revision == 'worktree' or not (source / RENDERER).exists():
        subprocess.run(cargo, cwd=source, check=True, stdout=sys.stderr)
    return source / RENDERER, name


def host_identity():
    return {'host': platform.node(), 'system': platform.system(),
            'release': platform.release(), 'machine': platform.machine(),
            'processor': platform.processor(), 'logical_cpus': os.cpu_count()}


def parse_run(stdout, case, warmups, samples):
    """Reject partial or mixed runs before selecting exactly the declared warm samples."""
    records = [json.loads(line) for line in stdout.splitlines() if line.strip()]
    if not records or any(row.get('foveonic_profile') != 1 or row.get('mode') != 'off'
                          or row.get('synchronized') is not False for row in records):
        raise ValueError('every renderer record must use unsynchronized profiling Off')
    grouped = {}
    for row in records:
        grouped.setdefault(row['event'], []).append(row['data'])
    if set(grouped) != {'session', 'source_load', 'processing', 'validation'}:
        raise ValueError('unexpected or missing renderer events')
    if any(len(grouped[name]) != 1 for name in ('session', 'source_load', 'validation')):
        raise ValueError('renderer session, source load and validation must each occur once')
    session, load, validation = (grouped[name][0] for name in ('session', 'source_load', 'validation'))
    processing = grouped['processing']
    if len(processing) != warmups + samples:
        raise ValueError('renderer returned the wrong number of fresh iterations')
    if (session.get('frontend') != 'completed_processing' or session.get('render_only') is not True
            or not session.get('adapter') or not isinstance(session.get('settings'), dict)
            or not session['settings'] or session.get('processing_quality') != case['processing_quality']):
        raise ValueError('renderer lacks its actual render-only settings, policy or adapter')
    if 'settings_snapshot' in case and session['settings'] != case['settings_snapshot']['settings']:
        raise ValueError('actual settings differ from the declared snapshot')
    scale = None
    for iteration, row in enumerate(processing):
        if (row.get('iteration') != iteration or row.get('cold') is not (iteration == 0)
                or row.get('execution') != 'fresh' or row.get('output') != 'final'
                or row.get('source') != case['source'] or row.get('settings') != session['settings']
                or row.get('processing_quality') != case['processing_quality']):
            raise ValueError('renderer iterations have inconsistent identities, settings or execution')
        current = row.get('effective_scale_divisor')
        if type(current) is not int or current <= 0 or (scale is not None and scale != current):
            raise ValueError('renderer iterations have missing or inconsistent effective grids')
        scale = current
        for name in ('completed_ms', 'render_call_ms'):
            value = row.get(name)
            if type(value) not in (int, float) or not math.isfinite(value) or value < 0:
                raise ValueError('renderer timing must be finite and nonnegative')
    if (session.get('source') != case['source'] or load.get('source') != case['source']
            or validation.get('source') != case['source'] or validation.get('outside_timing') is not True):
        raise ValueError('source or out-of-timing validation identity is missing')
    assertion = render_result('headless', 'fresh-processing', case['processing_quality'], [
        {'step': 'render', 'source': case['source'], 'family': session.get('family'),
         'scale_divisor': scale, 'processing_quality': session['processing_quality']}], True,
        resolution=case['resolution'])
    if assertion['outcome'] != 'passed' or session.get('scale_divisor') != scale:
        raise ValueError('actual processing policy or effective resolution fails the declared case')
    geometry = {key: validation[key] for key in IMAGE_GEOMETRY_FIELDS}
    if any(value is None for value in geometry.values()):
        raise ValueError('validation geometry must be complete')
    return {'adapter': session['adapter'], 'settings': session['settings'],
            'effective_scale_divisor': scale, 'geometry': geometry, 'source_load': load,
            'iterations': [{key: row[key] for key in ('iteration', 'cold', 'render_call_ms', 'completed_ms')}
                           for row in processing],
            'warm_completed_ms': [row['completed_ms'] for row in processing[warmups:]],
            'preview_assertions': assertion['assertions']}


def peak_memory(samples):
    """The renderer's largest sampled physical footprint, where the host can sample it."""
    if not samples.exists():
        return None
    values = [json.loads(line).get('physical_footprint_bytes') for line in samples.read_text().splitlines() if line.strip()]
    return max((value for value in values if value is not None), default=None)


def record(binary, cases, directory, *, build_source_revision=None, warmups=1, samples=5,
           max_memory_mb=5120, retain_floats=False):
    if type(warmups) is not int or warmups < 1 or type(samples) is not int or samples < 1:
        raise ValueError('warmups and samples must be positive integers')
    if type(max_memory_mb) is not int or max_memory_mb <= 0:
        raise ValueError('physical-footprint limit must be a positive integer')
    if build_source_revision is not None and (not isinstance(build_source_revision, str) or not build_source_revision.strip()):
        raise ValueError('declared executable source revision must be a nonempty string')
    binary = binary.resolve(strict=True)
    matrix, coverage = expand_cases(cases)
    if not matrix:
        raise ValueError('the matrix must contain at least one case')
    directory = directory.resolve()
    directory.mkdir(parents=True, exist_ok=False)
    home = directory / 'home'
    home.mkdir()
    report = {'schema_version': 1, 'kind': 'fresh-processing-off-benchmark', 'mode': 'off',
              'passed': False, 'timing_valid': False, 'hardware': host_identity(),
              'build': {'binary': str(binary), 'binary_sha256': digest(binary),
                        'source_revision': build_source_revision, 'source_revision_identity': 'caller-declared'},
              'measurement': {'execution': 'fresh', 'warmups': warmups, 'samples': samples,
                              'read_cache_state': 'uncontrolled; input hashing precedes launch',
                              'verification': 'first cold image read back outside timing',
                              'physical_memory_guard_limit_bytes': max_memory_mb * 1024 * 1024},
              'coverage': coverage, 'cases_requested': len(matrix), 'cases': [],
              'limits': ['Fresh graph repetitions measure completed processing, not retained edits or UI delivery.',
                         'Known-workload sampling cannot prove that all system GPU activity was absent.',
                         'Recorded output identities are observations. Equality requires a separate comparison.',
                         'Median and nearest-rank p95 describe these samples, not statistical significance.']}
    try:
        for case in matrix:
            name = case['id']
            row = {**case, 'passed': False, 'timing_valid': False}
            report['cases'].append(row)
            golden_dir = directory / 'floats' / name
            workload_path = directory / (name + '.workloads.jsonl')
            owned = []
            try:
                require_quiet()
                command = [str(binary), case['source'], str(directory / name), '--render-only',
                           '--warm', str(warmups + samples - 1), '--goldens', str(golden_dir),
                           *render_arguments(case, directory / (name + '.settings.json'))]
                with workload_monitor(workload_path):
                    result = run_captured(command, limit_bytes=max_memory_mb * 1024 * 1024,
                                          samples=directory / (name + '.memory.jsonl'),
                                          env=recording_environment(home, 'off'), on_started=owned.append)
                row['contention'] = concurrent_workloads(workload_path, owned)
                (directory / (name + '.jsonl')).write_text(result.stdout)
                (directory / (name + '.stderr')).write_text(result.stderr)
                if result.returncode:
                    raise ValueError(f'render exited with status {result.returncode}; see stderr')
                row.update(parse_run(result.stdout, case, warmups, samples))
                if digest(case['source']) != case['source_sha256'] or digest(binary) != report['build']['binary_sha256']:
                    raise ValueError('source or executable changed during the run')
                golden = golden_dir / (case['source_stem'] + '.f32')
                row['pixels_sha256'] = digest(golden)
                row['pixel_bytes'] = golden.stat().st_size
                if row['pixel_bytes'] == 0 or row['pixel_bytes'] % 4:
                    raise ValueError('validation did not produce complete floating samples')
                row['summary_ms'] = distribution(row['warm_completed_ms'])
                row['peak_memory_bytes'] = peak_memory(directory / (name + '.memory.jsonl'))
                row['passed'] = True
                row['timing_valid'] = row['contention']['checked'] and row['contention']['uncontended']
                if not retain_floats and row['timing_valid']:
                    golden.unlink()
                    golden_dir.rmdir()
                else:
                    row['float_artifact'] = str(golden)
            except (OSError, ValueError, KeyError, RuntimeError, subprocess.TimeoutExpired) as error:
                row['failure'] = str(error)
                break
            finally:
                (directory / 'report.json').write_text(json.dumps(report, indent=2, allow_nan=False) + '\n')
        report['passed'] = len(report['cases']) == len(matrix) and all(row['passed'] for row in report['cases'])
        report['timing_valid'] = report['passed'] and all(row['timing_valid'] for row in report['cases'])
    finally:
        (directory / 'report.json').write_text(json.dumps(report, indent=2, allow_nan=False) + '\n')
    return report


def checked_cases(report):
    if (report.get('schema_version') != 1 or report.get('kind') != 'fresh-processing-off-benchmark'
            or report.get('mode') != 'off' or report.get('passed') is not True
            or report.get('timing_valid') is not True):
        raise ValueError('comparison requires complete, uncontended fresh-processing Off reports')
    if (not report.get('hardware', {}).get('host') or not report['hardware'].get('system')
            or not report['hardware'].get('machine') or not report.get('build', {}).get('binary_sha256')):
        raise ValueError('report lacks hardware or executable identity')
    rows = report['cases']
    if not rows or len(rows) != report['cases_requested'] or len({row['id'] for row in rows}) != len(rows):
        raise ValueError('report has incomplete or duplicate cases')
    for row in rows:
        if (row.get('passed') is not True or row.get('timing_valid') is not True
                or row.get('contention', {}).get('checked') is not True
                or row['contention'].get('uncontended') is not True):
            raise ValueError('every case must have successful, uncontended timing')
        values = row['warm_completed_ms']
        if (len(values) != report['measurement']['samples'] or not values
                or any(type(value) not in (int, float) or not math.isfinite(value) or value < 0 for value in values)):
            raise ValueError('report has incomplete or invalid warm samples')
        if (not row.get('adapter') or not row.get('settings') or not row.get('source_sha256')
                or type(row.get('effective_scale_divisor')) is not int
                or row['effective_scale_divisor'] <= 0
                or any(key not in row.get('geometry', {}) or row['geometry'][key] is None for key in IMAGE_GEOMETRY_FIELDS)
                or len(row.get('pixels_sha256', '')) != 64 or row.get('pixel_bytes', 0) <= 0):
            raise ValueError('report lacks complete actual settings, geometry or numerical identity')
        warmups = report['measurement']['warmups']
        if type(warmups) is not int or warmups < 1 or values != [item['completed_ms'] for item in row['iterations'][warmups:]]:
            raise ValueError('warm samples do not match the declared raw iterations')
        if len(row['iterations']) != warmups + len(values):
            raise ValueError('report has incomplete raw iterations')
    return {row['id']: row for row in rows}


def render_image(report, row, directory):
    """Render one recorded case again with its run's renderer and return the image file."""
    directory.mkdir(parents=True)
    result = subprocess.run([report['build']['binary'], row['source'], str(directory / 'export'), '--render-only',
                             '--warm', '0', '--goldens', str(directory / 'image'),
                             *render_arguments(row, directory / 'settings.json')],
                            capture_output=True, text=True, env=recording_environment(directory, 'off'))
    image = directory / 'image' / (row['source_stem'] + '.f32')
    if result.returncode or digest(image) != row['pixels_sha256']:
        raise ValueError(f"{row['id']}: rendering again did not reproduce the recorded image")
    return image


def measure_difference(before, after, old, new, directory):
    """Render a changed case from both runs and describe how far the images differ."""
    images = [render_image(report, row, directory / side)
              for side, report, row in (('before', before, old), ('after', after, new))]
    result = subprocess.run([after['build']['binary'], '--compare-samples', *map(str, images),
                             '--endianness', new['geometry']['endianness']], capture_output=True, text=True)
    if result.returncode:
        raise ValueError(f"{new['id']}: sample comparison failed: {result.stderr.strip()}")
    data = json.loads(result.stdout)['data']
    for image in images:
        image.unlink()
    return {key: data[key] for key in ('samples', 'different_samples', 'nonfinite_samples',
                                       'max_absolute', 'mean_absolute', 'rms')}


def compare_reports(before, after, max_regression_percent=None, *, measure=None):
    """Compare the cases both runs share, matched by name and by what each asked for.

    `measure(old, new)` describes the difference between two cases whose images differ.
    """
    if max_regression_percent is not None and (not math.isfinite(max_regression_percent) or max_regression_percent < 0):
        raise ValueError('regression budget must be a finite nonnegative percentage')
    left, right = checked_cases(before), checked_cases(after)
    if before['hardware'] != after['hardware'] or before['measurement'] != after['measurement']:
        raise ValueError('reports have different hardware or measurement setup')
    comparisons, decisions = [], []
    for name in left.keys() & right.keys():
        old, new = left[name], right[name]
        if any(old.get(key) != new.get(key) for key in REQUESTED):
            raise ValueError(f'{name}: the two runs asked for different photographs or recipes')
        baseline, candidate = distribution(old['warm_completed_ms']), distribution(new['warm_completed_ms'])
        metrics = {}
        for metric in ('median', 'p95'):
            a, b = baseline[metric], candidate[metric]
            within = None if max_regression_percent is None else b <= a * (1 + max_regression_percent / 100)
            if within is not None:
                decisions.append(within)
            metrics[metric] = {'before_ms': a, 'after_ms': b, 'delta_ms': b - a,
                               'delta_percent': 100 * (b - a) / a if a else (0. if b == 0 else None),
                               'within_budget': within}
        # A change counts as beyond the spread when the two sets of samples do not overlap.
        beyond_spread = (min(new['warm_completed_ms']) > max(old['warm_completed_ms'])
                         or max(new['warm_completed_ms']) < min(old['warm_completed_ms']))
        geometry_equal = old['geometry'] == new['geometry']
        pixels_equal = old['pixels_sha256'] == new['pixels_sha256']
        numerical = {'exact': pixels_equal and geometry_equal, 'pixels_equal': pixels_equal,
                     'geometry_equal': geometry_equal}
        if not pixels_equal and geometry_equal and measure is not None:
            numerical['difference'] = measure(old, new)
        memory = {'before_bytes': old.get('peak_memory_bytes'), 'after_bytes': new.get('peak_memory_bytes')}
        memory['delta_percent'] = (100 * (memory['after_bytes'] - memory['before_bytes']) / memory['before_bytes']
                                   if memory['before_bytes'] and memory['after_bytes'] is not None else None)
        comparisons.append({'id': name, 'before_samples': baseline['count'], 'after_samples': candidate['count'],
                            'peak_memory': memory,
                            'delivered_scale': {'before': old['effective_scale_divisor'],
                                                'after': new['effective_scale_divisor']},
                            'settings_equal': old['settings'] == new['settings'],
                            'beyond_spread': beyond_spread, 'numerical': numerical, **metrics})
    comparisons.sort(key=lambda row: row['id'])
    return {'schema_version': 2, 'kind': 'fresh-processing-off-comparison',
            'max_regression_percent': max_regression_percent,
            'timing_passed': all(decisions) if decisions else None,
            'numerical_exact': all(row['numerical']['exact'] for row in comparisons),
            'only_before': sorted(left.keys() - right.keys()), 'only_after': sorted(right.keys() - left.keys()),
            'cases': comparisons}


def summary(result):
    """The comparison as text: pixel changes first, then timing changes beyond the spread."""
    cases = result['cases']
    changed = [row for row in cases if not row['numerical']['exact']]
    lines = [f"{len(cases)} cases compared. Pixels: {len(cases) - len(changed)} identical, {len(changed)} changed."]
    for row in changed:
        difference = row['numerical'].get('difference')
        if not row['numerical']['geometry_equal']:
            detail = 'size or layout differs'
        elif difference is None:
            detail = 'not measured'
        else:
            share = 100 * difference['different_samples'] / difference['samples']
            detail = (f"{share:.1f}% of samples, max {difference['max_absolute']:.4g}, "
                      f"mean {difference['mean_absolute']:.4g}")
        lines.append(f"  {row['id']}: {detail}")
    moved = [row for row in cases if row['beyond_spread']]
    lines.append(f"Timing: {len(moved)} of {len(cases)} cases changed beyond their sample spread.")
    for row in sorted(moved, key=lambda row: row['median']['delta_percent'] or 0):
        median = row['median']
        scale = row['delivered_scale']
        note = '' if scale['before'] == scale['after'] else f" (delivered 1/{scale['before']} -> 1/{scale['after']})"
        lines.append(f"  {row['id']}: {median['before_ms']:.1f} -> {median['after_ms']:.1f} ms "
                     f"({median['delta_percent']:+.0f}%){note}")
    memory_moved = [row for row in cases if abs(row['peak_memory']['delta_percent'] or 0) >= MEMORY_CHANGE_PERCENT]
    lines.append(f"Peak memory: {len(memory_moved)} of {len(cases)} cases changed by {MEMORY_CHANGE_PERCENT}% or more.")
    for row in sorted(memory_moved, key=lambda row: -row['peak_memory']['delta_percent']):
        memory = row['peak_memory']
        lines.append(f"  {row['id']}: {memory['before_bytes'] / 2**30:.2f} -> {memory['after_bytes'] / 2**30:.2f} GiB "
                     f"({memory['delta_percent']:+.0f}%)")
    for label, names in (('Only in the first run', result['only_before']), ('Only in the second run', result['only_after'])):
        if names:
            lines.append(f"{label}: {', '.join(names)}")
    return '\n'.join(lines)


def portable_report(report, source_report_sha256):
    """Keep the comparable report schema while removing local input and artifact paths."""
    checked_cases(report)
    result = copy.deepcopy(report)
    result['build'].pop('binary', None)
    for row in result['cases']:
        row.pop('source', None)
        row.pop('float_artifact', None)
        row['source_load'].pop('source', None)
        for assertion in row['preview_assertions']:
            assertion.pop('source', None)
    result['evidence'] = {'source_report_sha256': source_report_sha256}
    result['limits'].append('This export records observed performance. It does not approve a latency target or promote output expectations.')
    return result


def load_report(path):
    """A run is named by its directory or by the report inside it."""
    path = path / 'report.json' if path.is_dir() else path
    return path, json.loads(path.read_text())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    capture = commands.add_parser('record', help='run the cases through one renderer and write a run')
    renderer = capture.add_mutually_exclusive_group(required=True)
    renderer.add_argument('--revision', help='a commit, tag or branch to build, or "worktree" for the working tree')
    renderer.add_argument('--binary', type=Path, help='an already built profile_pipeline')
    capture.add_argument('--offline', action='store_true', help='build without network access')
    capture.add_argument('--cases', type=Path, default=ROOT / '.local/test-inputs/render-cases.json')
    capture.add_argument('--directory', type=Path, help='new run directory; defaults to one under .local/runs')
    capture.add_argument('--build-source-revision', help='source revision of a supplied --binary')
    capture.add_argument('--warmups', type=int, default=1)
    capture.add_argument('--samples', type=int, default=5)
    capture.add_argument('--max-memory-mb', type=int, default=5120)
    capture.add_argument('--retain-floats', action='store_true')
    comparison = commands.add_parser('compare', help='compare two runs made on the same machine')
    comparison.add_argument('before', type=Path)
    comparison.add_argument('after', type=Path)
    comparison.add_argument('--max-regression-percent', type=float)
    comparison.add_argument('--output', type=Path, help='new file for the full comparison as JSON')
    comparison.add_argument('--fingerprints-only', action='store_true',
                            help='do not render changed cases again to measure their difference')
    export = commands.add_parser('export', help='write a validated report without local input and artifact paths')
    export.add_argument('report', type=Path)
    export.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.command == 'record':
            binary, revision = (args.binary, args.build_source_revision) if args.binary else build(
                args.revision, offline=args.offline)
            directory = args.directory or ROOT / '.local/runs' / f"{(revision or 'binary')[:12]}-{time.strftime('%Y%m%d-%H%M%S')}"
            directory.parent.mkdir(parents=True, exist_ok=True)
            result = record(binary, json.loads(args.cases.read_text()), directory,
                            build_source_revision=revision, warmups=args.warmups,
                            samples=args.samples, max_memory_mb=args.max_memory_mb, retain_floats=args.retain_floats)
            failed = not result['passed'] or not result['timing_valid']
            failures = [f"{row['id']}: {row['failure']}" for row in result['cases'] if row.get('failure')]
            text = '\n'.join([f"{sum(row['passed'] for row in result['cases'])} of {result['cases_requested']} cases recorded in {directory}",
                              *failures, *([] if result['timing_valid'] or failures else ['timing is not valid: other work ran during the run'])])
        elif args.command == 'compare':
            (before_path, before), (after_path, after) = load_report(args.before), load_report(args.after)
            scratch = after_path.parent / f"difference-{time.strftime('%Y%m%d-%H%M%S')}"
            measure = None if args.fingerprints_only else (
                lambda old, new: measure_difference(before, after, old, new, scratch / new['id']))
            result = compare_reports(before, after, args.max_regression_percent, measure=measure)
            result['reports'] = {name: {'path': str(path.resolve()), 'sha256': digest(path),
                                        'source_revision': report['build']['source_revision']}
                                 for name, path, report in (('before', before_path, before), ('after', after_path, after))}
            if args.output:
                with args.output.open('x') as output:
                    output.write(json.dumps(result, indent=2, allow_nan=False) + '\n')
            failed = result['timing_passed'] is False
            text = summary(result)
        else:
            result = portable_report(json.loads(args.report.read_text()), digest(args.report))
            with args.output.open('x') as output:
                output.write(json.dumps(result, indent=2, allow_nan=False) + '\n')
            failed, text = False, f'wrote {args.output}'
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.CalledProcessError) as error:
        parser.exit(1, str(error) + '\n')
    print(text)
    if failed:
        raise SystemExit(1)


if __name__ == '__main__':
    main()

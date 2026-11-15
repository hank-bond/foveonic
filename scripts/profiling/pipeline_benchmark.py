"""Record matched fresh-processing Off distributions and compare timings and exact output separately."""

import argparse
import copy
import json
import math
import os
from pathlib import Path
import platform
import subprocess

from file_hash import digest
from native import recording_environment, workload_monitor
from process_memory import run_captured
from render_matrix import expand_cases, render_arguments
from results import IMAGE_GEOMETRY_FIELDS, render_result
from trace_analysis import distribution
from watch_workloads import concurrent_workloads, require_quiet


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


def compare_reports(before, after, max_regression_percent=None):
    if max_regression_percent is not None and (not math.isfinite(max_regression_percent) or max_regression_percent < 0):
        raise ValueError('regression budget must be a finite nonnegative percentage')
    left, right = checked_cases(before), checked_cases(after)
    if (before['hardware'] != after['hardware'] or before['measurement'] != after['measurement']
            or left.keys() != right.keys()):
        raise ValueError('reports have different hardware, measurement setup or case sets')
    comparisons, decisions = [], []
    for name, old in left.items():
        new = right[name]
        identity = ('source_sha256', 'family', 'raw_size', 'resolution', 'processing_quality',
                    'options', 'settings', 'effective_scale_divisor', 'adapter', 'geometry')
        if any(old[key] != new[key] for key in identity) or old.get('settings_snapshot') != new.get('settings_snapshot'):
            raise ValueError(f'{name}: reports have different source, recipe, actual settings, grid or adapter')
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
        exact = (old['pixels_sha256'] == new['pixels_sha256'] and old['pixel_bytes'] == new['pixel_bytes']
                 and old['geometry'] == new['geometry'])
        comparisons.append({'id': name, 'before_samples': baseline['count'], 'after_samples': candidate['count'],
                            'numerical': {'exact': exact, 'pixels_equal': old['pixels_sha256'] == new['pixels_sha256'],
                                          'geometry_equal': old['geometry'] == new['geometry']}, **metrics})
    return {'schema_version': 1, 'kind': 'fresh-processing-off-comparison',
            'max_regression_percent': max_regression_percent,
            'timing_passed': all(decisions) if decisions else None,
            'numerical_exact': all(row['numerical']['exact'] for row in comparisons), 'cases': comparisons}


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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    capture = commands.add_parser('record')
    capture.add_argument('--binary', type=Path, required=True)
    capture.add_argument('--cases', type=Path, required=True)
    capture.add_argument('--directory', type=Path, required=True)
    capture.add_argument('--build-source-revision', help='optional caller-declared source revision used to build the supplied executable')
    capture.add_argument('--warmups', type=int, default=1)
    capture.add_argument('--samples', type=int, default=5)
    capture.add_argument('--max-memory-mb', type=int, default=5120)
    capture.add_argument('--retain-floats', action='store_true')
    comparison = commands.add_parser('compare')
    comparison.add_argument('before', type=Path)
    comparison.add_argument('after', type=Path)
    comparison.add_argument('--max-regression-percent', type=float)
    comparison.add_argument('--output', type=Path)
    export = commands.add_parser('export', help='write a validated report without local input and artifact paths')
    export.add_argument('report', type=Path)
    export.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.command == 'record':
            result = record(args.binary, json.loads(args.cases.read_text()), args.directory,
                            build_source_revision=args.build_source_revision, warmups=args.warmups,
                            samples=args.samples, max_memory_mb=args.max_memory_mb, retain_floats=args.retain_floats)
            failed = not result['passed'] or not result['timing_valid']
        elif args.command == 'compare':
            result = compare_reports(json.loads(args.before.read_text()), json.loads(args.after.read_text()), args.max_regression_percent)
            result['reports'] = {name: {'path': str(path.resolve()), 'sha256': digest(path)}
                                 for name, path in (('before', args.before), ('after', args.after))}
            if args.output:
                with args.output.open('x') as output:
                    output.write(json.dumps(result, indent=2, allow_nan=False) + '\n')
            failed = result['timing_passed'] is False or not result['numerical_exact']
        else:
            result = portable_report(json.loads(args.report.read_text()), digest(args.report))
            with args.output.open('x') as output:
                output.write(json.dumps(result, indent=2, allow_nan=False) + '\n')
            failed = False
    except (OSError, ValueError, KeyError, RuntimeError) as error:
        parser.exit(1, str(error) + '\n')
    print(json.dumps(result, indent=2, allow_nan=False))
    if failed:
        raise SystemExit(1)


if __name__ == '__main__':
    main()

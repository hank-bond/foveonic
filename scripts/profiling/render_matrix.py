"""Record or compare exact rendered samples across an explicit camera and preview matrix."""

import argparse
import copy
import json
import math
import struct
from pathlib import Path
import re
import subprocess

from file_hash import digest
from image_replay import events
from results import IMAGE_GEOMETRY_FIELDS, RESOLUTIONS, PROCESSING_QUALITIES, compare_images, expected_resolution_scale, include_assertions, render_result
from watch_workloads import require_quiet
from process_memory import run_captured

REQUIRED_SOURCES_BY_FAMILY = {'classic': 3, 'merrill': 3, 'quattro': 3, 'tiger': 1}
RESERVED = {'--goldens', '--capture-float', '--resolution', '--processing-quality', '--working-scale', '--profile', '--warm', '--settings-snapshot', '--render-only'}


def expand_cases(cases):
    """Resolve declared inputs and make missing family/resolution/mode coverage visible."""
    expanded, seen, families = [], set(), {family: set() for family in REQUIRED_SOURCES_BY_FAMILY}
    for case in cases:
        if 'qualities' in case or 'quality' in case:
            raise ValueError('matrix cases require resolutions and processing_qualities as separate axes')
        label, family = case['label'], case['family']
        if re.fullmatch(r'[A-Za-z0-9_-]+', label) is None or family not in REQUIRED_SOURCES_BY_FAMILY:
            raise ValueError('case labels must be simple filenames and families must be known')
        source = Path(case['source']).resolve(strict=True)
        if not source.is_file():
            raise ValueError(f'not a source file: {source}')
        snapshot = case.get('settings_snapshot')
        if snapshot is not None and ('options' in case or not isinstance(snapshot, dict)
                                     or not isinstance(snapshot.get('settings'), dict) or not snapshot['settings']):
            raise ValueError(f'{label}: a settings snapshot needs settings and cannot be combined with options')
        options = case.get('options', [] if snapshot is not None else ['--wb', 'Sunlight', '--mode', 'Standard'])
        if not isinstance(options, list) or any(not isinstance(arg, str) or arg.split('=')[0] in RESERVED for arg in options):
            raise ValueError(f'{label}: options must be strings without matrix-owned flags')
        resolutions = case.get('resolutions', RESOLUTIONS)
        policies = case.get('processing_qualities', PROCESSING_QUALITIES)
        if not policies or len(set(policies)) != len(policies) or any(policy not in PROCESSING_QUALITIES for policy in policies):
            raise ValueError(f'{label}: invalid processing qualities')
        if not isinstance(case.get('raw_size', 'unspecified'), str):
            raise ValueError(f'{label}: RAW size must be a descriptive string')
        if not resolutions or len(set(resolutions)) != len(resolutions) or any(q not in RESOLUTIONS for q in resolutions):
            raise ValueError(f'{label}: invalid resolutions')
        source_hash = digest(source)
        families[family].add(source_hash)
        for resolution in resolutions:
            for policy in policies:
                key = f'{label}-{resolution}-{policy}'
                if key in seen:
                    raise ValueError(f'duplicate matrix case: {key}')
                seen.add(key)
                expanded.append({'id': key, 'label': label, 'family': family, 'raw_size': case.get('raw_size', 'unspecified'), 'source': str(source), 'source_stem': source.stem,
                                 'source_sha256': source_hash, 'resolution': resolution, 'processing_quality': policy, 'options': options,
                                 **({'settings_snapshot': snapshot} if snapshot is not None else {})})
    coverage = {family: {'provided': len(families[family]), 'required': needed,
                          'missing': max(0, needed-len(families[family]))}
                for family, needed in REQUIRED_SOURCES_BY_FAMILY.items()}
    missing_resolutions = [{'label': case['label'], 'resolution': q} for case in cases
                           for q in RESOLUTIONS if q not in case.get('resolutions', RESOLUTIONS)]
    missing_policies = [{'label': case['label'], 'processing_quality': policy} for case in cases
                        for policy in PROCESSING_QUALITIES if policy not in case.get('processing_qualities', PROCESSING_QUALITIES)]
    raw_sizes = {family: sorted({row['raw_size'] for row in expanded if row['family'] == family}) for family in REQUIRED_SOURCES_BY_FAMILY}
    return expanded, {'families': coverage, 'raw_sizes': raw_sizes, 'missing_resolutions': missing_resolutions, 'missing_processing_qualities': missing_policies,
                      'complete': bool(expanded) and not missing_resolutions and not missing_policies and all(row['missing']==0 for row in coverage.values())}


def same_input(left, right):
    return (all(left[key] == right[key] for key in ('id', 'family', 'raw_size', 'source_stem', 'source_sha256', 'resolution', 'processing_quality', 'options'))
            and left.get('settings_snapshot') == right.get('settings_snapshot'))


def render_arguments(case, snapshot_path):
    """Apply matrix-owned resolution and policy to either declared controls or a fixed snapshot."""
    if 'settings_snapshot' not in case:
        return ['--resolution', case['resolution'], '--processing-quality', case['processing_quality'],
                '--profile', 'off', *case['options']]
    snapshot = copy.deepcopy(case['settings_snapshot'])
    family = {'classic': 'Classic', 'merrill': 'Merrill', 'quattro': 'Quattro', 'tiger': 'Quattro'}[case['family']]
    snapshot['working_scale'] = expected_resolution_scale(case['resolution'], family)
    snapshot['processing_quality'] = case['processing_quality']
    with snapshot_path.open('x') as output:
        output.write(json.dumps(snapshot, indent=2, allow_nan=False) + '\n')
    return ['--settings-snapshot', str(snapshot_path)]



def processing_mode_difference(quality_file, speed_file, geometry):
    """Describe dense/reduced differences without approving either output as a golden."""
    if quality_file.stat().st_size != speed_file.stat().st_size or quality_file.stat().st_size % 4:
        raise ValueError('processing-mode images have inconsistent float storage')
    byte_order = {'little':'<', 'big':'>'}.get(geometry.get('endianness'))
    if byte_order is None:
        raise ValueError('processing-mode image endianness is missing')
    count = changed = nonfinite = 0
    maximum = absolute_sum = squared_sum = 0.
    with quality_file.open('rb') as quality, speed_file.open('rb') as speed:
        while left := quality.read(65536):
            right = speed.read(len(left))
            for (a,), (b,) in zip(struct.iter_unpack(byte_order+'f', left), struct.iter_unpack(byte_order+'f', right)):
                count += 1
                if not math.isfinite(a) or not math.isfinite(b):
                    nonfinite += 1
                    continue
                delta = abs(a-b)
                changed += delta != 0.
                maximum = max(maximum, delta)
                absolute_sum += delta
                squared_sum += delta*delta
    return {'kind':'processing-mode-difference', 'geometry':geometry,
            'quality':{'file':str(quality_file),'sha256':digest(quality_file)},
            'speed':{'file':str(speed_file),'sha256':digest(speed_file)},
            'samples':count, 'different_samples':changed, 'nonfinite_samples':nonfinite,
            'outcome':'failed' if nonfinite else 'complete',
            'failure':'processing-mode images contain nonfinite samples' if nonfinite else None,
            'max_absolute':None if nonfinite else maximum, 'mean_absolute':None if nonfinite else absolute_sum/max(count,1),
            'rms':None if nonfinite else math.sqrt(squared_sum/max(count,1)), 'visual_acceptance':'unreviewed',
            'limits':'Differences are descriptive evidence. They do not approve dense output or replace immutable exact baselines.'}


def run(binary, cases, directory, baseline=None, require_coverage=False, max_memory_mb=None):
    if max_memory_mb is not None and max_memory_mb <= 0:
        raise ValueError('physical-footprint limit must be positive')
    binary = binary.resolve(strict=True)
    matrix, coverage = expand_cases(cases)
    if not matrix:
        raise ValueError('the matrix must contain at least one case')
    if require_coverage and not coverage['complete']:
        raise ValueError('the declared matrix does not cover all camera families, resolutions and processing modes: '+json.dumps(coverage))
    prior = None
    if baseline:
        baseline = baseline.resolve(strict=True)
        prior = json.loads((baseline/'manifest.json').read_text())
        if prior.get('schema_version') != 2:
            raise ValueError('baseline must use the resolution/processing-quality matrix schema')
        if prior.get('passed') is not True:
            raise ValueError('baseline is incomplete or failed')
        previous = {row['id']: row for row in prior['cases']}
        if len(previous) != len(matrix) or any(row['id'] not in previous or not same_input(row, previous[row['id']]) for row in matrix):
            raise ValueError('baseline and current matrix have different source identities, recipes, resolutions or processing modes')
    directory.mkdir(parents=True, exist_ok=False)
    report = {'schema_version': 2, 'kind': 'exact-render-matrix', 'passed': False,
              'physical_memory_guard_limit_bytes':None if max_memory_mb is None else max_memory_mb * 1024 * 1024,
              'binary': str(binary), 'binary_sha256': digest(binary), 'baseline': str(baseline) if baseline else None,
              'coverage': coverage, 'cases': [], 'processing_mode_comparisons': [],
              'limits': ['Camera family and RAW size are supplied by the fixture manifest, not independently verified.',
                         'Exact samples and image geometry are checked outside latency measurements.',
                         'GPU/backend changes can affect bit-exact output. This is not a visual quality test.']}
    try:
        for case in matrix:
            name = case['id']
            out = directory/name
            golden_dir = directory/'goldens'/name
            golden = golden_dir/(Path(case['source']).stem+'.f32')
            row = {**case, 'passed': False}
            report['cases'].append(row)
            abort = False
            try:
                if baseline:
                    expected = previous[name]
                    baseline_image = baseline/'goldens'/name/golden.name
                    if not baseline_image.is_file() or digest(baseline_image) != expected['pixels_sha256']:
                        raise ValueError('baseline samples are missing or modified')
                require_quiet()
                command = [str(binary), case['source'], str(out), '--warm', '0', '--goldens', str(golden_dir),
                           *render_arguments(case, directory/(name+'.settings.json'))]
                result = run_captured(command,
                    limit_bytes=None if max_memory_mb is None else max_memory_mb * 1024 * 1024,
                    samples=directory/(name+'.memory.jsonl'))
                (directory/(name+'.jsonl')).write_text(result.stdout)
                (directory/(name+'.stderr')).write_text(result.stderr)
                if result.returncode:
                    raise ValueError(f'render exited with status {result.returncode}; see stderr')
                data = events(result.stdout)
                if 'settings_snapshot' in case and data['session']['settings'] != case['settings_snapshot']['settings']:
                    raise ValueError('actual settings differ from the declared snapshot')
                validation = data['validation']
                observation = {'step': 'render', 'source': case['source'], 'source_sha256': case['source_sha256'],
                               'family': data['session'].get('family'),
                               'scale_divisor': data['processing'].get('effective_scale_divisor'),
                               'processing_quality': data['session'].get('processing_quality')}
                environment = {'binary_sha256':report['binary_sha256'], 'adapter':data['session'].get('adapter')}
                row['test_result'] = render_result('headless', 'fresh-render', case['processing_quality'], [observation], True, environment=environment, resolution=case['resolution'])
                if row['test_result']['outcome'] != 'passed':
                    raise ValueError('preview-scale assertion failed or could not be evaluated')
                row.update(settings=data['session']['settings'], scale=data['session']['scale_divisor'],
                           geometry={key: validation[key] for key in IMAGE_GEOMETRY_FIELDS},
                           digest=validation['digest'], pixels_sha256=digest(golden))
                if baseline:
                    for key in ('settings', 'scale'):
                        if row[key] != expected[key]:
                            raise ValueError(f'{key} differs from baseline')
                    comparison = compare_images(case['source'], 'render', baseline_image, expected['geometry'],
                                                golden, row['geometry'])
                    include_assertions(row['test_result'], [comparison])
                    if comparison['outcome'] != 'passed':
                        raise ValueError('image samples or geometry differ from baseline')
                row['passed'] = True
                tiff = out/(Path(case['source']).stem+'-0.tiff')
                tiff.unlink()
                out.rmdir()
            except (OSError, ValueError, KeyError, json.JSONDecodeError, RuntimeError, subprocess.TimeoutExpired) as error:
                row['failure'] = str(error)
                if 'test_result' in row:
                    if row['test_result']['outcome'] == 'passed':
                        row['test_result']['outcome'] = 'failed'
                    row['test_result']['failure'] = str(error)
                else:
                    row['test_result'] = render_result('headless', 'fresh-render', case['processing_quality'], [], False, str(error),
                                                       {'binary_sha256':report['binary_sha256'], 'adapter':None}, resolution=case['resolution'])
                abort = isinstance(error, (RuntimeError, subprocess.TimeoutExpired))
            test_result_path = directory/(name+'.test-result.json')
            test_result_path.write_text(json.dumps(row['test_result'], indent=2)+'\n')
            row['test_result_artifact'] = {'path':str(test_result_path), 'sha256':digest(test_result_path)}
            (directory/'manifest.json').write_text(json.dumps(report, indent=2)+'\n')
            print(f"{name}: {('exact' if baseline else 'recorded') if row['passed'] else 'FAILED: '+row['failure']}", flush=True)
            if abort:
                break
        paired = {}
        for row in report['cases']:
            if row['passed']:
                paired.setdefault((row['label'], row['resolution']), {})[row['processing_quality']] = row
        for (label, resolution), pair in paired.items():
            if set(pair) != set(PROCESSING_QUALITIES):
                continue
            quality, speed = pair['quality'], pair['speed']
            if quality['geometry'] != speed['geometry']:
                report['processing_mode_comparisons'].append({'label':label,'resolution':resolution,
                    'outcome':'incomplete','failure':'processing modes returned different image geometry'})
                continue
            files = [directory/'goldens'/row['id']/(row['source_stem']+'.f32') for row in (quality, speed)]
            report['processing_mode_comparisons'].append({'label':label,'resolution':resolution,
                'source_sha256':quality['source_sha256'], **processing_mode_difference(*files, quality['geometry'])})
        report['passed'] = (all(row['passed'] for row in report['cases']) and len(report['cases']) == len(matrix)
                            and not any(row.get('outcome') in ('incomplete','failed') for row in report['processing_mode_comparisons']))
    finally:
        (directory/'manifest.json').write_text(json.dumps(report, indent=2)+'\n')
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True, help='built profile_pipeline example')
    parser.add_argument('--cases', type=Path, required=True, help='JSON array of labelled camera sources and optional options/resolutions/processing_qualities')
    parser.add_argument('--directory', type=Path, required=True, help='new owned output directory')
    parser.add_argument('--baseline', type=Path, help='compare against a previously recorded matrix instead of recording')
    parser.add_argument('--require-coverage', action='store_true', help='require three Classic, Merrill and Quattro inputs, one Tiger input, and all resolutions and both processing modes')
    parser.add_argument('--max-memory-mb', type=int, help='sample and guard each renderer child physical footprint on macOS')
    args = parser.parse_args()
    try:
        report = run(args.binary, json.loads(args.cases.read_text()), args.directory, args.baseline, args.require_coverage, args.max_memory_mb)
    except (OSError, ValueError, KeyError, json.JSONDecodeError) as error:
        parser.exit(1, str(error)+'\n')
    if not report['passed']:
        raise SystemExit(1)


if __name__ == '__main__':
    main()

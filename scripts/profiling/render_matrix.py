"""Record or compare exact rendered samples across an explicit camera and preview matrix."""

import argparse
import json
from pathlib import Path
import re
import subprocess

from file_hash import digest
from results import PREVIEW_QUALITIES, render_result
from watch_workloads import require_quiet

FAMILIES = {'classic': 3, 'merrill': 3, 'quattro': 3, 'tiger': 1}
QUALITIES = PREVIEW_QUALITIES
RESERVED = {'--goldens', '--capture-float', '--preview-quality', '--working-scale', '--profile', '--warm'}


def expand_cases(cases):
    """Resolve declared inputs and make missing family/quality coverage visible."""
    expanded, seen, families = [], set(), {family: set() for family in FAMILIES}
    for case in cases:
        label, family = case['label'], case['family']
        if re.fullmatch(r'[A-Za-z0-9_-]+', label) is None or family not in FAMILIES:
            raise ValueError('case labels must be simple filenames and families must be known')
        source = Path(case['source']).resolve(strict=True)
        if not source.is_file():
            raise ValueError(f'not a source file: {source}')
        options = case.get('options', ['--wb', 'Sunlight', '--mode', 'Standard'])
        if not isinstance(options, list) or any(not isinstance(arg, str) or arg.split('=')[0] in RESERVED for arg in options):
            raise ValueError(f'{label}: options must be strings without matrix-owned flags')
        qualities = case.get('qualities', QUALITIES)
        if not isinstance(case.get('raw_size', 'unspecified'), str):
            raise ValueError(f'{label}: RAW size must be a descriptive string')
        if not qualities or len(set(qualities)) != len(qualities) or any(q not in QUALITIES for q in qualities):
            raise ValueError(f'{label}: invalid preview qualities')
        source_hash = digest(source)
        families[family].add(source_hash)
        for quality in qualities:
            key = f'{label}-{quality}'
            if key in seen:
                raise ValueError(f'duplicate matrix case: {key}')
            seen.add(key)
            expanded.append({'id': key, 'label': label, 'family': family, 'raw_size': case.get('raw_size', 'unspecified'), 'source': str(source), 'source_stem': source.stem,
                             'source_sha256': source_hash, 'quality': quality, 'options': options})
    coverage = {family: {'provided': len(families[family]), 'required': needed,
                          'missing': max(0, needed-len(families[family]))}
                for family, needed in FAMILIES.items()}
    missing_qualities = [{'label': case['label'], 'quality': q} for case in cases
                         for q in QUALITIES if q not in case.get('qualities', QUALITIES)]
    raw_sizes = {family: sorted({row['raw_size'] for row in expanded if row['family'] == family}) for family in FAMILIES}
    return expanded, {'families': coverage, 'raw_sizes': raw_sizes, 'missing_qualities': missing_qualities,
                      'complete': bool(expanded) and not missing_qualities and all(row['missing']==0 for row in coverage.values())}


def same_input(left, right):
    return all(left[key] == right[key] for key in ('id', 'family', 'raw_size', 'source_stem', 'source_sha256', 'quality', 'options'))


def events(stdout):
    found = {}
    for line in stdout.splitlines():
        row = json.loads(line)
        if row['event'] in ('session', 'processing', 'validation'):
            if row['event'] in found:
                raise ValueError(f"duplicate {row['event']} event")
            found[row['event']] = row['data']
    if set(found) != {'session', 'processing', 'validation'}:
        raise ValueError('render did not report its settings, image scale and validation')
    return found


def run(binary, cases, directory, baseline=None, require_coverage=False):
    binary = binary.resolve(strict=True)
    matrix, coverage = expand_cases(cases)
    if not matrix:
        raise ValueError('the matrix must contain at least one case')
    if require_coverage and not coverage['complete']:
        raise ValueError('the declared matrix does not cover all camera families and qualities: '+json.dumps(coverage))
    prior = None
    if baseline:
        baseline = baseline.resolve(strict=True)
        prior = json.loads((baseline/'manifest.json').read_text())
        if prior.get('passed') is not True:
            raise ValueError('baseline is incomplete or failed')
        previous = {row['id']: row for row in prior['cases']}
        if len(previous) != len(matrix) or any(row['id'] not in previous or not same_input(row, previous[row['id']]) for row in matrix):
            raise ValueError('baseline and current matrix have different source identities, recipes or qualities')
    directory.mkdir(parents=True, exist_ok=False)
    report = {'schema_version': 1, 'kind': 'exact-render-matrix', 'passed': False,
              'binary': str(binary), 'binary_sha256': digest(binary), 'baseline': str(baseline) if baseline else None,
              'coverage': coverage, 'cases': [],
              'limits': ['Camera family and RAW size are supplied by the fixture manifest, not independently verified.',
                         'Exact samples and image geometry are checked outside latency measurements.',
                         'GPU/backend changes can affect bit-exact output. This is not a visual quality test.']}
    try:
        for case in matrix:
            name = case['id']
            out = directory/name
            golden_dir = (baseline if baseline else directory)/'goldens'/name
            golden = golden_dir/(Path(case['source']).stem+'.f32')
            row = {**case, 'passed': False}
            report['cases'].append(row)
            abort = False
            try:
                if baseline:
                    expected = previous[name]
                    if not golden.is_file() or digest(golden) != expected['pixels_sha256']:
                        raise ValueError('baseline samples are missing or modified')
                require_quiet()
                command = [str(binary), case['source'], str(out), '--warm', '0', '--preview-quality', case['quality'],
                           '--profile', 'off', '--goldens', str(golden_dir), *case['options']]
                result = subprocess.run(command, capture_output=True, text=True)
                (directory/(name+'.jsonl')).write_text(result.stdout)
                (directory/(name+'.stderr')).write_text(result.stderr)
                if result.returncode:
                    raise ValueError(f'render exited with status {result.returncode}; see stderr')
                data = events(result.stdout)
                validation = data['validation']
                observation = {'step': 'render', 'source': case['source'], 'source_sha256': case['source_sha256'],
                               'family': data['session'].get('family'),
                               'scale_divisor': data['processing'].get('effective_scale_divisor')}
                environment = {'binary_sha256':report['binary_sha256'], 'adapter':data['session'].get('adapter')}
                row['test_result'] = render_result('headless', 'fresh-render', case['quality'], [observation], True, environment=environment)
                if row['test_result']['outcome'] != 'passed':
                    raise ValueError('preview-scale assertion failed or could not be evaluated')
                row.update(settings=data['session']['settings'], scale=data['session']['scale_divisor'],
                           geometry={key: validation[key] for key in ('extent', 'layout', 'view_extent', 'view_crop', 'view_rotation')},
                           digest=validation['digest'], pixels_sha256=digest(golden))
                if baseline:
                    for key in ('settings', 'scale', 'geometry', 'digest', 'pixels_sha256'):
                        if row[key] != expected[key]:
                            raise ValueError(f'{key} differs from baseline')
                row['passed'] = True
                tiff = out/(Path(case['source']).stem+'-0.tiff')
                tiff.unlink()
                out.rmdir()
            except (OSError, ValueError, KeyError, json.JSONDecodeError, RuntimeError) as error:
                row['failure'] = str(error)
                if 'test_result' in row:
                    if row['test_result']['outcome'] == 'passed':
                        row['test_result']['outcome'] = 'failed'
                    row['test_result']['failure'] = str(error)
                else:
                    row['test_result'] = render_result('headless', 'fresh-render', case['quality'], [], False, str(error),
                                                       {'binary_sha256':report['binary_sha256'], 'adapter':None})
                abort = isinstance(error, RuntimeError)
            test_result_path = directory/(name+'.test-result.json')
            test_result_path.write_text(json.dumps(row['test_result'], indent=2)+'\n')
            row['test_result_artifact'] = {'path':str(test_result_path), 'sha256':digest(test_result_path)}
            (directory/'manifest.json').write_text(json.dumps(report, indent=2)+'\n')
            print(f"{name}: {'exact' if row['passed'] else 'FAILED: '+row['failure']}", flush=True)
            if abort:
                break
        report['passed'] = all(row['passed'] for row in report['cases']) and len(report['cases']) == len(matrix)
    finally:
        (directory/'manifest.json').write_text(json.dumps(report, indent=2)+'\n')
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True, help='built profile_pipeline example')
    parser.add_argument('--cases', type=Path, required=True, help='JSON array of labelled camera sources and optional options/qualities')
    parser.add_argument('--directory', type=Path, required=True, help='new owned output directory')
    parser.add_argument('--baseline', type=Path, help='compare against a previously recorded matrix instead of recording')
    parser.add_argument('--require-coverage', action='store_true', help='require three Classic, Merrill and Quattro inputs, one Tiger input, and all preview qualities')
    args = parser.parse_args()
    try:
        report = run(args.binary, json.loads(args.cases.read_text()), args.directory, args.baseline, args.require_coverage)
    except (OSError, ValueError, KeyError, json.JSONDecodeError) as error:
        parser.exit(1, str(error)+'\n')
    if not report['passed']:
        raise SystemExit(1)


if __name__ == '__main__':
    main()

"""Remove only rehashed raw samples from complete successful GUI slider cases."""

import hashlib
import json
import math
import os
import re
from pathlib import Path
import stat

from results import IMAGE_GEOMETRY_FIELDS
from scenario_checks import checked_scenario_images


def _require(condition, reason):
    if not condition:
        raise ValueError(reason)


def _owned(root, path):
    path = Path(path)
    _require(path.is_absolute() and '..' not in path.parts and path.is_relative_to(root),
             'sample retention path is outside its case')
    _require(path.resolve(strict=True) == path and not path.is_symlink(),
             'sample retention does not follow symlinks or aliases')
    for parent in path.parents:
        _require(not parent.is_symlink(), 'sample retention parent is a symlink')
    _require(stat.S_ISREG(path.stat().st_mode), 'sample retention needs an ordinary file')
    return path


def _identity(path):
    info = path.stat(follow_symlinks=False)
    return (info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns, info.st_ctime_ns)


def _hash(path):
    before = _identity(path)
    value = hashlib.sha256()
    with path.open('rb') as stream:
        _require((os.fstat(stream.fileno()).st_dev, os.fstat(stream.fileno()).st_ino) == before[:2],
                 'sample retention file changed while opening')
        for block in iter(lambda: stream.read(1024*1024), b''):
            value.update(block)
    _require(_identity(path) == before, 'sample retention file changed while hashing')
    return value.hexdigest(), before


def _read(root, path, provenance):
    path = _owned(root, path)
    before = _identity(path)
    raw = path.read_bytes()
    _require(_identity(path) == before, 'sample retention metadata changed while reading')
    digest = hashlib.sha256(raw).hexdigest()
    value = json.loads(raw)
    _require(isinstance(value, dict), 'sample retention evidence must be an object')
    provenance.append({'path': str(path.relative_to(root)), 'sha256': digest})
    return value, digest


def _geometry(value):
    _require(isinstance(value, dict) and set(value) == set(IMAGE_GEOMETRY_FIELDS),
             'sample retention requires complete known image geometry')
    for name in ('extent', 'view_extent'):
        _require(isinstance(value[name], list) and len(value[name]) == 2
                 and all(type(v) is int and v > 0 for v in value[name]), 'invalid image extent')
    _require(value['layout'] in ('Rgb32Float', 'Scalar32Float')
             and isinstance(value['space'], str) and value['space']
             and type(value['linear_white']) in (int, float)
             and math.isfinite(value['linear_white']) and value['linear_white'] > 0
             and type(value['view_baked']) is bool and value['endianness'] in ('little', 'big')
             and type(value['view_rotation']) is int and value['view_rotation'] >= 0
             and isinstance(value['view_crop'], list) and len(value['view_crop']) == 4
             and all(type(v) is int and v >= 0 for v in value['view_crop']), 'invalid image geometry')
    return value['extent'][0]*value['extent'][1]*(12 if value['layout'] == 'Rgb32Float' else 4)


def _plan(root):
    provenance, images, selected, executions = [], [], [], []
    report, report_sha = _read(root, root/'regression.json', provenance)
    trials = report['trials']
    _require(report.get('schema_version') == 1 and report.get('scenario') == 'slider-drag'
             and report.get('mode') == 'off' and report.get('passed') is True
             and report.get('workload_consistent') is True and report.get('workload_key') is not None
             and type(report.get('trials_requested')) is int and report['trials_requested'] > 0
             and type(report.get('photo_count')) is int and report['photo_count'] > 0
             and isinstance(trials, list) and len(trials) == report['trials_requested'],
             'sample retention requires a complete passed Off slider report')
    for index, trial in enumerate(trials):
        directory = root/f'trial-{index:02}'
        _require(trial.get('passed') is True, 'sample retention preserves unsuccessful trials')
        shared = trial['test_result']
        assertions = shared['assertions']
        _require(shared.get('outcome') == 'passed' and isinstance(assertions, list) and assertions
                 and all(a.get('outcome') == 'passed' for a in assertions),
                 'sample retention preserves failed or incomplete assertions')
        artifacts = trial['artifacts']
        shared_path = directory/'evidence/test-result.json'
        _require(Path(artifacts['test_result']['path']) == shared_path
                 and Path(artifacts['launch']) == directory/'launch.json'
                 and Path(artifacts['result']) == directory/'evidence/scenario-result.json',
                 'sample retention evidence paths differ from the owned trial')
        on_disk, shared_sha = _read(root, shared_path, provenance)
        _require(on_disk == shared and shared_sha == artifacts['test_result']['sha256'],
                 'sample retention shared assertions changed')
        launch, _ = _read(root, directory/'launch.json', provenance)
        native, native_sha = _read(root, directory/'evidence/scenario-result.json', provenance)
        _require(native.get('passed') is True and native_sha == artifacts['result_sha256'],
                 'sample retention native result is unsuccessful or changed')
        scenario = launch['scenario']
        _require(native.get('scenario') == scenario
                 and scenario['name'] == shared['scenario'] == report['scenario']
                 and scenario['mode'] == report['mode']
                 and scenario['photo_count'] == report['photo_count']
                 and scenario['quality'] == shared['processing_quality'] == report['quality']
                 and shared.get('driver') == 'application', 'sample retention scenario policy differs')
        for key in ('binary_sha256', 'source_tree_sha256'):
            _require(isinstance(launch[key], str) and re.fullmatch('[0-9a-f]{64}', launch[key])
                     and launch[key] == artifacts[key], 'sample retention execution identity differs')
        fresh = launch['fresh_renderer']
        _require(isinstance(fresh['sha256'], str) and re.fullmatch('[0-9a-f]{64}', fresh['sha256'])
                 and artifacts['fresh_renderer'] == fresh and artifacts['binary'] == launch['binary']
                 and shared['environment']['binary_sha256'] == launch['binary_sha256']
                 and shared['environment']['fresh_binary_sha256'] == fresh['sha256']
                 and shared['environment']['adapter'] == native['hardware']['adapter'],
                 'sample retention application or fresh renderer differs')
        inputs, captures = launch['inputs'], native['image_captures']
        _require(scenario['sources'] == [item['staged_source'] for item in inputs],
                 'sample retention source order differs')
        _read(root, directory/'evidence/slider-drag.json', provenance)
        action, photos = checked_scenario_images(native, inputs, directory, scenario['control_pointer'],
                                                 scenario.get('native_photo_count', 1))
        executions.append({'trial': index, 'binary': launch['binary'], 'binary_sha256': launch['binary_sha256'],
                           'source_tree_sha256': launch['source_tree_sha256'], 'fresh_renderer': fresh})
        checks = [a for a in assertions if a.get('name') == 'image_samples']
        _require(isinstance(inputs, list) and isinstance(captures, list) and inputs
                 and len(inputs) == len(captures) == len(checks) == report['photo_count'],
                 'sample retention image coverage is missing or repeated')
        declared = {item['staged_source']: item for item in inputs}
        _require(len(declared) == len(inputs) and len({c['source'] for c in captures}) == len(captures),
                 'sample retention photo identities are repeated')
        for image_index, (capture, check) in enumerate(zip(captures, checks, strict=True)):
            source = capture['source']
            _require(source in declared and check['source'] == declared[source]['source'],
                     'sample retention image source differs from its declaration')
            photo = photos[source]
            _require(check['step'] == action
                     and (capture['task'], capture['revision']) == (photo['task'], photo['revision'])
                     and capture['settings']['working_scale'] == photo['scale_divisor']
                     and capture['settings']['processing_quality'] == photo['processing_quality'] == scenario['quality'],
                     'sample retention capture is not its checked settled frame')
            expected = directory/f'fresh-{image_index:02}-samples'/(Path(source).stem+'.f32')
            actual = directory/'evidence'/f'image-{image_index:02}.f32'
            _require(Path(check['expected']['file']) == expected and Path(check['actual']['file']) == actual
                     and Path(capture['file']) == actual, 'sample retention refuses unknown image paths')
            geometry = check['expected']['geometry']
            required = _geometry(geometry)
            _require(check['actual']['geometry'] == geometry
                     and {key: capture[key] for key in IMAGE_GEOMETRY_FIELDS} == geometry
                     and check['difference'] == {'geometry': False, 'missing_geometry': [],
                         'first_byte': None, 'required_bytes': required}, 'sample retention geometry or comparison differs')
            files = []
            for role, path in (('expected', expected), ('actual', actual)):
                path = _owned(root, path)
                digest, identity = _hash(path)
                _require(path.stat().st_nlink == 1 and type(check[role]['bytes']) is int
                         and identity[2] == check[role]['bytes'] == required
                         and digest == check[role]['sha256'], 'sample retention raw bytes changed or are aliased')
                selected.append((path, identity))
                files.append({'role': role, 'path': str(path.relative_to(root)), 'bytes': required, 'sha256': digest})
            _require(files[0]['sha256'] == files[1]['sha256'], 'sample retention samples are not identical')
            images.append({'trial': index, 'source': check['source'], 'staged_source': source,
                           'step': check['step'], 'geometry': geometry, 'files': files})
    _require(len({p for p, _ in selected}) == len(selected), 'sample retention files are repeated')
    return {'schema_version': 1, 'kind': 'verified-gui-sample-retention', 'state': 'verified',
            'case_directory': str(root), 'directory_sync': os.name == 'posix', 'regression_sha256': report_sha, 'provenance': provenance, 'executions': executions,
            'images': images, 'removed': [],
            'limits': ['Caller must have reaped the application and all renderers before pruning and prevent concurrent writers.',
                       'Only listed verified raw samples are removed. JSON, logs, PNGs, originals and baselines are retained.',
                       'Receipt files are fsynced. Directory entries are also fsynced on POSIX. No cross-platform crash-durability guarantee.']}, selected


def _write(path, value, *, exclusive=False):
    with path.open('x' if exclusive else 'w') as stream:
        json.dump(value, stream, indent=2, allow_nan=False)
        stream.write('\n')
        stream.flush()
        os.fsync(stream.fileno())


def _sync_directory(directory):
    if os.name == 'posix':
        descriptor = os.open(directory, os.O_RDONLY | getattr(os, 'O_DIRECTORY', 0))
        try:
            os.fsync(descriptor)
        finally:
            os.close(descriptor)


def prune(case_directory):
    """Prune one completed case after all its producers exit, retaining a receipt.

    Validation errors leave every sample intact. A partial unlink failure raises and
    retains its receipt. An existing receipt is never resumed or overwritten.
    """
    root = Path(case_directory).absolute()
    _require(root.is_dir() and root.resolve(strict=True) == root and not root.is_symlink(),
             'sample retention case must be an ordinary resolved directory')
    receipt_path = root/'sample-retention.json'
    for path in (receipt_path, root/'sample-retention.json.tmp'):
        _require(not path.exists() and not path.is_symlink(), 'sample retention receipt already exists')
    try:
        receipt, selected = _plan(root)
    except (KeyError, TypeError, AttributeError, IndexError) as error:
        raise ValueError('sample retention evidence is malformed: '+str(error)) from error
    _write(receipt_path, receipt, exclusive=True)
    _sync_directory(root)
    try:
        # Check the complete set again before removing its first member.
        for path, identity in selected:
            _require(_owned(root, path) == path and _identity(path) == identity,
                     'sample retention file changed before removal')
        for path, _ in selected:
            path.unlink()
            receipt['removed'].append(str(path.relative_to(root)))
        receipt['state'] = 'complete'
    except BaseException as error:
        receipt.update(state='partial', failure=f'{type(error).__name__}: {error}')
        raise
    finally:
        temporary = root/'sample-retention.json.tmp'
        _write(temporary, receipt, exclusive=True)
        temporary.replace(receipt_path)
        _sync_directory(root)
    return receipt

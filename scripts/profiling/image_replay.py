"""Validate captured image identity and compare its pixels with independent fresh graphs."""

import json
from pathlib import Path
import subprocess
import uuid

from results import IMAGE_GEOMETRY_FIELDS, compare_images
from watch_workloads import require_quiet
from process_memory import run_captured


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


def _quiet(cohort):
    return require_quiet() if cohort is None else cohort.check_quiet()


def _callbacks(cohort):
    return {} if cohort is None else {'before_start': cohort.check_alive, 'on_poll': cohort.check_alive}


def _batch(binary, prepared, directory, max_memory_mb, cohort):
    batch_id = uuid.uuid4().hex
    requests = [{'id': f'photo-{index:02}', 'source': item['source'],
                 'settings_snapshot': str(item['snapshot_path'].resolve()),
                 'golden': str(item['expected_file'].resolve()),
                 'events': str(item['events_path'].resolve()),
                 'terminal': str((directory/f'fresh-{index:02}-terminal.json').resolve())}
                for index, item in enumerate(prepared)]
    manifest = {'schema_version': 1, 'id': batch_id, 'requests': requests}
    manifest_path = directory/'fresh-batch.json'
    with manifest_path.open('x') as stream:
        stream.write(json.dumps(manifest, indent=2)+'\n')
    failure = 'batch did not reach a terminal result'
    try:
        _quiet(cohort)
        started = []
        result = run_captured([str(binary), '--verify-batch', str(manifest_path.resolve())],
                             limit_bytes=None if max_memory_mb is None else max_memory_mb*1024*1024,
                             samples=directory/'fresh-batch.memory.jsonl', on_started=started.append, **_callbacks(cohort))
        (directory/'fresh-batch.stdout.jsonl').write_text(result.stdout)
        (directory/'fresh-batch.stderr').write_text(result.stderr)
        if result.returncode:
            raise ValueError(f'fresh verification batch exited with status {result.returncode}; inspect stderr')
        summary = [json.loads(line) for line in result.stdout.splitlines()]
        if (len(summary) != 1 or summary[0].get('event') != 'verification_batch'
                or summary[0].get('data') != {'id': batch_id, 'requests': len(requests),
                    'pid': started[0] if len(started) == 1 else None, 'passed': True, 'timing_valid': False}):
            raise ValueError('fresh verification batch terminal summary is missing or mismatched')
        receipts = []
        for index, request in enumerate(requests):
            receipt = json.loads(Path(request['terminal']).read_text())
            expected = {'batch_id': batch_id, 'request_id': request['id'], 'index': index,
                        'count': len(requests), 'source': request['source'],
                        'settings_snapshot': request['settings_snapshot'], 'golden': request['golden'],
                        'events': request['events'], 'execution': 'fresh', 'source_reopened': True,
                        'fresh_executor': True, 'timing_valid': False, 'passed': True, 'outcome': 'passed'}
            if any(receipt.get(key) != value for key, value in expected.items()):
                raise ValueError('fresh verification receipt does not match its declared request')
            receipts.append(receipt)
        if len(started) != 1 or {r['pid'] for r in receipts} != {started[0]}:
            raise ValueError('fresh verification requests did not share one independent process')
        observations = []
        for item, request in zip(prepared, requests, strict=True):
            raw = item['events_path'].read_text()
            markers = [row['data'] for row in map(json.loads, raw.splitlines())
                       if row['event'] == 'verification_request_started']
            data = events(raw)
            if (markers != [{'id': request['id'], 'pid': started[0]}]
                    or any(data[name].get('source') != request['source'] for name in data)
                    or data['processing'].get('execution') != 'fresh'):
                raise ValueError('fresh verification events do not match their declared request and process')
            observations.append(data)
        return observations
    except BaseException as error:
        failure = f'{type(error).__name__}: {error}'
        raise
    finally:
        # A killed verifier cannot write its own remaining terminals. Preserve
        # missing requests explicitly without turning partial output into a pass.
        for index, request in enumerate(requests):
            path = Path(request['terminal'])
            if not path.exists():
                with path.open('x') as stream:
                    stream.write(json.dumps({'schema_version': 1, 'batch_id': batch_id,
                        'request_id': request['id'], 'index': index, 'count': len(requests),
                        'source': request['source'],
                        'outcome': 'aborted' if Path(request['events']).exists() else 'unrun', 'passed': False,
                        'failure': failure, 'timing_valid': False}, indent=2)+'\n')


def _compare(item, data, native_result, final_action):
    capture, photo = item['capture'], item['photo']
    if (data['session']['settings'] != capture['settings']['settings']
            or data['processing']['effective_scale_divisor'] != photo['scale_divisor']
            or data['session']['adapter'] != native_result['hardware']['adapter']):
        raise ValueError('fresh render used different settings, scale or GPU adapter')
    expected_geometry = {key: data['validation'][key] for key in IMAGE_GEOMETRY_FIELDS}
    actual_geometry = {key: capture[key] for key in IMAGE_GEOMETRY_FIELDS}
    return compare_images(item['record']['source'], final_action, item['expected_file'], expected_geometry,
                          item['actual_file'], actual_geometry)


def replay_images(native_result, inputs, photos, final_action, binary, directory,
                  max_memory_mb=None, *, cohort=None, batch=False):
    """Compare checked final photos with their captures and fresh independent renders."""
    captures = native_result.get('image_captures', [])
    declared = {item['staged_source']: item for item in inputs}
    if len(captures) != len(inputs) or len({c['source'] for c in captures}) != len(inputs):
        raise ValueError('captured photo identities are missing or repeated')
    prepared = []
    for index, capture in enumerate(captures):
        source = capture['source']
        if source not in photos or source not in declared:
            raise ValueError('an image capture references an undeclared photo')
        photo, record = photos[source], declared[source]
        if (capture['task'], capture['revision']) != (photo['task'], photo['revision']):
            raise ValueError('an image capture is not the settled final frame')
        snapshot = capture['settings']
        if snapshot['working_scale'] != photo['scale_divisor']:
            raise ValueError('captured image settings do not match the checked edited property and scale')
        actual_file = directory/'evidence'/f'image-{index:02}.f32'
        if Path(capture['file']) != actual_file:
            raise ValueError('image capture path does not match its owned evidence file')
        snapshot_path = directory/f'fresh-{index:02}-settings.json'
        snapshot_path.write_text(json.dumps(snapshot, indent=2)+'\n')
        prepared.append({'source': source, 'capture': capture, 'photo': photo, 'record': record,
                         'snapshot_path': snapshot_path, 'actual_file': actual_file,
                         'golden_dir': directory/f'fresh-{index:02}-samples',
                         'expected_file': directory/f'fresh-{index:02}-samples'/(Path(source).stem+'.f32'),
                         'events_path': directory/f'fresh-{index:02}.jsonl'})
    if batch:
        observations = _batch(binary, prepared, directory, max_memory_mb, cohort)
    else:
        assertions = []
        for index, item in enumerate(prepared):
            _quiet(cohort)
            command = [str(binary), item['source'], str(directory/f'fresh-{index:02}'), '--warm', '0',
                       '--settings-snapshot', str(item['snapshot_path']), '--goldens', str(item['golden_dir'])]
            fresh = run_captured(command,
                limit_bytes=None if max_memory_mb is None else max_memory_mb*1024*1024,
                samples=directory/f'fresh-{index:02}.memory.jsonl', **_callbacks(cohort))
            item['events_path'].write_text(fresh.stdout)
            (directory/f'fresh-{index:02}.stderr').write_text(fresh.stderr)
            if fresh.returncode:
                raise ValueError(f'fresh render {index} exited with status {fresh.returncode}; inspect stderr')
            assertions.append(_compare(item, events(fresh.stdout), native_result, final_action))
        return assertions
    return [_compare(item, data, native_result, final_action)
            for item, data in zip(prepared, observations, strict=True)]

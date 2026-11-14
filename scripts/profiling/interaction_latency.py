"""Describe comparable application workloads and summarize observed UI timing."""

import hashlib
import json
import math


def _positive_number(value):
    return type(value) in (int, float) and math.isfinite(value) and value > 0


def geometry_identity(native_result):
    """Return complete positive geometry, or None when timing identity is incomplete."""
    if not isinstance(native_result, dict):
        return None
    geometry = {}
    for key in ('window_points', 'viewport_points'):
        value = native_result.get(key)
        if not isinstance(value, list) or len(value) != 2 or not all(map(_positive_number, value)):
            return None
        geometry[key] = value[:]
    scale = native_result.get('pixels_per_point')
    if not _positive_number(scale):
        return None
    geometry['pixels_per_point'] = scale
    return geometry


def _distribution(sample_count, gaps):
    ordered = sorted(gaps)
    return {'sample_count': sample_count, 'gap_count': len(gaps),
            'max_ms': ordered[-1] if ordered else None,
            'p95_ms': ordered[math.ceil(len(ordered) * 0.95) - 1] if ordered else None}


def summarize_ui_ticks(samples, max_ui_gap_ms=None):
    """Summarize consecutive gaps during the active measurement window.

    Ticks describe UI update cadence, not physical input or screen presentation.
    """
    if max_ui_gap_ms is not None and not _positive_number(max_ui_gap_ms):
        raise ValueError('max_ui_gap_ms must be finite and positive')
    actions = {}
    all_gaps = []
    previous = None
    invalid_count = 0
    for sample in samples:
        elapsed = sample.get('elapsed_ms')
        action = sample.get('action')
        if (type(elapsed) not in (int, float) or not math.isfinite(elapsed) or elapsed < 0
                or not isinstance(action, str) or not action
                or type(sample.get('worker_busy')) is not bool):
            invalid_count += 1
            previous = None
            continue
        entry = actions.setdefault(action, {'count': 0, 'gaps': []})
        entry['count'] += 1
        if previous is not None:
            previous_elapsed, previous_action = previous
            if elapsed < previous_elapsed:
                invalid_count += 1
            else:
                gap = elapsed - previous_elapsed
                actions[previous_action]['gaps'].append(gap)
                all_gaps.append(gap)
        previous = (elapsed, action)
    summaries = {action: _distribution(entry['count'], entry['gaps'])
                 for action, entry in actions.items()}
    overall = _distribution(sum(entry['count'] for entry in actions.values()), all_gaps)
    complete = bool(summaries) and all(row['gap_count'] for row in summaries.values()) and not invalid_count
    assertion = None
    if max_ui_gap_ms is not None:
        actual = overall['max_ms']
        outcome = ('unverified' if not complete else
                   'passed' if actual <= max_ui_gap_ms else 'failed')
        assertion = {'name': 'max_ui_gap_ms', 'expected': max_ui_gap_ms,
                     'actual': actual, 'outcome': outcome}
    return {'overall': overall, 'actions': summaries, 'invalid_count': invalid_count,
            'outcome': 'complete' if complete else 'incomplete', 'assertion': assertion}


def value_hash(value):
    return hashlib.sha256(json.dumps(value,sort_keys=True,separators=(',',':'),allow_nan=False).encode()).hexdigest()

def matched_workload(manifest, native_result):
    """Fingerprint stable work, excluding staged paths, revisions and build identity."""
    initial=next((step for step in native_result['steps'] if step['action']=='initial-selection'),{})
    current={photo['source']:photo for photo in initial.get('photos',[])}
    inputs=[]
    for record in manifest['inputs']:
        photo=current.get(record['staged_source'])
        inputs.append({'sha256':record['sha256'],'declared_settings_sha256':value_hash(record['settings']),
                       'first_current_settings_sha256':None if photo is None else value_hash(photo['settings']),
                       'first_current_scale':None if photo is None else photo['scale_divisor'],
                       'first_current_processing_quality':None if photo is None else photo.get('processing_quality')})
    scenario=manifest['scenario']
    disk=native_result.get('disk_cache')
    expected=scenario['disk_cache_bytes']
    expected_memory=scenario['pressure_bytes'] if scenario['name'] in ('memory-pressure','selection-pressure') else scenario['memory_cache_bytes']
    cache_policy_matches=(isinstance(disk,dict) and disk.get('max_bytes')==expected
                          and disk.get('enabled') is (expected>0)
                          and native_result.get('memory_cache_bytes')==expected_memory)
    geometry = geometry_identity(native_result.get('geometry', {}))
    step_geometry = [{'action':step['action'], 'geometry':geometry_identity(step.get('geometry', {}))}
                     for step in native_result['steps']]
    geometry_complete = geometry is not None and bool(step_geometry) and all(row['geometry'] for row in step_geometry)
    descriptor={'scenario':{key:scenario[key] for key in
                           ('name','photo_count','quality','repeats','rapid_edits','pressure_bytes','disk_cache_bytes','memory_cache_bytes')},
                'inputs':inputs,'hardware':{'host':manifest['hardware'],'application':native_result.get('hardware')},
                'application_disk_cache':disk,
                'application_memory_cache_bytes':native_result.get('memory_cache_bytes'),
                'cache_policy_matches':cache_policy_matches,
                'geometry':geometry, 'step_geometry':step_geometry, 'geometry_complete':geometry_complete,
                'first_current_complete':geometry_complete and cache_policy_matches and len(current)==len(inputs) and all(row['first_current_settings_sha256'] and row['first_current_processing_quality']==scenario['quality'] for row in inputs),
                'measurement':manifest['measurement']}
    if scenario['name'] == 'cache-workflow':
        descriptor['scenario']['native_photo_count'] = scenario.get('native_photo_count', 1)
        workflow = native_result.get('cache_workflow') or {}
        descriptor['cache_workflow_protocol'] = [
            {key:row[key] for key in ('action', 'delta') if key in row}
            for row in workflow.get('rows', []) if row.get('event') == 'action'
        ]
    if scenario['name']=='slider-drag':
        descriptor['scenario'].update({key:scenario[key] for key in ('control_pointer','drag_steps','drag_fraction','drag_pattern','light_theme','mixed_values','editing_section')})
    descriptor['scenario'].update(folder_mode=scenario.get('folder_mode', 'import'),
                                  exit_with_pending=scenario.get('exit_with_pending', False))
    if native_result.get('journey'):
        journey = native_result['journey']
        descriptor['journey_commands'] = [{key:event[key] for key in ('action','delta','photo','alt','shift') if key in event}
                                          for event in journey['events'] if event.get('action') in ('exposure','click')]
        descriptor['journey_final_exposures'] = journey['expected']['exposures']
    if native_result.get('background'):
        descriptor['background_command'] = native_result['background']['command']
        descriptor['background_processing'] = (native_result['background']['background_at_edit'] or {}).get('processing')
    # Off and traced captures describe the same work, but only Off may be compared.
    descriptor['measurement']={key:value for key,value in descriptor['measurement'].items() if key!='input_kind'}
    if scenario['name'] == 'transition-journey':
        descriptor['scenario'].update(seed=scenario['seed'], action_count=scenario['action_count'])
    return {'sha256':value_hash(descriptor),'descriptor':descriptor}

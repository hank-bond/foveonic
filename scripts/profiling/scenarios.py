"""Run declared real-app diagnostic scenarios with isolated inputs and shared Perfetto artifacts."""

import argparse
from contextlib import nullcontext
import json
import os
from pathlib import Path
import platform
import subprocess
import struct
import re
import uuid

from native import recording_environment, source_tree_hash, workload_monitor
from trace_analysis import analyze_capture, distribution
from watch_workloads import require_quiet, concurrent_workloads
from file_hash import digest
from results import PROCESSING_QUALITIES, include_assertions, render_result
from interaction_latency import matched_workload, summarize_ui_ticks
from inputs import stage_inputs
from scenario_checks import checked_scenario_images, pointer_value, same_numeric, validate_choice_recipes, check_cache_workflow
from image_replay import replay_images
from cache_workflow import associate_trace
from process_memory import wait_guarded

NAMES = ('initial', 'warm-disk', 'prefetched-selection', 'warm-edit', 'cached-switch', 'rapid-supersession', 'memory-pressure', 'slider-drag', 'selection-journey', 'selection-pressure', 'background-edit', 'transition-journey', 'lifecycle-journey', 'browser-journey', 'collection-journey', 'choice-journey', 'source-loading', 'cache-workflow')


def selected_cases(cases, count, quality, scenario):
    if quality not in PROCESSING_QUALITIES:
        raise ValueError('quality must be quality or speed')
    if not 1 <= count <= 8 or len(cases) < count:
        raise ValueError('supply one through eight declared photographs for the requested count')
    if scenario in ('cached-switch', 'source-loading') and count < 2:
        raise ValueError(f'{scenario} needs at least two photographs')
    if scenario == 'slider-drag' and count < 2:
        raise ValueError('a mixed-value slider drag needs at least two photographs')
    if scenario in ('selection-journey', 'selection-pressure', 'background-edit', 'transition-journey', 'lifecycle-journey', 'browser-journey', 'choice-journey') and count < 2:
        raise ValueError('a selection journey needs at least two photographs')
    if scenario == 'cache-workflow' and count != 2:
        raise ValueError('cache-workflow needs exactly two photographs')
    if scenario == 'collection-journey' and count != 8:
        raise ValueError('collection-journey needs eight originals for its 100-photo folder')
    selected = cases[:count]
    sources = [str(Path(case['source']).resolve(strict=True)) for case in selected]
    if len(set(sources)) != count:
        raise ValueError('scenario photographs must have distinct source paths')
    if any(not Path(source).is_file() for source in sources):
        raise ValueError('scenario sources must be files')
    return [{**case, 'source': source} for case, source in zip(selected, sources)]


def scenario_environment(home, evidence, manifest):
    env = recording_environment(home, 'off')
    for key in list(env):
        if key.startswith(('FOVEONIC_GUI_SCENARIO_', 'FOVEONIC_DIAGNOSTICS_')):
            del env[key]
    env.update(FOVEONIC_GUI_SMOKE_DIR=str(evidence), FOVEONIC_GUI_SCENARIO_MANIFEST=str(manifest))
    return env


def finish_trial(directory, mode, trace_processor, capture_directory=None):
    result_path = directory / 'evidence' / 'scenario-result.json'
    if not result_path.exists():
        raise RuntimeError('application exited without a scenario result; inspect owned stdout/stderr')
    result = json.loads(result_path.read_text())
    if mode == 'off':
        return {'passed': result['passed'], 'mode':'off', 'input_kind':'baseline',
                'steps':result['steps'], 'result':str(result_path)}
    capture = capture_directory or directory / 'capture'
    analysis = analyze_capture(capture, trace_processor)
    summary_path = capture / 'summary.json'
    summary = json.loads(summary_path.read_text())
    summary['scenario'] = result
    summary_path.write_text(json.dumps(summary, indent=2) + '\n')
    manifest_path = capture / 'manifest.json'
    manifest = json.loads(manifest_path.read_text())
    manifest['application']['scenario_result'] = {'path':str(result_path),'sha256':digest(result_path)}
    manifest_path.write_text(json.dumps(manifest, indent=2) + '\n')
    complete = analysis['validity']['valid']
    behavior = analysis['behavior_valid']
    expected_steps = [(step['step'], step['action'], len(step['photos'])) for step in result['steps']]
    traced_steps = [(step['step'], step['action'], step['photo_count']) for step in analysis['steps']]
    steps_match = bool(expected_steps) and expected_steps == traced_steps
    trace_assertions = [
        {'name':'trace_integrity','expected':True,'actual':analysis['validity'],
         'outcome':'passed' if complete else 'unverified'},
        {'name':'trace_behavior','expected':True,'actual':behavior,
         'outcome':('passed' if behavior is True else 'failed' if behavior is False else 'unverified') if complete else 'unverified',
         'failure':'; '.join(analysis.get('behavior_failures', [])) or None},
        {'name':'trace_steps','expected':expected_steps,'actual':traced_steps,
         'outcome':('passed' if steps_match else 'failed') if complete else 'unverified',
         'failure':None if steps_match else 'trace steps do not match the completed application steps'},
    ]
    return {'passed':result['passed'] and complete and behavior is True and steps_match,
            'mode':mode, 'input_kind':'perfetto', 'steps':analysis['steps'],
            'capture':str(capture), 'result':str(result_path), 'trace_assertions':trace_assertions}


def application_result(native_result, scenario, quality, inputs, binary_sha256=None):
    sources = {item['staged_source']:item for item in inputs}
    observations = []
    missing = []
    for step in native_result['steps']:
        for photo in step.get('photos', []):
            source = sources.get(photo['source'])
            if source is None:
                missing.append(photo['source'])
            requested_quality = (photo.get('requested_processing_quality')
                                 if scenario in ('transition-journey','cache-workflow') and step['action'] != 'initial-selection'
                                 else quality)
            observations.append({'step':step['action'], 'source':source['source'] if source else photo['source'],
                                 'source_sha256':source['sha256'] if source else None,
                                 'family':photo.get('family'), 'scale_divisor':photo.get('scale_divisor'),
                                 **{key:photo[key] for key in ('processing_quality', 'display_density', 'display_size_points', 'native_extent', 'pixels_per_point') if key in photo},
                                 'requested_processing_quality':requested_quality})
    failure = native_result.get('failure')
    if missing:
        failure = f'output references undeclared staged inputs: {missing}'
    environment = {'binary_sha256':binary_sha256, 'adapter':native_result.get('hardware', {}).get('adapter')}
    return render_result('application', scenario, quality, observations,
                         native_result['passed'] and not missing, failure, environment)


def prefetched_assertions(native_result, inputs, repeats):
    """Check every prepared and warm result against declared exposure recipes."""
    steps = native_result.get('steps', [])
    expected_actions = ['initial-selection', 'prepare-demand-grid'] + ['warm-edit'] * repeats
    actual_actions = [step.get('action') for step in steps]
    valid = actual_actions == expected_actions
    expected = {item['staged_source']: struct.unpack('f', struct.pack('f', item['settings']['exposure']))[0]
                for item in inputs}
    for index, step in enumerate(steps):
        if index >= len(expected_actions):
            valid = False
            break
        if step.get('step') != index + 1:
            valid = False
        delta = 0.125 if index == 1 else (0.25 if index % 2 == 0 else -0.25) if index >= 2 else 0.
        if index == 1 and step.get('exposure_delta') != delta:
            valid = False
        expected = {path: struct.unpack('f', struct.pack('f', max(-4., min(4., value + delta))))[0]
                    for path, value in expected.items()}
        photos = {photo['source']: photo for photo in step.get('photos', [])}
        if len(step.get('photos', [])) != len(expected) or set(photos) != set(expected):
            valid = False
            continue
        for path, value in expected.items():
            settings = photos[path].get('settings', {})
            if settings.get('exposure_auto') is not False or not same_numeric(settings.get('exposure'), value):
                valid = False
    return [{'name':'prefetched_edits', 'outcome':'passed' if valid else 'failed',
             'expected':expected_actions, 'actual':actual_actions,
             'failure':None if valid else 'prepared or warm steps differ from declared exposure edits'}]


def selection_setup_assertions(native_result, inputs, scenario):
    """Verify preparation against the declared inputs and the complete journey sequence."""
    actions = ['initial-selection', 'prepare-demand-grid', 'selection-journey', 'cached-switch']
    actions += (['cached-switch'] if scenario == 'selection-journey'
                else ['cache-eviction', 'evicted-edit'])
    actions += ['restore-selection']
    steps = native_result.get('steps', [])
    actual = [step.get('action') for step in steps]
    valid = actual == actions and all(step.get('step') == index + 1 for index, step in enumerate(steps))
    declared = {item['staged_source']: item['settings']['exposure'] for item in inputs}
    for index, step in enumerate(steps[:2]):
        photos = {photo['source']:photo for photo in step.get('photos', [])}
        if len(step.get('photos', [])) != len(declared) or set(photos) != set(declared):
            valid = False
            continue
        if index == 1 and step.get('exposure_delta') != 0.125:
            valid = False
        for path, value in declared.items():
            original = struct.unpack('f', struct.pack('f', value))[0]
            expected = struct.unpack('f', struct.pack('f', min(4., original + (0.125 if index else 0.))))[0]
            settings = photos[path].get('settings', {})
            if settings.get('exposure_auto') is not False or not same_numeric(settings.get('exposure'), expected):
                valid = False
    return [{'name':'selection_grid_preparation', 'outcome':'passed' if valid else 'failed',
             'expected':actions, 'actual':actual,
             'failure':None if valid else 'selection preparation or action sequence differs from declared inputs'}]


def check_export(image, expected_extent, decoded):
    """Decode an actual exported image with macOS's image reader after the app exits."""
    assertion = {'name':'export_decode', 'expected':expected_extent, 'actual':None, 'outcome':'failed'}
    if (not isinstance(expected_extent, list) or len(expected_extent) != 2
            or any(type(value) is not int or value <= 0 for value in expected_extent)):
        assertion.update(outcome='unverified', failure='missing expected export dimensions')
        return assertion
    if not image.is_file():
        assertion['failure'] = 'the batch did not produce its expected TIFF'
        return assertion
    if platform.system() != 'Darwin':
        assertion.update(outcome='unverified', failure='export decoding needs the macOS image reader')
        return assertion
    result = subprocess.run(['/usr/bin/sips', '-s', 'format', 'png', str(image), '--out', str(decoded)],
                            capture_output=True, text=True)
    if result.returncode:
        assertion['failure'] = 'export could not be decoded: ' + result.stderr
        return assertion
    try:
        with decoded.open('rb') as stream:
            header = stream.read(24)
        if len(header) != 24 or header[:8] != b'\x89PNG\r\n\x1a\n' or header[12:16] != b'IHDR':
            raise ValueError('decoder did not produce a PNG image')
        assertion['actual'] = list(struct.unpack('>II', header[16:24]))
        assertion['outcome'] = 'passed' if assertion['actual'] == expected_extent else 'failed'
    except (OSError, ValueError) as error:
        assertion['failure'] = str(error)
    return assertion


def validate_shutdown(args, selected):
    folder_mode = getattr(args, 'folder_mode', 'import')
    if args.scenario == 'background-edit' and folder_mode != 'import':
        raise ValueError('background-edit requires import mode')
    if not getattr(args, 'exit_with_pending', False):
        return
    if (folder_mode != 'browse' or not getattr(args, 'save_on_exit', False)
            or args.mode == 'off' or getattr(args, 'compare_images', None)
            or args.scenario not in ('initial', 'warm-disk') or not 2 <= len(selected) <= 8):
        raise ValueError('exit-with-pending requires browse, save-on-exit, two to eight photos and initial or warm-disk')
    for case in selected:
        settings = case.get('settings')
        value = settings.get('exposure') if isinstance(settings, dict) else None
        if (not isinstance(settings, dict) or settings.get('exposure_auto') is not False
                or type(value) not in (int, float) or not -4 <= value <= 3.75):
            raise ValueError('exit-with-pending requires manual exposures between -4 and 3.75')


def shutdown_assertions(native_result, inputs):
    """Check queued edit evidence and independently read saved owned recipes after exit."""
    pending = native_result.get('shutdown_pending') or {}
    before, after = pending.get('requested_revision_before'), pending.get('requested_revision_after')
    photos = pending.get('photos')
    expected_paths = {item['staged_source'] for item in inputs}
    proof = (type(before) is int and type(after) is int and after > before
             and pending.get('delta') == 0.25 and isinstance(photos, list)
             and len(photos) == len(inputs)
             and {row.get('source') for row in photos if isinstance(row, dict)} == expected_paths)
    rows = {row['source']:row for row in photos if isinstance(row, dict) and 'source' in row} if isinstance(photos, list) else {}
    assertions = []
    for item in inputs:
        initial = struct.unpack('f', struct.pack('f', item['settings']['exposure']))[0]
        expected = struct.unpack('f', struct.pack('f', initial + 0.25))[0]
        row = rows.get(item['staged_source'], {})
        proof = bool(proof and same_numeric(row.get('initial_exposure'), initial)
                     and same_numeric(row.get('expected_exposure'), expected)
                     and type(row.get('old_presented_revision')) is int
                     and row['old_presented_revision'] < after
                     and type(row.get('old_task_revision')) is int
                     and row['old_task_revision'] < after)
        source = Path(item['staged_source'])
        sidecar = source.parent/'foveonic'/(source.name+'.foveonic.json')
        actual, failure = None, None
        try:
            envelope = json.loads(sidecar.read_text())
            settings = envelope['settings']
            if envelope.get('format') != 'foveonic-editing' or envelope.get('version') != 1 or settings.get('exposure_auto') is not False:
                raise ValueError('invalid sidecar envelope or automatic exposure')
            actual = settings.get('exposure')
            passed = same_numeric(actual, expected)
        except (OSError, ValueError, KeyError, TypeError, json.JSONDecodeError, OverflowError) as error:
            passed = False
            failure = str(error)
        assertions.append({'name':'shutdown_saved_exposure','source':item['source'],
                           'expected':expected,'actual':actual,'outcome':'passed' if passed else 'failed','failure':failure})
    assertions.insert(0, {'name':'shutdown_pending_proof','expected':'all requested edits advanced beyond the presented frames',
                          'actual':pending,'outcome':'passed' if proof else 'failed'})
    return assertions


def compare_scenario_images(native_result, inputs, binary, directory, control_pointer='/exposure', native_photo_count=1, max_memory_mb=None):
    """Check the scenario's intended edits, then replay its captured final images."""
    action, photos = checked_scenario_images(native_result, inputs, directory, control_pointer, native_photo_count)
    return replay_images(native_result, inputs, photos, action, binary, directory, max_memory_mb)


def prepare_trial(args, selected, directory, source_hash):
    directory.mkdir(parents=True, exist_ok=False)
    home, evidence = directory / 'home', directory / 'evidence'
    home.mkdir(); evidence.mkdir()
    if args.scenario == 'background-edit':
        (directory/'background-output').mkdir()
    inputs = stage_inputs(selected, directory / 'inputs')
    manifest = {
        'scenario': {'name':args.scenario,'photo_count':args.count,'quality':args.quality,
                     'mode':args.mode,'directory':str(directory/'inputs'),
                     'sources':[item['staged_source'] for item in inputs],
                     'capture_directory':str(directory/'capture'),'repeats':args.repeats,
                     'rapid_edits':args.rapid_edits,'pressure_bytes':args.pressure_mib*1024*1024,
                     'disk_cache_bytes':args.disk_cache_bytes,'memory_cache_bytes':args.memory_cache_bytes,
                     'timeout_seconds':args.step_timeout},
        'binary':str(args.binary),'binary_sha256':digest(args.binary),
        'source_tree_sha256':source_hash,'inputs':inputs,
        'hardware':{'system':platform.system(),'release':platform.release(),'machine':platform.machine(),'logical_cpus':os.cpu_count()},
        'cache_namespace':getattr(args,'cache_namespace','portable-test'),
        'measurement':{'input_kind':'baseline' if args.mode=='off' else 'perfetto',
                       'window_mode':'background' if getattr(args,'background',False) else 'foreground',
                       'coverage':'real application commands and egui input dispatch; not physical pointer injection',
                       'setup':'one active photo and GPU device before measured selection; processing reset before group; enabled disk cache primed except initial',
                       'physical_memory_guard':({'limit_bytes':args.max_memory_mb * 1024 * 1024,
                           'sample_interval_ms':100,'metric':'macOS proc_pid_rusage physical footprint'}
                           if getattr(args,'max_memory_mb',None) is not None else None),
                       'os_file_cache':'uncontrolled; input hashing warms filesystem cache',
                       'scenario_poll_ms':20,'numerical_equality':'verified separately by maintained headless cases',
                       'capture_images_after_scenario':bool(getattr(args, 'compare_images', None))},
    }
    if args.scenario == 'cache-workflow':
        manifest['measurement']['setup'] = (f'initial {args.quality} selection starts with cold processing; disk may be primed. '
            f"Automatic Fit sizing follows that selection, then {getattr(args, 'native_photo_count', None) or 1} native-detail viewport(s) and warm edits")
    manifest['measurement']['controlled_delivery'] = args.scenario in ('selection-journey','selection-pressure','transition-journey','source-loading')
    if args.scenario == 'cache-workflow':
        manifest['scenario']['native_photo_count'] = getattr(args, 'native_photo_count', None) or 1
    manifest['scenario']['capture_images'] = bool(getattr(args, 'compare_images', None))
    manifest['scenario']['save_on_exit'] = bool(getattr(args, 'save_on_exit', False))
    manifest['scenario']['folder_mode'] = getattr(args, 'folder_mode', 'import')
    manifest['scenario']['exit_with_pending'] = bool(getattr(args, 'exit_with_pending', False))
    if args.scenario == 'transition-journey':
        manifest['scenario'].update(seed=args.seed, action_count=args.action_count)
    if args.scenario == 'slider-drag':
        manifest['scenario'].update(control_pointer=args.control_pointer, drag_steps=args.drag_steps,
                                    drag_fraction=args.drag_fraction, drag_pattern=args.drag_pattern, light_theme=args.theme=='light', mixed_values=args.initial_values=='mixed', editing_section=args.editing_section)
    launch = directory / 'launch.json'
    launch.write_text(json.dumps(manifest, indent=2) + '\n')
    return {'directory':directory, 'home':home, 'evidence':evidence,
            'inputs':inputs, 'manifest':manifest, 'launch':launch}


def run_trial(args, selected, directory, source_hash):
    prepared = prepare_trial(args, selected, directory, source_hash)
    home, evidence = prepared['home'], prepared['evidence']
    inputs, manifest, launch = prepared['inputs'], prepared['manifest'], prepared['launch']
    env = scenario_environment(home, evidence, launch)
    if manifest['scenario']['save_on_exit']:
        env['FOVEONIC_DIAGNOSTICS_DIR'] = str(directory/'capture')
    if getattr(args,'background',False):
        env['FOVEONIC_GUI_SCENARIO_BACKGROUND'] = '1'
    env['FOVEONIC_DIAGNOSTICS_CACHE_NAMESPACE'] = manifest['cache_namespace']
    monitor = nullcontext() if args.no_monitor else workload_monitor(directory / 'workloads.jsonl')
    with monitor, (directory/'stdout.txt').open('x') as stdout, (directory/'stderr.txt').open('x') as stderr:
        manifest['preflight'] = require_quiet()
        launch.write_text(json.dumps(manifest, indent=2) + '\n')
        process = subprocess.Popen([str(args.binary), inputs[0]['staged_source']], env=env,
                                   stdout=stdout, stderr=stderr, start_new_session=True)
        try:
            memory_limit = getattr(args, 'max_memory_mb', None)
            returncode = (process.wait(timeout=args.timeout) if memory_limit is None else
                          wait_guarded(process, args.timeout, memory_limit * 1024 * 1024,
                                       directory/'physical-memory.jsonl'))
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill(); process.wait()
    if returncode:
        raise RuntimeError(f'owned scenario application exited with status {returncode}; inspect {directory}')
    contention = ({'checked':False,'uncontended':None,'samples':0,'other_workloads':[]}
                  if args.no_monitor else concurrent_workloads(directory/'workloads.jsonl', owned_pids=[process.pid]))
    timing_valid = contention['checked'] and contention['uncontended']
    manifest.update(contention=contention, timing_valid=timing_valid)
    launch.write_text(json.dumps(manifest, indent=2) + '\n')
    return collect_trial(args, prepared, contention, timing_valid)


def collect_trial(args, prepared, contention, timing_valid, session=None):
    directory, evidence = prepared['directory'], prepared['evidence']
    inputs, manifest, launch = prepared['inputs'], prepared['manifest'], prepared['launch']
    if session is not None:
        timing_valid = False
        manifest['session_reuse'] = session
    capture_directory = directory/'capture'
    if manifest['scenario']['save_on_exit']:
        children = list(capture_directory.iterdir()) if capture_directory.is_dir() else []
        if len(children) != 1 or not children[0].is_dir():
            raise ValueError('orderly exit did not save exactly one owned capture')
        capture_directory = children[0]
    result = finish_trial(directory, args.mode, args.trace_processor, capture_directory)
    native_result=json.loads(Path(result['result']).read_text())
    test_result = application_result(native_result, args.scenario, args.quality, inputs, manifest['binary_sha256'])
    if session is not None:
        include_assertions(test_result, [
            {'name':'slider_session_identity', 'expected':session['expected'], 'actual':native_result.get('session'),
             'outcome':'passed' if native_result.get('session') == session['expected'] else 'failed'},
            {'name':'slider_session_process_exit', 'expected':0, 'actual':session['process_exit'],
             'outcome':'passed' if session['process_exit'] == 0 else 'failed'},
            {'name':'slider_session_scenario', 'expected':manifest['scenario'], 'actual':native_result.get('scenario'),
             'outcome':'passed' if native_result.get('scenario') == manifest['scenario'] else 'failed'},
        ])
    include_assertions(test_result, result.pop('trace_assertions', []))
    if args.scenario == 'prefetched-selection':
        include_assertions(test_result, prefetched_assertions(native_result, inputs, args.repeats))
    if args.scenario in ('selection-journey', 'selection-pressure'):
        include_assertions(test_result, selection_setup_assertions(native_result, inputs, args.scenario))
    if manifest['scenario']['save_on_exit']:
        saved_manifest = json.loads((capture_directory/'manifest.json').read_text())
        actions = saved_manifest.get('application', {}).get('capture_actions', [])
        include_assertions(test_result, [{'name':'capture_saved_on_exit',
            'outcome':'passed' if any(action.get('action') == 'application_exit' for action in actions) else 'failed',
            'expected':'application_exit', 'actual':actions}])
    if manifest['scenario']['exit_with_pending']:
        include_assertions(test_result, shutdown_assertions(native_result, inputs))
    ui_latency = summarize_ui_ticks(native_result.get('ui_samples', []), getattr(args, 'max_ui_gap_ms', None))
    if ui_latency['assertion'] is not None:
        include_assertions(test_result, [ui_latency['assertion']])
    if args.scenario == 'cache-workflow':
        try:
            result['cache_workflow'] = check_cache_workflow(native_result, inputs, manifest['scenario']['native_photo_count'])
            if args.mode != 'off':
                analysis = json.loads((capture_directory/'summary.json').read_text())['analysis']
                result['cache_workflow']['trace'] = associate_trace(native_result['cache_workflow'], analysis)
                include_assertions(test_result, [{'name':'cache_workflow_trace',
                    'outcome':'passed' if result['cache_workflow']['trace']['available'] else 'unverified',
                    'actual':result['cache_workflow']['trace']}])
            assertion = {'name':'cache_workflow_actions', 'outcome':'passed', 'actual':result['cache_workflow']}
        except (ValueError, KeyError, TypeError) as error:
            assertion = {'name':'cache_workflow_actions', 'outcome':'failed', 'failure':str(error)}
        include_assertions(test_result, [assertion])
    if args.scenario == 'background-edit':
        at_edit = (native_result.get('background') or {}).get('background_at_edit') or {}
        overlaps = sum(sample.get('action') == 'background-edit' and sample.get('worker_busy') is True
                       and sample.get('background_running') is True for sample in native_result.get('ui_samples', []))
        complete = (at_edit.get('running') is True and at_edit.get('decisions_saved') is True
                    and (at_edit.get('exposure_control') or {}).get('enabled') is True and overlaps > 0)
        include_assertions(test_result, [{'name':'background_overlap','step':'background-edit','source':None,
            'expected':'a started batch active during foreground work','actual':{'at_edit':at_edit,'ui_samples':overlaps},
            'outcome':'passed' if complete else 'unverified'}])
        include_assertions(test_result, [check_export(
            directory/'background-output'/(Path(inputs[0]['staged_source']).stem+'.tiff'),
            at_edit.get('expected_export_extent'), directory/'export-decoded.png')])
    fresh_binary = getattr(args, 'compare_images', None)
    if fresh_binary and native_result['passed']:
        manifest['fresh_renderer'] = {'path':str(fresh_binary), 'sha256':digest(fresh_binary)}
        test_result['environment']['fresh_binary_sha256'] = manifest['fresh_renderer']['sha256']
        try:
            comparisons = compare_scenario_images(native_result, inputs, fresh_binary, directory, getattr(args, 'control_pointer', '/exposure'),
                                                  manifest['scenario'].get('native_photo_count', 1), getattr(args,'max_memory_mb',None))
        except (OSError, ValueError, KeyError, RuntimeError, json.JSONDecodeError, subprocess.TimeoutExpired) as error:
            comparisons = [{'name':'image_samples', 'step':'slider-drag', 'source':None,
                            'outcome':'failed', 'failure':str(error)}]
        include_assertions(test_result, comparisons)
    test_result_path = evidence/'test-result.json'
    test_result_path.write_text(json.dumps(test_result, indent=2)+'\n')
    workload_key=matched_workload(manifest,native_result)
    timing_valid = timing_valid and workload_key['descriptor']['geometry_complete']
    manifest['timing_valid'] = timing_valid
    result['passed'] = result['passed'] and test_result['outcome']=='passed' and workload_key['descriptor']['cache_policy_matches']
    result['test_result'] = test_result
    result['ui_latency'] = ui_latency
    if session is not None:
        result['measurement_kind'] = 'reused-session-correctness'
        result['session_reuse'] = session
    manifest['workload_key']=workload_key
    launch.write_text(json.dumps(manifest,indent=2)+'\n')
    result.update(contention=contention, timing_valid=timing_valid, workload_key=workload_key,
                  artifacts={'launch':str(launch),'result':result['result'],'result_sha256':digest(result['result']),
                             'binary':manifest['binary'],'binary_sha256':manifest['binary_sha256'],
                             'source_tree_sha256':manifest['source_tree_sha256']})
    if 'fresh_renderer' in manifest:
        result['artifacts']['fresh_renderer'] = manifest['fresh_renderer']
    for name in ('slider-drag.json', 'slider-observation.json', 'slider-failure.png'):
        path=evidence/name
        if path.exists():
            result['artifacts'][name]={'path':str(path),'sha256':digest(path)}
    result['artifacts']['test_result']={'path':str(test_result_path),'sha256':digest(test_result_path)}
    if args.mode != 'off':
        summary_path=capture_directory/'summary.json'
        summary=json.loads(summary_path.read_text())
        summary.update(contention=contention, timing_valid=timing_valid, test_result=test_result, ui_latency=ui_latency)
        summary_path.write_text(json.dumps(summary,indent=2)+'\n')
        capture_manifest_path=capture_directory/'manifest.json'
        capture_manifest=json.loads(capture_manifest_path.read_text())
        capture_manifest['application']['test_result']=result['artifacts']['test_result']
        capture_manifest_path.write_text(json.dumps(capture_manifest,indent=2)+'\n')
    return result


def validated_inputs(args):
    if getattr(args, 'max_memory_mb', None) is not None and args.max_memory_mb <= 0:
        raise ValueError('physical-footprint limit must be positive')
    args.binary = args.binary.resolve(strict=True)
    cases = json.loads(args.cases.read_text()) if args.cases else [{'source':str(path)} for path in args.source]
    args.count = len(cases) if args.count is None else args.count
    if getattr(args, 'native_photo_count', None) is not None and args.scenario != 'cache-workflow':
        raise ValueError('--native-photo-count is only available for cache-workflow')
    if args.scenario == 'cache-workflow' and getattr(args, 'native_photo_count', None) is None:
        args.native_photo_count = 1
    if args.scenario == 'cache-workflow' and args.native_photo_count not in (1, 2):
        raise ValueError('cache-workflow native photo count must be 1 or 2')
    selected = selected_cases(cases, args.count, args.quality, args.scenario)
    validate_shutdown(args, selected)
    if getattr(args, 'save_on_exit', False) and (args.mode == 'off' or getattr(args, 'compare_images', None)):
        raise ValueError('save-on-exit needs an enabled capture and no post-scenario image readback')
    if args.disk_cache_bytes < 0 or (args.scenario in ('warm-disk', 'prefetched-selection') and args.disk_cache_bytes == 0):
        raise ValueError('disk cache budget must be nonnegative and the requested cache scenario requires an enabled disk cache')
    if args.scenario == 'prefetched-selection' and (args.count < 2):
        raise ValueError('prefetched-selection requires at least two photographs')
    if args.scenario == 'prefetched-selection' and any(
            not isinstance(case.get('settings'), dict)
            or case['settings'].get('exposure_auto') is not False
            or type(case['settings'].get('exposure')) not in (int, float)
            or not -4 <= case['settings']['exposure'] <= 3.625 for case in selected):
        raise ValueError('prefetched-selection needs manual exposures between -4 and 3.625')
    if args.memory_cache_bytes < 1:
        raise ValueError('memory cache budget must be positive')
    if args.scenario == 'slider-drag' and (not args.control_pointer.startswith('/') or args.drag_steps < 3 or not 0 < args.drag_fraction < 0.45):
        raise ValueError('slider drag needs a setting pointer, at least three steps, and a fraction between 0 and 0.45')
    if args.scenario == 'transition-journey' and (not 0 <= args.seed < 2**64 or not 1 <= args.action_count <= 64):
        raise ValueError('transition seed must be uint64 and action-count must be 1 through 64')
    if args.scenario == 'collection-journey' and any(not isinstance(case.get('settings'), dict) or case['settings'].get('exposure_auto') is not False or type(case['settings'].get('exposure')) not in (int,float) for case in selected):
        raise ValueError('collection-journey needs explicit manual exposures')
    if args.scenario == 'source-loading' and any(not isinstance(case.get('settings'), dict) or case['settings'].get('exposure_auto') is not False or type(case['settings'].get('exposure')) not in (int, float) for case in selected):
        raise ValueError('source-loading needs explicit manual exposures')
    journeys = ('selection-journey','selection-pressure','background-edit','transition-journey','lifecycle-journey','browser-journey','choice-journey','cache-workflow')
    if args.scenario == 'choice-journey':
        validate_choice_recipes(selected)
    if getattr(args, 'compare_images', None) and args.scenario not in ('slider-drag', *journeys):
        raise ValueError('image comparison requires a slider or editing journey')
    if args.scenario in journeys or (getattr(args, 'compare_images', None) and args.control_pointer == '/exposure'):
        if any(not isinstance(case.get('settings'), dict)
               or type(case['settings'].get('exposure')) not in (int, float)
               or case['settings'].get('exposure_auto') is not False for case in selected):
            raise ValueError('image comparison needs an explicit manual exposure recipe for every photo')
    if args.scenario in ('selection-journey', 'selection-pressure') and any(
            not -3.625 <= case['settings']['exposure'] <= 3.625 for case in selected):
        raise ValueError('selection journey requires manual exposures between -3.625 and 3.625')
    if getattr(args, 'compare_images', None):
        for case in selected:
            if type(pointer_value(case['settings'], args.control_pointer)) not in (int, float):
                raise ValueError('image comparison needs a declared numeric slider value for every photo')
        args.compare_images = args.compare_images.resolve(strict=True)
    if getattr(args, 'max_ui_gap_ms', None) is not None:
        summarize_ui_ticks([], args.max_ui_gap_ms)
    args.cache_namespace = args.cache_namespace or 'scenario-'+uuid.uuid4().hex
    if re.fullmatch(r'[A-Za-z0-9_-]{1,64}',args.cache_namespace) is None:
        raise ValueError('cache namespace must be one ASCII alphanumeric, dash or underscore component (up to 64 characters)')
    if args.mode != 'off' and args.trace_processor is None:
        raise ValueError('diagnostic scenarios require an explicit local --trace-processor executable')
    if args.trace_processor:
        args.trace_processor = args.trace_processor.resolve(strict=True)
    if min(args.trials,args.repeats,args.rapid_edits,args.timeout,args.step_timeout) < 1 or args.pressure_mib < 64:
        raise ValueError('counts and timeouts must be positive; memory pressure must be at least 64 MiB')
    return selected


def run(args):
    selected = validated_inputs(args)
    directory = args.directory.resolve()
    directory.mkdir(parents=True, exist_ok=False)
    tree = source_tree_hash(Path(__file__).resolve().parents[2])
    results = []
    try:
        for trial in range(args.trials):
            results.append(run_trial(args, selected, directory/f'trial-{trial:02}', tree))
            if results[-1]['workload_key'] != results[0]['workload_key']:
                raise ValueError('trials have different matched workload keys; inspect inputs, resolved settings and hardware')
    finally:
        report = write_trial_report(args, directory, results)
    return report


def write_trial_report(args, directory, results):
    metrics = {}
    for result in results:
        if not result['timing_valid']:
            continue
        for step in result['steps']:
            key = (result['mode'],step['action'])
            value = step.get('baseline_elapsed_ms') if result['input_kind']=='baseline' else step['group_settled_ms']
            if value is not None:
                metrics.setdefault(key,[]).append(value)
    matched=bool(results) and all(result['workload_key']==results[0]['workload_key'] for result in results)
    report = {'schema_version':1,'trials_requested':args.trials,
              'workload_key':results[0]['workload_key'] if matched else None,
              'workload_consistent':matched,'scenario':args.scenario,'photo_count':args.count,'quality':args.quality,
              'mode':args.mode,'passed':matched and len(results)==args.trials and all(result['passed'] for result in results),
              'timing_valid':len(results)==args.trials and all(result['timing_valid'] for result in results),
              'trials':results,'group_elapsed_ms':[{'mode':mode,'action':action,**distribution(values)} for (mode,action),values in metrics.items()],
              'limits':['Off baseline elapsed values and detailed trace attribution remain separate datasets.',
                        'Native scenarios do not measure physical screen presentation.',
                        'Fresh numerical equality is checked only when image comparison is requested.']}
    (directory/'regression.json').write_text(json.dumps(report, indent=2)+'\n')
    return report


def argument_parser():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,required=True)
    inputs=parser.add_mutually_exclusive_group(required=True)
    inputs.add_argument('--cases',type=Path,help='existing source/settings case array, as used by profile_editing')
    inputs.add_argument('--source',type=Path,action='append',help='repeat explicit sources in desired browser order')
    parser.add_argument('--directory',type=Path,required=True)
    parser.add_argument('--scenario',choices=NAMES,required=True)
    parser.add_argument('--count',type=int)
    parser.add_argument('--native-photo-count',type=int,choices=[1,2],help='cache-workflow photos zoomed to native detail (default: 1)')
    parser.add_argument('--quality',choices=PROCESSING_QUALITIES,default='quality')
    parser.add_argument('--disk-cache-bytes',type=int,default=10_000_000_000,
                        help='owned disk cache budget; zero measures retained processing after original loading')
    parser.add_argument('--memory-cache-bytes',type=int,default=2*1024*1024*1024,
                        help='application retained-memory budget, separate from disk storage')
    parser.add_argument('--mode',choices=['off','wall','gpu'],default='wall')
    parser.add_argument('--trace-processor',type=Path)
    parser.add_argument('--trials',type=int,default=3)
    parser.add_argument('--repeats',type=int,default=5)
    parser.add_argument('--rapid-edits',type=int,default=12)
    parser.add_argument('--seed',type=int,default=0,help='replay seed for transition-journey')
    parser.add_argument('--action-count',type=int,default=8,help='bounded selection/edit pairs for transition-journey')
    parser.add_argument('--save-on-exit',action='store_true',help='leave the capture active and verify the app saves it during orderly shutdown')
    parser.add_argument('--folder-mode',choices=['browse','import'],default='import',help='open the isolated folder in Browse or Import mode')
    parser.add_argument('--exit-with-pending',action='store_true',help='queue a final edit and close immediately; verify the saved recipes after exit')
    parser.add_argument('--control-pointer',default='/exposure',help='setting JSON pointer for slider-drag')
    parser.add_argument('--drag-steps',type=int,default=12,help='number of pointer movements for slider-drag')
    parser.add_argument('--drag-fraction',type=float,default=0.2,help='fraction of slider width for slider-drag')
    parser.add_argument('--drag-pattern',choices=['forward','scrub'],default='forward',help='pointer path for slider-drag')
    parser.add_argument('--theme',choices=['dark','light'],default='dark',help='workspace theme for slider-drag')
    parser.add_argument('--initial-values',choices=['mixed','equal'],default='mixed',help='expected relation of staged values for slider-drag')
    parser.add_argument('--background',action='store_true',help='run the scenario window behind normal windows without requesting foreground activation')
    parser.add_argument('--editing-section',choices=['WhiteBalance','Mix','Tone','Saturation','ColorEditor','Noise','Detail','Sharpening','Corrections'],default='Tone',help='move this editing section into view for slider-drag')
    parser.add_argument('--compare-images',type=Path,metavar='RENDERER',help='compare final slider or journey frames with fresh renders from this profile_pipeline binary')
    parser.add_argument('--max-ui-gap-ms',type=float,help='optional caller-selected maximum UI update gap during measured scenario actions')
    parser.add_argument('--pressure-mib',type=int,default=128)
    parser.add_argument('--step-timeout',type=int,default=60)
    parser.add_argument('--timeout',type=int,default=600)
    parser.add_argument('--max-memory-mb',type=int,
                        help='kill the owned app if its sampled macOS physical footprint exceeds this limit')
    parser.add_argument('--no-monitor',action='store_true')
    parser.add_argument('--cache-namespace',help='explicit controlled reuse; default is unique and isolated from the owner cache')
    return parser


def main():
    parser = argument_parser()
    args=parser.parse_args()
    try:
        result=run(args)
    except (ValueError,RuntimeError,subprocess.TimeoutExpired) as error:
        parser.exit(1,str(error)+'\n')
    if not result['passed']:raise SystemExit(1)
    print(json.dumps(result,indent=2))


if __name__=='__main__':main()

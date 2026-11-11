"""Run declared real-app diagnostic scenarios with isolated inputs and shared Perfetto artifacts."""

import argparse
from contextlib import nullcontext
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import re
import uuid

from native import recording_environment, source_tree_hash, workload_monitor
from trace_analysis import analyze_capture, distribution
from watch_workloads import require_quiet, concurrent_workloads
from file_hash import digest
from results import PREVIEW_QUALITIES, render_result

NAMES = ('initial', 'warm-disk', 'prefetched-selection', 'warm-edit', 'cached-switch', 'rapid-supersession', 'memory-pressure', 'slider-drag')


def selected_cases(cases, count, quality, scenario):
    if not 1 <= count <= 8 or len(cases) < count:
        raise ValueError('supply one through eight declared photographs for the requested count')
    if quality != 'thumb' and count > 1:
        raise ValueError('native multi-photo overviews use Thumb; use profile_editing for other multi-photo scales')
    if scenario == 'cached-switch' and count < 2:
        raise ValueError('cached switching needs at least two photographs')
    if scenario == 'slider-drag' and count < 2:
        raise ValueError('a mixed-value slider drag needs at least two photographs')
    selected = cases[:count]
    sources = [str(Path(case['source']).resolve(strict=True)) for case in selected]
    if len(set(sources)) != count:
        raise ValueError('scenario photographs must have distinct source paths')
    if any(not Path(source).is_file() for source in sources):
        raise ValueError('scenario sources must be files')
    return [{**case, 'source': source} for case, source in zip(selected, sources)]


def stage_inputs(cases, directory):
    """Hard links isolate sidecars without copying RAW bytes or modifying caller files."""
    directory.mkdir(parents=True, exist_ok=False)
    inputs = []
    for index, case in enumerate(cases):
        source = Path(case['source'])
        staged = directory / f'{index:02}-{source.name}'
        try:
            os.link(source, staged)
        except OSError as error:
            raise ValueError('scenario inputs must support hard links into the owned run directory') from error
        record = {'source': str(source), 'staged_source': str(staged),
                  'sha256': digest(source), 'length': source.stat().st_size,
                  'modified_ns': source.stat().st_mtime_ns, 'label': case.get('label', source.name)}
        if 'settings' in case:
            # This is the maintained v1 sidecar format, validated by the actual app.
            sidecar = directory / 'foveonic' / (staged.name + '.foveonic.json')
            sidecar.parent.mkdir(exist_ok=True)
            payload = json.dumps({'format':'foveonic-editing','version':1,'settings':case['settings']}, indent=2).encode()
            if len(payload) > 64 * 1024:
                raise ValueError('declared scenario recipe exceeds the application sidecar limit')
            sidecar.write_bytes(payload)
            record['settings'] = case['settings']
            record['sidecar_sha256'] = hashlib.sha256(payload).hexdigest()
        else:
            record['settings'] = 'camera-defaults'
        inputs.append(record)
    return inputs


def scenario_environment(home, evidence, manifest):
    env = recording_environment(home, 'off')
    for key in list(env):
        if key.startswith(('FOVEONIC_GUI_SCENARIO_', 'FOVEONIC_DIAGNOSTICS_')):
            del env[key]
    env.update(FOVEONIC_GUI_SMOKE_DIR=str(evidence), FOVEONIC_GUI_SCENARIO_MANIFEST=str(manifest))
    return env


def finish_trial(directory, mode, trace_processor):
    result_path = directory / 'evidence' / 'scenario-result.json'
    if not result_path.exists():
        raise RuntimeError('application exited without a scenario result; inspect owned stdout/stderr')
    result = json.loads(result_path.read_text())
    if mode == 'off':
        return {'passed': result['passed'], 'mode':'off', 'input_kind':'baseline',
                'steps':result['steps'], 'result':str(result_path)}
    capture = directory / 'capture'
    analysis = analyze_capture(capture, trace_processor)
    summary_path = capture / 'summary.json'
    summary = json.loads(summary_path.read_text())
    summary['scenario'] = result
    summary_path.write_text(json.dumps(summary, indent=2) + '\n')
    manifest_path = capture / 'manifest.json'
    manifest = json.loads(manifest_path.read_text())
    manifest['application']['scenario_result'] = {'path':str(result_path),'sha256':digest(result_path)}
    manifest_path.write_text(json.dumps(manifest, indent=2) + '\n')
    return {'passed':result['passed'] and analysis['validity']['valid'] and analysis['behavior_valid'] is True,
            'mode':mode, 'input_kind':'perfetto', 'steps':analysis['steps'],
            'capture':str(capture), 'result':str(result_path)}


def application_result(native_result, scenario, quality, inputs, binary_sha256=None):
    sources = {item['staged_source']:item for item in inputs}
    observations = []
    missing = []
    for step in native_result['steps']:
        for photo in step.get('photos', []):
            source = sources.get(photo['source'])
            if source is None:
                missing.append(photo['source'])
            observations.append({'step':step['action'], 'source':source['source'] if source else photo['source'],
                                 'source_sha256':source['sha256'] if source else None,
                                 'family':photo.get('family'), 'scale_divisor':photo.get('scale_divisor')})
    failure = native_result.get('failure')
    if missing:
        failure = f'output references undeclared staged inputs: {missing}'
    environment = {'binary_sha256':binary_sha256, 'adapter':native_result.get('hardware', {}).get('adapter')}
    return render_result('application', scenario, quality, observations,
                         native_result['passed'] and not missing, failure, environment)


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
                       'first_current_scale':None if photo is None else photo['scale_divisor']})
    scenario=manifest['scenario']
    disk=native_result.get('disk_cache')
    expected=scenario['disk_cache_bytes']
    expected_memory=scenario['pressure_bytes'] if scenario['name']=='memory-pressure' else scenario['memory_cache_bytes']
    cache_policy_matches=(isinstance(disk,dict) and disk.get('max_bytes')==expected
                          and disk.get('enabled') is (expected>0)
                          and native_result.get('memory_cache_bytes')==expected_memory)
    descriptor={'scenario':{key:scenario[key] for key in
                           ('name','photo_count','quality','repeats','rapid_edits','pressure_bytes','disk_cache_bytes','memory_cache_bytes')},
                'inputs':inputs,'hardware':{'host':manifest['hardware'],'application':native_result.get('hardware')},
                'application_disk_cache':disk,
                'application_memory_cache_bytes':native_result.get('memory_cache_bytes'),
                'cache_policy_matches':cache_policy_matches,
                'first_current_complete':cache_policy_matches and len(current)==len(inputs) and all(row['first_current_settings_sha256'] for row in inputs),
                'measurement':manifest['measurement']}
    if scenario['name']=='slider-drag':
        descriptor['scenario'].update({key:scenario[key] for key in ('control_pointer','drag_steps','drag_fraction','drag_pattern','light_theme','mixed_values','editing_section')})
    # Off and traced captures describe the same work, but only Off may be compared.
    descriptor['measurement']={key:value for key,value in descriptor['measurement'].items() if key!='input_kind'}
    return {'sha256':value_hash(descriptor),'descriptor':descriptor}


def run_trial(args, selected, directory, source_hash):
    directory.mkdir(parents=True, exist_ok=False)
    home, evidence = directory / 'home', directory / 'evidence'
    home.mkdir(); evidence.mkdir()
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
                       'os_file_cache':'uncontrolled; input hashing warms filesystem cache',
                       'scenario_poll_ms':20,'numerical_equality':'verified separately by maintained headless cases'},
    }
    if args.scenario == 'slider-drag':
        manifest['scenario'].update(control_pointer=args.control_pointer, drag_steps=args.drag_steps,
                                    drag_fraction=args.drag_fraction, drag_pattern=args.drag_pattern, light_theme=args.theme=='light', mixed_values=args.initial_values=='mixed', editing_section=args.editing_section)
    launch = directory / 'launch.json'
    launch.write_text(json.dumps(manifest, indent=2) + '\n')
    env = scenario_environment(home, evidence, launch)
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
            returncode = process.wait(timeout=args.timeout)
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
    result = finish_trial(directory, args.mode, args.trace_processor)
    native_result=json.loads(Path(result['result']).read_text())
    test_result = application_result(native_result, args.scenario, args.quality, inputs, manifest['binary_sha256'])
    test_result_path = evidence/'test-result.json'
    test_result_path.write_text(json.dumps(test_result, indent=2)+'\n')
    workload_key=matched_workload(manifest,native_result)
    result['passed'] = result['passed'] and test_result['outcome']=='passed' and workload_key['descriptor']['cache_policy_matches']
    result['test_result'] = test_result
    manifest['workload_key']=workload_key
    launch.write_text(json.dumps(manifest,indent=2)+'\n')
    result.update(contention=contention, timing_valid=timing_valid, workload_key=workload_key,
                  artifacts={'launch':str(launch),'result':result['result'],'result_sha256':digest(result['result']),
                             'binary':manifest['binary'],'binary_sha256':manifest['binary_sha256'],
                             'source_tree_sha256':manifest['source_tree_sha256']})
    for name in ('slider-drag.json', 'slider-observation.json', 'slider-failure.png'):
        path=evidence/name
        if path.exists():
            result['artifacts'][name]={'path':str(path),'sha256':digest(path)}
    result['artifacts']['test_result']={'path':str(test_result_path),'sha256':digest(test_result_path)}
    if args.mode != 'off':
        summary_path=directory/'capture'/'summary.json'
        summary=json.loads(summary_path.read_text())
        summary.update(contention=contention, timing_valid=timing_valid, test_result=test_result)
        summary_path.write_text(json.dumps(summary,indent=2)+'\n')
        capture_manifest_path=directory/'capture'/'manifest.json'
        capture_manifest=json.loads(capture_manifest_path.read_text())
        capture_manifest['application']['test_result']=result['artifacts']['test_result']
        capture_manifest_path.write_text(json.dumps(capture_manifest,indent=2)+'\n')
    return result


def run(args):
    args.binary = args.binary.resolve(strict=True)
    cases = json.loads(args.cases.read_text()) if args.cases else [{'source':str(path)} for path in args.source]
    args.count = len(cases) if args.count is None else args.count
    selected = selected_cases(cases, args.count, args.quality, args.scenario)
    if args.disk_cache_bytes < 0 or (args.scenario in ('warm-disk', 'prefetched-selection') and args.disk_cache_bytes == 0):
        raise ValueError('disk cache budget must be nonnegative and the requested cache scenario requires an enabled disk cache')
    if args.scenario == 'prefetched-selection' and (args.count < 2 or args.quality != 'thumb'):
        raise ValueError('prefetched-selection requires at least two photographs at Thumb quality')
    if args.memory_cache_bytes < 1:
        raise ValueError('memory cache budget must be positive')
    if args.scenario == 'slider-drag' and (not args.control_pointer.startswith('/') or args.drag_steps < 3 or not 0 < args.drag_fraction < 0.45):
        raise ValueError('slider drag needs a setting pointer, at least three steps, and a fraction between 0 and 0.45')
    args.cache_namespace = args.cache_namespace or 'scenario-'+uuid.uuid4().hex
    if re.fullmatch(r'[A-Za-z0-9_-]{1,64}',args.cache_namespace) is None:
        raise ValueError('cache namespace must be one ASCII alphanumeric, dash or underscore component (up to 64 characters)')
    if args.mode != 'off' and args.trace_processor is None:
        raise ValueError('diagnostic scenarios require an explicit local --trace-processor executable')
    if args.trace_processor:
        args.trace_processor = args.trace_processor.resolve(strict=True)
    if min(args.trials,args.repeats,args.rapid_edits,args.timeout,args.step_timeout) < 1 or args.pressure_mib < 64:
        raise ValueError('counts and timeouts must be positive; memory pressure must be at least 64 MiB')
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
                            'Native scenarios do not measure physical screen presentation or fresh numerical equality.']}
        (directory/'regression.json').write_text(json.dumps(report, indent=2)+'\n')
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,required=True)
    inputs=parser.add_mutually_exclusive_group(required=True)
    inputs.add_argument('--cases',type=Path,help='existing source/settings case array, as used by profile_editing')
    inputs.add_argument('--source',type=Path,action='append',help='repeat explicit sources in desired browser order')
    parser.add_argument('--directory',type=Path,required=True)
    parser.add_argument('--scenario',choices=NAMES,required=True)
    parser.add_argument('--count',type=int)
    parser.add_argument('--quality',choices=PREVIEW_QUALITIES,default='thumb')
    parser.add_argument('--disk-cache-bytes',type=int,default=10_000_000_000,
                        help='owned disk cache budget; zero measures retained processing after original loading')
    parser.add_argument('--memory-cache-bytes',type=int,default=1024*1024*1024,
                        help='application retained-memory budget, separate from disk storage')
    parser.add_argument('--mode',choices=['off','wall','gpu'],default='wall')
    parser.add_argument('--trace-processor',type=Path)
    parser.add_argument('--trials',type=int,default=3)
    parser.add_argument('--repeats',type=int,default=5)
    parser.add_argument('--rapid-edits',type=int,default=12)
    parser.add_argument('--control-pointer',default='/exposure',help='setting JSON pointer for slider-drag')
    parser.add_argument('--drag-steps',type=int,default=12,help='number of pointer movements for slider-drag')
    parser.add_argument('--drag-fraction',type=float,default=0.2,help='fraction of slider width for slider-drag')
    parser.add_argument('--drag-pattern',choices=['forward','scrub'],default='forward',help='pointer path for slider-drag')
    parser.add_argument('--theme',choices=['dark','light'],default='dark',help='workspace theme for slider-drag')
    parser.add_argument('--initial-values',choices=['mixed','equal'],default='mixed',help='expected relation of staged values for slider-drag')
    parser.add_argument('--background',action='store_true',help='run the scenario window behind normal windows without requesting foreground activation')
    parser.add_argument('--editing-section',choices=['WhiteBalance','Mix','Tone','Saturation','ColorEditor','Noise','Detail','Sharpening','Corrections'],default='Tone',help='move this editing section into view for slider-drag')
    parser.add_argument('--pressure-mib',type=int,default=128)
    parser.add_argument('--step-timeout',type=int,default=60)
    parser.add_argument('--timeout',type=int,default=600)
    parser.add_argument('--no-monitor',action='store_true')
    parser.add_argument('--cache-namespace',help='explicit controlled reuse; default is unique and isolated from the owner cache')
    args=parser.parse_args()
    try:
        result=run(args)
    except (ValueError,RuntimeError,subprocess.TimeoutExpired) as error:
        parser.exit(1,str(error)+'\n')
    if not result['passed']:raise SystemExit(1)
    print(json.dumps(result,indent=2))


if __name__=='__main__':main()

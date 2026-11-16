"""Run finite slider correctness groups in one owned app, then check each case after exit."""

from contextlib import nullcontext
import json
from pathlib import Path
import re
import subprocess
import uuid

from file_hash import digest
import scenarios


def group_jobs(jobs):
    """Keep declared case order within each fixed fixture, theme and processing policy."""
    groups = {}
    for fixture, case, theme in jobs:
        key = (fixture['name'], theme, case['processing_quality'])
        groups.setdefault(key, []).append((fixture, case, theme))
    return list(groups.values())


def validate_requests(requests, fixture):
    if not isinstance(requests, list) or not requests or not fixture:
        raise ValueError('a slider session needs a finite case list and fixture')
    case_ids, directories, validated = set(), set(), []
    signature = None
    for request in requests:
        if not isinstance(request, dict) or not {'case_id', 'args'} <= request.keys():
            raise ValueError('slider session cases need an identity and scenario arguments')
        case_id, args = request['case_id'], request['args']
        if (not isinstance(case_id, str) or re.fullmatch(r'[A-Za-z0-9_-]+', case_id) is None
                or case_id in case_ids):
            raise ValueError('slider session case identities must be unique simple names')
        case_ids.add(case_id)
        if (args.scenario != 'slider-drag' or args.mode != 'off' or args.trials != 1
                or args.compare_images is None or args.folder_mode != 'import'
                or args.save_on_exit or args.exit_with_pending or args.max_ui_gap_ms is not None):
            raise ValueError('slider sessions require one Off slider trial with image checks in Import mode and no exit or latency options')
        selected = scenarios.validated_inputs(args)
        args.directory = args.directory.resolve()
        if args.directory in directories or any(
                args.directory.is_relative_to(other) or other.is_relative_to(args.directory)
                for other in directories):
            raise ValueError('slider session case directories must be distinct and disjoint')
        directories.add(args.directory)
        current = (args.binary, args.compare_images, args.theme, args.quality,
                   args.disk_cache_bytes, args.memory_cache_bytes, args.background,
                   args.no_monitor, args.max_memory_mb,
                   tuple(item['source'] for item in selected))
        if signature is not None and signature != current:
            raise ValueError('slider session cases must share fixture sources, theme, quality, binaries and cache policy')
        signature = current
        validated.append((request, selected))
    return validated


def run_group(requests, directory, fixture):
    """Preserve normal case staging/assertions while sharing only the native app process."""
    validated = validate_requests(requests, fixture)
    directory = directory.resolve()
    if any(directory.is_relative_to(request['args'].directory)
           or request['args'].directory.is_relative_to(directory) for request, _ in validated):
        raise ValueError('slider session control directory must be separate from case directories')
    directory.mkdir(parents=True, exist_ok=False)
    home = directory/'home'
    home.mkdir()
    session_id = uuid.uuid4().hex
    cache_namespace = 'slider-session-'+session_id
    envelope_path, result_path = directory/'session.json', directory/'session-result.json'
    result = {'schema_version': 1, 'kind': 'slider-correctness', 'id': session_id,
              'fixture': fixture, 'passed': False, 'status': 'preparing', 'timing_valid': False,
              'runner': {'path': str(Path(__file__).resolve()), 'sha256': digest(Path(__file__))},
              'cases': [], 'limits': ['Reused application sessions are correctness evidence. Their timings are not fresh-launch latency baselines.']}
    prepared = []

    def save():
        temporary = result_path.with_suffix('.tmp')
        temporary.write_text(json.dumps(result, indent=2)+'\n')
        temporary.replace(result_path)

    save()
    try:
        tree = scenarios.source_tree_hash(Path(__file__).resolve().parents[2])
        source_identity = None
        for request, selected in validated:
            args = request['args']
            args.cache_namespace = cache_namespace
            args.directory.mkdir(parents=True, exist_ok=False)
            trial = scenarios.prepare_trial(args, selected, args.directory/'trial-00', tree)
            (trial['directory']/'capture').mkdir()
            manifest = trial['manifest']
            manifest.update(case_id=request['case_id'], evidence_directory=str(trial['evidence']), fixture=fixture)
            manifest['measurement'].update(session_reuse=True,
                setup='One reused app. Each case opens its own declared input folder and settles before its gesture.')
            identity = [(item['sha256'], item['length']) for item in trial['inputs']]
            if source_identity is not None and identity != source_identity:
                raise ValueError('slider session source SHA order changed while staging cases')
            source_identity = identity
            trial['launch'].write_text(json.dumps(manifest, indent=2)+'\n')
            prepared.append((request, trial))
        envelope = {'session': {'id': session_id, 'kind': 'slider-correctness',
                                'cases': [trial['manifest'] for _, trial in prepared]}}
        envelope_path.write_text(json.dumps(envelope, indent=2)+'\n')
        args = prepared[0][0]['args']
        env = scenarios.scenario_environment(home, prepared[0][1]['evidence'], envelope_path)
        env['FOVEONIC_DIAGNOSTICS_CACHE_NAMESPACE'] = cache_namespace
        if args.background:
            env['FOVEONIC_GUI_SCENARIO_BACKGROUND'] = '1'
        monitor = nullcontext() if args.no_monitor else scenarios.workload_monitor(directory/'workloads.jsonl')
        with monitor, (directory/'stdout.txt').open('x') as stdout, (directory/'stderr.txt').open('x') as stderr:
            result['preflight'] = scenarios.require_quiet()
            process = subprocess.Popen([str(args.binary), prepared[0][1]['inputs'][0]['staged_source']],
                                       env=env, stdout=stdout, stderr=stderr, start_new_session=True)
            try:
                result.update(status='running', pid=process.pid, manifest_sha256=digest(envelope_path))
                save()
                timeout = sum(request['args'].timeout for request, _ in prepared)
                code = (process.wait(timeout=timeout) if args.max_memory_mb is None else
                        scenarios.wait_guarded(process, timeout, args.max_memory_mb*1024*1024,
                                               directory/'physical-memory.jsonl'))
            finally:
                if process.poll() is None:
                    process.terminate()
                    try:
                        process.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait()
        contention = ({'checked': False, 'uncontended': None, 'samples': 0, 'other_workloads': []}
                      if args.no_monitor else scenarios.concurrent_workloads(
                          directory/'workloads.jsonl', owned_pids=[process.pid]))
        result.update(status='checking-images', exit_code=code, contention=contention)
        save()
        # Fresh renderer checks require the native app and its monitor to have exited.
        for index, (request, trial) in enumerate(prepared):
            args = request['args']
            session = {'expected': {'id': session_id, 'case_id': request['case_id'],
                                   'index': index, 'count': len(prepared), 'pid': process.pid},
                       'kind': 'slider-correctness', 'shared_home': str(home), 'process_exit': code}
            trial['manifest'].update(contention=contention, timing_valid=False, session_reuse=session)
            trial['launch'].write_text(json.dumps(trial['manifest'], indent=2)+'\n')
            failure = None
            try:
                collected = scenarios.collect_trial(args, trial, contention, False, session=session)
                report = scenarios.write_trial_report(args, args.directory, [collected])
                if not report['passed']:
                    failure = collected['test_result'].get('failure') or 'case assertions did not pass'
            except (OSError, ValueError, KeyError, TypeError, RuntimeError, json.JSONDecodeError,
                    subprocess.TimeoutExpired) as error:
                report = scenarios.write_trial_report(args, args.directory, [])
                failure = f'{type(error).__name__}: {error}'
            report.update(session_reuse=session, timing_valid=False)
            report['limits'].append(result['limits'][0])
            if failure:
                report['failure'] = failure
            (args.directory/'regression.json').write_text(json.dumps(report, indent=2)+'\n')
            result['cases'].append({'case_id': request['case_id'], 'directory': str(args.directory),
                                    'passed': report['passed'], 'failure': failure})
            save()
        result['passed'] = (code == 0 and len(result['cases']) == len(prepared)
                            and all(case['passed'] for case in result['cases']))
        result['status'] = 'passed' if result['passed'] else 'failed'
        save()
        return result
    except BaseException as error:
        result.update(status='interrupted' if isinstance(error, KeyboardInterrupt) else 'failed',
                      passed=False, failure=f'{type(error).__name__}: {error}')
        save()
        raise

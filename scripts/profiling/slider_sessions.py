"""Run finite slider correctness groups in one owned app, then check each case after exit."""

from contextlib import nullcontext
import json
from pathlib import Path
import re
import subprocess
import time
import uuid

from correctness_cohort import CohortStopped
from file_hash import digest
import scenarios
from worker_resources import resolve_worker_memory_mb


def group_jobs(jobs, *, concurrent=False, guard_mb=None):
    """Keep declared case order within each fixed fixture, theme and processing policy."""
    groups = {}
    for fixture, case, theme in jobs:
        key = (fixture['name'], theme, case['processing_quality'])
        if concurrent:
            key += (resolve_worker_memory_mb(fixture, guard_mb, case=case),)
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
                or args.save_on_exit or args.exit_with_pending or args.max_ui_gap_ms is not None
                or type(args.timeout) is not int or args.timeout <= 0):
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
                   args.no_monitor, args.max_memory_mb, getattr(args, 'batch_image_verification', False),
                   getattr(args, 'prune_verified_images', False),
                   tuple(item['source'] for item in selected))
        if signature is not None and signature != current:
            raise ValueError('slider session cases must share fixture sources, theme, quality, binaries and cache policy')
        signature = current
        validated.append((request, selected))
    return validated


def _progress_watchdog(prepared, session_id, pid, on_poll=None):
    """Renew a bounded case deadline only for the next complete, identity-bound terminal."""
    index = 0
    budget = prepared[0][0]['args'].timeout
    began = time.monotonic()
    total_deadline = began + sum(request['args'].timeout for request, _ in prepared)
    deadline = began + budget

    def check_deadline():
        now = time.monotonic()
        if now >= deadline:
            phase = prepared[index][0]['case_id'] if index < len(prepared) else 'orderly-exit'
            raise subprocess.TimeoutExpired(f'slider session {phase} (PID {pid})', budget)
        return now

    def check():
        nonlocal index, budget, deadline
        if on_poll is not None:
            on_poll()
        check_deadline()
        while index < len(prepared):
            request, trial = prepared[index]
            try:
                terminal = json.loads((trial['evidence'] / 'scenario-result.json').read_text())
            except (FileNotFoundError, UnicodeDecodeError, json.JSONDecodeError):
                return
            now = check_deadline()
            expected = {'id': session_id, 'case_id': request['case_id'], 'index': index,
                        'count': len(prepared), 'pid': pid}
            if (not isinstance(terminal, dict) or type(terminal.get('passed')) is not bool
                    or terminal.get('session') != expected
                    or any(type(terminal['session'][key]) is not int for key in ('index', 'count', 'pid'))):
                raise ValueError(f'slider session terminal identity or passed flag differs: {request["case_id"]}')
            # A failed case stops the native queue. Preserve its normal post-exit checks.
            index = index + 1 if terminal['passed'] else len(prepared)
            next_budget = (prepared[index][0]['args'].timeout if index < len(prepared)
                           else request['args'].timeout)
            budget = min(next_budget, total_deadline - now)
            deadline = now + budget
    return check


def _wait_for_exit(process, on_poll):
    while process.poll() is None:
        on_poll()
        try:
            return process.wait(timeout=.1)
        except subprocess.TimeoutExpired:
            pass
    return process.wait()


def run_group(requests, directory, fixture, *, cohort=None):
    """Preserve normal case staging/assertions while sharing only the native app process."""
    validated = validate_requests(requests, fixture)
    if cohort is not None:
        cohort.check_alive()
        if any(not request['args'].background or request['args'].no_monitor
               or request['args'].max_memory_mb is None for request, _ in validated):
            raise ValueError('concurrent correctness requires background windows, monitoring and memory guards')
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
    if cohort is not None:
        result['cohort'] = cohort.reference()
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
            if cohort is not None:
                manifest['correctness_cohort'] = cohort.reference()
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
        options = {} if cohort is None else {'cohort': cohort}
        monitor = nullcontext() if args.no_monitor else scenarios.workload_monitor(directory/'workloads.jsonl', **options)
        with monitor, (directory/'stdout.txt').open('x') as stdout, (directory/'stderr.txt').open('x') as stderr:
            result['preflight'] = scenarios.require_quiet() if cohort is None else cohort.check_quiet()
            if cohort is not None:
                cohort.check_alive()
            process = subprocess.Popen([str(args.binary), prepared[0][1]['inputs'][0]['staged_source']],
                                       env=env, stdout=stdout, stderr=stderr, start_new_session=cohort is None)
            try:
                result.update(status='running', pid=process.pid, manifest_sha256=digest(envelope_path))
                save()
                timeout = sum(request['args'].timeout for request, _ in prepared)
                progress = _progress_watchdog(prepared, session_id, process.pid,
                                              None if cohort is None else cohort.check_alive)
                code = (_wait_for_exit(process, progress) if args.max_memory_mb is None else
                        scenarios.wait_guarded(process, timeout, args.max_memory_mb*1024*1024,
                                               directory/'physical-memory.jsonl',
                                               on_poll=progress))
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
        if cohort is not None:
            contention['correctness_cohort'] = cohort.reference()
        save()
        # Fresh renderer checks require the native app and its monitor to have exited.
        for index, (request, trial) in enumerate(prepared):
            if cohort is not None:
                cohort.check_alive()
            args = request['args']
            session = {'expected': {'id': session_id, 'case_id': request['case_id'],
                                   'index': index, 'count': len(prepared), 'pid': process.pid},
                       'kind': 'slider-correctness', 'shared_home': str(home), 'process_exit': code}
            trial['manifest'].update(contention=contention, timing_valid=False, session_reuse=session)
            trial['launch'].write_text(json.dumps(trial['manifest'], indent=2)+'\n')
            failure = None
            try:
                collected = scenarios.collect_trial(args, trial, contention, False, session=session, **options)
                report = scenarios.write_trial_report(args, args.directory, [collected], session=session)
                if not report['passed']:
                    failure = report.get('retention_failure') or collected['test_result'].get('failure') or 'case assertions did not pass'
            except CohortStopped:
                raise
            except (OSError, ValueError, KeyError, TypeError, RuntimeError, json.JSONDecodeError,
                    subprocess.TimeoutExpired) as error:
                failure = f'{type(error).__name__}: {error}'
                report = scenarios.write_trial_report(args, args.directory, [], session=session, failure=failure)
            result['cases'].append({'case_id': request['case_id'], 'directory': str(args.directory),
                                    'passed': report['passed'], 'failure': failure})
            save()
            if cohort is not None and not report['passed']:
                result['cases'].extend({'case_id': other['case_id'], 'directory': str(other['args'].directory),
                                        'passed': False, 'outcome': 'unrun',
                                        'failure': 'previous correctness verification failed'}
                                       for other, _ in prepared[index + 1:])
                break
        result['passed'] = (code == 0 and len(result['cases']) == len(prepared)
                            and all(case['passed'] for case in result['cases']))
        result['status'] = 'passed' if result['passed'] else 'failed'
        save()
        return result
    except BaseException as error:
        result.update(status='interrupted' if isinstance(error, (KeyboardInterrupt, CohortStopped)) else 'failed',
                      passed=False, failure=f'{type(error).__name__}: {error}')
        save()
        raise

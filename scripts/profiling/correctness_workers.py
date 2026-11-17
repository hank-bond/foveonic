"""Schedule finite isolated slider groups and validate their complete case evidence."""

import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time
import traceback

from correctness_cohort import CohortPolicy, CohortStopped, Controller
from file_hash import digest
from process_memory import MemoryGuardExceeded, birth_reader
from worker_resources import resolve_worker_memory_mb, select_admissions, validate_admission


def case_key(row):
    return (row['fixture'], row['case'], row['theme'], row['processing_quality'])


def validate_options(groups, workers, budget_mb, guard_mb, mode, reuse, renderer, background):
    """Reject incompatible execution and impossible reservations before staging inputs."""
    if mode != 'off' or not reuse or renderer is None or not background:
        raise ValueError('concurrent sliders require Off mode, session reuse, image verification and background windows')
    guards = []
    for group in groups:
        resolved = {resolve_worker_memory_mb(fixture, guard_mb, case=case) for fixture, case, _ in group}
        if len(resolved) != 1:
            raise ValueError('each concurrent group must have one resolved worker memory guard')
        guards.append(resolved.pop())
    validate_admission(workers, budget_mb, guards)
    keys = [(f['name'], c['name'], t, c['processing_quality']) for g in groups for f, c, t in g]
    if len(keys) != len(set(keys)):
        raise ValueError('concurrent slider case identities must be unique')
    # The first backend deliberately uses the same macOS process identities and guards.
    birth_reader()(os.getpid())
    return guards


def _write(path, value):
    temporary = path.with_suffix('.tmp')
    temporary.write_text(json.dumps(value, indent=2) + '\n')
    temporary.replace(path)


def _worker(job_path):
    import behavioral
    job = json.loads(job_path.read_text())
    policy = CohortPolicy(job['cohort'], job['id'])
    receipt = {'group_id': job['id'], 'outcome': 'failed', 'passed': False}
    os.environ.pop('RUST_MIN_STACK', None)
    try:
        policy.wait_registered()
        report = behavioral.run(Path(job['binary']), [job['fixture']], job['cases'], Path(job['directory']),
                                disk_cache_bytes=job['disk_cache_bytes'], background=True,
                                compare_images=Path(job['renderer']), max_memory_mb=job['guard_mb'],
                                reuse_session=True, cohort=policy, batch_image_verification=job['batch_image_verification'],prune_verified_images=job['prune_verified_images'])
        receipt.update(passed=report['passed'], outcome='passed' if report['passed'] else 'failed')
        if not report['passed'] and only_verifier_guards_failed(job):
            receipt['outcome'] = 'guarded'
    except MemoryGuardExceeded as error:
        receipt.update(outcome='failed' if native_failure_recorded(job) else 'guarded', failure=str(error))
    except (CohortStopped, KeyboardInterrupt) as error:
        receipt.update(outcome='aborted', failure=str(error))
    except Exception as error:
        receipt['failure'] = f'{type(error).__name__}: {error}'
        receipt['failure_traceback'] = traceback.format_exc()
    finally:
        _write(Path(job['receipt']), receipt)
    return 0 if receipt['passed'] else 1


def native_failure_recorded(job):
    """Keep a native failure visible when it races with a sampled memory stop."""
    for path in Path(job['directory']).glob('*/trial-00/evidence/scenario-result.json'):
        try:
            if json.loads(path.read_text()).get('passed') is not True:
                return True
        except (OSError, ValueError, AttributeError):
            return True
    return False


def only_verifier_guards_failed(job):
    """A typed verifier guard may explain unrun checks, never an unrelated failure."""
    root = Path(job['directory'])
    native = [json.loads(p.read_text()) for p in root.glob('*/trial-00/evidence/scenario-result.json')]
    if len(native) != len(job['cases']) or any(x.get('passed') is not True for x in native):
        return False
    failures = [check for p in root.glob('*/trial-00/evidence/test-result.json')
                for check in json.loads(p.read_text())['assertions'] if check['outcome'] != 'passed']
    return bool(failures) and all(x.get('failure_kind') == 'memory_guard' for x in failures)


def validate_group(job, report, binary_hash, renderer_hash):
    """Bind every completed case to its declared group, inputs and independent checks."""
    if not report.get('passed') or report.get('timing_valid') is not False:
        raise ValueError('worker group did not complete correctness with invalidated timings')
    fixture, theme, quality = job['fixture'], job['theme'], job['quality']
    expected = {(fixture['name'], c['name'], theme, quality): c for c in job['cases']}
    rows = report['results']
    if len(rows) != len(expected) or {case_key(x) for x in rows} != expected.keys():
        raise ValueError('worker results do not match the exact declared case set')
    sessions = report['session_groups']
    if len(sessions) != 1 or not sessions[0].get('passed') or sessions[0].get('status') != 'passed':
        raise ValueError('worker did not produce one successful owned session')
    session_directory = Path(job['directory']) / 'session-groups' / '--'.join((fixture['name'], theme, quality))
    if Path(sessions[0]['directory']).resolve() != session_directory.resolve():
        raise ValueError('worker session escaped its declared group directory')
    session = json.loads((session_directory / 'session-result.json').read_text())
    if (session.get('cohort') != job['cohort_identity'] or session.get('id') != sessions[0]['id']
            or session.get('pid') not in job['owned_children']):
        raise ValueError('session does not bind to the registered worker and its owned application')
    order = {case['name']: index for index, case in enumerate(job['cases'])}
    for row in rows:
        key = case_key(row)
        if not row['passed']:
            raise ValueError('worker case failed')
        case = expected[key]
        case_dir = Path(job['directory']) / '--'.join(key)
        if Path(row['directory']).resolve() != case_dir.resolve():
            raise ValueError('worker case escaped its declared evidence directory')
        result = json.loads((case_dir / 'regression.json').read_text())
        if job.get('prune_verified_images', False):
            receipt = row['sample_retention']
            expected_receipt = case_dir / 'sample-retention.json'
            if Path(receipt['path']).resolve() != expected_receipt.resolve() or digest(receipt['path']) != receipt['sha256']:
                raise ValueError('verified sample retention receipt changed or escaped its case')
            retained = json.loads(expected_receipt.read_text())
            expected_pairs = [
                {'trial': 0, 'source': check['source'], 'step': check['step'],
                 'geometry': check['expected']['geometry'],
                 'files': [{'role': role, 'path': str(Path(check[role]['file']).resolve().relative_to(case_dir.resolve())),
                            'bytes': check[role]['bytes'], 'sha256': check[role]['sha256']}
                           for role in ('expected', 'actual')]}
                for check in result['trials'][0]['test_result']['assertions'] if check['name'] == 'image_samples']
            actual_pairs = [{key: image[key] for key in ('trial', 'source', 'step', 'geometry', 'files')}
                            for image in retained['images']]
            if (retained['state'] != 'complete' or retained['regression_sha256'] != digest(case_dir / 'regression.json')
                    or len(expected_pairs) != len(fixture['sources']) or actual_pairs != expected_pairs
                    or set(retained['removed']) != {f['path'] for image in expected_pairs for f in image['files']}):
                raise ValueError('verified sample retention is incomplete or does not describe this case')
        if not result['passed'] or result['timing_valid'] or len(result['trials']) != 1:
            raise ValueError('case is incomplete or presents concurrent timings as valid')
        trial = result['trials'][0]
        artifacts = trial['artifacts']
        if artifacts['binary_sha256'] != binary_hash or artifacts['fresh_renderer']['sha256'] != renderer_hash:
            raise ValueError('case used a different app or fresh renderer')
        trial_directory = case_dir / 'trial-00'
        paths = {'launch': trial_directory / 'launch.json',
                 'result': trial_directory / 'evidence/scenario-result.json'}
        if any(Path(artifacts[name]).resolve() != path.resolve() for name, path in paths.items()):
            raise ValueError('case artifacts escaped their owned directory')
        shared = artifacts['test_result']
        if (Path(shared['path']).resolve() != (trial_directory / 'evidence/test-result.json').resolve()
                or digest(shared['path']) != shared['sha256']
                or json.loads(Path(shared['path']).read_text()) != trial['test_result']):
            raise ValueError('shared assertion evidence changed or belongs to another case')
        launch = json.loads(Path(artifacts['launch']).read_text())
        batch = job.get('batch_image_verification', False)
        if launch.get('batch_image_verification', False) is not batch:
            raise ValueError('case verification mode differs from its declaration')
        if batch:
            expected_artifacts = {trial_directory / 'fresh-batch.json',
                                  *(trial_directory / f'fresh-{i:02}-terminal.json' for i in range(len(fixture['sources'])))}
            batch_artifacts = artifacts['fresh_verification_batch']
            if (len(batch_artifacts) != len(expected_artifacts)
                    or {Path(a['path']).resolve() for a in batch_artifacts} != {p.resolve() for p in expected_artifacts}
                    or any(digest(a['path']) != a['sha256'] for a in batch_artifacts)):
                raise ValueError('batch verification evidence is missing, modified or outside its case')
        scenario = launch['scenario']
        if (launch['fixture'] != fixture['name'] or launch['case_id'] != '--'.join(key)
                or scenario['name'] != 'slider-drag' or scenario['control_pointer'] != case['pointer']
                or scenario['quality'] != quality or scenario['mode'] != 'off'
                or scenario['light_theme'] != (theme == 'light')
                or scenario['drag_pattern'] != case.get('pattern', 'forward')
                or scenario['mixed_values'] != (case.get('initial_values', 'mixed') == 'mixed')
                or scenario['editing_section'] != case['section']
                or launch.get('correctness_cohort') != job['cohort_identity']):
            raise ValueError('case launch does not match its declaration')
        if len(launch['inputs']) != len(fixture['sources']) or len(case['recipes']) != len(fixture['sources']):
            raise ValueError('case source count differs from its declaration')
        for item, source, recipe, source_hash in zip(launch['inputs'], fixture['sources'], case['recipes'], job['source_hashes'], strict=True):
            if (Path(item['source']).resolve() != Path(source).resolve() or item['settings'] != recipe
                    or item['sha256'] != source_hash):
                raise ValueError('case source or starting recipe differs from its declaration')
        if digest(artifacts['result']) != artifacts['result_sha256']:
            raise ValueError('native terminal result changed after verification')
        expected_session = {'id': session['id'], 'case_id': '--'.join(key), 'index': order[case['name']],
                            'count': len(job['cases']), 'pid': session['pid']}
        native = json.loads(Path(artifacts['result']).read_text())
        if (native.get('passed') is not True or native.get('scenario') != scenario
                or native.get('session') != expected_session
                or trial['session_reuse']['expected'] != expected_session):
            raise ValueError('native case/session identity differs from its declaration')
        checks = trial['test_result']['assertions']
        names = {x['name'] for x in checks}
        if (trial['test_result']['outcome'] != 'passed' or any(x['outcome'] != 'passed' for x in checks)
                or sum(x['name'] == 'image_samples' for x in checks) != len(fixture['sources'])
                or not {'slider_session_identity', 'slider_session_process_exit', 'slider_session_scenario'} <= names):
            raise ValueError('native/session/image assertions are missing or failed')
    return rows


def run_parallel(binary, renderer, groups, guards, directory, workers, budget_mb, report, disk_cache_bytes, *, batch_image_verification=False, prune_verified_images=False):
    """Keep ownership, admission and aggregate writes in one coordinator process."""
    import behavioral
    directory = directory.resolve()
    jobs_dir, outputs = directory / 'worker-jobs', directory / 'groups'
    jobs_dir.mkdir(); outputs.mkdir()
    jobs = []
    binary_hash, renderer_hash = digest(binary), digest(renderer)
    for index, group in enumerate(groups):
        fixture, case, theme = group[0]
        quality = case['processing_quality']
        identity = '--'.join((fixture['name'], theme, quality, f'guard-{guards[index]}mb'))
        jobs.append({'id': identity, 'fixture': fixture, 'theme': theme, 'quality': quality,
                     'cases': [{**c, 'themes': [theme], 'processing_qualities': [quality]} for _, c, _ in group],
                     'directory': str(outputs / identity), 'binary': str(binary), 'renderer': str(renderer),
                     'binary_sha256': binary_hash, 'renderer_sha256': renderer_hash,
                     'guard_mb': guards[index], 'disk_cache_bytes': disk_cache_bytes,
                     'batch_image_verification': batch_image_verification, 'prune_verified_images': prune_verified_images,
                     'receipt': str(jobs_dir / (identity + '-result.json')),
                     'source_hashes': [digest(p) for p in fixture['sources']]})
    expected = [(f['name'], c['name'], t, c['processing_quality']) for g in groups for f, c, t in g]
    _write(directory / 'expected-cases.json', {'cases': expected, 'guards_mb': guards})
    controller = Controller(directory, binary, renderer, {j['id']: {'guard_mb': j['guard_mb']} for j in jobs})
    for job in jobs:
        job['cohort'] = str(controller.path)
        _write(jobs_dir / (job['id'] + '.json'), job)
    _write(directory / 'expected-groups.json', {'groups': jobs})
    report['provenance'] = dict(report.get('provenance') or {})
    for name in ('correctness_workers', 'correctness_cohort', 'worker_resources', 'slider_sessions',
                 'image_replay', 'native', 'process_memory'):
        path = Path(__file__).with_name(name + '.py')
        report['provenance'][name] = {'path': str(path), 'sha256': digest(path)}
    # Admit light groups first, without changing declaration/report order or equal-guard order.
    pending, active, completed, finished_groups = sorted(range(len(jobs)), key=guards.__getitem__), {}, {}, {}
    statuses = [{'id': j['id'], 'outcome': 'unrun', 'guard_mb': j['guard_mb'],
                 'report': str(Path(j['directory']) / 'behavioral-report.json'),
                 'receipt': j['receipt']} for j in jobs]
    report.update(session_reuse=True, timing_valid=False, worker_groups=statuses,
                  worker_limits={'workers': workers, 'guarded_child_budget_mb': budget_mb,
                                 'headroom': 'The selected child budget excludes OS and Python wrapper headroom.'})
    failure = None
    interrupted = False
    try:
        while pending or active:
            changed = False
            observation = controller.observe()
            with (directory / 'cohort-workloads.jsonl').open('a') as stream:
                stream.write(json.dumps(observation) + '\n')
            for index, process in list(active.items()):
                if process.poll() is None:
                    continue
                code = process.wait()
                jobs[index]['owned_children'] = dict(controller.anchors[jobs[index]['id']])
                controller.unregister(jobs[index]['id'])
                del active[index]
                receipt_path = Path(jobs[index]['receipt'])
                receipt = json.loads(receipt_path.read_text()) if receipt_path.exists() else {'outcome': 'failed', 'failure': 'worker terminal receipt missing'}
                statuses[index].update(receipt, exit_code=code)
                changed = True
                if (code or receipt.get('group_id') != jobs[index]['id']
                        or receipt.get('outcome') != 'passed' or receipt.get('passed') is not True):
                    raise RuntimeError(f"correctness group {jobs[index]['id']} {receipt.get('outcome')}: {receipt.get('failure', 'inspect group evidence')}")
                group_report = json.loads((Path(jobs[index]['directory']) / 'behavioral-report.json').read_text())
                try:
                    rows = validate_group(jobs[index], group_report, binary_hash, renderer_hash)
                except Exception as error:
                    statuses[index].update(outcome='failed', passed=False, failure=f'evidence validation: {error}')
                    raise
                for row in rows:
                    key = case_key(row)
                    if key in completed:
                        raise ValueError('duplicate completed correctness case')
                    completed[key] = row
                finished_groups[index] = group_report
            for index in select_admissions(pending, list(active), guards, workers, budget_mb):
                job = jobs[index]
                path = jobs_dir / (job['id'] + '.json')
                controller.check_quiet()
                with (jobs_dir / (job['id'] + '.log')).open('x') as log:
                    process = subprocess.Popen([sys.executable, str(Path(__file__).resolve()), '--worker', str(path)],
                                               stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
                active[index] = process
                statuses[index]['outcome'] = 'running'
                pending.remove(index)
                controller.register(job['id'], process)
                job['cohort_identity'] = {'run_id': controller.document['id'], 'group_id': job['id'],
                                          'controller': dict(controller.document['controller']),
                                          'descriptor': str(controller.path),
                                          'worker': dict(controller.document['workers'][job['id']])}
                changed = True
            report['results'] = [completed[k] for k in expected if k in completed]
            if changed:
                _write(directory / 'behavioral-report.json', report)
            if pending or active:
                time.sleep(.2)
        report['passed'] = len(completed) == len(expected) and set(completed) == set(expected)
    except BaseException as error:
        failure = error
        interrupted = isinstance(error, (KeyboardInterrupt, CohortStopped))
        report['failure'] = f'{type(error).__name__}: {error}'
        report['failure_traceback'] = traceback.format_exc()
    finally:
        if failure is not None:
            try:
                controller.abort()
            except Exception as error:
                report['cleanup_failure'] = f'cannot publish abort: {error}'
        for index, process in list(active.items()):
            try:
                if jobs[index]['id'] in controller.document['workers']:
                    controller.terminate(jobs[index]['id'], process)
                else:
                    process.terminate()
                    try: process.wait(timeout=5)
                    except subprocess.TimeoutExpired: process.kill(); process.wait()
                statuses[index].update(outcome='aborted', exit_code=process.returncode)
            except Exception as error:
                report['cleanup_failure'] = f'{type(error).__name__}: {error}'
                statuses[index].update(outcome='cleanup_failed', passed=False, failure=str(error))
                report['passed'] = False
        try:
            controller.close()
        except Exception as error:
            report['cleanup_failure'] = f'{type(error).__name__}: {error}'
            report['passed'] = False
        report['results'] = [completed[k] for k in expected if k in completed]
        fixtures = {g[0][0]['name']: g[0][0] for g in groups}
        report['session_groups'] = [session for index in sorted(finished_groups)
                                    for session in finished_groups[index]['session_groups']]
        report['coverage'] = behavioral.report_coverage(list(fixtures.values()), report['results'])
        report['complete'] = len(completed) == len(expected)
        _write(directory / 'behavioral-report.json', report)
    if interrupted:
        raise failure
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--worker', type=Path, required=True)
    args = parser.parse_args()
    raise SystemExit(_worker(args.worker))


if __name__ == '__main__':
    main()

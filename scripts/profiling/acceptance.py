"""Run selected acceptance evidence and report the maintained user-path inventory."""

import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import uuid

from native import recording_environment
from file_hash import digest
from inputs import stage_inputs
from watch_workloads import require_quiet

HERE = Path(__file__).resolve().parent
CATALOG = HERE / 'acceptance_cases.json'


def select(catalog, domains, jobs, full):
    known_domains = {p['domain'] for p in catalog['paths']}
    known_jobs = {j['id'] for j in catalog['jobs']}
    if full and (domains or jobs):
        raise ValueError('--full cannot be combined with selectors')
    if not full and not domains and not jobs:
        raise ValueError('select --domain or --job, or explicitly request --full')
    if set(domains) - known_domains or set(jobs) - known_jobs:
        raise ValueError('unknown domain or job selector')
    paths = [p for p in catalog['paths'] if (full and p['domain'] != 'latency') or p['domain'] in domains or set(p['jobs']) & set(jobs)]
    domain_jobs = {job for path in paths if path['domain'] in domains for job in path['jobs']}
    selected = [j for j in catalog['jobs'] if (full and j['driver'] != 'latency') or j['id'] in domain_jobs or j['id'] in jobs]
    return selected, paths


def command(job, args, out):
    driver = job['driver']
    def require(*names):
        for name in names:
            value = getattr(args, name, None)
            if not value:
                raise ValueError('missing --' + name.replace('_', '-'))
            if not Path(value).exists():
                raise ValueError('missing input: ' + str(value))
    if driver == 'pending':
        raise ValueError('scenario integration is pending')
    if driver == 'smoke':
        require('binary')
        if len(args.source) < job['minimum']:
            raise ValueError('requires at least %d --source inputs' % job['minimum'])
        sources = [Path(s).resolve(strict=True) for s in args.source[:job['minimum']]]
        if job.get('source_requirement') == 'tiger':
            require('tiger_source')
            sources = [Path(args.tiger_source).resolve(strict=True)]
        elif job.get('source_requirement') == 'dehaze':
            require('dehaze_source')
            sources = [Path(args.dehaze_source).resolve(strict=True)]
        if len(set(sources)) != len(sources) or any(not s.is_file() for s in sources):
            raise ValueError('sources must be distinct files')
        out.mkdir()
        inputs = stage_inputs([{'source':str(s)} for s in sources], out / 'inputs')
        (out / 'inputs.json').write_text(json.dumps(inputs, indent=2) + '\n')
        home = out / 'home'
        home.mkdir()
        evidence = out / 'evidence'
        evidence.mkdir()
        env = recording_environment(home, 'off')
        env['FOVEONIC_GUI_SMOKE_DIR'] = str(evidence)
        env['FOVEONIC_DIAGNOSTICS_CACHE_NAMESPACE'] = 'acceptance-' + uuid.uuid4().hex
        budget = job.get('disk_cache_bytes', 0)
        env['FOVEONIC_GUI_SMOKE_DISK_CACHE_BYTES'] = str(budget)
        if job.get('flag'):
            env['FOVEONIC_GUI_SMOKE_' + job['flag']] = '1'
        if not args.foreground:
            env['FOVEONIC_GUI_SCENARIO_BACKGROUND'] = '1'
        (out / 'launch.json').write_text(json.dumps({
            'job':job['id'], 'binary':str(Path(args.binary).resolve()),
            'binary_sha256':digest(args.binary), 'inputs':inputs,
            'home':str(home), 'cache_namespace':env['FOVEONIC_DIAGNOSTICS_CACHE_NAMESPACE'],
            'disk_cache_bytes':budget, 'window_mode':'foreground' if args.foreground else 'background',
        }, indent=2) + '\n')
        return [str(Path(args.binary).resolve()), inputs[0]['staged_source']], env, evidence / (job['terminal'] + '.png')
    if driver == 'scenario':
        case_input = 'choice_cases' if job['scenario'] == 'choice-journey' else 'cases'
        require('binary', case_input)
        cmd = [sys.executable, str(HERE / 'scenarios.py'), '--binary', str(Path(args.binary).resolve()),
               '--cases', str(Path(getattr(args, case_input)).resolve()), '--directory', str(out), '--scenario', job['scenario'],
               '--count', str(job.get('count', 1 if job['scenario'] in ('initial', 'warm-disk', 'warm-edit', 'memory-pressure') else 2)),
               '--mode', job.get('mode', 'off'), '--trials', '1',
               '--disk-cache-bytes', str(job.get('disk_cache_bytes', 0))]
        if job.get('mode', 'off') != 'off':
            require('trace_processor')
            cmd += ['--trace-processor', str(args.trace_processor.resolve())]
        if job.get('save_on_exit'):
            cmd += ['--save-on-exit']
        if job.get('exit_with_pending'):
            cmd += ['--exit-with-pending']
        cmd += ['--folder-mode', job.get('folder_mode', 'import')]
        if job['scenario'] in ('selection-journey', 'selection-pressure', 'transition-journey', 'lifecycle-journey', 'browser-journey', 'background-edit', 'choice-journey'):
            require('renderer')
            cmd += ['--compare-images', str(args.renderer.resolve())]
        if not args.foreground:
            cmd.append('--background')
        if job['scenario'] == 'transition-journey':
            cmd += ['--seed', str(args.seed), '--action-count', str(args.action_count)]
        return cmd, None, out / 'regression.json'
    if driver == 'latency':
        require('binary', 'cases')
        if not getattr(args, 'record_latency_baseline', False):
            require('latency_before')
        cmd = [sys.executable, str(HERE / 'scenarios.py'), '--binary', str(Path(args.binary).resolve()),
               '--cases', str(Path(args.cases).resolve()), '--directory', str(out), '--scenario', job['scenario'],
               '--count', str(job.get('count', 1)), '--mode', 'off', '--trials', str(args.latency_trials),
               '--disk-cache-bytes', str(job.get('disk_cache_bytes', 0))]
        cmd += ['--quality', job.get('quality', 'thumb')]
        cmd += ['--folder-mode', job.get('folder_mode', 'import')]
        if not args.foreground:
            cmd.append('--background')
        return cmd, None, out / 'regression.json'
    if driver == 'behavioral':
        require('binary', 'fixtures', 'slider_cases', 'renderer')
        cmd = [sys.executable, str(HERE / 'behavioral.py'), '--binary', str(Path(args.binary).resolve()),
               '--fixtures', str(Path(args.fixtures).resolve()), '--cases', str(Path(args.slider_cases).resolve()),
               '--directory', str(out), '--full', '--compare-images', str(args.renderer.resolve()),
               '--disk-cache-bytes', str(job.get('disk_cache_bytes', 0))]
        if args.foreground:
            cmd.append('--foreground')
        return cmd, None, out / 'behavioral-report.json'
    if driver == 'matrix':
        require('renderer', 'render_cases', 'baseline')
        return [sys.executable, str(HERE / 'render_matrix.py'), '--binary', str(Path(args.renderer).resolve()),
                '--cases', str(Path(args.render_cases).resolve()), '--baseline', str(Path(args.baseline).resolve()),
                '--directory', str(out)], None, out / 'manifest.json'
    raise ValueError('unknown driver: ' + driver)


def execute(job, args, root):
    out = root / job['id']
    result = {'id':job['id'], 'driver':job['driver'], 'evidence_description':job['evidence'], 'outcome':'unverified'}
    try:
        cmd, env, required = command(job, args, out)
        if job['driver'] == 'smoke':
            require_quiet()
        result['command'] = cmd
        # Delegated runners own their children and timeouts. The direct smoke is isolated
        # in a process group so a timeout or interruption only stops this launch.
        log = root / (job['id'] + '.log')
        with log.open('w') as stream:
            process = subprocess.Popen(cmd, env=env, stdout=stream, stderr=subprocess.STDOUT,
                                       start_new_session=job['driver'] == 'smoke')
            try:
                code = process.wait(timeout=args.timeout if job['driver'] == 'smoke' else None)
            except BaseException:
                if job['driver'] == 'smoke':
                    os.killpg(process.pid, signal.SIGTERM)
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        os.killpg(process.pid, signal.SIGKILL)
                        process.wait()
                else:
                    process.send_signal(signal.SIGINT)
                    process.wait()
                raise
        result.update(returncode=code, log=str(log), evidence=str(required))
        if code != 0:
            result.update(outcome='failed', reason='runner returned nonzero')
        elif not required.is_file():
            result.update(outcome='failed', reason='expected completion evidence is missing')
        elif required.suffix == '.json' and not json.loads(required.read_text()).get('passed', False):
            result.update(outcome='failed', reason='completion report did not pass')
        elif job['driver'] == 'latency':
            if json.loads(required.read_text()).get('timing_valid') is not True:
                result.update(outcome='unverified', reason='latency observations are incomplete or contaminated')
                return result
            if getattr(args, 'record_latency_baseline', False):
                result.update(outcome='observed', baseline_recorded=True)
                return result
            comparison = root / (job['id'] + '-comparison.json')
            before = Path(args.latency_before).resolve()
            if before.is_dir():
                before = before/job['id']/'regression.json'
            compare = [sys.executable, str(HERE / 'compare.py'), str(before),
                       str(required), '--output', str(comparison)]
            if args.latency_budget is not None:
                compare += ['--max-regression-percent', str(args.latency_budget)]
            with log.open('a') as stream:
                compared = subprocess.run(compare, stdout=stream, stderr=subprocess.STDOUT)
            result['comparison'] = str(comparison)
            if compared.returncode or not comparison.is_file():
                result.update(outcome='failed', reason='latency comparison failed')
            else:
                decision = json.loads(comparison.read_text()).get('passed')
                if decision is False or (args.latency_budget is not None and decision is not True):
                    result.update(outcome='failed', reason='latency budget was not met or verified')
                else:
                    result['outcome'] = 'passed' if decision is True else 'observed'
        else:
            result['outcome'] = 'passed'
        if job['driver'] in ('behavioral', 'matrix') and required.is_file():
            coverage = json.loads(required.read_text()).get('coverage', {})
            result['coverage'] = coverage
            key = 'complete_for_observed_controls' if job['driver'] == 'behavioral' else 'complete'
            result['coverage_complete'] = coverage.get(key) is True
    except (OSError, ValueError, RuntimeError, KeyError, json.JSONDecodeError, subprocess.TimeoutExpired) as error:
        result['reason'] = str(error)
    return result


def inventory(paths, results):
    by_id = {r['id']:r['outcome'] for r in results}
    incomplete = {r['id'] for r in results if r.get('coverage_complete') is False}
    rows = []
    for path in paths:
        applicable = [by_id[j] for j in path['jobs'] if j in by_id]
        success = any(x in ('passed', 'observed') for x in applicable)
        complete = (bool(path.get('checks')) and bool(path['jobs']) and not path.get('gaps')
                    and all(by_id.get(job) == 'passed' and job not in incomplete for job in path['jobs']))
        status = 'covered' if complete else 'partial' if success else 'missing'
        rows.append({**path, 'status':status, 'selected_outcomes':applicable})
    return rows


def build_report(paths, results, selected_count):
    rows = inventory(paths, results)
    complete = len(results) == selected_count
    return {'schema_version':1, 'jobs':results, 'paths':rows,
            'counts':{s:sum(row['status'] == s for row in rows) for s in ('covered', 'partial', 'missing')},
            'jobs_passed':bool(results) and complete and all(r['outcome'] in ('passed', 'observed') for r in results),
            'coverage_complete':complete and bool(rows) and all(row['status'] == 'covered' for row in rows),
            'limits':['Partial paths do not establish complete user-path coverage.', 'Smoke screenshot durations are not latency measurements.', 'Synthetic input does not establish physical presentation.']}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--domain', action='append', default=[])
    p.add_argument('--job', action='append', default=[])
    p.add_argument('--full', action='store_true')
    p.add_argument('--list', action='store_true')
    p.add_argument('--directory', type=Path)
    p.add_argument('--binary', type=Path)
    p.add_argument('--source', type=Path, action='append', default=[])
    for name in ('cases', 'fixtures', 'slider-cases', 'renderer', 'render-cases', 'baseline'):
        p.add_argument('--' + name, type=Path)
    p.add_argument('--choice-cases', type=Path, help='two choice-journey recipes: Sunlight and Incandescent white balance, neutral bias, tone enabled with contrast 0.25, and manual exposure')
    p.add_argument('--tiger-source', type=Path, help='Tiger source for the Tiger smoke suite')
    p.add_argument('--dehaze-source', type=Path, help='source with an automatic haze estimate for the Dehaze suite')
    p.add_argument('--trace-processor', type=Path, help='local Perfetto Trace Processor for capture checks')
    p.add_argument('--latency-before', type=Path, help='matched natural Off regression.json baseline')
    p.add_argument('--record-latency-baseline', action='store_true', help='record explicit latency jobs without approving or comparing a baseline')
    p.add_argument('--latency-budget', type=float, help='optional maximum regression percent')
    p.add_argument('--latency-trials', type=int, default=3)
    p.add_argument('--seed', type=int, default=1)
    p.add_argument('--action-count', type=int, default=12)
    p.add_argument('--foreground', action='store_true')
    p.add_argument('--timeout', type=int, default=600)
    args = p.parse_args()
    try:
        if args.record_latency_baseline and args.latency_before is not None:
            raise ValueError('choose latency baseline recording or comparison')
        catalog = json.loads(CATALOG.read_text())
        jobs, paths = select(catalog, args.domain, args.job, args.full)
        if args.list:
            print(json.dumps({'jobs':jobs, 'paths':paths}, indent=2))
            return
        if args.directory is None or args.timeout < 1 or args.latency_trials < 3 or args.action_count < 1:
            raise ValueError('a run needs a new --directory and a positive timeout')
        root = args.directory.resolve()
        root.mkdir(parents=True, exist_ok=False)
        results = []
        try:
            for job in jobs:
                results.append(execute(job, args, root))
                report = build_report(paths, results, len(jobs))
                (root / 'acceptance.json').write_text(json.dumps(report, indent=2) + '\n')
        finally:
            report = build_report(paths, results, len(jobs))
            (root / 'acceptance.json').write_text(json.dumps(report, indent=2) + '\n')
        if not report['jobs_passed']:
            raise SystemExit(1)
    except (OSError, ValueError, json.JSONDecodeError) as error:
        p.exit(1, str(error) + '\n')


if __name__ == '__main__':
    main()

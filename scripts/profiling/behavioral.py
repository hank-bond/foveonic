"""Run declared real-app slider cases across camera fixtures and report observed coverage."""

import argparse
import json
from pathlib import Path
import re
import signal
import subprocess
import sys

from file_hash import digest
from worker_resources import FAMILIES, validate_case_memory

SECTIONS = ('WhiteBalance', 'Mix', 'Tone', 'Saturation', 'ColorEditor', 'Noise', 'Detail', 'Sharpening', 'Corrections')


def processing_qualities(case):
    policies = case.get('processing_qualities', ['quality','speed'])
    if (not isinstance(policies, list) or not policies or len(set(policies)) != len(policies)
            or any(policy not in ('quality','speed') for policy in policies)):
        raise ValueError(f"{case['name']}: unsupported processing qualities")
    return policies


def select(fixtures, cases, fixture_names=(), families=(), case_names=(), themes=()):
    """Select declared work without silently accepting misspelled or empty selections."""
    known_fixtures = {row['name'] for row in fixtures}
    known_cases = {row['name'] for row in cases}
    known_families = {row['family'] for row in fixtures}
    for requested, available, label in ((set(fixture_names), known_fixtures, 'fixtures'),
                                       (set(families), known_families, 'families'),
                                       (set(case_names), known_cases, 'cases')):
        if requested - available:
            raise ValueError(f'unknown or unavailable {label}: {sorted(requested - available)}')
    selected_fixtures = [row for row in fixtures if (not fixture_names or row['name'] in fixture_names)
                         and (not families or row['family'] in families)]
    selected_cases = []
    for case in cases:
        if case_names and case['name'] not in case_names:
            continue
        selected_themes = [theme for theme in case.get('themes', ['dark', 'light']) if not themes or theme in themes]
        if selected_themes:
            selected_cases.append({**case, 'themes':selected_themes})
    jobs = [(fixture,{**case,'processing_quality':policy},theme) for fixture in selected_fixtures for case in selected_cases
            if fixture['family'] in case.get('families', FAMILIES) for theme in case['themes']
            for policy in processing_qualities(case)]
    if not jobs:
        raise ValueError('the selection contains no applicable test runs')
    if case_names and set(case_names) - {case['name'] for _,case,_ in jobs}:
        raise ValueError('a requested case has no applicable fixture or theme')
    if fixture_names and set(fixture_names) - {fixture['name'] for fixture,_,_ in jobs}:
        raise ValueError('a requested fixture has no applicable case')
    if families and set(families) - {fixture['family'] for fixture,_,_ in jobs}:
        raise ValueError('a requested family has no applicable case')
    return selected_fixtures, selected_cases, jobs


def validate(fixtures, cases):
    names = set()
    for fixture in fixtures:
        name, family, sources = fixture['name'], fixture['family'], fixture['sources']
        if re.fullmatch(r'[A-Za-z0-9_-]+', name) is None or name in names or family not in FAMILIES:
            raise ValueError('fixture names must be unique and simple, and families must be known')
        if not 2 <= len(sources) <= 8 or len({str(Path(s).resolve()) for s in sources}) != len(sources):
            raise ValueError(f'{name}: supply two through eight distinct source paths')
        for source in sources:
            if not Path(source).is_file():
                raise ValueError(f'{name}: missing source {source}')
        names.add(name)
    names.clear()
    for case in cases:
        name = case['name']
        validate_case_memory(case)
        if re.fullmatch(r'[A-Za-z0-9_-]+', name) is None or name in names:
            raise ValueError('case names must be unique simple filenames')
        if not case['pointer'].startswith('/') or case.get('initial_values', 'mixed') not in ('mixed', 'equal'):
            raise ValueError(f'{name}: specify a JSON pointer and mixed or equal initial values')
        if case.get('pattern', 'forward') not in ('forward', 'scrub'):
            raise ValueError(f'{name}: unsupported drag pattern')
        processing_qualities(case)
        if case['section'] not in SECTIONS:
            raise ValueError(f'{name}: unsupported editing section')
        if any(theme not in ('dark', 'light') for theme in case.get('themes', ['dark', 'light'])):
            raise ValueError(f'{name}: unsupported theme')
        if not case.get('recipes') or any(not isinstance(recipe, dict) for recipe in case['recipes']):
            raise ValueError(f'{name}: specify per-photo recipes')
        if any(family not in FAMILIES for family in case.get('families', FAMILIES)):
            raise ValueError(f'{name}: unsupported camera family')
        for fixture in fixtures:
            if fixture['family'] in case.get('families', FAMILIES) and len(case['recipes']) != len(fixture['sources']):
                raise ValueError(f"{fixture['name']}/{name}: recipes must match the source count")
        names.add(name)


def report_coverage(fixtures, results):
    tested, observed = set(), {}
    for result in results:
        key = (result['fixture'], result['theme'], result.get('processing_quality'))
        if result.get('passed'):
            tested.add((*key, result['pointer']))
        artifact = Path(result['directory'])/'trial-00/evidence/slider-drag.json'
        observation = Path(result['directory'])/'trial-00/evidence/slider-observation.json'
        if artifact.exists():
            inventory = json.loads(artifact.read_text()).get('inventory', [])
        elif observation.exists():
            inventory = (json.loads(observation.read_text()) or {}).get('controls', [])
        else:
            continue
        for control in inventory:
            if control.get('pointer'):
                row = {'fixture':key[0], 'theme':key[1], 'processing_quality':key[2], 'pointer':control['pointer'],
                       'label':control['label'], 'enabled':control['enabled'], 'visible':control['visible']}
                existing = observed.get((*key, control['pointer']))
                if existing:
                    row['enabled'] |= existing['enabled']
                    row['visible'] |= existing['visible']
                observed[(*key, control['pointer'])] = row
    missing = sorted(set(FAMILIES)-{fixture['family'] for fixture in fixtures})
    untested_families = sorted(set(FAMILIES)-{row['family'] for row in results if row.get('passed')})
    untested = [row for key,row in sorted(observed.items()) if key not in tested]
    missing_modes = [{'fixture':fixture['name'], 'processing_quality':policy}
                     for fixture in fixtures for policy in ('quality','speed')
                     if not any(row['fixture']==fixture['name'] and row.get('processing_quality')==policy and row.get('passed') for row in results)]
    return {'missing_processing_modes':missing_modes, 'missing_families': missing, 'untested_families': untested_families, 'observed_controls': len(observed),
            'tested_controls': len(tested), 'observed_untested': untested,
            'complete_for_observed_controls': bool(observed) and not missing and not missing_modes and not untested_families and not untested,
            'limits': ['Only controls drawn by the declared recipes are inventoried.',
                       'Camera family and RAW size are supplied by the fixture manifest, not independently verified.',
                       'Other camera RAW sizes, modes and conditional controls require declared fixtures and cases.']}


def run_scenario(command, log):
    """Let the scenario runner release its owned app before returning an interrupt."""
    process = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
    try:
        return process.wait()
    except BaseException:
        if process.poll() is None:
            try:
                process.send_signal(signal.SIGINT)
            except ProcessLookupError:
                pass
            try:
                # The scenario runner allows ten seconds for its app to exit.
                process.wait(timeout=20)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        raise


def run(binary, fixtures, cases, directory, mode='off', trace_processor=None, disk_cache_bytes=0, background=True, selection=None, provenance=None, compare_images=None, max_memory_mb=None, reuse_session=False,
        *, workers=1, memory_budget_mb=None, cohort=None, batch_image_verification=False, prune_verified_images=False):
    binary = binary.resolve(strict=True)
    validate(fixtures, cases)
    if not fixtures or not cases:
        raise ValueError('declare at least one fixture and case')
    if mode != 'off' and trace_processor is None:
        raise ValueError('traced behavioral runs need a trace processor')
    if disk_cache_bytes < 0:
        raise ValueError('disk cache budget must be nonnegative')
    if max_memory_mb is not None and max_memory_mb <= 0:
        raise ValueError('physical-footprint limit must be positive')
    if compare_images is not None:
        compare_images = compare_images.resolve(strict=True)
    if batch_image_verification and (mode != 'off' or compare_images is None):
        raise ValueError('batch image verification requires Off correctness mode and a fresh renderer')
    if prune_verified_images and (mode != 'off' or compare_images is None):
        raise ValueError('verified-image pruning requires Off mode and independent image verification')
    if reuse_session and (mode != 'off' or compare_images is None):
        raise ValueError('session reuse requires Off correctness mode and a fresh image renderer')
    if isinstance(workers, bool) or not isinstance(workers, int) or workers < 1:
        raise ValueError('workers must be a positive integer')
    groups, guards = None, None
    if workers > 1:
        if cohort is not None:
            raise ValueError('nested correctness worker pools are not supported')
        from slider_sessions import group_jobs
        from correctness_workers import validate_options
        groups = group_jobs(select(fixtures, cases)[2], concurrent=True, guard_mb=max_memory_mb)
        guards = validate_options(groups, workers, memory_budget_mb, max_memory_mb,
                                  mode, reuse_session, compare_images, background)
    elif memory_budget_mb is not None:
        raise ValueError('a guarded-child memory budget requires concurrent workers')
    directory = directory.resolve()
    directory.mkdir(parents=True, exist_ok=False)
    report = {'schema_version':1, 'kind':'slider-behavior-matrix', 'mode':mode,
              'binary':str(binary), 'disk_cache_bytes':disk_cache_bytes, 'background':background, 'batch_image_verification':batch_image_verification,
              'selection':selection, 'provenance':provenance, 'passed':False, 'results':[],
              'limits':['Passed describes the declared selected cases; inspect coverage for untested controls and configurations.']}
    if workers > 1:
        from correctness_workers import run_parallel
        return run_parallel(binary, compare_images, groups, guards, directory, workers,
                            memory_budget_mb, report, disk_cache_bytes, batch_image_verification=batch_image_verification, prune_verified_images=prune_verified_images)
    def prepare_case(fixture, case, theme):
        recipes = case['recipes']
        name = fixture['name']+'--'+case['name']+'--'+theme+'--'+case['processing_quality']
        out = directory/name
        inputs = directory/(name+'-inputs.json')
        inputs.write_text(json.dumps([{'source':source,'settings':recipe} for source,recipe in zip(fixture['sources'],recipes)],indent=2)+'\n')
        cmd = [sys.executable, str(Path(__file__).with_name('scenarios.py')), '--binary',str(binary),
               '--cases',str(inputs), '--directory',str(out), '--scenario','slider-drag',
               '--control-pointer',case['pointer'], '--editing-section',case['section'],
               '--initial-values',case.get('initial_values','mixed'), '--theme',theme,
               '--drag-pattern',case.get('pattern','forward'),
               '--quality',case.get('processing_quality', 'quality'), '--mode',mode, '--trials','1', '--disk-cache-bytes',str(disk_cache_bytes)]
        if compare_images:
            cmd += ['--compare-images',str(compare_images)]
        if prune_verified_images:
            cmd.append('--prune-verified-images')
        if batch_image_verification:
            cmd.append('--batch-image-verification')
        if max_memory_mb is not None:
            cmd += ['--max-memory-mb',str(max_memory_mb)]
        if trace_processor:
            cmd += ['--trace-processor',str(trace_processor)]
        if background:
            cmd += ['--background']
        return name, out, cmd

    def append_result(fixture, case, theme, name, out, returncode):
        result_path = out/'trial-00/evidence/scenario-result.json'
        native = json.loads(result_path.read_text()) if result_path.exists() else {}
        failure = native.get('failure')
        shared_path = out/'trial-00/evidence/test-result.json'
        if not failure and shared_path.exists():
            failure = json.loads(shared_path.read_text()).get('failure')
        if not native:
            failure = 'scenario did not produce a result; inspect the log'
        elif returncode and not failure:
            failure = 'scenario runner failed; inspect the log'
        report['results'].append({'fixture':fixture['name'], 'family':fixture['family'],
            'raw_size':fixture.get('raw_size','unspecified'), 'case':case['name'], 'theme':theme,
            'pointer':case['pointer'], 'processing_quality':case['processing_quality'], 'directory':str(out), 'passed':returncode==0 and native.get('passed') is True,
            'failure':failure})
        row = report['results'][-1]
        if row['passed'] and prune_verified_images:
            receipt = out/'sample-retention.json'
            row['sample_retention'] = {'path': str(receipt), 'sha256': digest(receipt)}
        if reuse_session:
            row['session'] = native.get('session')
            report['results'].sort(key=lambda r: case_order[(r['fixture'], r['case'], r['theme'], r['processing_quality'])])
        print(name+': '+('passed' if row['passed'] else 'FAILED'),flush=True)
        report['coverage'] = report_coverage(fixtures, report['results'])
        (directory/'behavioral-report.json').write_text(json.dumps(report, indent=2)+'\n')

    try:
        _,_,jobs = select(fixtures,cases)
        case_order = {(fixture['name'], case['name'], theme, case['processing_quality']): index
                      for index, (fixture, case, theme) in enumerate(jobs)}
        if reuse_session:
            from slider_sessions import group_jobs, run_group
            from scenarios import argument_parser
            report.update(session_reuse=True, timing_valid=False, session_groups=[])
            report['provenance'] = dict(provenance or {})
            session_runner = Path(__file__).with_name('slider_sessions.py').resolve()
            report['provenance']['session_runner'] = {'path': str(session_runner), 'sha256': digest(session_runner)}
            report['limits'].append('Reused application sessions are correctness evidence, not fresh-launch latency baselines.')
            for group in group_jobs(jobs):
                prepared = [(fixture, case, theme, *prepare_case(fixture, case, theme))
                            for fixture, case, theme in group]
                requests = [{'case_id': name, 'args': argument_parser().parse_args(cmd[2:])}
                            for _, _, _, name, _, cmd in prepared]
                fixture, case, theme = group[0]
                group_directory = directory/'session-groups'/f"{fixture['name']}--{theme}--{case['processing_quality']}"
                entry = {'directory': str(group_directory), 'cases': len(group), 'passed': False}
                report['session_groups'].append(entry)
                session = run_group(requests, group_directory, fixture['name'],
                                    **({} if cohort is None else {'cohort': cohort}))
                entry.update(id=session['id'], passed=session['passed'], status=session['status'], pid=session.get('pid'))
                for (fixture, case, theme, name, out, _), outcome in zip(prepared, session['cases']):
                    append_result(fixture, case, theme, name, out, 0 if outcome['passed'] else 1)
                if not session['passed']:
                    break
        else:
            for fixture, case, theme in jobs:
                name, out, cmd = prepare_case(fixture, case, theme)
                with (directory/(name+'.log')).open('w') as log:
                    returncode = run_scenario(cmd, log)
                append_result(fixture, case, theme, name, out, returncode)
        report['passed'] = (bool(report['results']) and len(report['results']) == len(jobs)
                            and all(row['passed'] for row in report['results'])
                            and (not reuse_session or all(group['passed'] for group in report['session_groups'])))
    finally:
        report['coverage'] = report_coverage(fixtures, report['results'])
        (directory/'behavioral-report.json').write_text(json.dumps(report, indent=2)+'\n')
    return report


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path)
    parser.add_argument('--fixtures',type=Path,required=True,help='JSON array of named camera-family fixtures and source paths')
    parser.add_argument('--cases',type=Path,required=True,help='JSON array of pointer, section, recipes and theme cases')
    parser.add_argument('--directory',type=Path,help='new owned output directory')
    parser.add_argument('--full',action='store_true',help='explicitly run every declared fixture, case and theme')
    parser.add_argument('--fixture',action='append',default=[],help='select a fixture by name; repeat to include more')
    parser.add_argument('--family',action='append',choices=FAMILIES,default=[],help='select a camera family; repeat to include more')
    parser.add_argument('--case',action='append',default=[],help='select a case by name; repeat to include more')
    parser.add_argument('--theme',action='append',choices=['dark','light'],default=[],help='select a theme; repeat to include both')
    parser.add_argument('--list',action='store_true',help='show the selected runs without opening the app')
    parser.add_argument('--mode',choices=['off','wall'],default='off')
    parser.add_argument('--reuse-session',action='store_true',help='opt-in slider correctness groups sharing one app per fixture, theme and quality; requires --mode off and --compare-images')
    parser.add_argument('--trace-processor',type=Path)
    parser.add_argument('--compare-images',type=Path,metavar='RENDERER',help='compare settled app frames with fresh renders')
    parser.add_argument('--disk-cache-bytes',type=int,default=0,help='optional owned decoded-cache budget; disabled by default')
    parser.add_argument('--foreground',action='store_true',help='show scenario windows in the foreground')
    parser.add_argument('--max-memory-mb',type=int,help='guard each owned app and fresh renderer physical footprint on macOS')
    parser.add_argument('--prune-verified-images',action='store_true',help='keep JSON proofs while pruning only independently checked successful raw samples')
    parser.add_argument('--batch-image-verification',action='store_true',help='verify each case in a finite fresh-graph renderer batch')
    parser.add_argument('--workers',type=int,default=1,help='maximum isolated slider correctness groups; default one')
    parser.add_argument('--memory-budget-mb',type=int,help='admission budget for guarded child processes; leave OS/Python headroom')
    args=parser.parse_args()
    try:
        filters={'fixtures':args.fixture, 'families':args.family, 'cases':args.case, 'themes':args.theme}
        if args.full and any(filters.values()):
            raise ValueError('--full cannot be combined with subset selectors')
        if not args.list and not args.full and not any(filters.values()):
            raise ValueError('choose subset selectors, or explicitly request --full')
        fixtures,cases,jobs=select(json.loads(args.fixtures.read_text()),json.loads(args.cases.read_text()),args.fixture,args.family,args.case,args.theme)
        if args.list:
            print(json.dumps({'count':len(jobs),'runs':[{'fixture':fixture['name'],'family':fixture['family'],
                  'case':case['name'],'pointer':case['pointer'],'theme':theme,'processing_quality':case['processing_quality']} for fixture,case,theme in jobs]},indent=2))
            return
        if args.binary is None or args.directory is None:
            raise ValueError('a run needs --binary and --directory')
        selection={'full':args.full, **filters}
        provenance={name:{'path':str(path.resolve()),'sha256':digest(path)} for name,path in
                    (('fixtures',args.fixtures),('cases',args.cases),('runner',Path(__file__)),('scenario_runner',Path(__file__).with_name('scenarios.py')))}
        result=run(args.binary,fixtures,cases,args.directory,args.mode,args.trace_processor,args.disk_cache_bytes,not args.foreground,selection,provenance,args.compare_images,args.max_memory_mb,args.reuse_session,
                   workers=args.workers,memory_budget_mb=args.memory_budget_mb,batch_image_verification=args.batch_image_verification,prune_verified_images=args.prune_verified_images)
    except (OSError,ValueError,KeyError,RuntimeError,json.JSONDecodeError,subprocess.TimeoutExpired) as error:
        parser.exit(1,str(error)+'\n')
    if not result['passed']:
        raise SystemExit(1)


if __name__=='__main__':
    main()

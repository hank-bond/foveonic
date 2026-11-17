"""Check acceptance selection and incomplete evidence handling without launching the app."""

import json
import copy
import sys
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import acceptance


class AcceptanceTests(unittest.TestCase):
    def setUp(self):
        self.catalog = json.loads(acceptance.CATALOG.read_text())

    def test_explicit_selection_and_unknown_names(self):
        with self.assertRaises(ValueError):
            acceptance.select(self.catalog, [], [], False)
        with self.assertRaises(ValueError):
            acceptance.select(self.catalog, ['unknown'], [], False)
        with self.assertRaises(ValueError):
            acceptance.select(self.catalog, ['editor'], [], True)
        jobs, paths = acceptance.select(self.catalog, ['browser'], [], False)
        self.assertTrue(any(j['id'] == 'smoke-browser' for j in jobs))
        self.assertTrue(any(j['id'] == 'scenario-browser-journey' for j in jobs))
        jobs, _ = acceptance.select(self.catalog, ['editor'], [], False)
        self.assertIn('scenario-lifecycle-journey', {job['id'] for job in jobs})

    def test_missing_and_partial_are_never_covered(self):
        paths = [{'domain':'x', 'path':'a', 'jobs':['one']},
                 {'domain':'x', 'path':'b', 'jobs':[]}]
        rows = acceptance.inventory(paths, [{'id':'one', 'outcome':'passed'}])
        self.assertEqual([r['status'] for r in rows], ['partial', 'missing'])
        self.assertEqual(acceptance.inventory(paths, [{'id':'one', 'outcome':'failed'}])[0]['status'], 'missing')
        declared = {'domain':'x','path':'a','jobs':['one','two'],'checks':['expected state']}
        self.assertEqual(acceptance.inventory([declared],[{'id':'one','outcome':'passed'}])[0]['status'],'partial')
        results=[{'id':job,'outcome':'passed'} for job in ('one','two')]
        self.assertEqual(acceptance.inventory([declared],results)[0]['status'],'covered')
        self.assertEqual(acceptance.inventory([declared],[results[0],{**results[1],'coverage_complete':False}])[0]['status'],'partial')
        self.assertEqual(acceptance.inventory([{**declared,'gaps':['unexercised transition']}],results)[0]['status'],'partial')

    def test_catalog_integrity(self):
        jobs = self.catalog['jobs']
        ids = [j['id'] for j in jobs]
        self.assertEqual(len(ids), len(set(ids)))
        for path in self.catalog['paths']:
            self.assertTrue(set(path['jobs']) <= set(ids))

    def test_progress_report_is_incomplete_until_all_selected_jobs_finish(self):
        paths = [{'domain':'x','path':'a','jobs':['one'],'checks':['expected state']}]
        results = [{'id':'one','outcome':'passed'}]
        pending = acceptance.build_report(paths, results, 2)
        self.assertFalse(pending['jobs_passed'])
        self.assertFalse(pending['coverage_complete'])
        done = acceptance.build_report(paths, results, 1)
        self.assertTrue(done['jobs_passed'])
        self.assertTrue(done['coverage_complete'])

    def test_optional_latency_excluded_from_full(self):
        jobs, paths = acceptance.select(self.catalog, [], [], True)
        self.assertFalse(any(job['driver'] == 'latency' for job in jobs))
        selected, _ = acceptance.select(self.catalog, [], ['latency-warm-edit-quality-1'], False)
        self.assertEqual(len(selected), 1)
        self.assertFalse(any(p['domain'] == 'latency' for p in paths))

    def test_latency_commands_cover_unique_workloads_with_explicit_single_photo_modes(self):
        class Args:
            foreground = False
            latency_trials = 3
            record_latency_baseline = True
        with tempfile.TemporaryDirectory() as tmp:
            args = Args()
            args.binary = args.cases = Path(tmp) / 'input'
            args.binary.touch()
            jobs, _ = acceptance.select(self.catalog, ['latency'], [], False)
            self.assertEqual(len(jobs), 10)
            commands = []
            for job in jobs:
                cmd, _, _ = acceptance.command(job, args, Path(tmp) / job['id'])
                index = cmd.index('--directory')
                commands.append(tuple(cmd[:index] + cmd[index + 2:]))
                self.assertEqual(cmd[cmd.index('--trials') + 1], '3')
            self.assertEqual(len(commands), len(set(commands)))
            for quality in ('quality', 'speed'):
                job = next(j for j in jobs if j['id'] == 'latency-warm-edit-' + quality + '-1')
                self.assertEqual((job['scenario'], job['count'], job['quality']), ('warm-edit', 1, quality))
            with patch.object(sys, 'argv', ['acceptance.py', '--domain', 'latency',
                                           '--latency-trials', '2', '--directory', str(Path(tmp) / 'out')]), \
                    patch('acceptance.execute') as execute, self.assertRaises(SystemExit):
                acceptance.main()
            execute.assert_not_called()
            for old in ('latency-warm-edit', 'latency-warm-edit-full-1',
                        'latency-warm-edit-fast-1', 'latency-warm-edit-draft-1'):
                with self.subTest(old=old), self.assertRaises(ValueError):
                    acceptance.select(self.catalog, [], [old], False)

    def test_transition_forwards_seed_and_action_count(self):
        class Args:
            foreground = False
            seed = 42
            action_count = 7
        with tempfile.TemporaryDirectory() as tmp:
            args = Args()
            args.binary = args.cases = Path(tmp) / 'input'
            args.binary.touch()
            args.renderer = args.binary
            args.trace_processor = args.binary
            job = next(j for j in self.catalog['jobs'] if j['id'] == 'scenario-transition-journey')
            cmd, _, _ = acceptance.command(job, args, Path(tmp) / 'out')
            self.assertEqual(cmd[cmd.index('--seed') + 1], '42')
            self.assertEqual(cmd[cmd.index('--action-count') + 1], '7')
            self.assertIn('--compare-images',cmd)

    def test_batch_verification_only_reaches_off_correctness_with_images(self):
        class Args:
            foreground = False
            batch_image_verification = True
            seed = 1
            action_count = 12
            latency_trials = 3
            record_latency_baseline = True
        with tempfile.TemporaryDirectory() as tmp:
            args = Args()
            args.binary = args.cases = args.choice_cases = args.renderer = args.trace_processor = Path(tmp) / 'input'
            args.binary.touch()
            for job in self.catalog['jobs']:
                if job['driver'] not in ('scenario', 'latency'):
                    continue
                with self.subTest(job=job['id']):
                    cmd, _, _ = acceptance.command(job, args, Path(tmp) / job['id'])
                    expected = (job['driver'] == 'scenario' and job.get('mode', 'off') == 'off'
                                and '--compare-images' in cmd)
                    self.assertEqual('--batch-image-verification' in cmd, expected)
                    if '--compare-images' in cmd:
                        self.assertEqual(cmd[cmd.index('--compare-images') + 1], str(args.renderer.resolve()))

    def test_cache_budgets_are_forwarded_for_scenarios_and_latency(self):
        class Args:
            foreground = False
            latency_trials = 3
            record_latency_baseline = True
        with tempfile.TemporaryDirectory() as tmp:
            args=Args(); args.binary=args.cases=Path(tmp)/'input';args.binary.touch()
            args.trace_processor=args.binary
            for job in self.catalog['jobs']:
                if job['driver'] not in ('scenario', 'latency'):
                    continue
                budget=job.get('disk_cache_bytes',0)
                self.assertIsInstance(budget, int)
                self.assertGreaterEqual(budget, 0)
                if job['scenario'] in ('warm-disk','prefetched-selection'):
                    self.assertGreater(budget, 0)
            for name in ('scenario-warm-disk','latency-warm-disk-8','latency-initial-1'):
                job=next(j for j in self.catalog['jobs'] if j['id']==name)
                baseline,_,_=acceptance.command(job,args,Path(tmp)/'baseline')
                args.record_latency_baseline=False; args.latency_before=args.binary
                candidate,_,_=acceptance.command(job,args,Path(tmp)/'candidate')
                self.assertEqual(baseline[baseline.index('--disk-cache-bytes')+1],str(job.get('disk_cache_bytes',0)))
                self.assertEqual(baseline[baseline.index('--disk-cache-bytes')+1],candidate[candidate.index('--disk-cache-bytes')+1])
                self.assertEqual(baseline[baseline.index('--scenario')+1],candidate[candidate.index('--scenario')+1])
                args.record_latency_baseline=True
            for quality in ('quality', 'speed'):
                job=next(j for j in self.catalog['jobs'] if j['id']=='latency-warm-edit-'+quality+'-1')
                cmd,_,_=acceptance.command(job,args,Path(tmp)/quality)
                self.assertEqual(cmd[cmd.index('--quality')+1],job['quality'])

    def test_smart_normal_and_low_memory_jobs_keep_explicit_budgets(self):
        class Args:
            foreground = False
            seed = 0
            action_count = 1
        with tempfile.TemporaryDirectory() as tmp:
            args = Args()
            args.binary = args.cases = args.renderer = args.trace_processor = Path(tmp) / 'input'
            args.binary.touch()
            for name, budget in [('scenario-transition-journey', 2147483648),
                                 ('scenario-smart-low-memory', 1073741824)]:
                job = next(j for j in self.catalog['jobs'] if j['id'] == name)
                cmd, _, _ = acceptance.command(job, args, Path(tmp) / name)
                self.assertEqual(cmd[cmd.index('--memory-cache-bytes') + 1], str(budget))
                self.assertEqual(cmd[cmd.index('--count') + 1], '4')
                self.assertEqual(cmd[cmd.index('--step-timeout') + 1], '150')

    def test_behavioral_cache_defaults_to_disabled(self):
        class Args:
            foreground = False
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp); args=Args()
            args.binary=args.fixtures=args.slider_cases=args.renderer=root/'input'
            args.binary.touch()
            job=next(j for j in self.catalog['jobs'] if j['id']=='slider-matrix')
            cmd,_,_=acceptance.command(job,args,root/'out')
            self.assertEqual(cmd[cmd.index('--disk-cache-bytes')+1],'0')
            self.assertNotIn('--reuse-session', cmd)
            args.reuse_slider_sessions = True
            grouped,_,_=acceptance.command(job,args,root/'grouped')
            self.assertIn('--reuse-session', grouped)
            args.cases = args.trace_processor = root/'input'
            initial=next(j for j in self.catalog['jobs'] if j['id']=='scenario-initial')
            separate,_,_=acceptance.command(initial,args,root/'initial')
            self.assertNotIn('--reuse-session', separate)

    def test_smoke_records_its_isolated_launch_and_explicit_cache_budget(self):
        class Args:
            foreground = False
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp); args=Args();args.binary=root/'gui';args.binary.write_bytes(b'binary')
            source=root/'photo.x3f';source.write_bytes(b'fixture');args.source=[source]
            job=next(j for j in self.catalog['jobs'] if j['id']=='smoke-browser')
            _,env,_=acceptance.command(job,args,root/'run')
            launch=json.loads((root/'run/launch.json').read_text())
            self.assertEqual(launch['disk_cache_bytes'],0)
            self.assertEqual(env['FOVEONIC_GUI_SMOKE_DISK_CACHE_BYTES'],'0')
            self.assertEqual(launch['cache_namespace'],env['FOVEONIC_DIAGNOSTICS_CACHE_NAMESPACE'])
            self.assertIn('binary_sha256',launch)

    def test_choice_journey_uses_separate_inputs_and_renderer(self):
        class Args:
            foreground = False
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);args=Args()
            args.binary=root/'app';args.binary.touch()
            args.choice_cases=root/'choices';args.choice_cases.touch()
            args.renderer=root/'renderer';args.renderer.touch()
            job=next(j for j in self.catalog['jobs'] if j['id']=='scenario-choice-journey')
            cmd,_,_=acceptance.command(job,args,root/'out')
            self.assertEqual(cmd[cmd.index('--cases')+1],str(args.choice_cases.resolve()))
            self.assertEqual(cmd[cmd.index('--count')+1],'2')
            self.assertEqual(cmd[cmd.index('--compare-images')+1],str(args.renderer.resolve()))
            del args.choice_cases
            with self.assertRaisesRegex(ValueError,'missing --choice-cases'):
                acceptance.command(job,args,root/'out')

    def test_missing_completion_is_failure(self):
        class Args:
            timeout = 1
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            job = {'id':'missing', 'driver':'scenario', 'evidence':'test'}
            with patch.object(acceptance, 'command', return_value=([str(Path('/usr/bin/true'))], None, root/'missing.json')):
                result = acceptance.execute(job, Args(), root)
            self.assertEqual(result['outcome'], 'failed')

    def merge_input(self, root, jobs, name='report'):
        results = [{'id':job['id'], 'driver':job['driver'], 'outcome':'passed',
                    'returncode':0, 'command':['remote-runner', job['id']],
                    'evidence':'/retained-host/'+job['id']+'/completion.json',
                    'log':'/retained-host/'+job['id']+'.log', 'completion_sha256':'a'*64}
                   for job in jobs]
        report = acceptance.build_report([], results, len(jobs))
        report.update(execution_identity={**acceptance.contract_identity(), 'platform':'darwin',
                      'binary_sha256':'b'*64, 'renderer_sha256':'c'*64},
                      selected_job_ids=[job['id'] for job in jobs])
        path = root/(name+'.json')
        path.write_text(json.dumps(report))
        return path, report

    def test_full_merge_retains_exact_jobs_remote_evidence_and_input_hashes(self):
        jobs, paths = acceptance.select(self.catalog, [], [], True)
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            first, a = self.merge_input(root, jobs[:2], 'first')
            second, b = self.merge_input(root, jobs[2:], 'second')
            merged = acceptance.merge_reports([second, first], self.catalog, jobs, paths)
            self.assertTrue(merged['jobs_passed'])
            self.assertEqual(merged['jobs'], a['jobs']+b['jobs'])
            self.assertEqual(merged['selected_job_ids'], [j['id'] for j in jobs])
            self.assertEqual(merged['paths'], acceptance.inventory(paths, merged['jobs']))
            self.assertEqual(merged['merged_from'][0]['sha256'], acceptance.digest(second))
            self.assertEqual(merged['execution_identity'], a['execution_identity'])

    def test_merge_rejects_incomplete_failed_duplicate_and_unpinned_reports(self):
        jobs = self.catalog['jobs'][:2]
        mutations = [
            lambda d:d.update(jobs_passed=False),
            lambda d:d.update(jobs=[]),
            lambda d:d.update(selected_job_ids=[]),
            lambda d:d.update(selected_job_ids=d['selected_job_ids']*2),
            lambda d:d.update(jobs=d['jobs']*2),
            lambda d:d.pop('execution_identity'),
            lambda d:d['execution_identity'].pop('renderer_sha256'),
            lambda d:d['execution_identity'].update(profiling_contract_sha256='d'*64),
            lambda d:d['jobs'][0].update(outcome='failed'),
            lambda d:d['jobs'][0].update(outcome='observed'),
            lambda d:d['jobs'][0].update(returncode=1),
            lambda d:d['jobs'][0].update(returncode=False),
            lambda d:d['jobs'][0].pop('completion_sha256'),
            lambda d:d['jobs'][0].update(completion_sha256='bad'),
            lambda d:d['jobs'][0].update(command=[]),
            lambda d:d['jobs'][0].update(evidence=''),
            lambda d:d['jobs'][0].update(driver='unknown'),
            lambda d:d['jobs'][0].update(id='unknown'),
        ]
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            path, original = self.merge_input(root, jobs)
            for mutate in mutations:
                with self.subTest(mutate=mutate):
                    report = copy.deepcopy(original); mutate(report)
                    path.write_text(json.dumps(report))
                    with self.assertRaises(ValueError):
                        acceptance.merge_reports([path], self.catalog, jobs, [])
            path.write_text(json.dumps(original))
            for inputs, selected in [([path, path], jobs), ([], jobs), ([path], self.catalog['jobs'][:3]),
                                     ([path], jobs[:1])]:
                with self.assertRaises(ValueError):
                    acceptance.merge_reports(inputs, self.catalog, selected, [])

    def test_merge_rejects_candidate_renderer_and_platform_mismatch(self):
        jobs = self.catalog['jobs'][:2]
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            first, _ = self.merge_input(root, jobs[:1], 'first')
            second, original = self.merge_input(root, jobs[1:], 'second')
            for key, value in [('platform','win32'), ('binary_sha256','d'*64),
                               ('renderer_sha256','d'*64)]:
                report=copy.deepcopy(original); report['execution_identity'][key]=value
                second.write_text(json.dumps(report))
                with self.assertRaisesRegex(ValueError, 'identities differ'):
                    acceptance.merge_reports([first,second], self.catalog, jobs, [])

    def test_merge_validation_precedes_output_creation(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); output=root/'output'
            path, report=self.merge_input(root, self.catalog['jobs'][:1])
            report['jobs_passed']=False;path.write_text(json.dumps(report))
            with patch('sys.argv',['acceptance','--full','--merge',str(path),'--directory',str(output)]):
                with self.assertRaises(SystemExit):acceptance.main()
            self.assertFalse(output.exists())

    def test_execution_records_selected_ids_identity_and_completion_hash(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp); binary=root/'app';binary.write_bytes(b'app');output=root/'run'
            job=self.catalog['jobs'][0]
            def run(*_):
                return {'id':job['id'],'driver':job['driver'],'outcome':'passed'}
            with patch('sys.argv',['acceptance','--job',job['id'],'--binary',str(binary),'--directory',str(output)]), \
                    patch.object(acceptance,'execute',side_effect=run):
                acceptance.main()
            report=json.loads((output/'acceptance.json').read_text())
            self.assertEqual(report['selected_job_ids'],[job['id']])
            self.assertEqual(report['execution_identity']['binary_sha256'],acceptance.digest(binary))
            evidence=root/'completion.json';evidence.write_text('{"passed":true}')
            args=type('Args',(),{'timeout':1})()
            with patch.object(acceptance,'command',return_value=([sys.executable,'-c','pass'],None,evidence)):
                result=acceptance.execute({'id':'test','driver':'scenario','evidence':'test'},args,root)
            self.assertEqual(result['completion_sha256'],acceptance.digest(evidence))
            self.assertEqual(result['outcome'],'passed')

    def test_frozen_contract_pins_driver_changes_but_not_tests_or_baselines(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp)
            for name in ('acceptance_cases.json','test_inputs.json','external_raw_samples.json'):
                (root/name).write_text('{}')
            driver=root/'scenarios.py';driver.write_text('before')
            with patch.object(acceptance,'HERE',root), patch.object(acceptance,'CATALOG',root/'acceptance_cases.json'):
                original=acceptance.contract_identity()
                (root/'test_example.py').write_text('test only')
                baseline=root/'baselines';baseline.mkdir();(baseline/'observations.json').write_text('{}')
                self.assertEqual(acceptance.contract_identity(),original)
                driver.write_text('after')
                self.assertNotEqual(acceptance.contract_identity()['profiling_contract_sha256'],
                                    original['profiling_contract_sha256'])


if __name__ == '__main__':
    unittest.main()

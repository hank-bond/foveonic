"""Check acceptance selection and incomplete evidence handling without launching the app."""

import json
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
        self.assertNotIn('latency-warm-edit', [j['id'] for j in jobs])
        selected, _ = acceptance.select(self.catalog, [], ['latency-warm-edit'], False)
        self.assertEqual(len(selected), 1)
        self.assertFalse(any(p['domain'] == 'latency' for p in paths))

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
                self.assertEqual(budget == 1073741824, job['scenario'] in ('warm-disk','prefetched-selection'))
            for name in ('scenario-warm-disk','latency-warm-disk-8','latency-initial-1'):
                job=next(j for j in self.catalog['jobs'] if j['id']==name)
                baseline,_,_=acceptance.command(job,args,Path(tmp)/'baseline')
                args.record_latency_baseline=False; args.latency_before=args.binary
                candidate,_,_=acceptance.command(job,args,Path(tmp)/'candidate')
                self.assertEqual(baseline[baseline.index('--disk-cache-bytes')+1],str(job.get('disk_cache_bytes',0)))
                self.assertEqual(baseline[baseline.index('--disk-cache-bytes')+1],candidate[candidate.index('--disk-cache-bytes')+1])
                self.assertEqual(baseline[baseline.index('--scenario')+1],candidate[candidate.index('--scenario')+1])
                args.record_latency_baseline=True
            for quality in ('full', 'fast', 'draft'):
                job=next(j for j in self.catalog['jobs'] if j['id']=='latency-warm-edit-'+quality+'-1')
                cmd,_,_=acceptance.command(job,args,Path(tmp)/quality)
                self.assertEqual(cmd[cmd.index('--quality')+1],quality)

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


if __name__ == '__main__':
    unittest.main()

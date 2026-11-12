"""Check declared native scenario inputs, isolated environment and owned-process cleanup portably."""

from contextlib import nullcontext
import json
import os
from pathlib import Path
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import MagicMock, patch

from scenarios import application_result, check_export, finish_trial, run, run_trial, scenario_environment, selected_cases, shutdown_assertions, validate_shutdown
from results import include_assertions


class ScenarioTests(unittest.TestCase):
    def setUp(self):
        quiet=patch('scenarios.require_quiet',return_value={'monotonic_ns':1,'potential_workloads':[]})
        self.quiet=quiet.start();self.addCleanup(quiet.stop)

    def test_pending_shutdown_options_and_manual_recipe_validation(self):
        args=SimpleNamespace(scenario='initial', folder_mode='browse', save_on_exit=True,
                             exit_with_pending=True, mode='wall', compare_images=None)
        cases=[{'settings':{'exposure_auto':False,'exposure':value}} for value in (0.1,3.75)]
        validate_shutdown(args,cases)
        for field,value in [('folder_mode','import'),('save_on_exit',False),('mode','off'),
                            ('scenario','selection-journey'),('compare_images',Path('renderer'))]:
            old=getattr(args,field);setattr(args,field,value)
            with self.assertRaises(ValueError):validate_shutdown(args,cases)
            setattr(args,field,old)
        for exposure,automatic in [(3.76,False),(-4.1,False),(0,True),(float('nan'),False)]:
            bad=[cases[0],{'settings':{'exposure':exposure,'exposure_auto':automatic}}]
            with self.assertRaises(ValueError):validate_shutdown(args,bad)
        with self.assertRaises(ValueError):validate_shutdown(args,cases[:1])
        args.scenario='background-edit';args.exit_with_pending=False
        with self.assertRaises(ValueError):validate_shutdown(args,cases)

    def test_pending_shutdown_requires_proof_and_latest_saved_sidecars(self):
        import struct
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary); inputs=[]; photos=[]
            for index,value in enumerate((0.1,-0.75)):
                source=root/f'{index}.x3f'; source.write_bytes(b'raw')
                initial=struct.unpack('f',struct.pack('f',value))[0]
                expected=struct.unpack('f',struct.pack('f',initial+0.25))[0]
                inputs.append({'source':str(source),'staged_source':str(source),'settings':{'exposure_auto':False,'exposure':value}})
                photos.append({'source':str(source),'initial_exposure':initial,'expected_exposure':expected,
                               'old_presented_revision':4,'old_task_revision':4})
                sidecar=root/'foveonic'/(source.name+'.foveonic.json');sidecar.parent.mkdir(exist_ok=True)
                sidecar.write_text(json.dumps({'format':'foveonic-editing','version':1,'settings':{'exposure_auto':False,'exposure':expected}}))
            pending={'delta':0.25,'requested_revision_before':4,'requested_revision_after':5,'photos':photos}
            native={'shutdown_pending':pending}
            self.assertTrue(all(row['outcome']=='passed' for row in shutdown_assertions(native,inputs)))
            pending['requested_revision_after']=4
            self.assertEqual(shutdown_assertions(native,inputs)[0]['outcome'],'failed')
            pending['requested_revision_after']=5;photos[0]['expected_exposure']=99
            self.assertEqual(shutdown_assertions(native,inputs)[0]['outcome'],'failed')
            photos[0]['expected_exposure']=0.35
            (root/'foveonic'/'0.x3f.foveonic.json').unlink()
            self.assertEqual(shutdown_assertions(native,inputs)[1]['outcome'],'failed')

    def test_export_is_decoded_and_its_extent_checked(self):
        import struct
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);image=root/'export.tiff';image.write_bytes(b'encoded image')
            decoded=root/'decoded.png'
            def decode(*_args, **_kwargs):
                decoded.write_bytes(b'\x89PNG\r\n\x1a\n'+struct.pack('>I',13)+b'IHDR'+struct.pack('>II',640,480))
                return SimpleNamespace(returncode=0,stderr='')
            with patch('scenarios.platform.system',return_value='Darwin'), patch('scenarios.subprocess.run',side_effect=decode):
                self.assertEqual(check_export(image,[640,480],decoded)['outcome'],'passed')
                self.assertEqual(check_export(image,[640,481],decoded)['outcome'],'failed')
            with patch('scenarios.platform.system',return_value='Darwin'), patch('scenarios.subprocess.run',return_value=SimpleNamespace(returncode=1,stderr='damaged TIFF')):
                self.assertEqual(check_export(image,[640,480],decoded)['outcome'],'failed')
            self.assertEqual(check_export(root/'missing',[640,480],decoded)['outcome'],'failed')

    def test_trace_behavior_and_missing_evidence_reach_the_shared_result(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);(root/'evidence').mkdir();(root/'capture').mkdir()
            native={'passed':True,'steps':[{'step':1,'action':'cached-switch','photos':[
                {'source':'staged.x3f','family':'Merrill','scale_divisor':8}]}]}
            (root/'evidence/scenario-result.json').write_text(json.dumps(native))
            inputs=[{'staged_source':'staged.x3f','source':'original.x3f','sha256':'raw-hash'}]
            for complete,behavior,expected in ((True,True,'passed'),(True,False,'failed'),(False,False,'incomplete')):
                (root/'capture/summary.json').write_text('{}')
                (root/'capture/manifest.json').write_text('{"application":{}}')
                analysis={'validity':{'valid':complete},'behavior_valid':behavior,
                          'steps':[{'step':1,'action':'cached-switch','photo_count':1}],
                          'behavior_failures':[] if behavior else ['unexpected photographic dispatch']}
                with patch('scenarios.analyze_capture',return_value=analysis):
                    trial=finish_trial(root,'wall',None)
                result=application_result(native,'cached-switch','thumb',inputs)
                include_assertions(result,trial['trace_assertions'])
                self.assertEqual(result['outcome'],expected)
                if complete and not behavior:
                    self.assertIn('unexpected photographic dispatch',result['failure'])

    def test_trace_must_include_every_completed_step_in_order(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);(root/'evidence').mkdir();(root/'capture').mkdir()
            native={'passed':True,'steps':[
                {'step':1,'action':'initial-selection','photos':[{'source':'staged.x3f'}]},
                {'step':2,'action':'warm-edit','photos':[{'source':'staged.x3f'}]}]}
            (root/'evidence/scenario-result.json').write_text(json.dumps(native))
            steps=[{'step':step['step'],'action':step['action'],'photo_count':1} for step in native['steps']]
            for traced,expected in ((steps,True),(steps[:1],False),(steps[::-1],False),
                                    ([steps[0],{**steps[1],'photo_count':2}],False),
                                    ([steps[0],{**steps[1],'action':'cached-switch'}],False)):
                (root/'capture/summary.json').write_text('{}')
                (root/'capture/manifest.json').write_text('{"application":{}}')
                analysis={'validity':{'valid':True},'behavior_valid':True,'steps':traced,'behavior_failures':[]}
                with patch('scenarios.analyze_capture',return_value=analysis):
                    trial=finish_trial(root,'wall',None)
                self.assertEqual(trial['passed'],expected)
                assertion=next(row for row in trial['trace_assertions'] if row['name']=='trace_steps')
                self.assertEqual(assertion['outcome'],'passed' if expected else 'failed')

    def test_native_quality_contract_and_distinct_inputs(self):
        with tempfile.TemporaryDirectory() as temporary:
            paths=[Path(temporary)/f'{index}.x3f' for index in range(2)]
            for path in paths:path.write_bytes(b'original')
            cases=[{'source':str(path)} for path in paths]
            self.assertEqual(len(selected_cases(cases,1,'draft','initial')),1)
            self.assertEqual(len(selected_cases(cases,1,'full','initial')),1)
            self.assertEqual(len(selected_cases(cases,1,'fast','initial')),1)
            for count,quality,name in [(2,'draft','initial'),(1,'thumb','cached-switch'),(1,'thumb','choice-journey'),(3,'thumb','initial')]:
                with self.assertRaises(ValueError):selected_cases(cases,count,quality,name)
            with self.assertRaises(ValueError):selected_cases([cases[0],cases[0]],2,'thumb','initial')


    def test_environment_cannot_inherit_another_smoke_or_capture(self):
        with patch.dict(os.environ,{'FOVEONIC_PROFILE':'gpu','FOVEONIC_GUI_SMOKE_MULTI':'1','FOVEONIC_GUI_SCENARIO_MANIFEST':'old','FOVEONIC_GUI_SCENARIO_BACKGROUND':'old','FOVEONIC_DIAGNOSTICS_DIR':'old'}):
            env=scenario_environment(Path('/owned/home'),Path('/owned/evidence'),Path('/owned/launch.json'))
            self.assertEqual(env['FOVEONIC_PROFILE'],'off')
            self.assertEqual(env['FOVEONIC_GUI_SCENARIO_MANIFEST'],'/owned/launch.json')
            self.assertNotIn('FOVEONIC_GUI_SMOKE_MULTI',env)
            self.assertNotIn('FOVEONIC_GUI_SCENARIO_BACKGROUND',env)
            self.assertNotIn('FOVEONIC_DIAGNOSTICS_DIR',env)

    def arguments(self,root):
        binary=root/'app';binary.write_bytes(b'executable')
        source=root/'source.x3f';source.write_bytes(b'raw')
        args=SimpleNamespace(binary=binary,scenario='warm-edit',count=1,quality='thumb',mode='off',repeats=2,rapid_edits=3,pressure_mib=128,step_timeout=10,no_monitor=True,timeout=30,trace_processor=None,disk_cache_bytes=10_000_000_000,memory_cache_bytes=1024*1024*1024)
        return args,[{'source':str(source)}]

    def test_owned_process_result_and_manifest_keep_off_baseline_separate(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);args,cases=self.arguments(root)
            process=MagicMock();process.wait.return_value=0;process.poll.return_value=0
            def launch(_command,**kwargs):
                env=kwargs['env'];evidence=Path(env['FOVEONIC_GUI_SMOKE_DIR'])
                (evidence/'scenario-result.json').write_text(json.dumps({'passed':True,'memory_cache_bytes':1024*1024*1024,'disk_cache':{'enabled':True,'max_bytes':10_000_000_000},'steps':[{'action':'initial-selection','baseline_elapsed_ms':8.,'photos':[{'source':str(evidence.parent/'inputs'/'00-source.x3f'),'family':'Merrill','settings':{'exposure':0.},'scale_divisor':8}]}]}))
                return process
            with patch('scenarios.subprocess.Popen',side_effect=launch):
                result=run_trial(args,cases,root/'run','tree-hash')
            self.assertTrue(result['passed']);self.assertEqual(result['input_kind'],'baseline')
            self.assertEqual(result['test_result']['driver'],'application')
            self.assertEqual(result['test_result']['outcome'],'passed')
            self.assertEqual(result['test_result']['environment']['binary_sha256'],result['artifacts']['binary_sha256'])
            self.assertTrue((root/'run'/'evidence'/'test-result.json').exists())
            self.assertFalse(result['timing_valid']);self.assertFalse(result['contention']['checked'])
            self.assertFalse((root/'run'/'capture'/'trace.pftrace').exists())
            manifest=json.loads((root/'run'/'launch.json').read_text())
            self.assertEqual(manifest['source_tree_sha256'],'tree-hash')
            self.assertEqual(manifest['scenario']['quality'],'thumb')
            process.terminate.assert_not_called()

    def test_background_is_explicit_and_in_the_workload_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);args,cases=self.arguments(root);args.background=True
            process=MagicMock();process.wait.return_value=0;process.poll.return_value=0
            def launch(_command,**kwargs):
                env=kwargs['env'];self.assertEqual(env['FOVEONIC_GUI_SCENARIO_BACKGROUND'],'1')
                evidence=Path(env['FOVEONIC_GUI_SMOKE_DIR'])
                (evidence/'scenario-result.json').write_text(json.dumps({'passed':True,'memory_cache_bytes':1073741824,'disk_cache':{'enabled':True,'max_bytes':10000000000},'steps':[{'action':'initial-selection','photos':[{'source':str(evidence.parent/'inputs'/'00-source.x3f'),'family':'Merrill','settings':{},'scale_divisor':8}]}]}))
                return process
            with patch('scenarios.subprocess.Popen',side_effect=launch):
                result=run_trial(args,cases,root/'run','tree-hash')
            self.assertEqual(result['workload_key']['descriptor']['measurement']['window_mode'],'background')

    def test_competing_app_blocks_launch_before_any_child_app(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);args,cases=self.arguments(root)
            self.quiet.side_effect=RuntimeError('profiling postponed: other Foveonic')
            with patch('scenarios.subprocess.Popen') as launch:
                with self.assertRaisesRegex(RuntimeError,'other Foveonic'):
                    run_trial(args,cases,root/'run','tree')
                launch.assert_not_called()

    def test_contention_invalidates_timing_without_changing_behavior(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);args,cases=self.arguments(root);args.no_monitor=False
            process=MagicMock();process.pid=123;process.wait.return_value=0;process.poll.return_value=0
            contention={'checked':True,'uncontended':False,'samples':3,'other_workloads':[{'pid':456,'executable':'rustc'}]}
            def launch(_command,**kwargs):
                evidence=Path(kwargs['env']['FOVEONIC_GUI_SMOKE_DIR'])
                (evidence/'scenario-result.json').write_text(json.dumps({'passed':True,'memory_cache_bytes':1073741824,'disk_cache':{'enabled':True,'max_bytes':10000000000},'steps':[{'action':'initial-selection','photos':[{'source':str(evidence.parent/'inputs'/'00-source.x3f'),'family':'Merrill','settings':{},'scale_divisor':8}]}]}))
                return process
            with patch('scenarios.workload_monitor',return_value=nullcontext()),patch('scenarios.concurrent_workloads',return_value=contention) as checked,patch('scenarios.subprocess.Popen',side_effect=launch):
                result=run_trial(args,cases,root/'run','tree')
            self.assertTrue(result['passed']);self.assertFalse(result['timing_valid'])
            checked.assert_called_once_with(root/'run'/'workloads.jsonl',owned_pids=[123])
            self.assertEqual(json.loads((root/'run'/'launch.json').read_text())['contention'],contention)

    def test_application_result_checks_the_same_classic_thumb_contract(self):
        native={'passed':True,'steps':[{'action':'initial-selection','photos':[{'source':'sample.x3f','family':'Classic','scale_divisor':8}]}]}
        result=application_result(native,'initial','thumb',[{'source':'sample.x3f','staged_source':'sample.x3f','sha256':'sample-hash'}])
        self.assertEqual(result['outcome'],'failed')
        self.assertEqual(result['assertions'][0]['expected'],4)
        self.assertEqual(result['assertions'][0]['actual'],8)

    def test_application_result_requires_the_declared_source_identity(self):
        native={'passed':True,'steps':[{'action':'initial-selection','photos':[{'source':'other.x3f','family':'Merrill','scale_divisor':8}]}]}
        result=application_result(native,'initial','thumb',[{'source':'a.x3f','staged_source':'staged.x3f','sha256':'source-hash'}])
        self.assertEqual(result['outcome'],'failed')
        self.assertIn('undeclared staged inputs',result['failure'])

    def test_shared_assertion_can_fail_a_successful_app_scenario(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);args,cases=self.arguments(root)
            process=MagicMock();process.wait.return_value=0;process.poll.return_value=0
            def launch(_command,**kwargs):
                evidence=Path(kwargs['env']['FOVEONIC_GUI_SMOKE_DIR'])
                native={'passed':True,'memory_cache_bytes':1073741824,
                        'disk_cache':{'enabled':True,'max_bytes':10000000000},
                        'steps':[{'action':'initial-selection','photos':[{'source':str(evidence.parent/'inputs'/'00-source.x3f'),
                            'family':'Classic','settings':{},'scale_divisor':8}]}]}
                (evidence/'scenario-result.json').write_text(json.dumps(native))
                return process
            with patch('scenarios.subprocess.Popen',side_effect=launch):
                result=run_trial(args,cases,root/'run','tree-hash')
            self.assertFalse(result['passed'])
            self.assertEqual(result['test_result']['outcome'],'failed')
            self.assertEqual(result['test_result']['assertions'][0]['expected'],4)
            self.assertEqual(json.loads((root/'run'/'evidence'/'test-result.json').read_text()),result['test_result'])


    def test_image_comparison_failure_fails_successful_app_trial(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);args,cases=self.arguments(root)
            args.compare_images=root/'fresh-renderer';args.compare_images.write_bytes(b'renderer')
            process=MagicMock();process.wait.return_value=0;process.poll.return_value=0
            def launch(_command,**kwargs):
                evidence=Path(kwargs['env']['FOVEONIC_GUI_SMOKE_DIR'])
                native={'passed':True,'memory_cache_bytes':1073741824,
                        'disk_cache':{'enabled':True,'max_bytes':10000000000},
                        'steps':[{'action':'initial-selection','photos':[{'source':str(evidence.parent/'inputs'/'00-source.x3f'),
                            'family':'Merrill','settings':{},'scale_divisor':8}]}]}
                (evidence/'scenario-result.json').write_text(json.dumps(native))
                return process
            failure={'name':'image_samples','outcome':'failed'}
            with patch('scenarios.subprocess.Popen',side_effect=launch),patch('scenarios.compare_scenario_images',return_value=[failure]):
                result=run_trial(args,cases,root/'run','tree-hash')
            self.assertFalse(result['passed'])
            self.assertEqual(result['test_result']['failure'],'image_samples assertion failed')
            self.assertTrue(json.loads((root/'run'/'launch.json').read_text())['scenario']['capture_images'])

    def test_runner_rejects_mixed_workload_keys_and_preserves_failed_report(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);args,cases=self.arguments(root)
            args.cases=None;args.source=[Path(cases[0]['source'])];args.cache_namespace='test'
            args.directory=root/'suite';args.trials=2
            trials=[{'passed':True,'timing_valid':True,'mode':'off','input_kind':'baseline',
                     'steps':[{'action':'initial-selection','baseline_elapsed_ms':1.}],
                     'workload_key':{'sha256':name}} for name in ('one','two')]
            with patch('scenarios.source_tree_hash',return_value='tree'),patch('scenarios.run_trial',side_effect=trials):
                with self.assertRaisesRegex(ValueError,'different matched workload keys'):
                    run(args)
            report=json.loads((args.directory/'regression.json').read_text())
            self.assertFalse(report['passed']);self.assertFalse(report['workload_consistent'])
            self.assertIsNone(report['workload_key'])

    def test_timeout_terminates_only_owned_child_and_preserves_artifacts(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);args,cases=self.arguments(root)
            process=MagicMock();process.wait.side_effect=[subprocess.TimeoutExpired('owned app',30),0];process.poll.return_value=None
            with patch('scenarios.subprocess.Popen',return_value=process):
                with self.assertRaises(subprocess.TimeoutExpired):run_trial(args,cases,root/'run','tree')
            process.terminate.assert_called_once();self.assertTrue((root/'run'/'launch.json').exists())
            self.assertTrue((root/'run'/'stderr.txt').exists())


if __name__=='__main__':unittest.main()

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

from scenarios import run, run_trial, scenario_environment, selected_cases, stage_inputs


class ScenarioTests(unittest.TestCase):
    def setUp(self):
        quiet=patch('scenarios.require_quiet',return_value={'monotonic_ns':1,'potential_workloads':[]})
        self.quiet=quiet.start();self.addCleanup(quiet.stop)

    def test_native_quality_contract_and_distinct_inputs(self):
        with tempfile.TemporaryDirectory() as temporary:
            paths=[Path(temporary)/f'{index}.x3f' for index in range(2)]
            for path in paths:path.write_bytes(b'original')
            cases=[{'source':str(path)} for path in paths]
            self.assertEqual(len(selected_cases(cases,1,'draft','initial')),1)
            self.assertEqual(len(selected_cases(cases,1,'full','initial')),1)
            self.assertEqual(len(selected_cases(cases,1,'fast','initial')),1)
            for count,quality,name in [(2,'draft','initial'),(1,'thumb','cached-switch'),(3,'thumb','initial')]:
                with self.assertRaises(ValueError):selected_cases(cases,count,quality,name)
            with self.assertRaises(ValueError):selected_cases([cases[0],cases[0]],2,'thumb','initial')

    def test_inputs_are_hardlinked_and_recipe_writes_are_owned(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);source=root/'source.x3f';source.write_bytes(b'RAW sample bytes')
            cases=[{'source':str(source),'settings':{'exposure':.25}}]
            records=stage_inputs(cases,root/'inputs')
            staged=Path(records[0]['staged_source'])
            self.assertEqual(source.stat().st_ino,staged.stat().st_ino)
            self.assertEqual(source.read_bytes(),b'RAW sample bytes')
            recipe=json.loads((staged.parent/'foveonic'/(staged.name+'.foveonic.json')).read_text())
            self.assertEqual(recipe,{'format':'foveonic-editing','version':1,'settings':{'exposure':.25}})
            self.assertFalse((root/'foveonic').exists())

    def test_environment_cannot_inherit_another_smoke_or_capture(self):
        with patch.dict(os.environ,{'FOVEONIC_PROFILE':'gpu','FOVEONIC_GUI_SMOKE_MULTI':'1','FOVEONIC_GUI_SCENARIO_MANIFEST':'old','FOVEONIC_DIAGNOSTICS_DIR':'old'}):
            env=scenario_environment(Path('/owned/home'),Path('/owned/evidence'),Path('/owned/launch.json'))
            self.assertEqual(env['FOVEONIC_PROFILE'],'off')
            self.assertEqual(env['FOVEONIC_GUI_SCENARIO_MANIFEST'],'/owned/launch.json')
            self.assertNotIn('FOVEONIC_GUI_SMOKE_MULTI',env)
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
                (evidence/'scenario-result.json').write_text(json.dumps({'passed':True,'memory_cache_bytes':1024*1024*1024,'disk_cache':{'enabled':True,'max_bytes':10_000_000_000},'steps':[{'action':'initial-selection','baseline_elapsed_ms':8.,'photos':[{'source':str(evidence.parent/'inputs'/'00-source.x3f'),'settings':{'exposure':0.},'scale_divisor':8}]}]}))
                return process
            with patch('scenarios.subprocess.Popen',side_effect=launch):
                result=run_trial(args,cases,root/'run','tree-hash')
            self.assertTrue(result['passed']);self.assertEqual(result['input_kind'],'baseline')
            self.assertFalse(result['timing_valid']);self.assertFalse(result['contention']['checked'])
            self.assertFalse((root/'run'/'capture'/'trace.pftrace').exists())
            manifest=json.loads((root/'run'/'launch.json').read_text())
            self.assertEqual(manifest['source_tree_sha256'],'tree-hash')
            self.assertEqual(manifest['scenario']['quality'],'thumb')
            process.terminate.assert_not_called()

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
                (evidence/'scenario-result.json').write_text('{"passed":true,"memory_cache_bytes":1073741824,"disk_cache":{"enabled":true,"max_bytes":10000000000},"steps":[]}')
                return process
            with patch('scenarios.workload_monitor',return_value=nullcontext()),patch('scenarios.concurrent_workloads',return_value=contention) as checked,patch('scenarios.subprocess.Popen',side_effect=launch):
                result=run_trial(args,cases,root/'run','tree')
            self.assertTrue(result['passed']);self.assertFalse(result['timing_valid'])
            checked.assert_called_once_with(root/'run'/'workloads.jsonl',owned_pids=[123])
            self.assertEqual(json.loads((root/'run'/'launch.json').read_text())['contention'],contention)

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

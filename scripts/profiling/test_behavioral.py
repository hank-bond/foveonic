"""Check behavioral matrix coverage, fixture validation and preserved results."""

from contextlib import redirect_stdout
import io
import json
import signal
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from behavioral import report_coverage, run, run_scenario, select, validate


class BehavioralTests(unittest.TestCase):
    def test_subsets_are_explicit_and_misspelled_or_inapplicable_cases_fail(self):
        fixtures=[{'name':'m','family':'merrill'}, {'name':'c','family':'classic'}]
        cases=[{'name':'exposure','pointer':'/exposure'}, {'name':'speckle','pointer':'/speckle','families':['merrill']}]
        selected_fixtures, selected_cases, jobs=select(fixtures,cases,families=['merrill'],case_names=['speckle'],themes=['dark'])
        self.assertEqual([row['name'] for row in selected_fixtures],['m'])
        self.assertEqual([row['name'] for row in selected_cases],['speckle'])
        self.assertEqual([(f['name'],c['name'],theme) for f,c,theme in jobs],[('m','speckle','dark'),('m','speckle','dark')])
        with self.assertRaisesRegex(ValueError,'unknown or unavailable cases'):
            select(fixtures,cases,case_names=['typo'])
        with self.assertRaisesRegex(ValueError,'no applicable'):
            select(fixtures,cases,families=['classic'],case_names=['speckle'])

    def test_observed_but_untested_controls_and_missing_families_are_visible(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp); evidence=root/'trial-00/evidence';evidence.mkdir(parents=True)
            controls=[{'pointer':'/exposure','label':'Exposure','enabled':True,'visible':True},
                      {'pointer':'/tone/contrast','label':'Contrast','enabled':True,'visible':False}]
            (evidence/'slider-drag.json').write_text(json.dumps({'inventory':controls}))
            fixture=[{'name':'a','family':'merrill'}]
            result=[{'fixture':'a','family':'merrill','theme':'dark','pointer':'/exposure','passed':True,'directory':str(root)}]
            coverage=report_coverage(fixture,result)
            self.assertEqual(coverage['tested_controls'],1)
            self.assertEqual([row['pointer'] for row in coverage['observed_untested']],['/tone/contrast'])
            self.assertEqual(coverage['missing_families'],['classic','quattro','tiger'])
            self.assertFalse(coverage['complete_for_observed_controls'])

    def test_matrix_runs_declared_themes_and_preserves_per_case_inputs(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp); binary=root/'app';binary.write_bytes(b'app')
            sources=[root/'a.x3f',root/'b.x3f']
            for path in sources:path.write_bytes(b'raw')
            fixtures=[{'name':'merrill-pair','family':'merrill','sources':list(map(str,sources))}]
            cases=[{'name':'exposure','pointer':'/exposure','section':'Tone','recipes':[{'exposure':1},{'exposure':-1}]}]
            renderer=root/'renderer';renderer.write_bytes(b'renderer')
            commands=[]
            def fake_run(cmd, log):
                commands.append(cmd)
                out=Path(cmd[cmd.index('--directory')+1]); evidence=out/'trial-00/evidence';evidence.mkdir(parents=True)
                (evidence/'scenario-result.json').write_text('{"passed":true}')
                (evidence/'slider-drag.json').write_text(json.dumps({'inventory':[{'pointer':'/exposure','label':'Exposure','enabled':True,'visible':True}]}))
                return 0
            with patch('behavioral.run_scenario',side_effect=fake_run), redirect_stdout(io.StringIO()):
                report=run(binary,fixtures,cases,root/'result',compare_images=renderer)
            self.assertTrue(report['passed']);self.assertEqual(len(report['results']),4)
            self.assertTrue(all(cmd[cmd.index('--compare-images')+1] == str(renderer.resolve()) for cmd in commands))
            self.assertEqual({row['theme'] for row in report['results']},{'dark','light'})
            self.assertEqual({row['processing_quality'] for row in report['results']},{'quality','speed'})
            self.assertEqual({cmd[cmd.index('--quality')+1] for cmd in commands},{'quality','speed'})
            stored=json.loads((root/'result'/'merrill-pair--exposure--dark--quality-inputs.json').read_text())
            self.assertEqual([item['settings']['exposure'] for item in stored],[1,-1])
            self.assertFalse(report['coverage']['complete_for_observed_controls'])

    def test_interrupt_is_forwarded_and_child_is_reaped(self):
        child = unittest.mock.Mock()
        child.wait.side_effect = [KeyboardInterrupt(), 130]
        child.poll.return_value = None
        with patch('behavioral.subprocess.Popen', return_value=child):
            with self.assertRaises(KeyboardInterrupt):
                run_scenario(['scenario'], io.StringIO())
        child.send_signal.assert_called_once_with(signal.SIGINT)
        self.assertEqual(child.wait.call_count, 2)
        child.wait.assert_called_with(timeout=20)
        child.kill.assert_not_called()

    def test_unresponsive_scenario_is_killed_and_reaped(self):
        child = unittest.mock.Mock()
        child.wait.side_effect = [KeyboardInterrupt(), subprocess.TimeoutExpired('scenario', 20), -9]
        child.poll.return_value = None
        with patch('behavioral.subprocess.Popen', return_value=child):
            with self.assertRaises(KeyboardInterrupt):
                run_scenario(['scenario'], io.StringIO())
        child.send_signal.assert_called_once_with(signal.SIGINT)
        child.kill.assert_called_once_with()
        self.assertEqual(child.wait.call_count, 3)

    def test_fixture_and_recipe_problems_are_rejected_before_launch(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);a=root/'a';b=root/'b';a.write_bytes(b'a');b.write_bytes(b'b')
            fixture=[{'name':'pair','family':'classic','sources':[str(a),str(b)]}]
            case=[{'name':'x','pointer':'/exposure','section':'Tone','recipes':[{}]}]
            with self.assertRaisesRegex(ValueError,'recipes must match'):
                validate(fixture,case)
            with self.assertRaisesRegex(ValueError,'distinct source paths'):
                validate([{**fixture[0],'sources':[str(a),str(a)]}],[])

    def test_command_requires_a_subset_or_full_and_list_does_not_launch(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp)
            fixtures=root/'fixtures.json';cases=root/'cases.json'
            fixtures.write_text(json.dumps([{'name':'m','family':'merrill','sources':['a','b']}]))
            cases.write_text(json.dumps([{'name':'exposure','pointer':'/exposure','section':'Tone','recipes':[{},{}]}]))
            command=[sys.executable,str(Path(__file__).with_name('behavioral.py')),'--fixtures',str(fixtures),'--cases',str(cases)]
            missing=subprocess.run(command,capture_output=True,text=True)
            self.assertNotEqual(missing.returncode,0)
            self.assertIn('choose subset selectors',missing.stderr)
            listed=subprocess.run(command+['--list','--case','exposure','--theme','dark'],capture_output=True,text=True)
            self.assertEqual(listed.returncode,0)
            self.assertEqual(json.loads(listed.stdout)['count'],2)


if __name__=='__main__':
    unittest.main()

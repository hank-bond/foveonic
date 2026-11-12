"""Check exact matrix coverage, immutable baselines and failure reporting without a GPU."""

from contextlib import redirect_stdout
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from render_matrix import expand_cases, run
from scenarios import application_result


FAKE_RENDERER = '''#!/usr/bin/env python3
import hashlib, json, pathlib, sys
source=pathlib.Path(sys.argv[1]); output=pathlib.Path(sys.argv[2]); output.mkdir()
quality=sys.argv[sys.argv.index('--preview-quality')+1]
goldens=pathlib.Path(sys.argv[sys.argv.index('--goldens')+1]); goldens.mkdir(parents=True, exist_ok=True)
golden=goldens/(source.stem+'.f32')
samples=hashlib.sha256(source.read_bytes()+quality.encode()).digest()[:24]
if golden.exists() and golden.read_bytes()!=samples:
    sys.exit('non-equivalent output')
if not golden.exists():golden.write_bytes(samples)
(output/(source.stem+'-0.tiff')).write_bytes(b'tiff')
scale={'full':1,'fast':2,'draft':4,'thumb':8}[quality]
for event,data in [('session',{'settings':{'mode':'test'},'scale_divisor':scale,'family':'Merrill'}),('processing',{'effective_scale_divisor':scale}),('validation',{'digest':'abc','extent':[1,2],'layout':'Rgb32Float','space':'test','linear_white':1,'view_baked':False,'endianness':'little','view_extent':[1,2],'view_crop':[0,0,1,2],'view_rotation':0})]:
    print(json.dumps({'event':event,'data':data}))
'''


class MatrixTests(unittest.TestCase):
    def test_coverage_uses_distinct_sources_and_reports_missing_qualities(self):
        with tempfile.TemporaryDirectory() as tmp:
            path=Path(tmp)/'sample.x3f';path.write_bytes(b'source')
            _, coverage=expand_cases([{'label':'a','family':'classic','source':str(path),'qualities':['thumb']},
                                      {'label':'b','family':'classic','source':str(path)}])
            self.assertEqual(coverage['families']['classic']['provided'],1)
            self.assertEqual(coverage['families']['classic']['missing'],2)
            self.assertEqual(len(coverage['missing_qualities']),3)
            self.assertFalse(coverage['complete'])

    def test_record_repeat_and_changed_input_never_rewrite_the_baseline(self):
        with tempfile.TemporaryDirectory() as tmp, patch('render_matrix.require_quiet'), redirect_stdout(io.StringIO()):
            root=Path(tmp); binary=root/'renderer';binary.write_text(FAKE_RENDERER);binary.chmod(0o755)
            source=root/'sample.x3f';source.write_bytes(b'original')
            cases=[{'label':'sample','family':'merrill','source':str(source),'qualities':['thumb']}]
            first=run(binary,cases,root/'baseline')
            self.assertTrue(first['passed']);self.assertFalse(first['coverage']['complete'])
            self.assertEqual(first['cases'][0]['test_result']['outcome'],'passed')
            self.assertEqual(first['cases'][0]['test_result']['assertions'][0]['name'],'preview_scale')
            self.assertEqual(first['cases'][0]['test_result']['environment']['binary_sha256'],first['binary_sha256'])
            headless=first['cases'][0]['test_result']
            native={'passed':True,'steps':[{'action':'initial-selection','photos':[{'source':'staged.x3f',
                    'family':'Merrill','scale_divisor':8}]}]}
            inputs=[{'source':str(source.resolve()),'staged_source':'staged.x3f','sha256':first['cases'][0]['source_sha256']}]
            application=application_result(native,'initial','thumb',inputs)
            self.assertEqual(set(headless),set(application))
            for key in ('source','source_sha256','family','scale_divisor'):
                self.assertEqual(headless['observations'][0][key],application['observations'][0][key])
            for key in ('name','expected','actual','outcome'):
                self.assertEqual(headless['assertions'][0][key],application['assertions'][0][key])
            golden=root/'baseline/goldens/sample-thumb/sample.f32'
            original=golden.read_bytes()
            repeated=run(binary,cases,root/'repeat',root/'baseline')
            self.assertTrue(repeated['passed'])
            self.assertEqual(repeated['cases'][0]['test_result']['assertions'][-1]['name'],'image_samples')
            binary.write_text(FAKE_RENDERER.replace(".digest()[:24]", ".digest()[-24:]"))
            changed_output=run(binary,cases,root/'different-output',root/'baseline')
            self.assertFalse(changed_output['passed'])
            self.assertEqual(changed_output['cases'][0]['test_result']['assertions'][-1]['outcome'],'failed')
            self.assertEqual(golden.read_bytes(),original)
            source.write_bytes(b'changed')
            with self.assertRaisesRegex(ValueError,'different source identities'):
                run(binary,cases,root/'changed',root/'baseline')
            self.assertEqual(golden.read_bytes(),original)
            golden.unlink()
            source.write_bytes(b'original')
            result=run(binary,cases,root/'missing',root/'baseline')
            self.assertFalse(result['passed'])
            self.assertIn('missing or modified',result['cases'][0]['failure'])
            self.assertFalse(golden.exists())
            self.assertFalse(json.loads((root/'missing/manifest.json').read_text())['passed'])

    def test_headless_report_fails_when_the_delivered_scale_violates_the_contract(self):
        with tempfile.TemporaryDirectory() as tmp, patch('render_matrix.require_quiet'), redirect_stdout(io.StringIO()):
            root=Path(tmp);binary=root/'renderer'
            fake=FAKE_RENDERER.replace("'scale_divisor':scale,'family':'Merrill'", "'scale_divisor':4,'family':'Classic'")
            binary.write_text(fake);binary.chmod(0o755)
            source=root/'sample.x3f';source.write_bytes(b'original')
            report=run(binary,[{'label':'sample','family':'classic','source':str(source),'qualities':['thumb']}],root/'result')
            self.assertFalse(report['passed'])
            result=report['cases'][0]['test_result']
            self.assertEqual(result['driver'],'headless')
            self.assertEqual(result['outcome'],'failed')
            self.assertEqual(result['assertions'][0]['expected'],4)
            self.assertEqual(result['assertions'][0]['actual'],8)

    def test_incomplete_coverage_and_owned_flags_are_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);binary=root/'renderer';binary.write_bytes(b'binary')
            source=root/'sample.x3f';source.write_bytes(b'source')
            case={'label':'a','family':'classic','source':str(source)}
            with self.assertRaisesRegex(ValueError,'does not cover'):
                run(binary,[case],root/'run',require_coverage=True)
            with self.assertRaisesRegex(ValueError,'matrix-owned'):
                expand_cases([{**case,'options':['--goldens','elsewhere']}])


if __name__ == '__main__':
    unittest.main()

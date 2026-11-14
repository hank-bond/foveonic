"""Check exact matrix coverage, immutable baselines and failure reporting without a GPU."""

from contextlib import redirect_stdout
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from render_matrix import expand_cases, run, processing_mode_difference
from scenarios import application_result


FAKE_RENDERER = '''#!/usr/bin/env python3
import hashlib, json, pathlib, sys
source=pathlib.Path(sys.argv[1]); output=pathlib.Path(sys.argv[2]); output.mkdir()
resolution=sys.argv[sys.argv.index('--resolution')+1]
quality=sys.argv[sys.argv.index('--processing-quality')+1]
goldens=pathlib.Path(sys.argv[sys.argv.index('--goldens')+1]); goldens.mkdir(parents=True, exist_ok=True)
golden=goldens/(source.stem+'.f32')
samples=hashlib.sha256(source.read_bytes()+resolution.encode()+quality.encode()).digest()[:24]
if golden.exists() and golden.read_bytes()!=samples:
    sys.exit('non-equivalent output')
if not golden.exists():golden.write_bytes(samples)
(output/(source.stem+'-0.tiff')).write_bytes(b'tiff')
scale={'full':1,'fast':2,'draft':4,'thumb':8}[resolution]
for event,data in [('session',{'settings':{'mode':'test'},'processing_quality':quality,'scale_divisor':scale,'family':'Merrill'}),('processing',{'effective_scale_divisor':scale}),('validation',{'digest':'abc','extent':[1,2],'layout':'Rgb32Float','space':'test','linear_white':1,'view_baked':False,'endianness':'little','view_extent':[1,2],'view_crop':[0,0,1,2],'view_rotation':0})]:
    print(json.dumps({'event':event,'data':data}))
'''


class MatrixTests(unittest.TestCase):
    def test_coverage_uses_distinct_sources_and_reports_missing_qualities(self):
        with tempfile.TemporaryDirectory() as tmp:
            path=Path(tmp)/'sample.x3f';path.write_bytes(b'source')
            _, coverage=expand_cases([{'label':'a','family':'classic','source':str(path),'resolutions':['thumb'],'processing_qualities':['quality']},
                                      {'label':'b','family':'classic','source':str(path)}])
            self.assertEqual(coverage['families']['classic']['provided'],1)
            self.assertEqual(coverage['families']['classic']['missing'],2)
            self.assertEqual(len(coverage['missing_resolutions']),3)
            self.assertFalse(coverage['complete'])

    def test_record_repeat_and_changed_input_never_rewrite_the_baseline(self):
        with tempfile.TemporaryDirectory() as tmp, patch('render_matrix.require_quiet'), redirect_stdout(io.StringIO()):
            root=Path(tmp); binary=root/'renderer';binary.write_text(FAKE_RENDERER);binary.chmod(0o755)
            source=root/'sample.x3f';source.write_bytes(b'original')
            cases=[{'label':'sample','family':'merrill','source':str(source),'resolutions':['thumb'],'processing_qualities':['quality']}]
            first=run(binary,cases,root/'baseline')
            self.assertTrue(first['passed']);self.assertFalse(first['coverage']['complete'])
            self.assertEqual(first['cases'][0]['test_result']['outcome'],'passed')
            self.assertEqual(first['cases'][0]['test_result']['assertions'][0]['name'],'preview_scale')
            self.assertEqual(first['cases'][0]['test_result']['environment']['binary_sha256'],first['binary_sha256'])
            headless=first['cases'][0]['test_result']
            native={'passed':True,'steps':[{'action':'initial-selection','photos':[{'source':'staged.x3f',
                    'family':'Merrill','processing_quality':'quality','display_density':0.1,'scale_divisor':8}]}]}
            inputs=[{'source':str(source.resolve()),'staged_source':'staged.x3f','sha256':first['cases'][0]['source_sha256']}]
            application=application_result(native,'initial','quality',inputs)
            self.assertEqual(set(headless),set(application))
            for key in ('source','source_sha256','family','scale_divisor'):
                self.assertEqual(headless['observations'][0][key],application['observations'][0][key])
            for key in ('name','expected','actual','outcome'):
                self.assertEqual(headless['assertions'][0][key],application['assertions'][0][key])
            golden=root/'baseline/goldens/sample-thumb-quality/sample.f32'
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
            report=run(binary,[{'label':'sample','family':'classic','source':str(source),'resolutions':['thumb'],'processing_qualities':['quality']}],root/'result')
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


class ProcessingModeMatrixTests(unittest.TestCase):
    def test_resolution_and_processing_quality_are_separate_axes(self):
        with tempfile.TemporaryDirectory() as temporary:
            source=Path(temporary)/'a.x3f';source.write_bytes(b'fixed source')
            rows,coverage=expand_cases([{'label':'a','family':'merrill','source':str(source)}])
            self.assertEqual(len(rows),8)
            self.assertEqual({row['processing_quality'] for row in rows},{'quality','speed'})
            self.assertEqual({row['resolution'] for row in rows},{'full','fast','draft','thumb'})
            self.assertEqual(len({row['source_sha256'] for row in rows}),1)
            self.assertEqual(coverage['missing_processing_qualities'],[])
            with self.assertRaisesRegex(ValueError,'separate axes'):
                expand_cases([{'label':'a','family':'merrill','source':str(source),'qualities':['full']}])
            with self.assertRaisesRegex(ValueError,'processing qualities'):
                expand_cases([{'label':'a','family':'merrill','source':str(source),'processing_qualities':['smart']}])

    def test_maintained_cases_keep_all_camera_inputs_and_explicit_dense_recipes(self):
        manifest=json.loads(Path(__file__).with_name('test_inputs.json').read_text())
        cases=manifest['render_cases']
        self.assertEqual({case['family'] for case in cases},{'classic','merrill','quattro','tiger'})
        self.assertEqual(len({case['source'] for case in cases}),10)
        self.assertTrue(all(case['processing_qualities']==['quality','speed'] for case in cases))
        self.assertTrue({'merrill-dense-adaptive','merrill-dense-hdr','merrill-dense-color','merrill-dense-dehaze'} <= {case['label'] for case in cases})

    def test_dense_differences_are_evidence_and_never_an_approved_golden(self):
        import struct
        with tempfile.TemporaryDirectory() as temporary:
            quality,speed=(Path(temporary)/name for name in ('quality.f32','speed.f32'))
            quality.write_bytes(struct.pack('<3f',1.,2.,3.));speed.write_bytes(quality.read_bytes())
            original=quality.read_bytes()
            exact=processing_mode_difference(quality,speed,{'endianness':'little'})
            self.assertEqual(exact['different_samples'],0)
            speed.write_bytes(struct.pack('<3f',1.,2.5,2.5))
            changed=processing_mode_difference(quality,speed,{'endianness':'little'})
            self.assertEqual(changed['different_samples'],2)
            self.assertEqual(changed['max_absolute'],0.5)
            self.assertEqual(changed['visual_acceptance'],'unreviewed')
            self.assertEqual(quality.read_bytes(),original)

    def test_nonfinite_policy_pairs_fail_without_reporting_zero_difference_metrics(self):
        import struct
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);quality=root/'quality.f32';speed=root/'speed.f32'
            for invalid in (float('nan'),float('inf'),float('-inf')):
                quality.write_bytes(struct.pack('<3f',invalid,invalid,invalid))
                speed.write_bytes(quality.read_bytes())
                result=processing_mode_difference(quality,speed,{'endianness':'little'})
                self.assertEqual(result['outcome'],'failed')
                self.assertEqual(result['nonfinite_samples'],3)
                for metric in ('max_absolute','mean_absolute','rms'):
                    self.assertIsNone(result[metric])
            binary=root/'renderer'
            fake=FAKE_RENDERER.replace('import hashlib, json, pathlib, sys','import hashlib, json, pathlib, sys, struct')
            fake=fake.replace('samples=hashlib.sha256(source.read_bytes()+resolution.encode()+quality.encode()).digest()[:24]',
                              "samples=struct.pack('<6f',float('nan'),0.,0.,0.,0.,0.)")
            binary.write_text(fake);binary.chmod(0o755)
            source=root/'sample.x3f';source.write_bytes(b'original')
            with patch('render_matrix.require_quiet'), redirect_stdout(io.StringIO()):
                report=run(binary,[{'label':'sample','family':'merrill','source':str(source),'resolutions':['full']}],root/'matrix')
            self.assertTrue(all(row['passed'] for row in report['cases']))
            self.assertFalse(report['passed'])
            self.assertEqual(report['processing_mode_comparisons'][0]['outcome'],'failed')


if __name__ == '__main__':
    unittest.main()

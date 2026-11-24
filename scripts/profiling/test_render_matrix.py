"""Check exact matrix coverage and failure reporting without a GPU."""

from contextlib import redirect_stdout
import io
import json
from pathlib import Path
import tempfile
import subprocess
import unittest
from unittest.mock import patch

from render_matrix import expand_cases, render_arguments, run, processing_mode_difference
from scenarios import application_result


FAKE_RENDERER = '''#!/usr/bin/env python3
import hashlib, json, pathlib, sys
if sys.argv[1]=='--compare-samples':
    import struct,math
    left=pathlib.Path(sys.argv[2]).read_bytes();right=pathlib.Path(sys.argv[3]).read_bytes()
    endian='<' if sys.argv[-1]=='little' else '>'
    pairs=list(zip(struct.iter_unpack(endian+'f',left),struct.iter_unpack(endian+'f',right)))
    delta=[abs(a[0]-b[0]) for a,b in pairs if math.isfinite(a[0]) and math.isfinite(b[0])]
    nf=len(pairs)-len(delta);n=len(pairs)
    data={'samples':n,'different_samples':sum(x!=0 for x in delta),'nonfinite_samples':nf,
          'max_absolute':None if nf else max(delta,default=0),
          'mean_absolute':None if nf else sum(delta)/max(n,1),
          'rms':None if nf else math.sqrt(sum(x*x for x in delta)/max(n,1)),
          'quality_sha256':hashlib.sha256(left).hexdigest(),'speed_sha256':hashlib.sha256(right).hexdigest()}
    print(json.dumps({'event':'sample_difference','data':data}));sys.exit(0)
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
    def test_contended_correctness_records_workloads_without_claiming_timing_validity(self):
        workload = {'pid': 42, 'executable': 'foveonic-gui'}
        with tempfile.TemporaryDirectory() as tmp, redirect_stdout(io.StringIO()), \
                patch('render_matrix.require_quiet', side_effect=RuntimeError('app running')) as quiet, \
                patch('render_matrix.running_workloads', return_value=[workload]):
            root = Path(tmp)
            binary = root / 'renderer'
            binary.write_text(FAKE_RENDERER)
            binary.chmod(0o755)
            source = root / 'sample.x3f'
            source.write_bytes(b'original')
            cases = [{'label': 'sample', 'family': 'merrill', 'source': str(source),
                      'resolutions': ['thumb'], 'processing_qualities': ['quality']}]
            blocked = run(binary, cases, root / 'blocked')
            self.assertFalse(blocked['passed'])
            allowed = run(binary, cases, root / 'allowed', allow_contended_correctness=True)
            self.assertTrue(allowed['passed'])
            self.assertEqual(quiet.call_count, 1)
            self.assertEqual(allowed['cases'][0]['concurrent_workloads_before_render'], [workload])
            self.assertIn('correctness-only', allowed['cases'][0]['timing_validity'])

    def test_contended_matrix_uses_current_comparator_for_an_older_renderer(self):
        with tempfile.TemporaryDirectory() as tmp, redirect_stdout(io.StringIO()), \
                patch('render_matrix.require_quiet', side_effect=RuntimeError('app running')), \
                patch('render_matrix.running_workloads', return_value=[{'pid': 42, 'executable': 'profile_wide'}]):
            root = Path(tmp)
            renderer = root / 'older-renderer'
            renderer.write_text(FAKE_RENDERER.replace("if sys.argv[1]=='--compare-samples':",
                                                     "if sys.argv[1]=='--compare-samples':\n    sys.exit('unsupported by older renderer')"))
            renderer.chmod(0o755)
            comparator = root / 'current-comparator'
            comparator.write_text(FAKE_RENDERER)
            comparator.chmod(0o755)
            source = root / 'sample.x3f'
            source.write_bytes(b'original')
            cases = [{'label': 'sample', 'family': 'merrill', 'source': str(source),
                      'resolutions': ['thumb'], 'processing_qualities': ['quality', 'speed']}]
            report = run(renderer, cases, root / 'matrix', allow_contended_correctness=True,
                         sample_comparator=comparator)
            self.assertTrue(report['passed'])
            self.assertEqual(report['sample_comparator']['path'], str(comparator.resolve()))
            self.assertEqual(report['processing_mode_comparisons'][0]['outcome'], 'complete')
            self.assertTrue(all('correctness-only' in row['timing_validity'] for row in report['cases']))

    def test_snapshot_recipes_preserve_transients_and_use_matrix_owned_axes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            source=root/'source.x3f';source.write_bytes(b'fixed original')
            snapshot={'settings':{'exposure':-0.6},'working_scale':1,'processing_quality':'quality',
                      'dehaze_estimate':True}
            case={'label':'saved','family':'classic','source':str(source),'settings_snapshot':snapshot,
                  'resolutions':['thumb'],'processing_qualities':['speed']}
            rows,_=expand_cases([case])
            arguments=render_arguments(rows[0],root/'owned-settings.json')
            self.assertEqual(arguments,['--settings-snapshot',str(root/'owned-settings.json')])
            effective=json.loads((root/'owned-settings.json').read_text())
            self.assertEqual(effective['working_scale'],4)
            self.assertEqual(effective['processing_quality'],'speed')
            self.assertTrue(effective['dehaze_estimate'])
            self.assertEqual(snapshot['working_scale'],1)
            with self.assertRaisesRegex(ValueError,'cannot be combined'):
                expand_cases([{**case,'options':[]}])

    def test_coverage_uses_distinct_sources_and_reports_missing_qualities(self):
        with tempfile.TemporaryDirectory() as tmp:
            path=Path(tmp)/'sample.x3f';path.write_bytes(b'source')
            _, coverage=expand_cases([{'label':'a','family':'classic','source':str(path),'resolutions':['thumb'],'processing_qualities':['quality']},
                                      {'label':'b','family':'classic','source':str(path)}])
            self.assertEqual(coverage['families']['classic']['provided'],1)
            self.assertEqual(coverage['families']['classic']['missing'],2)
            self.assertEqual(len(coverage['missing_resolutions']),3)
            self.assertFalse(coverage['complete'])

    def test_recorded_result_matches_the_application_result_shape(self):
        with tempfile.TemporaryDirectory() as tmp, patch('render_matrix.require_quiet'), redirect_stdout(io.StringIO()):
            root=Path(tmp); binary=root/'renderer';binary.write_text(FAKE_RENDERER);binary.chmod(0o755)
            source=root/'sample.x3f';source.write_bytes(b'original')
            cases=[{'label':'sample','family':'merrill','source':str(source),'resolutions':['thumb'],'processing_qualities':['quality']}]
            first=run(binary,cases,root/'recorded')
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
    def test_stats_helper_must_describe_complete_files_and_valid_metrics(self):
        import copy, struct
        from file_hash import digest
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);quality=root/'quality.f32';speed=root/'speed.f32'
            quality.write_bytes(struct.pack('<2f',1.,2.));speed.write_bytes(quality.read_bytes())
            valid={'samples':2,'different_samples':0,'nonfinite_samples':0,
                   'max_absolute':0.,'mean_absolute':0.,'rms':0.,
                   'quality_sha256':digest(quality),'speed_sha256':digest(speed)}
            mutations=[{'samples':1},{'different_samples':3},{'nonfinite_samples':-1},
                       {'samples':True},{'max_absolute':float('nan')},{'rms':-1.},
                       {'nonfinite_samples':1},{'quality_sha256':'old-output'},
                       {'different_samples':1},{'rms':1.}]
            for mutation in mutations:
                with self.subTest(mutation=mutation):
                    data={**copy.deepcopy(valid),**mutation}
                    result=subprocess.CompletedProcess([],0,json.dumps({'event':'sample_difference','data':data}),'')
                    with patch('render_matrix.run_captured',return_value=result), self.assertRaises(ValueError):
                        processing_mode_difference(quality,speed,{'endianness':'little'},comparator=root/'tool')
            result=subprocess.CompletedProcess([],1,'','failed')
            with patch('render_matrix.run_captured',return_value=result), self.assertRaisesRegex(ValueError,'status 1'):
                processing_mode_difference(quality,speed,{'endianness':'little'},comparator=root/'tool')
            for record in [[],{'event':'sample_difference','data':None}]:
                result=subprocess.CompletedProcess([],0,json.dumps(record),'')
                with patch('render_matrix.run_captured',return_value=result), self.assertRaises(ValueError):
                    processing_mode_difference(quality,speed,{'endianness':'little'},comparator=root/'tool')

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
        self.assertEqual(len({case['source'] for case in cases}),13)
        for size in ('medium','low'):
            reduced=next(case for case in cases if case['label']==f'dp3-merrill-{size}')
            self.assertEqual(reduced['raw_size'],size)
            self.assertIn(reduced['source'],manifest['sample_hashes'])
            self.assertEqual(reduced['resolutions'],['full','fast','draft','thumb'])
            self.assertEqual(reduced['options'],['--wb','Sunlight','--mode','Standard'])
        self.assertTrue(all(case['processing_qualities']==['quality','speed'] for case in cases))
        self.assertTrue({'merrill-dense-adaptive','merrill-dense-hdr','merrill-dense-color','merrill-dense-dehaze'} <= {case['label'] for case in cases})
        flower=next(case for case in cases if case['label']=='sunlit-red-flower-edited')
        self.assertNotIn('options',flower)
        self.assertEqual(flower['settings_snapshot']['settings']['dehaze']['amount'],61)
        self.assertTrue(flower['settings_snapshot']['dehaze_estimate'])
        self.assertIn(flower['source'],manifest['sample_hashes'])

    def test_dense_differences_are_evidence_and_never_an_approved_golden(self):
        import struct
        with tempfile.TemporaryDirectory() as temporary:
            quality,speed=(Path(temporary)/name for name in ('quality.f32','speed.f32'))
            comparator=Path(temporary)/'comparator';comparator.write_text(FAKE_RENDERER);comparator.chmod(0o755)
            quality.write_bytes(struct.pack('<3f',1.,2.,3.));speed.write_bytes(quality.read_bytes())
            original=quality.read_bytes()
            exact=processing_mode_difference(quality,speed,{'endianness':'little'},comparator=comparator)
            self.assertEqual(exact['different_samples'],0)
            speed.write_bytes(struct.pack('<3f',1.,2.5,2.5))
            changed=processing_mode_difference(quality,speed,{'endianness':'little'},comparator=comparator)
            self.assertEqual(changed['different_samples'],2)
            self.assertEqual(changed['max_absolute'],0.5)
            self.assertEqual(changed['visual_acceptance'],'unreviewed')
            self.assertEqual(quality.read_bytes(),original)

    def test_nonfinite_policy_pairs_fail_without_reporting_zero_difference_metrics(self):
        import struct
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);quality=root/'quality.f32';speed=root/'speed.f32'
            comparator=root/'comparator';comparator.write_text(FAKE_RENDERER);comparator.chmod(0o755)
            for invalid in (float('nan'),float('inf'),float('-inf')):
                quality.write_bytes(struct.pack('<3f',invalid,invalid,invalid))
                speed.write_bytes(quality.read_bytes())
                result=processing_mode_difference(quality,speed,{'endianness':'little'},comparator=comparator)
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

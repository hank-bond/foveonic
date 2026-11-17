"""Reject incomplete or transplanted worker evidence and preserve bounded scheduling outcomes."""

import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from correctness_workers import case_key, native_failure_recorded, only_verifier_guards_failed, run_parallel, validate_group, validate_options
from file_hash import digest


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value))


class WorkerEvidenceTests(unittest.TestCase):
    def fixture(self, root):
        sources = [root / 'a.x3f', root / 'b.x3f']
        for p in sources:p.write_bytes(p.name.encode())
        fixture = {'name':'pair', 'family':'merrill', 'sources':list(map(str, sources)), 'worker_memory_mb':2048}
        case = {'name':'exposure', 'pointer':'/exposure', 'section':'Tone',
                'recipes':[{'exposure':0.25}, {'exposure':-0.25}]}
        key = ('pair', 'exposure', 'dark', 'quality')
        directory = root / 'group'
        case_dir = directory / '--'.join(key)
        trial_dir = case_dir / 'trial-00'
        cohort = {'run_id':'current', 'group_id':'pair--dark--quality', 'worker':{'pid':10,'birth':100}}
        job = {'id':'pair--dark--quality', 'directory':str(directory), 'fixture':fixture,
               'theme':'dark', 'quality':'quality', 'cases':[case], 'source_hashes':[digest(p) for p in sources],
               'cohort_identity':cohort, 'owned_children':{11:101}}
        session = {'id':'session', 'cohort':cohort, 'pid':11}
        session_dir = directory / 'session-groups/pair--dark--quality'
        write(session_dir / 'session-result.json', session)
        expected_session = {'id':'session', 'case_id':'--'.join(key), 'index':0, 'count':1, 'pid':11}
        launch = {'fixture':'pair', 'case_id':'--'.join(key), 'correctness_cohort':cohort,
                  'scenario':{'name':'slider-drag', 'control_pointer':'/exposure', 'quality':'quality',
                              'mode':'off', 'light_theme':False, 'drag_pattern':'forward',
                              'mixed_values':True, 'editing_section':'Tone'},
                  'inputs':[{'source':str(p),'settings':s,'sha256':digest(p)} for p,s in zip(sources,case['recipes'])]}
        write(trial_dir / 'launch.json', launch)
        write(trial_dir / 'evidence/scenario-result.json', {'session':expected_session,'scenario':launch['scenario'],'passed':True})
        geometry = {'extent':[1,1], 'layout':'Rgb32Float', 'space':'rgb', 'linear_white':1,
                    'view_baked':False, 'view_extent':[1,1], 'view_crop':[0,0,1,1],
                    'view_rotation':0, 'endianness':'little'}
        shared = {'outcome':'passed', 'assertions':[{'name':name,'outcome':'passed'} for name in
                  ['slider_session_identity','slider_session_process_exit','slider_session_scenario']]}
        for index, source in enumerate(sources):
            shared['assertions'].append({'name':'image_samples', 'outcome':'passed',
                'source':str(source), 'step':'slider-drag',
                'expected':{'file':str(trial_dir/f'fresh-{index:02}-samples'/(source.stem+'.f32')),
                            'geometry':copy.deepcopy(geometry), 'bytes':12, 'sha256':'d'*64},
                'actual':{'file':str(trial_dir/'evidence'/f'image-{index:02}.f32'),
                          'geometry':copy.deepcopy(geometry), 'bytes':12, 'sha256':'d'*64}})
        shared_path = trial_dir / 'evidence/test-result.json';write(shared_path, shared)
        artifacts = {'binary_sha256':'app', 'fresh_renderer':{'sha256':'renderer'},
                     'launch':str(trial_dir/'launch.json'), 'result':str(trial_dir/'evidence/scenario-result.json'),
                     'result_sha256':digest(trial_dir/'evidence/scenario-result.json'),
                     'test_result':{'path':str(shared_path),'sha256':digest(shared_path)}}
        trial = {'artifacts':artifacts, 'test_result':shared, 'session_reuse':{'expected':expected_session}}
        write(case_dir / 'regression.json', {'passed':True,'timing_valid':False,'trials':[trial]})
        row = {'fixture':key[0],'case':key[1],'theme':key[2],'processing_quality':key[3],
               'directory':str(case_dir),'passed':True}
        report = {'passed':True,'timing_valid':False,'results':[row],
                  'session_groups':[{'id':'session','directory':str(session_dir),'passed':True,'status':'passed'}]}
        return job, report, copy.deepcopy(launch), trial_dir, case_dir

    def test_complete_case_binds_all_owned_evidence(self):
        with tempfile.TemporaryDirectory() as tmp:
            job,report,*_ = self.fixture(Path(tmp))
            self.assertEqual(validate_group(job,report,'app','renderer'), report['results'])

    def retention_fixture(self, root):
        job, report, launch, trial_dir, case_dir = self.fixture(root)
        job['prune_verified_images'] = True
        self.save_retention(report, case_dir)
        return job, report, launch, trial_dir, case_dir

    def save_retention(self, report, case_dir):
        regression = json.loads((case_dir/'regression.json').read_text())
        images = []
        for check in regression['trials'][0]['test_result']['assertions']:
            if check['name'] != 'image_samples':
                continue
            images.append({'trial':0, 'source':check['source'], 'step':check['step'],
                'geometry':copy.deepcopy(check['expected']['geometry']),
                'files':[{'role':role, 'path':str(Path(check[role]['file']).relative_to(case_dir)),
                          'bytes':check[role]['bytes'], 'sha256':check[role]['sha256']}
                         for role in ('expected','actual')]})
        receipt = {'state':'complete', 'regression_sha256':digest(case_dir/'regression.json'),
                   'images':images, 'removed':[f['path'] for image in images for f in image['files']]}
        path = case_dir/'sample-retention.json'
        write(path, receipt)
        report['results'][0]['sample_retention'] = {'path':str(path), 'sha256':digest(path)}
        return receipt

    def test_pruned_case_validates_from_frozen_assertions_without_raw_files(self):
        with tempfile.TemporaryDirectory() as tmp:
            job, report, _, _, case_dir = self.retention_fixture(Path(tmp))
            self.assertEqual(list(case_dir.rglob('*.f32')), [])
            self.assertEqual(validate_group(job, report, 'app', 'renderer'), report['results'])

    def test_self_consistent_but_transplanted_retention_evidence_is_rejected(self):
        for mutation in ('source','step','trial','geometry','path','bytes','sha256','role','missing','duplicate'):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as tmp:
                job, report, _, _, case_dir = self.retention_fixture(Path(tmp))
                path = case_dir/'sample-retention.json'
                receipt = json.loads(path.read_text())
                image = receipt['images'][0]
                if mutation == 'source': image['source'] = 'another-original.x3f'
                elif mutation == 'step': image['step'] = 'another-action'
                elif mutation == 'trial': image['trial'] = 1
                elif mutation == 'geometry': image['geometry']['view_rotation'] = 1
                elif mutation == 'path': image['files'][0]['path'] = 'trial-00/unknown.f32'
                elif mutation == 'bytes': image['files'][0]['bytes'] = 24
                elif mutation == 'sha256': image['files'][0]['sha256'] = 'e'*64
                elif mutation == 'role': image['files'][0]['role'] = 'actual'
                elif mutation == 'missing': receipt['images'].pop()
                else: receipt['images'][1] = copy.deepcopy(image)
                # Rebind the receipt's hash and its own removed set. Only a check
                # against the independent frozen assertions can reject this proof.
                receipt['removed'] = [f['path'] for image in receipt['images'] for f in image['files']]
                write(path, receipt)
                report['results'][0]['sample_retention']['sha256'] = digest(path)
                with self.assertRaisesRegex(ValueError, 'retention is incomplete'):
                    validate_group(job, report, 'app', 'renderer')

    def test_retention_cannot_replace_shared_assertions_or_the_frozen_report(self):
        for mutation in ('shared','regression_hash','receipt_hash','removed'):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as tmp:
                job, report, _, _, case_dir = self.retention_fixture(Path(tmp))
                path = case_dir/'sample-retention.json'
                if mutation == 'shared':
                    regression_path = case_dir/'regression.json'
                    regression = json.loads(regression_path.read_text())
                    check = regression['trials'][0]['test_result']['assertions'][-1]
                    check['expected']['sha256'] = check['actual']['sha256'] = 'e'*64
                    write(regression_path, regression)
                    self.save_retention(report, case_dir)
                else:
                    receipt = json.loads(path.read_text())
                    if mutation == 'regression_hash': receipt['regression_sha256'] = 'f'*64
                    elif mutation == 'removed': receipt['removed'].pop()
                    else: receipt['extra'] = 'post-hash mutation'
                    write(path, receipt)
                    if mutation != 'receipt_hash':
                        report['results'][0]['sample_retention']['sha256'] = digest(path)
                with self.assertRaises(ValueError):
                    validate_group(job, report, 'app', 'renderer')

    def test_missing_duplicate_and_foreign_case_rows_are_rejected(self):
        for mutation in ['missing','duplicate','substitute']:
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as tmp:
                job,report,*_ = self.fixture(Path(tmp))
                if mutation=='missing':report['results']=[]
                elif mutation=='duplicate':report['results']*=2
                else:report['results'][0]['case']='another-control'
                with self.assertRaises(ValueError):validate_group(job,report,'app','renderer')

    def test_wrong_gesture_recipe_and_stale_cohort_are_rejected(self):
        for mutation in ['pattern','recipe','cohort','section']:
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as tmp:
                job,report,launch,trial_dir,_ = self.fixture(Path(tmp))
                if mutation=='pattern':launch['scenario']['drag_pattern']='scrub'
                elif mutation=='recipe':launch['inputs'][0]['settings']['exposure']=2
                elif mutation=='cohort':launch['correctness_cohort']['run_id']='previous-run'
                else:launch['scenario']['editing_section']='Noise'
                write(trial_dir/'launch.json',launch)
                with self.assertRaises(ValueError):validate_group(job,report,'app','renderer')

    def test_transplanted_or_modified_shared_assertions_are_rejected(self):
        for mutation in ['outside','modified','empty','timing','app']:
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as tmp:
                job,report,_,trial_dir,case_dir = self.fixture(Path(tmp))
                path=case_dir/'regression.json';result=json.loads(path.read_text())
                if mutation=='outside':
                    outside=Path(tmp)/'old-result.json';outside.write_bytes((trial_dir/'evidence/test-result.json').read_bytes())
                    result['trials'][0]['artifacts']['test_result']['path']=str(outside)
                elif mutation=='modified':write(trial_dir/'evidence/test-result.json', {'outcome':'passed','assertions':[]})
                elif mutation=='empty':result['trials']=[]
                elif mutation=='timing':result['timing_valid']=True
                else:result['trials'][0]['artifacts']['binary_sha256']='old-app'
                write(path,result)
                with self.assertRaises(ValueError):validate_group(job,report,'app','renderer')

    def test_raw_native_failure_cannot_be_hidden_by_a_passed_summary(self):
        with tempfile.TemporaryDirectory() as tmp:
            job,report,_,trial_dir,case_dir = self.fixture(Path(tmp))
            native=trial_dir/'evidence/scenario-result.json';value=json.loads(native.read_text());value['passed']=False;write(native,value)
            result=json.loads((case_dir/'regression.json').read_text());result['trials'][0]['artifacts']['result_sha256']=digest(native);write(case_dir/'regression.json',result)
            with self.assertRaises(ValueError):validate_group(job,report,'app','renderer')

    def test_guarded_classification_rejects_mixed_assertion_failures(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);job={'directory':str(root),'cases':[{}]}
            write(root/'case/trial-00/evidence/scenario-result.json',{'passed':True})
            path=root/'case/trial-00/evidence/test-result.json'
            guard={'outcome':'failed','failure_kind':'memory_guard'}
            write(path,{'assertions':[guard]});self.assertTrue(only_verifier_guards_failed(job))
            write(path,{'assertions':[guard,{'outcome':'failed','failure_kind':'verification'}]})
            self.assertFalse(only_verifier_guards_failed(job))

    def test_invalid_parallel_request_does_not_stage_inputs(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);job,*_=self.fixture(root);fixture=job['fixture'];case=job['cases'][0]
            groups=[[(fixture,{**case,'processing_quality':'quality'},'dark')]]
            with self.assertRaisesRegex(ValueError,'background'):
                validate_options(groups,2,4096,None,'off',True,root/'renderer',False)
            self.assertFalse((root/'cohort.json').exists())

    def test_all_group_members_must_resolve_to_the_same_guard_before_process_probe(self):
        fixture = {'name': 'quattro', 'family': 'quattro', 'worker_memory_mb': 4096}
        cases = [{'name': 'light', 'processing_quality': 'quality'},
                 {'name': 'heavy', 'processing_quality': 'quality',
                  'worker_memory_mb_by_family': {'quattro': 8192}}]
        with patch('correctness_workers.birth_reader') as probe:
            with self.assertRaisesRegex(ValueError, 'one resolved worker memory guard'):
                validate_options([[(fixture, case, 'dark') for case in cases]],
                                 2, 16384, None, 'off', True, Path('renderer'), True)
            probe.assert_not_called()

    def test_native_guard_preserves_existing_failed_or_malformed_terminals(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);job={'directory':str(root)}
            self.assertFalse(native_failure_recorded(job))
            path=root/'case/trial-00/evidence/scenario-result.json'
            write(path,{'passed':True});self.assertFalse(native_failure_recorded(job))
            for value in [{'passed':False},{},[],None]:
                write(path,value);self.assertTrue(native_failure_recorded(job))
            path.write_text('{');self.assertTrue(native_failure_recorded(job))


class ParentSchedulingTests(unittest.TestCase):
    def test_parent_admits_within_limits_and_combines_complete_union(self):
        self.run_fake()

    def test_missing_terminal_receipt_stops_admission_without_a_pass(self):
        self.run_fake(missing=True)

    def test_abort_publication_error_still_cleans_active_worker(self):
        self.run_fake(register_failure=True, abort_failure=True)

    def test_guard_qualified_groups_admit_light_first_and_keep_report_order(self):
        self.run_fake(partition=True)

    def run_fake(self, missing=False, register_failure=False, abort_failure=False, partition=False):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);app=root/'app';renderer=root/'renderer';app.write_bytes(b'app');renderer.write_bytes(b'renderer')
            sources=[root/'a.x3f',root/'b.x3f']
            for p in sources:p.write_bytes(p.name.encode())
            case={'name':'exposure','pointer':'/exposure','section':'Tone','processing_quality':'quality','recipes':[{},{}]}
            groups=[[({'name':name,'family':'merrill','sources':list(map(str,sources))},case,'dark')] for name in ['one','two','three']]
            guards, budget = [2048] * 3, 4096
            if partition:
                groups[1][0][0]['name'] = 'one'
                groups[1][0] = (groups[1][0][0], {**case, 'name': 'heavy'}, 'dark')
                guards, budget = [2048, 8192, 2048], 12288
            output=root/'output';output.mkdir();spawned=[];controllers=[]
            class ControllerFake:
                def __init__(self,directory,*args):
                    self.path=directory/'cohort.json';self.document={'id':'cohort','controller':{},'workers':{}};self.anchors={};self.maximum=0;controllers.append(self)
                def observe(self):return {'potential_workloads':[]}
                check_quiet=observe
                def register(self,key,process):
                    if register_failure:raise RuntimeError('registration failed')
                    self.document['workers'][key]={'pid':process.pid};self.anchors[key]={};self.maximum=max(self.maximum,len(self.document['workers']))
                def unregister(self,key):del self.document['workers'][key]
                def abort(self):
                    if abort_failure:raise OSError('abort receipt unavailable')
                def terminate(self,key,process,grace=10):process.terminate();process.wait();self.unregister(key)
                def close(self):assert not self.document['workers']
            class Process:
                def __init__(self,command,**kwargs):
                    self.pid=100+len(spawned);self.returncode=None;self.reaped=False;spawned.append(self)
                    job=json.loads(Path(command[-1]).read_text());self.job=job
                    write(Path(job['directory'])/'behavioral-report.json', {'session_groups':[]})
                    if not missing:write(Path(job['receipt']),{'group_id':job['id'],'passed':True,'outcome':'passed'})
                def poll(self):self.returncode=0 if self.returncode is None else self.returncode;return self.returncode
                def wait(self,timeout=None):self.reaped=True;return self.returncode
                def terminate(self):self.returncode=-15
                def kill(self):self.returncode=-9
            def valid(job,*args):
                return [{'fixture':job['fixture']['name'],'family':'merrill','case':job['cases'][0]['name'],'theme':'dark','processing_quality':'quality','pointer':'/exposure','passed':True,'directory':job['directory']}]
            with patch('correctness_workers.Controller',ControllerFake),patch('correctness_workers.subprocess.Popen',Process), \
                    patch('correctness_workers.validate_group',side_effect=valid),patch('correctness_workers.time.sleep'):
                report=run_parallel(app,renderer,groups,guards,output,2,budget,{'passed':False,'results':[]},0)
            if partition:
                self.assertEqual([process.job['guard_mb'] for process in spawned], [2048, 2048, 8192])
                self.assertEqual([group['guard_mb'] for group in report['worker_groups']], guards)
                self.assertEqual([row['fixture'] for row in report['results']], ['one', 'one', 'three'])
                self.assertEqual(len({process.job['id'] for process in spawned}), 3)
                self.assertTrue(all(process.job['id'].endswith(f"guard-{process.job['guard_mb']}mb") for process in spawned))
            self.assertLessEqual(controllers[0].maximum,2)
            self.assertTrue(all(p.reaped for p in spawned))
            if missing or register_failure:
                self.assertFalse(report['passed']);self.assertFalse(report['complete'])
                self.assertEqual(report['worker_groups'][-1]['outcome'],'unrun')
            else:
                self.assertTrue(report['passed']);self.assertTrue(report['complete'])
                expected = ({('one', 'exposure', 'dark', 'quality'), ('one', 'heavy', 'dark', 'quality'),
                             ('three', 'exposure', 'dark', 'quality')} if partition else
                            {(n, 'exposure', 'dark', 'quality') for n in ['one', 'two', 'three']})
                self.assertEqual({case_key(x) for x in report['results']}, expected)


if __name__=='__main__':unittest.main()

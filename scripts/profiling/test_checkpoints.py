"""Check graph dependency unions and allocation ownership without a GPU."""

from copy import deepcopy
import unittest
from checkpoints import analyze_profile, analyze_records, ancestry, cache_inventory, theoretical_avoidance, unique_allocations


def resource(allocation, size, generation=0):
    return {'allocation_id':allocation, 'generation':generation, 'byte_len':size,
            'working_scale':1, 'description':None, 'view':None}


def entry(allocation, version, **flags):
    return {**resource(allocation, 100), 'version':version, 'role':'linear', 'names':['final.linear'],
            **({'selected':False, 'primary':False, 'restart':False, 'pinned':False,
                'required_cut':False, 'priority':None, 'used_frame':0} | flags)}


def fixture():
    snapshot={'entries':[entry(1, 10, selected=True), entry(1, 11, restart=True), entry(2, 12, pinned=True)],
              'cache_structure_bytes':5, 'parameter_payload_bytes':8, 'preparation_bytes':3, 'output_parameters':[], 'versions':[]}
    graph={'id':1, 'requested_roots':[2], 'outputs':[
        {'id':0, 'name':'sensor', 'producer':0, 'dependencies':[], 'data_type':'rgb', 'session_hint':False, 'sealed_cut':False},
        {'id':1, 'name':'linear', 'producer':1, 'dependencies':[0], 'data_type':'rgb', 'session_hint':True, 'sealed_cut':False},
        {'id':2, 'name':'final', 'producer':2, 'dependencies':[0,1], 'data_type':'rgb', 'session_hint':False, 'sealed_cut':False}]}
    def node(ordinal, rid, start, end, shared_start=None, shared_end=None):
        return {'graph_id':1, 'ordinal':ordinal, 'operation':f'op{ordinal}', 'executed':True,
                'started_ms':ordinal*2., 'host_milliseconds':0.5, 'nested_gpu_start':start, 'nested_gpu_end':end,
                'shared_gpu_start':shared_start, 'shared_gpu_end':shared_end,
                'outputs':[{'id':ordinal, 'resource':rid, 'required':True, 'cached_available':False, 'requested_for_execution':True, 'published':True}],
                'memory_before':{'union_bytes':1}, 'memory_after_publication':{'union_bytes':2},
                'memory_after_retirement':{'union_bytes':1}}
    profile={'graphs':[graph], 'gpu':[{'milliseconds':1.}, {'milliseconds':2.}, {'milliseconds':3.}],
             'nodes':[node(0,resource(1,100),0,1,1,2), node(1,resource(2,100),1,2,1,2), node(2,resource(3,20),2,3)]}
    return profile,snapshot


class CheckpointTests(unittest.TestCase):
    def test_unique_allocations_and_cache_flags_do_not_count_aliases_twice(self):
        _,snapshot=fixture(); inventory=cache_inventory(snapshot)
        self.assertEqual(inventory['gpu'], {'unique_bytes':200,'allocation_count':2})
        self.assertEqual(inventory['gpu_bytes_by_flag_nonadditive']['restart'],100)
        self.assertEqual(len(inventory['allocations'][0]['aliases']),2)
        snapshot['entries'][1]['byte_len']=99
        with self.assertRaisesRegex(ValueError,'conflicting sizes'): cache_inventory(snapshot)
        self.assertEqual(unique_allocations([resource(1,100,0),resource(1,100,1)])['unique_bytes'],100)

    def test_ancestor_union_deduplicates_node_and_nested_shared_gpu_ranges(self):
        profile,snapshot=fixture(); graph=profile['graphs'][0]
        union=theoretical_avoidance(profile,graph,[1,2])
        self.assertEqual(union['node_ordinals'],[0,1,2])
        self.assertEqual(union['gpu_timing_indices'],[0,1,2])
        self.assertEqual(union['observed_ancestor_gpu_ms'],6.)
        result=analyze_profile(profile,snapshot)['graphs'][0]
        self.assertEqual(len(result['candidates'][0]['checkpoint_aliases']),2)
        self.assertEqual(result['theoretical_rank'][0]['output_id'],2)
        self.assertEqual(result['node_boundaries'][1]['started_ms'],2.)
        self.assertEqual(result['node_boundaries'][1]['memory_after_publication']['union_bytes'],2)
        profile['nodes'][0]['executed']=False
        self.assertFalse(theoretical_avoidance(profile,graph,[1])['complete_ancestor_observation'])
        profile['nodes'][0]['executed']=True
        profile['nodes'][0]['outputs'][0]['published']=False
        result=theoretical_avoidance(profile,graph,[1])
        self.assertEqual(result['unobserved_output_ids'], [0])
        self.assertFalse(result['complete_ancestor_observation'])

    def test_cached_sibling_is_not_ranked_as_new_producer_work(self):
        profile,snapshot=fixture()
        profile['graphs'][0]['outputs'].append({'id':3,'name':'sibling','producer':1,
            'dependencies':[0], 'data_type':'rgb', 'session_hint':False, 'sealed_cut':False})
        profile['nodes'][1]['outputs'].append({'id':3, 'resource':resource(1,100),
            'required':True, 'cached_available':True, 'requested_for_execution':False, 'published':False})
        graph=analyze_profile(profile,snapshot)['graphs'][0]
        sibling=next(row for row in graph['candidates'] if row['output_id']==3)
        self.assertEqual(sibling['ancestor_gpu']['unobserved_output_ids'],[3])
        self.assertNotIn(3,[row['output_id'] for row in graph['theoretical_rank']])
        self.assertIn(1,[row['output_id'] for row in graph['theoretical_rank']])

    def test_graph_ids_do_not_conflate_identically_named_outputs(self):
        profile,snapshot=fixture(); graph=deepcopy(profile['graphs'][0]); graph['id']=2
        nodes=deepcopy(profile['nodes'])
        for node in nodes:
            node['graph_id']=2; node['outputs'][0]['resource']=resource(77,150)
        profile['graphs'].append(graph); profile['nodes'].extend(nodes)
        result=analyze_profile(profile,snapshot)['graphs']
        self.assertEqual([row['graph_id'] for row in result],[1,2])
        self.assertEqual(result[0]['candidates'][0]['resource']['allocation_id'],1)
        self.assertEqual(result[1]['candidates'][0]['resource']['allocation_id'],77)
        self.assertEqual(result[1]['candidates'][0]['checkpoint_aliases'],[])
        profile['nodes'][0]['outputs'][0]['resource']['generation']=1
        changed=analyze_profile(profile,snapshot)['graphs'][0]['candidates'][0]
        self.assertEqual(changed['checkpoint_aliases'],[])

    def test_jsonl_keeps_trials_controls_and_phases_separate(self):
        profile,snapshot=fixture()
        def record(event, data, mode='gpu'):
            return {'foveonic_profile':1, 'event':event, 'mode':mode, 'data':data}
        records=[record('checkpoint_session', {'case':'photo', 'source':'fixture', 'repeats':2, 'verify':False, 'timing_usable':False, 'controls':['exposure']})]
        for trial in (0, 1):
            for phase in ('base', 'unchanged', 'edit-a', 'edit-b', 'return-a'):
                data={'case':'photo','control':'exposure','trial':trial,'phase':phase,
                      'settings':{},'completed_ms':trial+1.,'render_call_ms':trial+.5,
                      'profile':deepcopy(profile),'checkpoints':deepcopy(snapshot),
                      'activity':{},'resources':{},'engine_preparation_bytes':10,'rendered_image':{}}
                records.append(record('checkpoint_render', data))
                if trial == 0 and phase == 'base':
                    records.append(record('checkpoint_verify', {'case':'photo','control':'exposure',
                                                                  'trial':trial,'phase':phase,'equal':True,'outside_timing':True}))
        out=analyze_records(records)['cases'][0]
        self.assertEqual(len(out['phases']),10)
        self.assertEqual(out['phase_timings'][0]['completed_median_ms'],1.5)
        self.assertEqual(out['incomplete_trials'],[])
        self.assertTrue(out['phases'][0]['verification']['equal'])
        self.assertIsNone(out['phases'][1]['verification'])
        self.assertTrue(out['performance_evidence_complete'])
        self.assertFalse(out['performance_usable'])
        off = deepcopy(records)
        for record in off:
            record['mode'] = 'off'
            if record['event'] == 'checkpoint_session': record['data']['timing_usable'] = True
        self.assertTrue(analyze_records(off)['cases'][0]['performance_usable'])
        records.pop()
        self.assertEqual(analyze_records(records)['cases'][0]['incomplete_trials'][0]['missing'],['return-a'])
        records.append(records[-1])
        with self.assertRaisesRegex(ValueError,'duplicate'): analyze_records(records)

    def test_missing_trials_and_required_verification_are_independent(self):
        profile,snapshot=fixture()
        def record(event,data):
            return {'foveonic_profile':1,'event':event,'mode':'gpu','data':data}
        session={'case':'photo','controls':['exposure','noise'],'repeats':2,'verify':True,'timing_usable':False}
        data={'case':'photo','control':'exposure','trial':0,'phase':'base',
              'settings':{},'completed_ms':1.,'render_call_ms':.5,'profile':profile,'checkpoints':snapshot,
              'activity':{},'resources':{},'engine_preparation_bytes':10,'rendered_image':{}}
        records=[record('checkpoint_session',session),record('checkpoint_render',data)]
        result=analyze_records(records)
        self.assertFalse(result['capture_complete'])
        self.assertFalse(result['verification_failed'])
        by_control={row['control']:row for row in result['cases']}
        self.assertEqual(len(by_control['noise']['incomplete_trials']),2)
        self.assertEqual(by_control['exposure']['incomplete_trials'][1]['missing'],['base','unchanged','edit-a','edit-b','return-a'])
        self.assertEqual(by_control['exposure']['verification']['status'],'incomplete')
        self.assertFalse(by_control['exposure']['performance_usable'])
        verify={'case':'photo','control':'exposure','trial':0,'phase':'base','equal':False,'outside_timing':True}
        records.append(record('checkpoint_verify',verify))
        result=analyze_records(records)
        self.assertTrue(result['verification_failed'])
        self.assertEqual(result['cases'][0]['verification']['status'],'failed')
        self.assertFalse(result['capture_complete'])

    def test_required_verification_and_performance_completeness_are_separate(self):
        profile,snapshot=fixture()
        def record(event,data):
            return {'foveonic_profile':1,'event':event,'mode':'wall','data':data}
        session={'case':'photo','controls':['exposure'],'repeats':1,'verify':True,'timing_usable':False}
        records=[record('checkpoint_session',session)]
        for phase in ('base','unchanged','edit-a','edit-b','return-a'):
            data={'case':'photo','control':'exposure','trial':0,'phase':phase,
                  'settings':{},'completed_ms':1.,'render_call_ms':.5,'profile':profile,'checkpoints':snapshot,
                  'activity':{},'resources':{},'engine_preparation_bytes':10,'rendered_image':{}}
            records.append(record('checkpoint_render',data))
            if phase != 'return-a':
                records.append(record('checkpoint_verify',{'case':'photo','control':'exposure','trial':0,
                                                           'phase':phase,'equal':True,'outside_timing':True}))
        result=analyze_records(records)
        self.assertFalse(result['capture_complete'])
        case=result['cases'][0]
        self.assertTrue(case['performance_evidence_complete'])
        self.assertFalse(case['performance_usable'])
        self.assertEqual(case['verification']['status'],'incomplete')
        records.append(record('checkpoint_verify',{'case':'photo','control':'exposure','trial':0,
                                                   'phase':'return-a','equal':True,'outside_timing':True}))
        result=analyze_records(records)
        self.assertTrue(result['capture_complete'])
        self.assertEqual(result['cases'][0]['verification']['status'],'passed')
        self.assertFalse(result['cases'][0]['performance_usable'])
        records.pop()
        first = next(row for row in records if row['event'] == 'checkpoint_verify')
        first['data']['equal'] = False
        result=analyze_records(records)
        self.assertEqual(result['cases'][0]['verification']['status'],'failed')
        self.assertFalse(result['capture_complete'])

    def test_enabled_modes_require_graph_and_node_diagnostics(self):
        profile,snapshot=fixture()
        def record(event,data,mode):
            return {'foveonic_profile':1,'event':event,'mode':mode,'data':data}
        for mode in ('off','wall','gpu'):
            session={'case':'photo','controls':['exposure'],'repeats':1,'verify':False,'timing_usable':mode=='off'}
            records=[record('checkpoint_session',session,mode)]
            for phase in ('base','unchanged','edit-a','edit-b','return-a'):
                data={'case':'photo','control':'exposure','trial':0,'phase':phase,'settings':{},
                      'completed_ms':1.,'render_call_ms':.5,'profile':{'gpu':[]},'checkpoints':snapshot,
                      'activity':{},'resources':{},'engine_preparation_bytes':10,'rendered_image':{}}
                records.append(record('checkpoint_render',data,mode))
            result=analyze_records(records)
            self.assertEqual(result['capture_complete'],mode=='off')
            self.assertEqual(result['cases'][0]['performance_usable'],mode=='off')
            if mode != 'off': self.assertEqual(len(result['cases'][0]['missing_diagnostics']),5)

    def test_cli_failure_on_missing_trial_preserves_report(self):
        import json, subprocess, sys, tempfile
        from pathlib import Path
        profile,snapshot=fixture()
        def record(event,data):
            return {'foveonic_profile':1,'event':event,'mode':'off','data':data}
        session={'case':'photo','controls':['exposure'],'repeats':2,'verify':False,'timing_usable':True}
        records=[record('checkpoint_session',session)]
        for phase in ('base','unchanged','edit-a','edit-b','return-a'):
            records.append(record('checkpoint_render',{'case':'photo','control':'exposure','trial':0,'phase':phase,
                'settings':{},'completed_ms':1.,'render_call_ms':.5,'profile':profile,'checkpoints':snapshot,
                'activity':{},'resources':{},'engine_preparation_bytes':10,'rendered_image':{}}))
        with tempfile.TemporaryDirectory() as temporary:
            path=Path(temporary)/'input.jsonl'; output=Path(temporary)/'report.json'
            path.write_text(''.join(json.dumps(row)+'\n' for row in records))
            result=subprocess.run([sys.executable,str(Path(__file__).with_name('checkpoints.py')),str(path),'--output',str(output)],capture_output=True,text=True)
            self.assertNotEqual(result.returncode,0)
            self.assertIn('capture is incomplete', result.stderr)
            report=json.loads(output.read_text())
            self.assertFalse(report['capture_complete'])
            self.assertEqual(report['cases'][0]['incomplete_trials'][0]['trial'],1)

    def test_cli_failure_on_unequal_verification_preserves_report(self):
        import json, subprocess, sys, tempfile
        from pathlib import Path
        profile,snapshot=fixture()
        def record(event,data):
            return {'foveonic_profile':1,'event':event,'mode':'wall','data':data}
        session={'case':'photo','controls':['exposure'],'repeats':1,'verify':True,'timing_usable':False}
        data={'case':'photo','control':'exposure','trial':0,'phase':'base',
              'settings':{},'completed_ms':1.,'render_call_ms':.5,'profile':profile,'checkpoints':snapshot,
              'activity':{},'resources':{},'engine_preparation_bytes':10,'rendered_image':{}}
        verify={'case':'photo','control':'exposure','trial':0,'phase':'base','equal':False,'outside_timing':True}
        with tempfile.TemporaryDirectory() as temporary:
            path=Path(temporary)/'input.jsonl'; output=Path(temporary)/'report.json'
            path.write_text(''.join(json.dumps(row)+'\n' for row in [record('checkpoint_session',session),record('checkpoint_render',data),record('checkpoint_verify',verify)]))
            result=subprocess.run([sys.executable,str(Path(__file__).with_name('checkpoints.py')),str(path),'--output',str(output)],capture_output=True,text=True)
            self.assertNotEqual(result.returncode,0)
            self.assertTrue(json.loads(output.read_text())['verification_failed'])

    def test_invalid_dependencies_and_gpu_ranges_are_rejected(self):
        profile,_=fixture(); graph=profile['graphs'][0]; graph['outputs'][0]['dependencies']=[2]
        with self.assertRaisesRegex(ValueError,'cyclic'): ancestry(graph,[2])
        graph['outputs'][0]['dependencies']=[99]
        with self.assertRaisesRegex(ValueError,'missing'): ancestry(graph,[2])
        graph['outputs'][0]['dependencies']=[]; profile['nodes'][0]['nested_gpu_end']=99
        with self.assertRaisesRegex(ValueError,'GPU timing'): theoretical_avoidance(profile,graph,[2])


if __name__ == '__main__': unittest.main()

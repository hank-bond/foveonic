"""Check real-trace query decoding and scheduling-independent scenario assertions without GPU work."""

import unittest
from trace_analysis import expected_work, analyze_tables, csv_tables, decode_transport, loss_status, typed_arg, memory_snapshots



class MemorySnapshotTests(unittest.TestCase):
    def test_pairs_only_same_track_and_complete_task_identity(self):
        def event(ts, phase, task=1, track='a'):
            return {'name':'memory.snapshot', 'ts':ts, 'track_id':track,
                    'args':{'phase':phase, 'task':task, 'revision':4, 'photo':7, 'retained_bytes':100}}
        result = memory_snapshots([event(1, 'before_trim'), event(2, 'after_trim', track='b'),
                                   event(3, 'after_trim'), event(4, 'before_trim', task=0),
                                   event(5, 'after_trim', task=0), event(6, 'before_trim', task=2)])
        self.assertEqual(len(result['pairs']), 1)
        self.assertEqual(result['pairs'][0]['before']['ts_ns'], 1)
        self.assertEqual(result['pairs'][0]['after']['ts_ns'], 3)
        self.assertEqual(len(result['unpaired']), 4)


def tables(events):
    result = {'slices': [], 'args': [], 'stats': [{'name':'traced_buf_chunks_overwritten','value':'0','severity':'info','idx':'0','source':'trace'}], 'counters':[], 'flows':[]}
    for index,(time,name,fields) in enumerate(events):
        result['slices'].append({'id':str(index),'ts':str(10_000_000_000+time*1_000_000),'dur':'0','track_id':'1','name':name,'category':'foveonic.scenario','arg_set_id':str(index)})
        for key,value in fields.items():
            if value is None:
                continue
            kind='bool' if isinstance(value,bool) else 'int' if isinstance(value,int) else 'real' if isinstance(value,float) else 'string'
            result['args'].append({'arg_set_id':str(index),'key':'debug.'+key,'value_type':kind,'int_value':str(int(value)) if kind in ('bool','int') else '[NULL]','real_value':str(value) if kind=='real' else '[NULL]','string_value':str(value) if kind=='string' else '[NULL]'})
    return result


def scenario():
    common={'step':1,'action':'warm-edit','coverage':'direct-command','photo_count':2,'scale_divisor':8,'expected_scale_divisor':8}
    return [(0,'scenario.step',{**common,'phase':'begin','expect_original_reads':0,'expect_local_reads':0}),
            (2,'image_accepted',{'task':22,'revision':7,'photo':2,'scale':8}),
            (3,'scenario.step',{**common,'phase':'current','source':'B.x3f','task':22,'revision':7,'photo':2}),
            (4,'image_accepted',{'task':11,'revision':7,'photo':1,'scale':8}),
            (5,'scenario.step',{**common,'phase':'current','source':'A.x3f','task':11,'revision':7,'photo':1}),
            (6,'scenario.step',{**common,'phase':'settled','worker_busy':False,'progress_running':False}),
            (5,'image_frame_use',{'task':22,'revision':7,'photo':2,'scale':8}),
            (5,'image_frame_use',{'task':11,'revision':7,'photo':1,'scale':8})]


class TraceAnalysisTests(unittest.TestCase):
    def test_multiline_csv_and_empty_result_sets(self):
        rows=csv_tables('"a","b"\n1,"two\nlines"\n\n"empty"\n\n"c"\n3\n')
        self.assertEqual(rows,[[{'a':'1','b':'two\nlines'}],[],[{'c':'3'}]])

    def test_hex_transport_preserves_json_quotes_newlines_unicode_and_null(self):
        text = '{"recipe":{"name":"A, B", "value":"line\\nnext"},"label":"雪"}\nsecond line'
        encoded = text.encode('utf-8').hex()
        csv = '"key_hex","string_value_hex"\n64656275672e73657474696e6773,"'+encoded+'"\n64656275672e656d707479,""\n64656275672e6e756c6c,[NULL]\n'
        rows = decode_transport(csv_tables(csv))[0]
        self.assertEqual(rows, [{'key':'debug.settings','string_value':text},
                                {'key':'debug.empty','string_value':''},
                                {'key':'debug.null','string_value':None}])

    def test_unsigned_context_ids_retain_all_bits(self):
        self.assertEqual(typed_arg({'value_type':'uint','int_value':'-1'}),(1<<64)-1)

    def test_loss_and_errors_are_not_hidden_by_successful_stop(self):
        for name,severity in [('traced_buf_chunks_overwritten','info'),('clock_sync_failure','error'),('traced_buf_trace_writer_packet_loss','data_loss')]:
            stats=tables([])['stats']+[{'name':name,'value':'1','severity':severity}]
            self.assertFalse(loss_status(stats,True)['valid'])
        self.assertFalse(loss_status([],True)['loss_checked'])
        self.assertFalse(loss_status(tables([])['stats'],False)['valid'])
        stats=tables([])['stats']+[{'name':'traced_chunks_discarded','value':'2','severity':'info'}]
        self.assertTrue(loss_status(stats,True)['valid'])
        self.assertEqual(len(loss_status(stats,True)['service_warnings']),1)
        self.assertFalse(analyze_tables(tables([]),{'complete':True,'emission_transition_drops':1})['validity']['valid'])

    def test_parallel_completion_order_and_shared_revisions_do_not_merge_photos(self):
        result=analyze_tables(tables(scenario()),{'complete':True})
        self.assertTrue(result['validity']['valid'])
        self.assertTrue(result['behavior_valid'])
        step=result['steps'][0]
        self.assertEqual(step['first_current_observed_ms'],3.)
        self.assertEqual(step['all_current_observed_ms'],5.)
        self.assertEqual(step['group_settled_ms'],6.)
        self.assertEqual(step['first_receipt_ms'],2.)
        self.assertEqual([row['task'] for row in step['current']],[22,11])

    def test_incomplete_group_stale_scale_and_pending_progress_fail_behavior(self):
        for mutate in [lambda rows:rows.pop(4),lambda rows:rows[4][2].update(scale_divisor=4),lambda rows:rows[5][2].update(progress_running=True),lambda rows:rows.pop()]:
            rows=scenario();mutate(rows)
            self.assertFalse(analyze_tables(tables(rows),{'complete':True})['behavior_valid'])

    def test_original_and_local_payloads_are_distinct_from_preview_io(self):
        rows=scenario()+[(1,'source.read.complete',{'storage':'original-preview','bytes':999,'complete':True}),
                         (1,'source.read.complete',{'storage':'original','bytes':100,'complete':True})]
        result=analyze_tables(tables(rows),{'complete':True})
        self.assertFalse(result['behavior_valid'])
        self.assertEqual(result['steps'][0]['payload_read_counts'],{'original':1})
        self.assertEqual(result['steps'][0]['payload_read_bytes'],100)

    def test_explicit_reuse_checks_target_work_and_retains_identities(self):
        for event in [('source.read.complete', {'photo': 1, 'task': 77, 'storage': 'decoded-cache', 'bytes': 20, 'complete': True}),
                      ('gpu.encode.dispatch', {'photo': 1, 'task': 77, 'purpose': 'current'})]:
            rows = scenario()
            rows[0][2].pop('expect_original_reads')
            rows[0][2].pop('expect_local_reads')
            rows[0][2]['expected_work'] = 'reuse'
            result = analyze_tables(tables(rows + [(1, *event)]), {'complete': True})
            self.assertFalse(result['behavior_valid'])
            work = result['steps'][0]
            observed = work['target_payload_reads'] or work['target_photographic_dispatches']
            self.assertEqual(observed[0]['photo'], 1)
            self.assertEqual(observed[0]['task'], 77)
        rows = scenario()
        rows[0][2].pop('expect_original_reads')
        rows[0][2].pop('expect_local_reads')
        rows[0][2]['expected_work'] = 'reuse'
        rows += [(1, 'gpu.encode.dispatch', {'photo': 999, 'task': 9, 'purpose': 'current'}),
                 (1, 'source.read.complete', {'photo': 999, 'task': 0, 'storage': 'original', 'bytes': 10})]
        self.assertTrue(analyze_tables(tables(rows), {'complete': True})['behavior_valid'])
        rows.append((1, 'gpu.encode.dispatch', {'task': 88, 'purpose': 'current'}))
        self.assertFalse(analyze_tables(tables(rows), {'complete': True})['behavior_valid'])

    def test_background_edit_requires_a_real_overlapping_batch_scope(self):
        rows=scenario()
        rows[0][2]['action']='background-edit'
        rows.append((1,'scenario.background_edit',{'batch_started':True}))
        self.assertFalse(analyze_tables(tables(rows),{'complete':True})['behavior_valid'])
        rows.append((0,'batch.photo',{'purpose':'batch_import'}))
        data=tables(rows)
        data['slices'][-1]['dur']='2000000'
        result=analyze_tables(data,{'complete':True})
        self.assertTrue(result['behavior_valid'])
        self.assertEqual(result['steps'][0]['background_overlap']['active_batch_photos'],1)

    def test_rebuild_needs_proof_and_target_dispatch(self):
        rows = scenario()
        rows[0][2]['expected_work'] = 'rebuild'
        dispatch = (1, 'gpu.encode.dispatch', {'photo': 1, 'task': 11, 'purpose': 'current'})
        self.assertFalse(analyze_tables(tables(rows + [dispatch]), {'complete': True})['behavior_valid'])
        rows[0][2].update(evicted_source='A.x3f', retained_before='[\"A.x3f\"]', retained_after='[]')
        self.assertFalse(analyze_tables(tables(rows), {'complete': True})['behavior_valid'])
        result = analyze_tables(tables(rows + [dispatch]), {'complete': True})
        self.assertTrue(result['behavior_valid'])
        self.assertEqual(result['steps'][0]['target_photographic_dispatch_count'], 1)
        for changes in ({'retained_before': '[]'}, {'retained_after': '["A.x3f"]'},
                        {'retained_before': 'invalid'}, {'evicted_source': 'other.x3f'}):
            broken = scenario()
            broken[0][2].update(rows[0][2], **changes)
            self.assertFalse(analyze_tables(tables(broken + [dispatch]), {'complete': True})['behavior_valid'])
        unrelated = (1, 'gpu.encode.dispatch', {'photo': 2, 'purpose': 'current'})
        self.assertFalse(analyze_tables(tables(rows + [unrelated]), {'complete': True})['behavior_valid'])
        rows[0][2]['expected_work'] = 'unexpected'
        self.assertFalse(analyze_tables(tables(rows), {'complete': True})['behavior_valid'])

    def test_cached_observation_does_not_invent_a_new_receipt(self):
        rows=[row for row in scenario() if row[1]!='image_accepted']
        result=analyze_tables(tables(rows),{'complete':True})
        self.assertTrue(result['behavior_valid'])
        self.assertIsNone(result['steps'][0]['first_receipt_ms'])

    def test_cached_selection_rejects_photographic_work_even_without_source_reads(self):
        for purpose in ('current', 'camera', 'defaults', 'detail', 'inspection'):
            rows = scenario()
            rows[0][2]['expect_photographic_dispatches'] = 0
            # Obsolete work still counts, independently of accepted task identity.
            rows.append((1, 'gpu.encode.dispatch', {'purpose': purpose, 'task': 999}))
            result = analyze_tables(tables(rows), {'complete': True})
            self.assertFalse(result['behavior_valid'], purpose)
            self.assertEqual(result['steps'][0]['photographic_dispatch_count'], 1)
            self.assertEqual(result['steps'][0]['payload_read_counts'], {})

    def test_cached_selection_allows_auxiliary_work_and_ignores_work_outside_step(self):
        rows = scenario()
        rows[0][2]['expect_photographic_dispatches'] = 0
        rows += [(1, 'gpu.encode.dispatch', {'purpose': 'scopes'}),
                 (1, 'gpu.encode.dispatch', {'purpose': 'navigator'}),
                 (7, 'gpu.encode.dispatch', {'purpose': 'current'})]
        result = analyze_tables(tables(rows), {'complete': True})
        self.assertTrue(result['behavior_valid'])
        self.assertEqual(result['steps'][0]['photographic_dispatch_count'], 0)

    def test_unrelated_prefetch_is_separate_but_a_retained_photo_reread_fails(self):
        rows = scenario()
        unrelated = (1, 'source.read.complete', {'task': 0, 'purpose': 'decode-prefetch', 'photo': 999,
                                              'storage': 'original', 'bytes': 100, 'complete': True})
        result = analyze_tables(tables(rows + [unrelated]), {'complete': True})
        self.assertTrue(result['behavior_valid'])
        self.assertEqual(result['steps'][0]['payload_read_counts'], {})
        self.assertEqual(result['steps'][0]['background_payload_read_counts'], {'original': 1})
        self.assertEqual(result['steps'][0]['background_payload_read_bytes'], 100)
        for changed in ({'photo': 1}, {'task': 12, 'purpose': 'current'},
                        {'photo': None}, {'task': None}):
            reread = (1, unrelated[1], {**unrelated[2], **changed})
            self.assertFalse(analyze_tables(tables(rows + [reread]), {'complete': True})['behavior_valid'])

    def test_unrelated_noncoordinator_reads_do_not_depend_on_purpose_names(self):
        for purpose in ('thumb_prefetch', 'decode-prefetch', 'other-background-work'):
            with self.subTest(purpose=purpose):
                read = (1, 'source.read.complete', {'task': 0, 'purpose': purpose,
                        'photo': 999, 'storage': 'decoded-cache', 'bytes': 0,
                        'complete': False})
                result = analyze_tables(tables(scenario() + [read]), {'complete': True})
                self.assertTrue(result['behavior_valid'])
                self.assertEqual(result['steps'][0]['background_payload_read_counts'],
                                 {'decoded-cache': 1})
                read[2]['photo'] = 1
                self.assertFalse(analyze_tables(tables(scenario() + [read]),
                                               {'complete': True})['behavior_valid'])

    def test_reader_cancelled_before_io_is_distinct_from_failed_or_incomplete_io(self):
        fields = {'task': 0, 'photo': 1, 'purpose': 'thumb_prefetch',
                  'storage': 'decoded-cache', 'bytes': 0, 'read_operations': 0,
                  'read_errors': 0, 'complete': False}
        rows = scenario() + [(1, 'source.read.complete', fields)]
        result = analyze_tables(tables(rows), {'complete': True})
        self.assertTrue(result['behavior_valid'])
        self.assertEqual(result['steps'][0]['payload_read_counts'], {})
        self.assertEqual(result['steps'][0]['payload_attempts_without_io'], 1)
        for changed in ({'read_operations': 1}, {'bytes': 1}, {'read_errors': 1}):
            read = (1, 'source.read.complete', {**fields, **changed})
            self.assertFalse(analyze_tables(tables(scenario() + [read]),
                                           {'complete': True})['behavior_valid'])

    def test_every_current_identity_needs_native_encoding_even_for_cached_reuse(self):
        rows=[row for row in scenario() if not (row[1]=='image_frame_use' and row[2]['photo']==1)]
        result=analyze_tables(tables(rows),{'complete':True})
        self.assertFalse(result['behavior_valid'])
        self.assertTrue(any('native encoding' in issue for issue in result['behavior_failures']))
        rows=scenario();rows[-1][2]['scale']=4
        self.assertFalse(analyze_tables(tables(rows),{'complete':True})['behavior_valid'])

    def test_scope_hierarchy_and_thread_metadata_keep_inclusive_stages_separate(self):
        context={'task':11,'revision':7,'photo':1,'purpose':'current'}
        data=tables([(0,'render_task',context),(1,'decode.plane.top',{**context,'layer':'top','width':4928,'height':3264,'compressed_bytes':12345}),(2,'gpu.pass',{**context,'node':3,'submission':4,'clock_id':5,'start_tick':10,'end_tick':30,'duration_ns':2_000_000})])
        data['slices'][0].update(dur='10000000',depth='0',parent_id='[NULL]')
        data['slices'][1].update(dur='5000000',depth='1',parent_id='0')
        data['tracks']=[{'id':'1','utid':'8','tid':'42','thread_name':'worker','upid':'9','pid':'99','process_name':'app'}]
        result=analyze_tables(data,{'complete':True})
        hierarchy=result['scope_attribution']['scopes']
        self.assertEqual(hierarchy[1]['ancestors'],[{'slice':0,'name':'render_task'}])
        self.assertEqual(hierarchy[1]['depth'],1)
        self.assertEqual(hierarchy[1]['thread']['thread_name'],'worker')
        self.assertEqual(hierarchy[1]['context'],context)
        self.assertEqual(hierarchy[1]['attributes'],{'layer':'top','width':4928,'height':3264,'compressed_bytes':12345})
        stages=result['scope_attribution']['task_stage_elapsed_ms'][0]['stages']
        self.assertEqual({row['name']:row['inclusive_total'] for row in stages},{'decode.plane.top':5.,'render_task':10.})
        gpu=result['gpu_node_submission'][0]
        self.assertEqual((gpu['task'],gpu['node'],gpu['submission']),(11,3,4))
        self.assertEqual(gpu['pass_duration_total_ms'],2.)

    def test_node_census_uses_submission_ownership_when_indices_repeat_across_graphs(self):
        context = {'task': 11, 'revision': 7, 'photo': 1, 'purpose': 'current', 'node': 0}
        rows = [(0, 'graph.execute', context),
                (1, 'Native correction', {**context, 'operation': 'Correction', 'executed': True}),
                (1, 'graph.geometry', {**context, 'scale': 1, 'width': 4000, 'height': 3000}),
                (2, 'gpu.encode.dispatch', context), (2, 'gpu.submit', {**context, 'submission': 41}),
                (4, 'graph.execute', context),
                (5, 'Working correction', {**context, 'operation': 'Correction', 'executed': True}),
                (5, 'graph.geometry', {**context, 'scale': 8, 'width': 500, 'height': 375}),
                (6, 'gpu.encode.dispatch', context), (6, 'gpu.submit', {**context, 'submission': 42}),
                (7, 'Retained input', {**context, 'node': 1, 'operation': 'Input', 'executed': False, 'seed_hits': 1}),
                (10, 'gpu.pass', {**context, 'submission': 42, 'clock_id': 5, 'start_tick': 10, 'end_tick': 30, 'duration_ns': 2_000_000}),
                (11, 'gpu.pass', {**context, 'submission': 41, 'clock_id': 5, 'start_tick': 1, 'end_tick': 10, 'duration_ns': 20_000_000})]
        data = tables(rows)
        for index, parent in {1:0, 2:1, 3:1, 4:3, 6:5, 7:6, 8:6, 9:8, 10:5}.items():
            data['slices'][index]['parent_id'] = str(parent)
        data['slices'][1]['dur'] = '3000000'
        data['slices'][6]['dur'] = '1000000'
        result = analyze_tables(data, {'complete': True})['work_census']
        nodes = {row['slice']: row for row in result['nodes']}
        self.assertEqual(result['totals'], {'native_nodes': 1, 'reduced_nodes': 1, 'reused_nodes': 1, 'dispatches': 2, 'submissions': 2})
        self.assertEqual(nodes[1]['gpu_pass_ms'], 20.)
        self.assertEqual(nodes[6]['gpu_pass_ms'], 2.)
        self.assertEqual(nodes[1]['inclusive_host_ms'], 3.)
        self.assertEqual(nodes[6]['geometry']['width'], 500)
        self.assertIsNone(nodes[10]['geometry'])
        self.assertIsNone(nodes[10]['gpu_pass_ms'])
        self.assertEqual(result['unattributed_gpu_passes'], 0)

    def test_incomplete_interaction_retains_intent_ingress_and_widget_result(self):
        common = {'step': 1, 'action': 'cached-switch', 'photo_count': 1, 'scale_divisor': 8}
        rows = [(0, 'scenario.step', {**common, 'phase': 'begin'}),
                (0, 'scenario.pointer_click', {'photo': 1, 'x': 10.5, 'y': 20.5}),
                (1, 'input.pointer_button', {'pressed': True, 'x': 10.5, 'y': 20.5}),
                (2, 'input.pointer_button', {'pressed': False, 'x': 10.5, 'y': 20.5}),
                (2, 'viewport.tile.release', {'photo': 1, 'clicked': False}),
                (60, 'scenario.step', {**common, 'phase': 'failed', 'error': 'selection did not apply', 'worker_busy': False})]
        result = analyze_tables(tables(rows), {'complete': True})
        self.assertFalse(result['behavior_valid'])
        self.assertEqual(result['steps'], [])
        failed = result['incomplete_steps'][0]
        self.assertFalse(failed['failure']['worker_busy'])
        self.assertEqual([row['event'] for row in failed['interactions']],
                         ['scenario.pointer_click', 'input.pointer_button', 'input.pointer_button', 'viewport.tile.release'])
        self.assertEqual(failed['interactions'][0]['x'], 10.5)

    def test_gpu_foreground_completion_does_not_hide_pending_auxiliary_query(self):
        current={'capture_id':1,'query':1,'clock_id':2,'task':10,'revision':7,'photo':3,'purpose':'current'}
        scopes={'capture_id':1,'query':2,'clock_id':2,'task':11,'revision':7,'photo':3,'purpose':'scopes'}
        rows=[(0,'gpu.query.started',current),(1,'gpu.query.resolved',current),
              (2,'gpu.query.started',scopes)]
        result=analyze_tables(tables(rows),{'complete':True})
        self.assertTrue(result['validity']['valid'])
        gpu=result['gpu_query_status']
        self.assertFalse(gpu['complete']);self.assertEqual(gpu['unresolved_queries'],[scopes])
        purposes={row['purpose']:row for row in gpu['by_purpose']}
        self.assertTrue(purposes['current']['complete'])
        self.assertFalse(purposes['scopes']['complete'])
        self.assertEqual(purposes['scopes']['unresolved'],1)

    def test_queue_render_and_delivery_measurements_keep_distinct_meanings(self):
        context={'task':11,'revision':7,'photo':1}
        rows=scenario()+[(1,'render_queue',{**context,'interval_kind':'elapsed_cpu_clock','duration_ns':1_000_000}),
                         (1,'render_task',context),
                         (4,'request_to_ui',{**context,'interval_kind':'elapsed_cpu_clock','duration_ns':4_000_000})]
        data=tables(rows);data['slices'][-2]['dur']='3000000'
        result=analyze_tables(data,{'complete':True})
        photo=next(row for row in result['steps'][0]['photo_lifecycle'] if row['photo']==1)
        self.assertEqual(photo['queue_wait_ms']['median'],1.)
        self.assertEqual(photo['render_elapsed_ms']['median'],3.)
        self.assertEqual(photo['request_to_ui_ms']['median'],4.)
        self.assertEqual(photo['receipt_to_encoding_ms'],1.)

    def test_existing_smoke_capture_preserves_pending_preview_and_original_lineage(self):
        common={'suite':'smart','step_count':2,'coverage':'mixed-existing-smoke','mode':'wall'}
        rows=[(0,'smoke.step',{**common,'step':1,'target':'preview','phase':'begin'}),
              (2,'smoke.step',{**common,'step':1,'target':'preview','phase':'checked','worker_busy':True}),
              (2,'smoke.step',{**common,'step':2,'target':'final','phase':'begin'}),
              (4,'smoke.step',{**common,'step':2,'target':'final','phase':'checked','worker_busy':False,
                               'task':12,'revision':7,'presentation_revision':9,'scale_divisor':1}),
              (5,'smoke.completed',{'suite':'smart','step_count':2,'checked_steps':2})]
        result=analyze_tables(tables(rows),{'complete':True})
        self.assertTrue(result['behavior_valid'])
        self.assertTrue(result['smoke']['steps'][0]['worker_busy_at_check'])
        self.assertEqual(result['smoke']['steps'][1]['displayed']['revision'],7)
        for changed in (rows[:-1],rows[:1]+rows[2:]):
            self.assertFalse(analyze_tables(tables(changed),{'complete':True})['behavior_valid'])

    def test_smoke_reuse_assertions_share_scenario_work_count_validation(self):
        common = {'suite': 'smart', 'step_count': 1, 'step': 1, 'target': 'cache-reopen'}
        rows = [(0, 'smoke.step', {**common, 'phase': 'begin', 'expect_original_reads': 0,
                                 'expect_local_reads': 0, 'expect_photographic_dispatches': 0}),
                (4, 'smoke.step', {**common, 'phase': 'checked', 'worker_busy': False}),
                (5, 'smoke.completed', {'suite': 'smart', 'step_count': 1, 'checked_steps': 1})]
        self.assertTrue(analyze_tables(tables(rows), {'complete': True})['behavior_valid'])
        for event in [('gpu.encode.dispatch', {'purpose': 'current'}),
                      ('source.read.complete', {'storage': 'original', 'complete': True}),
                      ('source.read.complete', {'storage': 'decoded-cache', 'complete': True})]:
            result = analyze_tables(tables(rows + [(2, *event)]), {'complete': True})
            self.assertFalse(result['behavior_valid'], event)

    def test_smoke_fatal_failure_is_explicit_before_first_step_or_after_scenario_settles(self):
        fatal=(7,'smoke.failed',{'reason':'panic'})
        for rows in ([fatal],scenario()+[fatal]):
            result=analyze_tables(tables(rows),{'complete':True})
            self.assertTrue(result['validity']['valid'])
            self.assertIs(result['behavior_valid'],False)
            self.assertIn('smoke failed: panic',result['behavior_failures'])
            self.assertFalse(result['smoke']['valid'])

    def test_mixed_camera_scales_follow_each_photo_through_native_encoding(self):
        rows=scenario()
        for _, name, fields in rows:
            if fields.get('photo')==2:
                if name=='scenario.step':
                    fields['scale_divisor']=4
                    fields['expected_scale_divisor']=4
                else:
                    fields['scale']=4
        result=analyze_tables(tables(rows),{'complete':True})
        self.assertTrue(result['behavior_valid'])
        self.assertEqual(result['steps'][0]['scale_divisors'],[4,8])
        for _, name, fields in rows:
            if name=='image_frame_use' and fields.get('photo')==2:
                fields['scale']=8
        self.assertFalse(analyze_tables(tables(rows),{'complete':True})['behavior_valid'])

    def test_manual_capture_has_no_invented_behavior_result_or_group_clock(self):
        result=analyze_tables(tables([(10,'capture.start',{})]),{'complete':True})
        self.assertIsNone(result['behavior_valid'])
        self.assertEqual(result['steps'],[])


class PrefetchCancellationTests(unittest.TestCase):
    def events(self):
        return [
            {'id': '1', 'parent_id': None, 'name': 'cache.processing.prefetch', 'args': {'source': 'A.x3f'}},
            {'id': '2', 'parent_id': '1', 'name': 'cache.decoded.lookup', 'args': {}},
            {'id': '3', 'parent_id': '2', 'name': 'source.read.complete', 'args': {
                'task': 0, 'photo': 1, 'purpose': 'thumb_prefetch', 'source': 'A.x3f', 'storage': 'decoded-cache',
                'read_operations': 1, 'bytes': 64, 'read_errors': 0, 'complete': False}},
            {'id': '5', 'parent_id': '2', 'name': 'cache.decoded.miss',
             'args': {'source': 'A.x3f', 'reason': 'background prefetch interrupted'}},
            {'id': '4', 'parent_id': '1', 'name': 'cache.processing.prefetch_result',
             'args': {'source': 'A.x3f', 'outcome': 'interrupted'}},
        ]

    def test_interrupted_read_is_not_an_io_failure_and_still_counts_physical_work(self):
        work, issues = expected_work(self.events(), {}, {1})
        self.assertEqual(issues, [])
        self.assertEqual(work['payload_read_counts'], {'decoded-cache': 1})
        self.assertEqual(work['payload_read_bytes'], 64)
        self.assertEqual(work['cancelled_payload_read_count'], 1)
        self.assertEqual(work['cancelled_payload_read_bytes'], 64)
        for expectations in ({'expect_local_reads': 0}, {'expected_work': 'reuse'}):
            self.assertTrue(expected_work(self.events(), expectations, {1})[1],
                            'cancellation must not hide unnecessary reads')

    def test_missing_or_unrelated_interruption_does_not_excuse_incomplete_reads(self):
        for change in ['missing', 'scope', 'source', 'outcome', 'read_error', 'foreground',
                       'missing_miss', 'miss_scope', 'miss_source', 'checksum', 'storage',
                       'purpose', 'missing_errors']:
            events = self.events()
            if change == 'missing': events.pop()
            if change == 'scope': events[-1]['parent_id'] = 'other-scope'
            if change == 'source': events[-1]['args']['source'] = 'B.x3f'
            if change == 'outcome': events[-1]['args']['outcome'] = 'not-retained'
            if change == 'read_error': events[2]['args']['read_errors'] = 1
            if change == 'foreground': events[2]['args']['task'] = 1
            if change == 'missing_miss': events.pop(3)
            if change == 'miss_scope': events[3]['parent_id'] = 'other-lookup'
            if change == 'miss_source': events[3]['args']['source'] = 'B.x3f'
            if change == 'checksum': events[3]['args']['reason'] = 'checksum mismatch'
            if change == 'storage': events[2]['args']['storage'] = 'original'
            if change == 'purpose': events[2]['args']['purpose'] = 'current'
            if change == 'missing_errors': events[2]['args'].pop('read_errors')
            work, issues = expected_work(events, {}, {1})
            self.assertIn('a source payload read failed or was incomplete', issues, change)
            self.assertEqual(work['cancelled_payload_read_count'], 0)


if __name__=='__main__':unittest.main()

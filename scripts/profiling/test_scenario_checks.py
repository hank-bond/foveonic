"""Check independent scenario edits before fresh-image replay."""

import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from scenarios import compare_scenario_images
from cache_workflow import associate_trace, retention_assertion, zoom_action
from scenario_checks import validate_choice_recipes, check_cache_workflow, checked_scenario_images, check_detail_layers


class ScenarioCheckTests(unittest.TestCase):
    def test_layering_requires_partial_overlap_distinct_images_and_both_views(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            evidence = root/'evidence'
            evidence.mkdir()
            for name in ('loupe-partial.png', 'zoom-partial.png'):
                (evidence/name).write_bytes(b'captured')
            observations = ('partial loupe retained overview and native section while replacement held',
                            'zoom movement retained overview and native section while replacement held')
            loupe = {'phase':'Done', 'single_whole':False, 'events':[
                {'observed':observation, 'overview':[1, 1], 'detail':[2, 2],
                 'region':[0, 0, 100, 100], 'required':[50, 0, 150, 100]}
                for observation in observations]}
            check_detail_layers(loupe, root)
            for change in ('complete', 'disjoint', 'same-image', 'missing-zoom', 'unfinished'):
                bad = json.loads(json.dumps(loupe))
                if change == 'complete': bad['events'][0]['required'] = [10, 10, 90, 90]
                elif change == 'disjoint': bad['events'][0]['required'] = [100, 0, 200, 100]
                elif change == 'same-image': bad['events'][0]['detail'] = [1, 1]
                elif change == 'missing-zoom': bad['events'].pop()
                else: bad['phase'] = 'PartialCapture'
                with self.subTest(change=change), self.assertRaises(ValueError):
                    check_detail_layers(bad, root)
            loupe['single_whole'] = True
            loupe['events'].pop()
            check_detail_layers(loupe, root)
            (evidence/'loupe-partial.png').unlink()
            with self.assertRaises(ValueError): check_detail_layers(loupe, root)

    def setUp(self):
        quiet = patch('image_replay.require_quiet')
        quiet.start()
        self.addCleanup(quiet.stop)

    def test_choice_journey_recipes_and_independent_final_evidence(self):
        cases=[{'settings':{'white_balance':wb,'tone_enabled':True,'tone':{'contrast':0.25},
                            'exposure':index,'exposure_auto':False}}
               for index,wb in enumerate(('Sunlight','Incandescent'))]
        validate_choice_recipes(cases)
        for change in ('same', 'disabled', 'neutral', 'missing'):
            bad=json.loads(json.dumps(cases))
            if change=='same':bad[1]['settings']['white_balance']='Sunlight'
            elif change=='disabled':bad[1]['settings']['tone_enabled']=False
            elif change=='neutral':bad[1]['settings']['tone']['contrast']=0
            else:bad[1].pop('settings')
            with self.assertRaises(ValueError):validate_choice_recipes(bad)
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            evidence=root/'evidence'
            evidence.mkdir()
            geometry={'extent':[2,1],'layout':'Rgb32Float','space':'test','linear_white':1,
                      'view_baked':False,'view_extent':[2,1],'view_crop':[0,0,2,1],
                      'view_rotation':0,'endianness':'little'}
            inputs=[]
            initial=[]
            photos=[]
            captures=[]
            for index,case in enumerate(cases):
                source=str(root/f'{index}.x3f')
                start=case['settings']
                final={**start,'white_balance':'Sunlight','tone_enabled':False}
                inputs.append({'source':f'original-{index}.x3f','staged_source':source,'settings':start})
                initial.append({'source':source,'settings':dict(start)})
                photos.append({'source':source,'task':index+10,'revision':4,'settings':dict(final),'scale_divisor':8})
                capture={**geometry,'source':source,'task':index+10,'revision':4,
                         'settings':{'settings':dict(final),'working_scale':8},'file':str(evidence/f'image-{index:02}.f32')}
                Path(capture['file']).write_bytes(bytes(range(24)))
                captures.append(capture)
            events=[{'observation':'mixed White Balance'},{'action':'choose Sunlight'},
                    {'action':'disable Tone'},{'observation':'Tone unchecked and final images current'}]
            native={'passed':True,'hardware':{'adapter':'GPU'},'image_captures':captures,
                    'steps':[{'action':'initial-selection','photos':initial},{'action':'choice-journey','photos':photos}],
                    'choices':{'events':events}}
            def fresh(command,**_kwargs):
                snapshot=json.loads(Path(command[command.index('--settings-snapshot')+1]).read_text())
                goldens=Path(command[command.index('--goldens')+1])
                goldens.mkdir(exist_ok=True)
                (goldens/(Path(command[1]).stem+'.f32')).write_bytes(bytes(range(24)))
                rows=[('session',{'settings':snapshot['settings'],'adapter':'GPU'}),
                      ('processing',{'effective_scale_divisor':8}),('validation',geometry)]
                return SimpleNamespace(returncode=0,stderr='',stdout='\n'.join(json.dumps({'event':event,'data':data}) for event,data in rows))
            with patch('image_replay.subprocess.run',side_effect=fresh):
                self.assertTrue(all(check['outcome']=='passed' for check in compare_scenario_images(native,inputs,root/'renderer',root)))
                photos[1]['settings']['white_balance']='Incandescent'
                with self.assertRaisesRegex(ValueError,'independent expectation'):compare_scenario_images(native,inputs,root/'renderer',root)
                photos[1]['settings']['white_balance']='Sunlight'
                photos[0]['settings']['exposure']=0.5
                with self.assertRaisesRegex(ValueError,'independent expectation'):compare_scenario_images(native,inputs,root/'renderer',root)
                photos[0]['settings']['exposure']=0
                captures[1]['settings']['settings']['tone_enabled']=True
                with self.assertRaisesRegex(ValueError,'captured image choices'):compare_scenario_images(native,inputs,root/'renderer',root)
                captures[1]['settings']['settings']['tone_enabled']=False
                initial[1]['settings']['white_balance']='Sunlight'
                with self.assertRaisesRegex(ValueError,'initial choices'):compare_scenario_images(native,inputs,root/'renderer',root)
                initial[1]['settings']['white_balance']='Incandescent'
                native['choices']['events']=events[:-1]
                with self.assertRaisesRegex(ValueError,'input evidence'):compare_scenario_images(native,inputs,root/'renderer',root)


    def test_numeric_slider_checks_declared_values_and_effective_settings(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            evidence=root/'evidence'
            evidence.mkdir()
            source=str(root/'photo.x3f')
            initial={'tone':{'contrast':0.1},'exposure_auto':True}
            final={'tone':{'contrast':0.2},'exposure_auto':True}
            photo={'source':source,'task':1,'revision':2,'settings':final,'scale_divisor':8}
            native={'passed':True,'hardware':{'adapter':'GPU'},
                    'steps':[{'action':'initial-selection','photos':[{'source':source,'settings':initial}]},
                             {'action':'slider-drag','photos':[photo]}],
                    'image_captures':[{'source':source,'task':1,'revision':2,
                                       'settings':{'settings':final,'working_scale':8}}]}
            inputs=[{'source':'original','staged_source':source,'settings':{'tone':{'contrast':0.1}}}]
            drag={'pointer':'/tone/contrast','paths':[source],'initial':[0.1],
                  'frames':[{'values':[0.2]}],'errors':[]}
            path=evidence/'slider-drag.json'
            path.write_text(json.dumps(drag))
            initial['tone']['contrast']=0.3
            with self.assertRaisesRegex(ValueError,'initial /tone/contrast'):
                compare_scenario_images(native,inputs,root/'renderer',root,'/tone/contrast')
            initial['tone']['contrast']=0.1
            native['image_captures'][0]['settings']['settings']={'tone':{'contrast':0.4},'exposure_auto':True}
            with self.assertRaisesRegex(ValueError,'checked edited property'):
                compare_scenario_images(native,inputs,root/'renderer',root,'/tone/contrast')
            native['image_captures'][0]['settings']['settings']=final
            geometry={'extent':[1,1],'layout':'Rgb32Float','space':'test','linear_white':1,
                      'view_baked':False,'view_extent':[1,1],'view_crop':[0,0,1,1],
                      'view_rotation':0,'endianness':'little'}
            capture=native['image_captures'][0]
            capture.update(geometry, file=str(evidence/'image-00.f32'))
            Path(capture['file']).write_bytes(bytes(12))
            def fresh(command,**_kwargs):
                snapshot=json.loads(Path(command[command.index('--settings-snapshot')+1]).read_text())
                self.assertTrue(snapshot['settings']['exposure_auto'])
                goldens=Path(command[command.index('--goldens')+1])
                goldens.mkdir()
                (goldens/(Path(source).stem+'.f32')).write_bytes(bytes(12))
                rows=[('session',{'settings':snapshot['settings'],'adapter':'GPU'}),
                      ('processing',{'effective_scale_divisor':8}),('validation',geometry)]
                return SimpleNamespace(returncode=0,stderr='',stdout='\n'.join(json.dumps({'event':event,'data':data}) for event,data in rows))
            with patch('image_replay.subprocess.run',side_effect=fresh):
                self.assertEqual(compare_scenario_images(native,inputs,root/'renderer',root,'/tone/contrast')[0]['outcome'],'passed')
            drag['frames'][-1]['values']=[0.4]
            path.write_text(json.dumps(drag))
            with self.assertRaisesRegex(ValueError,'slider drag values differ'):
                compare_scenario_images(native,inputs,root/'renderer',root,'/tone/contrast')



    def test_cache_trace_association_excludes_old_tasks_and_outside_windows(self):
        workflow = {'trace_origin_ns':1_000_000_000, 'rows':[
            {'event':'action', 'action':'one', 'elapsed_ms':0, 'minimum_revision':5},
            {'event':'first-current', 'action':'one', 'task':10, 'revision':5},
            {'event':'reuse-observed', 'action':'one', 'task':2, 'revision':2},
            {'event':'action', 'action':'return-fit', 'elapsed_ms':10, 'minimum_revision':6},
            {'event':'fit-current', 'action':'return-fit', 'elapsed_ms':12}]}
        def pair(ts, task, revision):
            return {'before':{'ts_ns':ts, 'task':task, 'revision':revision},
                    'after':{'ts_ns':ts+1, 'task':task, 'revision':revision}}
        analysis = {'memory_snapshots':{'pairs':[pair(1_001_000_000, 10, 5),
                     pair(1_002_000_000, 2, 2), pair(1_011_000_000, 10, 5)]},
                    'resource_events':[{'ts_ns':1_003_000_000, 'name':'cache.native_source', 'args':{'decision':'hit', 'reason':'same_source'}},
                    {'ts_ns':1_013_000_000, 'name':'cache.native_source', 'args':{}}]}
        result = associate_trace(workflow, analysis)
        self.assertEqual(len(result['actions'][0]['matched_memory_pairs']), 1)
        self.assertEqual(result['actions'][0]['window_event_counts'], {'cache.native_source':1})
        self.assertEqual(result['actions'][0]['window_native_decisions'], {'hit:same_source':1})
        self.assertEqual(result['actions'][1]['matched_memory_pairs'], [])
        self.assertEqual(result['actions'][1]['window_event_counts'], {})
        workflow['trace_origin_ns'] = None
        self.assertFalse(associate_trace(workflow, analysis)['available'])

    def test_cache_workflow_uses_declared_deltas_and_distinguishes_observations(self):
        inputs = [{'staged_source': name, 'source': name,
                   'settings': {'exposure': start, 'exposure_auto': False}}
                  for name, start in [('a', 0.), ('b', 0.5)]]
        initial = [{'source':item['staged_source'], 'settings':dict(item['settings'])} for item in inputs]
        final = [{'source':item['staged_source'], 'settings':{'exposure':item['settings']['exposure'] + 0.375,
                                                              'exposure_auto':False}} for item in inputs]
        rows = []
        def action(name, start, delta=None):
            row = {'event':'action', 'action':name, 'elapsed_ms':start}
            if delta is not None: row['delta'] = delta
            rows.append(row)
        def frame(event, name, photo, exposure, start, elapsed, scale=1):
            row = {'event':event, 'action':name, 'photo':photo, 'source':inputs[photo]['staged_source'],
                   'exposure':exposure, 'elapsed_ms':start+elapsed, 'action_elapsed_ms':elapsed,
                   'rendition':'Current', 'effective_scale':scale, 'actual_region':None,
                   'visible_region':[10,10,20,20], 'available_region':[0,0,30,30]}
            if event == 'first-current': row['current'] = True
            rows.append(row)
        action('fit-smart', 0)
        for photo, item in enumerate(inputs):
            frame('fit-overview', 'fit-smart', photo, item['settings']['exposure'], 0, photo + 1, 4)
            rows[-1]['demanded_scale'] = 4
        action('zoom-a-200', 10)
        for photo, item in enumerate(inputs):
            frame('reuse-observed', 'zoom-a-200', photo, item['settings']['exposure'], 10, photo + 1)
        frame('behind-current', 'zoom-a-200', 0, 0., 10, 4)
        for index, (delta, offset) in enumerate(zip((0.125, 0.125, -0.125, 0.25), (0.125, 0.25, 0.125, 0.375))):
            name=f'exposure-{index}'
            start=20+10*index
            action(name, start, delta)
            for photo, item in enumerate(inputs):
                frame('first-current' if photo == 0 else 'first-current-observed', name, photo,
                      item['settings']['exposure'] + offset, start, photo + 1)
            rows.append({'event':'all-foreground-observed', 'action':name, 'elapsed_ms':start+3, 'action_elapsed_ms':3})
            frame('behind-current', name, 0, offset, start, 4)
        action('drag', 55)
        rows.append({'event':'drag-last-move', 'action':'drag', 'elapsed_ms':56, 'action_elapsed_ms':1})
        rows.append({'event':'drag-patch-current', 'action':'drag', 'elapsed_ms':58, 'action_elapsed_ms':3,
                     'since_last_move_ms':200, 'covered_by':'patch'})
        action('return-fit', 60)
        rows.append({'event':'fit-current', 'action':'return-fit', 'elapsed_ms':61, 'action_elapsed_ms':1})
        native={'passed':True, 'steps':[{'action':'initial-selection', 'photos':initial},
                                        {'action':'cache-workflow', 'photos':final}],
                'cache_workflow':{'rows':rows, 'native_photo_count':1, 'expected_exposures':[99, 99]}}
        both = json.loads(json.dumps(native))
        both['cache_workflow']['native_photo_count'] = 2
        both_rows = both['cache_workflow']['rows']
        for row in both_rows:
            if row.get('action') == 'zoom-a-200': row['action'] = 'zoom-both-200'
        for row in list(reversed(both_rows)):
            if row.get('event') == 'behind-current':
                other = dict(row, photo=1, source='b', exposure=row['exposure'] + 0.5)
                both_rows.insert(both_rows.index(row) + 1, other)
        zoom_b = next(row for row in both_rows if row.get('event') == 'behind-current' and row.get('photo') == 1)
        zoom_b['elapsed_ms'] += 0.1
        zoom_b['action_elapsed_ms'] += 0.1
        result = check_cache_workflow(both, inputs, 2)
        self.assertEqual(result['native_photo_count'], 2)
        both['image_captures'] = [{'source':photo['source'], 'settings':{'settings':dict(photo['settings'])}}
                                  for photo in both['steps'][1]['photos']]
        self.assertEqual(checked_scenario_images(both, inputs, Path('.'), native_photo_count=2)[0], 'cache-workflow')
        with self.assertRaisesRegex(ValueError, 'declared count'):
            checked_scenario_images(both, inputs, Path('.'), native_photo_count=1)
        self.assertEqual(len(result['actions'][0]['behind_current_by_photo']), 2)
        self.assertAlmostEqual(result['actions'][0]['behind_current_observed_ms'], 4.1)
        with self.assertRaisesRegex(ValueError, 'declared count'):
            check_cache_workflow(both, inputs, 1)
        extra = next(row for row in both_rows if row.get('event') == 'behind-current' and row.get('photo') == 1)
        both_rows.remove(extra)
        with self.assertRaisesRegex(ValueError, 'behind every native photo'):
            check_cache_workflow(both, inputs, 2)
        both_rows.insert(both_rows.index(next(row for row in both_rows if row.get('event') == 'behind-current')) + 1, extra)
        second = next(row for row in both_rows if row.get('event') == 'reuse-observed' and row.get('photo') == 1)
        second['effective_scale'] = 4
        with self.assertRaisesRegex(ValueError, 'insufficient scale'):
            check_cache_workflow(both, inputs, 2)
        second['effective_scale'] = 1
        second['available_region'] = [11, 0, 30, 30]
        with self.assertRaisesRegex(ValueError, 'cover the viewport'):
            check_cache_workflow(both, inputs, 2)
        report=check_cache_workflow(native, inputs)
        self.assertEqual(report['expected_final_exposures'], [0.375, 0.875])
        edit=report['actions'][1]
        self.assertEqual(len(edit['first_current_admissions']), 1)
        self.assertEqual(len(edit['first_current_observations']), 1)
        self.assertEqual(edit['foreground_on_admission_ms'], [])
        self.assertEqual(edit['foreground_by_observation_ms'], [3])
        self.assertEqual(edit['behind_current_observed_ms'], 4)
        final[0]['settings']['exposure'] = 99
        with self.assertRaisesRegex(ValueError, 'independent expectation'):
            check_cache_workflow(native, inputs)
        final[0]['settings']['exposure'] = 0.375
        edit_action = next(row for row in rows if row['event'] == 'action' and row['action'] == 'exposure-0')
        edit_action['delta'] = 9
        with self.assertRaisesRegex(ValueError, 'declared delta'):
            check_cache_workflow(native, inputs)
        edit_action['delta'] = 0.125
        first = next(row for row in rows if row['event'] == 'first-current')
        first['exposure'] = 0.25
        with self.assertRaisesRegex(ValueError, 'wrong photo or exposure'):
            check_cache_workflow(native, inputs)
        first['exposure'] = 0.125
        first['available_region'] = [11, 0, 30, 30]
        with self.assertRaisesRegex(ValueError, 'cover the viewport'):
            check_cache_workflow(native, inputs)
        first['available_region'] = [0, 0, 30, 30]
        first['effective_scale'] = 2
        with self.assertRaisesRegex(ValueError, 'insufficient scale'):
            check_cache_workflow(native, inputs)
        first['effective_scale'] = 1
        first['rendition'] = 'Camera'
        with self.assertRaisesRegex(ValueError, 'wrong photo or exposure'):
            check_cache_workflow(native, inputs)
        first['rendition'] = 'Current'
        first['elapsed_ms'] = float('nan')
        with self.assertRaisesRegex(ValueError, 'nonmonotonic timing'):
            check_cache_workflow(native, inputs)
        first['elapsed_ms'] = 21
        foreground = next(row for row in rows if row['event'] == 'all-foreground-observed')
        foreground['event'] = 'unrelated'
        with self.assertRaisesRegex(ValueError, 'foreground completion'):
            check_cache_workflow(native, inputs)
        foreground['event'] = 'all-foreground-observed'
        whole = next(row for row in rows if row['event'] == 'behind-current' and row['action'] == 'exposure-0')
        whole['exposure'] = 0.
        with self.assertRaisesRegex(ValueError, 'behind the patch is stale'):
            check_cache_workflow(native, inputs)
        whole['exposure'] = 0.125
        drag = next(row for row in rows if row['event'] == 'drag-patch-current')
        drag['since_last_move_ms'] = 521
        with self.assertRaisesRegex(ValueError, 'latency target'):
            check_cache_workflow(native, inputs)
        drag['since_last_move_ms'] = 2
        with self.assertRaisesRegex(ValueError, 'latency target'):
            check_cache_workflow(native, inputs)
        drag['covered_by'] = 'full-size'
        with self.assertRaisesRegex(ValueError, 'not covered by a new patch'):
            check_cache_workflow(native, inputs)
        drag['covered_by'] = 'patch'
        drag['since_last_move_ms'] = 200
        moving = {'event':'drag-patch-requested-while-moving', 'action':'drag', 'elapsed_ms':56.5,
                  'action_elapsed_ms':1.5}
        rows.insert(rows.index(drag), moving)
        with self.assertRaisesRegex(ValueError, 'while the view was moving'):
            check_cache_workflow(native, inputs)
        rows.remove(moving)
        rows.insert(rows.index(drag), {'event':'drag-patch-accepted', 'action':'drag', 'elapsed_ms':57,
                                        'action_elapsed_ms':2, 'stale':True})
        with self.assertRaisesRegex(ValueError, 'region the view had left'):
            check_cache_workflow(native, inputs)

class RetentionTests(unittest.TestCase):
    def trace(self, decisions, evicted=0):
        actions = [{'action':name, 'window_native_decisions':decisions, 'window_event_counts':{'cache.processing.evicted':evicted}}
                   for name in ('zoom-6-200', 'exposure-0', 'exposure-1', 'exposure-2', 'exposure-3')]
        return {'available':True, 'actions':actions}

    def test_warm_edits_must_not_reload_or_evict_visible_photos(self):
        self.assertEqual(retention_assertion(self.trace({'hit:same_source':6}))['outcome'], 'passed')
        self.assertEqual(retention_assertion(self.trace({'loaded:not_retained':1}))['outcome'], 'failed')
        self.assertEqual(retention_assertion(self.trace({}, evicted=1))['outcome'], 'failed')
        self.assertEqual(retention_assertion({'available':False, 'reason':'no trace'})['outcome'], 'unverified')

    def test_zoom_names_stay_comparable_with_baselines(self):
        self.assertEqual([zoom_action(n) for n in (1, 2, 6)], ['zoom-a-200', 'zoom-both-200', 'zoom-6-200'])


if __name__ == '__main__':
    unittest.main()

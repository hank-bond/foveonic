"""Check strict matched Off comparisons and explicit caller regression budgets portably."""

from copy import deepcopy
import unittest

from compare import compare_reports
from interaction_latency import matched_workload, value_hash


def workload():
    manifest={'scenario':{'name':'warm-edit','photo_count':1,'quality':'thumb','repeats':3,'rapid_edits':12,'pressure_bytes':128,'disk_cache_bytes':10_000_000_000,'memory_cache_bytes':1073741824},
              'inputs':[{'sha256':'raw-sha','settings':'camera-defaults','staged_source':'/one/photo'}],
              'hardware':{'system':'test','machine':'test-cpu','logical_cpus':8},
              'measurement':{'input_kind':'baseline','scenario_poll_ms':20}}
    geometry={'window_points':[1200,800],'viewport_points':[800,600],'pixels_per_point':2.}
    result={'geometry':geometry,'memory_cache_bytes':1073741824,'disk_cache':{'enabled':True,'max_bytes':10_000_000_000},'hardware':{'adapter':'Test GPU','backend':'Test','vendor':1,'device':2},'steps':[{'geometry':geometry,'action':'initial-selection','photos':[{'source':'/one/photo','settings':{'exposure':0.},'scale_divisor':8}]}]}
    return manifest,result


def report(values, build='one'):
    manifest,native=workload();key=matched_workload(manifest,native)
    trial={'mode':'off','input_kind':'baseline','passed':True,'timing_valid':True,
           'contention':{'checked':True,'uncontended':True},'workload_key':key,
           'steps':[{'action':'warm-edit','baseline_elapsed_ms':value} for value in values],
           'artifacts':{'binary':build,'binary_sha256':build+'-sha','source_tree_sha256':build+'-source',
                        'launch':build+'-launch','result':build+'-result','result_sha256':build+'-result-sha'}}
    return {'mode':'off','passed':True,'timing_valid':True,'trials_requested':1,'workload_consistent':True,
            'workload_key':key,'trials':[trial]}


def cache_report():
    result = report([1])
    key = result['workload_key']
    key['descriptor']['scenario']['name'] = 'cache-workflow'
    key['descriptor']['cache_workflow_protocol'] = [{'action':'exposure-0', 'delta':0.125}]
    key['sha256'] = value_hash(key['descriptor'])
    actions = []
    for name in ['zoom-a-200', *[f'exposure-{index}' for index in range(4)]]:
        actions.append({'action':name,
                        'first_current_admissions':[{'photo':0, 'action_elapsed_ms':2}],
                        'first_current_observations':[{'photo':1, 'action_elapsed_ms':3}],
                        'foreground_on_admission_ms':[] if name == 'zoom-a-200' else [4],
                        'foreground_by_observation_ms':[],
                        'whole_current_admissions':[{'action_elapsed_ms':2.5}],
                        'whole_settled_observed_ms':8})
    trial = result['trials'][0]
    trial['steps'] = [{'action':'initial-selection', 'baseline_elapsed_ms':10},
                      {'action':'cache-workflow', 'baseline_elapsed_ms':None}]
    trial['cache_workflow'] = {'actions':actions}
    return result


class CompareTests(unittest.TestCase):
    def test_cache_workflow_compares_milestones_not_a_scenario_total(self):
        before, after = cache_report(), cache_report()
        comparison = compare_reports(before, after)
        actions = {row['action']:row for row in comparison['actions']}
        self.assertEqual(len(actions), 21)
        self.assertEqual(actions['initial-selection']['median']['before_ms'], 10)
        self.assertEqual(actions['zoom-a-200/foreground']['median']['before_ms'], 3)
        self.assertEqual(actions['exposure-0/foreground']['median']['before_ms'], 4)
        self.assertEqual(actions['exposure-0/whole-observed']['median']['before_ms'], 8)
        for mutation in (lambda trial:trial['cache_workflow']['actions'].pop(),
                         lambda trial:trial['cache_workflow']['actions'][1].update(whole_settled_observed_ms=float('nan')),
                         lambda trial:trial['cache_workflow']['actions'][1].update(foreground_on_admission_ms=[]),
                         lambda trial:trial['cache_workflow']['actions'][1].update(foreground_on_admission_ms=[1]),
                         lambda trial:trial['cache_workflow']['actions'][1].update(whole_settled_observed_ms=-1),
                         lambda trial:trial['cache_workflow']['actions'][0].update(first_current_observations=[]),
                         lambda trial:trial.update(steps=trial['steps'][:1]),
                         lambda trial:trial.update(mode='wall')):
            broken = cache_report()
            mutation(broken['trials'][0])
            with self.assertRaises(ValueError):
                compare_reports(before, broken)

    def test_cache_protocol_is_matched_without_runtime_paths_or_revisions(self):
        manifest, native = workload()
        manifest['scenario']['name'] = 'cache-workflow'
        native['cache_workflow'] = {'rows':[{'event':'action', 'action':'fit-smart', 'elapsed_ms':3},
                                          {'event':'action', 'action':'exposure-0', 'delta':0.125, 'minimum_revision':7}]}
        key = matched_workload(manifest, native)
        changed = deepcopy(native)
        changed['cache_workflow']['rows'][1]['minimum_revision'] = 99
        self.assertEqual(key, matched_workload(manifest, changed))
        changed['cache_workflow']['rows'][1]['delta'] = -0.125
        self.assertNotEqual(key, matched_workload(manifest, changed))

    def test_effective_disk_policy_must_match_the_declared_workload(self):
        manifest,native=workload()
        self.assertTrue(matched_workload(manifest,native)['descriptor']['cache_policy_matches'])
        manifest['scenario']['disk_cache_bytes']=0
        mismatch=matched_workload(manifest,native)['descriptor']
        self.assertFalse(mismatch['cache_policy_matches'])
        self.assertFalse(mismatch['first_current_complete'])
        native['disk_cache']={'enabled':False,'max_bytes':0}
        self.assertTrue(matched_workload(manifest,native)['descriptor']['cache_policy_matches'])

    def test_key_tracks_inputs_resolved_settings_hardware_and_config_not_staged_paths(self):
        manifest,native=workload();key=matched_workload(manifest,native)
        changed=deepcopy(manifest);actual=deepcopy(native)
        changed['inputs'][0]['staged_source']='/another/photo';actual['steps'][0]['photos'][0]['source']='/another/photo'
        self.assertEqual(key,matched_workload(changed,actual))
        for edit in (lambda m,n:m['inputs'][0].update(sha256='different'),
                     lambda m,n:m['inputs'][0].update(settings={'exposure':1.}),
                     lambda m,n:n['steps'][0]['photos'][0].update(settings={'exposure':1.}),
                     lambda m,n:m['hardware'].update(logical_cpus=16),
                     lambda m,n:n['hardware'].update(adapter='Different GPU'),
                     lambda m,n:n['geometry'].update(pixels_per_point=1.),
                     lambda m,n:m['scenario'].update(repeats=4)):
            changed=deepcopy(manifest);actual=deepcopy(native);edit(changed,actual)
            self.assertNotEqual(key,matched_workload(changed,actual))

    def test_missing_geometry_cannot_be_used_for_latency_comparison(self):
        candidate=report([10])
        candidate['workload_key']['descriptor']['geometry']=None
        candidate['workload_key']['sha256']=value_hash(candidate['workload_key']['descriptor'])
        with self.assertRaisesRegex(ValueError,'complete, consistent'):
            compare_reports(report([10]),candidate)

    def test_mismatched_input_keys_are_rejected(self):
        before=report([10]);after=report([10])
        after['workload_key']['descriptor']['inputs'][0]['sha256']='different'
        after['workload_key']['sha256']=value_hash(after['workload_key']['descriptor'])
        with self.assertRaisesRegex(ValueError,'different workload'):
            compare_reports(before,after)

    def test_modes_contention_failed_or_incomplete_trials_are_rejected(self):
        edits=(lambda r:r.update(mode='wall'),lambda r:r['trials'][0].update(mode='gpu'),
               lambda r:r['trials'][0].update(timing_valid=False),
               lambda r:r['trials'][0]['contention'].update(uncontended=False),
               lambda r:r.update(passed=False),lambda r:r.update(trials_requested=2))
        for edit in edits:
            with self.subTest(edit=edit):
                candidate=report([10]);edit(candidate)
                with self.assertRaises(ValueError):compare_reports(report([10]),candidate)

    def test_mixed_trial_keys_are_rejected(self):
        candidate=report([10]);candidate['trials'][0]['workload_key']=deepcopy(candidate['workload_key'])
        candidate['trials'][0]['workload_key']['sha256']='different'
        with self.assertRaisesRegex(ValueError,'mixed workload'):
            compare_reports(report([10]),candidate)

    def test_regression_budget_and_action_override_preserve_build_identities(self):
        before=report([10,20,30]);after=report([12,24,36],'two')
        result=compare_reports(before,after,10.)
        self.assertFalse(result['passed'])
        self.assertAlmostEqual(result['actions'][0]['median']['delta_percent'],20.)
        self.assertFalse(result['actions'][0]['p95']['within_budget'])
        self.assertEqual(result['artifacts']['after'][0]['binary_sha256'],'two-sha')
        self.assertTrue(compare_reports(before,after,10.,{'warm-edit':25.})['passed'])

    def test_improvement_and_unbudgeted_deltas_have_no_invented_threshold(self):
        before=report([10,20,30]);after=report([8,16,24])
        self.assertTrue(compare_reports(before,after,0.)['passed'])
        result=compare_reports(before,after)
        self.assertIsNone(result['passed'])
        self.assertIsNone(result['actions'][0]['median']['within_budget'])
        self.assertAlmostEqual(result['actions'][0]['p95']['delta_percent'],-20.)
        with self.assertRaises(ValueError):compare_reports(before,after,action_budgets={'missing':5.})


if __name__=='__main__':unittest.main()

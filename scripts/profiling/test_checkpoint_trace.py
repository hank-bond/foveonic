"""Check task explanations distinguish partial cache reuse from unknown history."""

import copy
import json
import os
from pathlib import Path
import unittest

from checkpoint_trace import explain


def event(event_name, ts, **fields):
    return {'name':event_name, 'ts':ts, 'dur':0, 'args':fields}


def fixture(cache=1, graph=4):
    common = {'graph_id':graph, 'cache_id':cache, 'task':7, 'revision':3, 'photo':5, 'purpose':'current'}
    entry = {'allocation_id':90, 'generation':1, 'byte_len':235,
             'version':10, 'names':['noise.tiles'], 'role':'noise.tiles'}
    header = event('graph.execute', 10, **common, outputs=2, nodes=1, requested_roots='[1]')
    header['dur'] = 100_000_000
    events = [header,
              event('cache.checkpoint.snapshot', 11, **common, cache_frame=8,
                    entries=json.dumps([entry, {**entry, 'version':12, 'names':['alias']}]))]
    for output, name in enumerate(['noise.tiles', 'noise.seed']):
        cached = output == 0
        events.append(event('graph.output', 30, **common, output_id=output, producer=0,
                            name=name, dependencies='[]', cache_version=10+output,
                            seeded=cached, present=True, required=True,
                            requested_for_execution=not cached, published=not cached,
                            allocation_id=90+output, allocation_generation=1,
                            byte_len=235 if cached else 78, working_scale=1))
    events.append(event('graph.complete', 31, graph_id=graph))
    work = {'nodes':[{'slice':22, 'graph_id':graph, 'node':0, 'output_count':2,
                      'executed':True, 'operation':'NoiseBase', 'inclusive_host_ms':100.,
                      'gpu_pass_ms':None, 'dispatches':8}]}
    return events, work


def report(events, work, valid=True):
    return explain(events, work, {'pairs':[], 'unpaired':[]}, valid)


class CheckpointTraceTests(unittest.TestCase):
    def test_warm_capture_explains_missing_sibling_and_counts_shared_storage_once(self):
        events, work = fixture()
        result = report(events, work)
        graph = result['graphs'][0]
        self.assertTrue(graph['local_evidence_complete'])
        self.assertTrue(result['recorded_history_valid'])
        self.assertEqual(result['history_before_capture'], 'unknown')
        self.assertEqual(graph['requested_roots'], [1])
        self.assertEqual(graph['entry_checkpoints']['unique_bytes'], 235)
        self.assertEqual(graph['producers_with_cached_siblings'], [0])
        producer = graph['executed_nodes'][0]
        self.assertEqual(producer['missing_required_outputs'][0]['name'], 'noise.seed')
        self.assertIsNone(producer['missing_required_outputs'][0]['prior_eviction'])
        self.assertEqual(producer['available_sibling_outputs'][0]['allocation_id'], 90)
        self.assertIsNone(producer['gpu_pass_ms'])
        self.assertEqual(result['tasks'][0]['graphs'], [4])

    def test_evictions_match_cache_and_version_and_admission_supersedes_history(self):
        events, work = fixture()
        unrelated = event('cache.checkpoint.evicted', 3, cache_id=2, generation=11, reason='other-cache')
        events.insert(0, unrelated)
        self.assertIsNone(report(events, work)['graphs'][0]['executed_nodes'][0]
                          ['missing_required_outputs'][0]['prior_eviction'])
        eviction = event('cache.checkpoint.evicted', 4, cache_id=1, generation=11,
                         reason='local-pixel-capacity', buffer_reference_bytes=78)
        events.insert(0, eviction)
        prior = report(events, work)['graphs'][0]['executed_nodes'][0]['missing_required_outputs'][0]['prior_eviction']
        self.assertEqual(prior['reason'], 'local-pixel-capacity')
        events.append(event('cache.checkpoint.admitted', 5, cache_id=1, generation=11))
        self.assertIsNone(report(events, work)['graphs'][0]['executed_nodes'][0]
                          ['missing_required_outputs'][0]['prior_eviction'])

    def test_native_reduced_and_replacement_caches_do_not_merge_for_one_photo(self):
        events, work = fixture()
        for cache, graph in [(2,5), (3,6)]:
            other, nodes = fixture(cache, graph)
            events += other
            work['nodes'] += nodes['nodes']
        result = report(events, work)
        self.assertEqual([row['cache_id'] for row in result['graphs']], [1,2,3])
        self.assertEqual([row['entry_checkpoints']['unique_bytes'] for row in result['graphs']], [235]*3)
        self.assertEqual(len(result['tasks']), 1)

    def test_cancellation_and_truncation_are_incomplete_without_inventing_history(self):
        for removed in ['graph.complete', 'cache.checkpoint.snapshot', 'graph.output']:
            with self.subTest(removed=removed):
                events, work = fixture()
                index = next(i for i,event in enumerate(events) if event['name']==removed)
                del events[index]
                result = report(events, work, False)
                self.assertFalse(result['recorded_history_valid'])
                self.assertFalse(result['graphs'][0]['local_evidence_complete'])
        events, work = fixture()
        events = [event for event in events if event['name']!='graph.execute']
        self.assertEqual(report(events, work, False)['orphan_graph_ids'], [4])

    def test_wrong_cache_snapshot_or_missing_node_is_not_complete(self):
        events, work = fixture()
        events[1]['args']['cache_id'] = 99
        self.assertFalse(report(events, work)['graphs'][0]['local_evidence_complete'])

        events, work = fixture()
        work['nodes'] = []
        self.assertFalse(report(events, work)['graphs'][0]['local_evidence_complete'])

    def test_capture_starting_mid_graph_does_not_silently_drop_unidentified_records(self):
        events, work = fixture()
        for event in events:
            event['args'].pop('graph_id', None)
        work['nodes'][0]['graph_id'] = None
        result = report(events, work)
        self.assertFalse(result['graphs'])
        self.assertEqual(result['unidentified_graph_records'], 6)

    def test_reader_does_not_mutate_events_or_existing_work_census(self):
        events, work = fixture()
        original = copy.deepcopy((events, work))
        report(events, work)
        self.assertEqual((events, work), original)

    def test_dependency_table_keeps_skipped_producer_outputs(self):
        events, work = fixture()
        events[0]['args']['nodes'] = 2
        events[2]['args']['producer'] = 0
        events[3]['args']['producer'] = 1
        events[3]['args']['dependencies'] = '[0]'
        work['nodes'][0].update(node=1, output_count=1)
        work['nodes'].append({'graph_id':4, 'node':0, 'output_count':1, 'executed':False})
        graph = report(events, work)['graphs'][0]
        self.assertTrue(graph['local_evidence_complete'])
        self.assertEqual(graph['outputs'][1]['dependencies'], [0])
        self.assertEqual(graph['outputs'][0]['name'], 'noise.tiles')


@unittest.skipUnless(os.environ.get('FOVEONIC_CHECKPOINT_TRACE_DIR'),
                     'requires the maintained warm-core capture and trace analysis')
class NativeCheckpointCaptureTests(unittest.TestCase):
    def test_recorded_warm_capture_roundtrips_actual_metadata(self):
        directory = Path(os.environ['FOVEONIC_CHECKPOINT_TRACE_DIR'])
        result = json.loads((directory / 'analysis/checkpoint-explanations.json').read_text())
        self.assertTrue(result['recorded_history_valid'])
        self.assertEqual(result['history_before_capture'], 'unknown')
        self.assertFalse(result['orphan_graph_ids'])
        self.assertEqual(result['unidentified_graph_records'], 0)
        self.assertTrue(result['graphs'])
        self.assertTrue(any(graph['entry_checkpoints'] and graph['entry_checkpoints']['unique_bytes']
                            for graph in result['graphs']))
        for graph in result['graphs']:
            self.assertTrue(graph['local_evidence_complete'], graph['issues'])
            ids = {output['output_id'] for output in graph['outputs']}
            self.assertTrue(all(root in ids for root in graph['requested_roots']))
            for output in graph['outputs']:
                self.assertTrue(all(dependency in ids for dependency in output['dependencies']))
                if graph['cache_id'] is not None:
                    self.assertIsInstance(output['cache_version'], int)


if __name__ == '__main__':
    unittest.main()

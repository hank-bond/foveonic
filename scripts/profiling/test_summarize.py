"""Check current JSON decoding, native windows, per-profile pass totals and diagnostic separation without a GPU."""

import json
import unittest

from summarize import parse_record, stats, summarize


def event(kind, data, mode='wall', external_ns=1_000_000_000):
    return {'monotonic_ns': external_ns, 'line': json.dumps({
        'foveonic_profile': 1, 'mode': mode, 'event': kind,
        'synchronized': mode == 'gpu', 'data': data})}


class SummarizeTests(unittest.TestCase):
    def test_json_escaping_and_non_profile_stderr(self):
        row = parse_record(event('processing', {'source': 'a "quoted" photo.x3f'}))
        self.assertEqual(row['source'], 'a "quoted" photo.x3f')
        self.assertIsNone(parse_record({'line': 'ordinary stderr'}))
        self.assertIsNone(parse_record({'line': '42'}))
        self.assertIsNone(parse_record({'event': 'other'}))

    def test_envelope_fields_cannot_be_overridden_by_context(self):
        row = parse_record(event('processing', {'mode': 'gpu', 'event': 'other'}))
        self.assertEqual(row['mode'], 'wall')
        self.assertEqual(row['event'], 'processing')

    def test_wall_and_gpu_runs_are_never_pooled(self):
        records = [event('processing', {'execution': 'fresh', 'completed_ms': 10., 'cold': False}),
                   event('processing', {'execution': 'fresh', 'completed_ms': 100., 'cold': False}, 'gpu')]
        result = summarize(records)
        self.assertFalse(result['modes']['wall']['synchronized'])
        self.assertTrue(result['modes']['gpu']['synchronized'])
        self.assertEqual(result['modes']['wall']['completed_processing'][0]['milliseconds']['median'], 10.)

    def test_repeated_passes_sum_within_a_profile_not_by_list_position(self):
        profile = {'host': [], 'gpu': [{'stage': 'denoiser iteration', 'milliseconds': 3.},
                                     {'stage': 'denoiser iteration', 'milliseconds': 4.}]}
        rows = [event('processing', {'profile': profile, 'activity': {'dispatches': 2}}, 'gpu')]
        result = summarize(rows)['modes']['gpu']
        stage = result['stages'][0]
        self.assertEqual(stage['per_profile_total_ms']['median'], 7.)
        self.assertEqual(stage['samples_per_profile']['median'], 2.)
        self.assertEqual(result['activity_per_profile'][0]['counts']['dispatches']['median'], 2.)

    def test_native_windows_join_upstream_records_and_exclude_warmup_and_cancellation(self):
        profile = lambda milliseconds: {'host': [{'stage': 'prepare', 'milliseconds': milliseconds}], 'gpu': []}
        rows = [event('processing', {'revision': 4, 'profile': profile(500.)}, external_ns=500_000_000),
                event('processing', {'revision': 5, 'profile': profile(3.)}, external_ns=500_000_000),
                event('processing', {'revision': 6, 'profile': profile(999.), 'status': 'superseded'}),
                event('image', {'revision': 5, 'worker_ms': 10., 'queue_to_ui_ms': 12.})]
        pointers = [{'kind': 'drag_begin', 'monotonic_ns': 1_000_000_000},
                    {'kind': 'drag_end', 'monotonic_ns': 2_000_000_000}]
        result = summarize(rows, pointers)['modes']['wall']
        self.assertEqual(result['delivered_revision_count'], 1)
        self.assertEqual(result['stages'][0]['per_profile_total_ms']['median'], 3.)
        self.assertEqual(result['processing_status']['superseded'], 1)

    def test_cold_and_warm_completed_processing_stay_separate(self):
        rows = [event('processing', {'execution': 'fresh', 'cold': True, 'completed_ms': 100.}),
                event('processing', {'execution': 'fresh', 'cold': False, 'completed_ms': 10.})]
        results = summarize(rows)['modes']['wall']['completed_processing']
        self.assertEqual([r['milliseconds']['median'] for r in results], [100., 10.])

    def test_callback_metrics_exclude_idle_callbacks_for_delivered_revision(self):
        rows = [event('image', {'revision': 5, 'worker_ms': 10., 'queue_to_ui_ms': 12.}),
                event('lifecycle', {'revision': 5, 'phase': 'image_received', 'at_ms': 20.}),
                event('span', {'revision': 5, 'phase': 'draw_encode', 'end_ms': 21., 'elapsed_ms': .01}),
                event('draw', {'revision': 5, 'prepared_to_encode_ms': 2.}),
                event('span', {'revision': 5, 'phase': 'draw_encode', 'end_ms': 1000., 'elapsed_ms': 500.}, external_ns=10_000_000_000)]
        pointers = [{'kind': 'drag_begin', 'monotonic_ns': 1_000_000_000},
                    {'kind': 'drag_end', 'monotonic_ns': 2_000_000_000}]
        result = summarize(rows, pointers)['modes']['wall']
        self.assertEqual(result['spans_ms']['draw_encode']['median'], .01)
        self.assertEqual(result['prepared_to_encode_ms']['median'], 2.)
        self.assertEqual(result['receipt_to_first_encoding_end_ms']['median'], 1.)

    def test_empty_stats_and_unfinished_windows(self):
        self.assertEqual(stats([])['count'], 0)
        self.assertIsNone(stats([])['median'])
        with self.assertRaises(ValueError):
            summarize([], [{'kind': 'drag_begin', 'monotonic_ns': 1}])


if __name__ == '__main__':
    unittest.main()

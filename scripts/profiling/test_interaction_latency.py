"""Portable checks for scenario UI tick and geometry evidence."""

import unittest

from interaction_latency import geometry_identity, summarize_ui_ticks


def tick(elapsed, action='drag'):
    return {'elapsed_ms': elapsed, 'action': action, 'worker_busy': True}


class InteractionLatencyTest(unittest.TestCase):
    def test_distributions_include_action_boundaries_and_use_nearest_rank(self):
        samples = [tick(100), tick(120), tick(145), tick(1000, 'settle'), tick(1030, 'settle')]
        result = summarize_ui_ticks(samples)
        self.assertEqual(result['overall'], {'sample_count': 5, 'gap_count': 4, 'max_ms': 855, 'p95_ms': 855})
        self.assertEqual(result['actions']['drag']['max_ms'], 855)
        self.assertEqual(result['outcome'], 'complete')
        self.assertIsNone(result['assertion'])

    def test_explicit_budget_passes_or_fails(self):
        samples = [tick(0), tick(20), tick(50)]
        self.assertEqual(summarize_ui_ticks(samples, 30)['assertion']['outcome'], 'passed')
        self.assertEqual(summarize_ui_ticks(samples, 29)['assertion'],
                         {'name': 'max_ui_gap_ms', 'expected': 29, 'actual': 30, 'outcome': 'failed'})
        for budget in (0, -1, float('nan'), float('inf')):
            with self.assertRaises(ValueError):
                summarize_ui_ticks(samples, budget)

    def test_missing_insufficient_or_invalid_evidence_cannot_pass(self):
        for samples in ([], [tick(0)], [tick(0), tick(5, 'other')],
                        [tick(10), tick(5)], [tick(0), tick(float('nan'))]):
            result = summarize_ui_ticks(samples, 100)
            self.assertEqual(result['outcome'], 'incomplete')
            self.assertEqual(result['assertion']['outcome'], 'unverified')
        self.assertIsNone(summarize_ui_ticks([])['overall']['p95_ms'])

    def test_complete_positive_geometry_required(self):
        geometry = {'window_points': [1200, 800], 'viewport_points': [800, 600], 'pixels_per_point': 2.0}
        self.assertEqual(geometry_identity(geometry), geometry)
        for override in ({'window_points': None}, {'viewport_points': [1, 0]},
                         {'pixels_per_point': float('nan')}, {'pixels_per_point': True},
                         {'window_points': [1, float('inf')]}):
            self.assertIsNone(geometry_identity({**geometry, **override}))
        self.assertIsNone(geometry_identity({}))


if __name__ == '__main__':
    unittest.main()

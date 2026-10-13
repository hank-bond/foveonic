"""Check deterministic pointer schedules and explicit capture environments without native input or app startup."""

import math
import os
from pathlib import Path
import unittest
from unittest.mock import patch

from native import drag_points, recording_environment


class NativeTests(unittest.TestCase):
    def test_drag_endpoints_and_monotonic_schedule(self):
        points = drag_points(10, 20, 30, 61, 2.)
        self.assertEqual(points[0], (0., 10., 30))
        self.assertEqual(points[-1], (2., 20., 30))
        self.assertTrue(all(a[0] < b[0] for a, b in zip(points, points[1:])))

    def test_reverse_drag_and_invalid_inputs(self):
        self.assertEqual(drag_points(20, 10, 30, 2, 1.)[-1], (1., 10., 30))
        for args in [(0, 1, 1, 1, 1), (0, 1, 1, 2, 0), (math.nan, 1, 1, 2, 1)]:
            with self.assertRaises(ValueError):
                drag_points(*args)

    def test_capture_mode_is_explicit_and_cannot_inherit_diagnostics(self):
        with patch.dict(os.environ, {'FOVEONIC_PROFILE': 'gpu', 'FOVEONIC_UI_GPU_PROFILE': '1',
                                     'FOVEONIC_UI_TIMING': '1', 'FOVEONIC_GUI_SMOKE_DETAIL': '1'}):
            env = recording_environment(Path('/tmp/test-home'), 'wall')
            self.assertEqual(env['FOVEONIC_PROFILE'], 'wall')
            self.assertEqual(env['HOME'], '/tmp/test-home')
            self.assertNotIn('FOVEONIC_UI_GPU_PROFILE', env)
            self.assertNotIn('FOVEONIC_UI_TIMING', env)
            self.assertNotIn('FOVEONIC_GUI_SMOKE_DETAIL', env)


if __name__ == '__main__':
    unittest.main()

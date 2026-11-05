"""Check deterministic pointer schedules and explicit capture environments without native input or app startup."""

import math
import os
import tempfile
from types import SimpleNamespace
from pathlib import Path
import unittest
from unittest.mock import MagicMock, patch

from native import drag_points, record, recording_environment, source_tree_hash, workload_monitor


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

    def test_source_hash_tracks_inputs_but_excludes_unrelated_documents(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'src').mkdir()
            (root / 'src' / 'main.rs').write_text('one')
            (root / 'notes.md').write_text('before')
            with patch('native.subprocess.check_output', return_value=b'notes.md\0src/main.rs\0'):
                first = source_tree_hash(root)
                (root / 'notes.md').write_text('after')
                self.assertEqual(first, source_tree_hash(root))
                (root / 'src' / 'main.rs').write_text('two')
                self.assertNotEqual(first, source_tree_hash(root))

    def test_monitor_stops_after_success_and_exception(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / 'workloads.jsonl'
            for fail in (False, True):
                output.unlink(missing_ok=True)
                process = MagicMock()
                process.poll.return_value = None
                def start(*_args, **_kwargs):
                    output.write_text('{}\n')
                    return process
                with patch('native.subprocess.Popen', side_effect=start):
                    if fail:
                        with self.assertRaisesRegex(RuntimeError, 'capture failed'):
                            with workload_monitor(output):
                                raise RuntimeError('capture failed')
                    else:
                        with workload_monitor(output) as tool:
                            self.assertEqual(tool.name, 'watch_workloads.py')
                process.terminate.assert_called_once()
                process.wait.assert_called_once()

    def test_monitor_early_exit_is_an_error_and_is_reaped(self):
        with tempfile.TemporaryDirectory() as temporary:
            process = MagicMock()
            process.poll.return_value = 1
            with patch('native.subprocess.Popen', return_value=process):
                with self.assertRaisesRegex(RuntimeError, 'before its first sample'):
                    with workload_monitor(Path(temporary) / 'missing'):
                        self.fail('monitor did not start')
            process.terminate.assert_not_called()
            process.wait.assert_called_once()

    def test_sigterm_during_monitor_start_reaps_monitor_without_launching_app(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary, fixture = root / 'app', root / 'photo.x3f'
            binary.write_bytes(b'app')
            fixture.write_bytes(b'fixture')
            args = SimpleNamespace(directory=root / 'record', binary=binary, fixture=fixture, mode='wall')
            process = MagicMock()
            process.poll.return_value = None
            handlers = []
            def register(_signal, handler):
                handlers.append(handler)
                return 'previous'
            def start(*_args, **_kwargs):
                handlers[0](None, None)
                return process
            with patch('native.source_tree_hash', return_value='tree'), \
                 patch('native.signal.signal', side_effect=register), \
                 patch('native.subprocess.Popen', side_effect=start) as spawn:
                record(args)
            self.assertEqual(spawn.call_count, 1)
            process.terminate.assert_called_once()
            process.wait.assert_called_once()
            self.assertEqual(handlers[-1], 'previous')

    def test_monitor_exit_after_first_sample_fails_capture(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / 'workloads.jsonl'
            output.write_text('{}\n')
            process = MagicMock()
            process.poll.return_value = None
            with patch('native.subprocess.Popen', return_value=process):
                with self.assertRaisesRegex(RuntimeError, 'during recording'):
                    with workload_monitor(output):
                        process.poll.return_value = 1
            process.wait.assert_called_once()


if __name__ == '__main__':
    unittest.main()

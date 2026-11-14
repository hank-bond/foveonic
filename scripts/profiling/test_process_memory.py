"""Check that memory guards stop only their owned child and preserve measured evidence."""

import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

from process_memory import footprint_reader, run_captured, wait_guarded


class Child:
    pid = 123
    args = ['owned-test']
    returncode = None
    killed = False

    def poll(self):
        return self.returncode

    def kill(self):
        self.killed = True
        self.returncode = -9

    def wait(self):
        return self.returncode


class ProcessMemoryTests(unittest.TestCase):
    def test_exceeded_limit_kills_owned_child_and_records_evidence(self):
        child = Child()
        with tempfile.TemporaryDirectory() as directory:
            samples = Path(directory)/'memory.jsonl'
            with self.assertRaisesRegex(RuntimeError, 'exceeded physical-footprint limit'):
                wait_guarded(child, 10, 1024, samples, reader=lambda _: 2048)
            self.assertTrue(child.killed)
            self.assertEqual(json.loads(samples.read_text())['physical_footprint_bytes'], 2048)
            self.assertTrue(json.loads(samples.read_text())['limit_exceeded'])

    def test_success_keeps_process_and_records_current_footprint(self):
        child = Child()

        def completed(pid):
            self.assertEqual(pid, child.pid)
            child.returncode = 0
            return 512

        with tempfile.TemporaryDirectory() as directory, patch('process_memory.time.sleep'):
            samples = Path(directory)/'memory.jsonl'
            self.assertEqual(wait_guarded(child, 10, 1024, samples, reader=completed), 0)
            self.assertFalse(child.killed)
            self.assertFalse(json.loads(samples.read_text())['limit_exceeded'])

    def test_captured_renderer_drains_output_without_pipe_backpressure(self):
        with tempfile.TemporaryDirectory() as directory, \
                patch('process_memory.footprint_reader', return_value=lambda _: 512):
            result = run_captured([sys.executable, '-c',
                                   "import sys; print('x'*100000); print('error',file=sys.stderr)"],
                                  limit_bytes=1024, samples=Path(directory)/'memory.jsonl')
            self.assertEqual(result.returncode, 0)
            self.assertEqual(len(result.stdout), 100001)
            self.assertEqual(result.stderr, 'error\n')

    @unittest.skipUnless(sys.platform == 'darwin', 'requires macOS libproc')
    def test_native_reader_observes_own_process(self):
        self.assertGreater(footprint_reader()(os.getpid()), 0)

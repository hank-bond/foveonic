"""Check owned process identities and memory-guard cleanup without photographic workloads."""

import ctypes
import errno
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import time
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from process_memory import (ProcessIdentity, ProcessStartTime, birth_reader,
                            footprint_reader, run_captured, wait_guarded)


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
    def identity_reader(self, *, query_size=4096, returned_size=648, read_error=0,
                        query_error=0, pid=123, status=2, seconds=100, microseconds=123456):
        calls = []

        def sysctl(name, count, buffer, size_pointer, new, new_size):
            self.assertEqual(list(name), [1, 14, 1, 123])
            self.assertEqual((count, new, new_size), (4, None, 0))
            size = ctypes.cast(size_pointer, ctypes.POINTER(ctypes.c_size_t)).contents
            calls.append(buffer is not None)
            error = read_error if buffer is not None else query_error
            if error:
                ctypes.set_errno(error)
                return -1
            if buffer is None:
                size.value = query_size
            else:
                self.assertEqual(size.value, query_size)
                info = ProcessIdentity(start=ProcessStartTime(seconds, microseconds),
                                       pid=pid, status=status)
                ctypes.memmove(buffer, ctypes.byref(info), ctypes.sizeof(info))
                size.value = returned_size
            return 0

        with patch('process_memory.sys.platform', 'darwin'), \
                patch('process_memory.ctypes.CDLL', return_value=SimpleNamespace(sysctl=sysctl)) as load:
            read = birth_reader()
        load.assert_called_once_with('/usr/lib/libSystem.B.dylib', use_errno=True)
        self.assertEqual(sysctl.argtypes, [ctypes.POINTER(ctypes.c_int), ctypes.c_uint,
                                          ctypes.c_void_p, ctypes.POINTER(ctypes.c_size_t),
                                          ctypes.c_void_p, ctypes.c_size_t])
        self.assertIs(sysctl.restype, ctypes.c_int)
        return read, calls

    def test_public_darwin_64_bit_prefix_offsets(self):
        self.assertEqual(ctypes.sizeof(ProcessStartTime), 16)
        self.assertEqual(ProcessStartTime.microseconds.offset, 8)
        self.assertEqual(ctypes.sizeof(ProcessIdentity), 48)
        self.assertEqual({name: getattr(ProcessIdentity, name).offset for name in
                          ('start', 'vmspace', 'sigacts', 'flags', 'status', 'pid')},
                         {'start': 0, 'vmspace': 16, 'sigacts': 24, 'flags': 32,
                          'status': 36, 'pid': 40})

    def test_identity_uses_start_microseconds_and_queried_full_record_size(self):
        for query_size, returned_size in ((4096, 648), (1024, 200), (48, 48)):
            with self.subTest(query_size=query_size, returned_size=returned_size):
                read, calls = self.identity_reader(query_size=query_size, returned_size=returned_size)
                self.assertEqual(read(123), 100123456)
                self.assertEqual(read(123), 100123456)
                self.assertEqual(calls, [False, True, False, True])
        read, _ = self.identity_reader(seconds=101)
        self.assertNotEqual(read(123), 100123456)

    def test_missing_zombie_and_errno_remain_explicit(self):
        for options in ({'query_size': 0}, {'returned_size': 0}, {'status': 5},
                        {'query_error': errno.ESRCH}, {'read_error': errno.ESRCH}):
            with self.subTest(options=options), self.assertRaises(ProcessLookupError):
                self.identity_reader(**options)[0](123)
        for phase in ('query_error', 'read_error'):
            with self.subTest(phase=phase), self.assertRaises(PermissionError):
                self.identity_reader(**{phase: errno.EPERM})[0](123)

    def test_malformed_identity_records_fail_closed(self):
        for options in ({'query_size': 47}, {'query_size': 65537},
                        {'returned_size': 47}, {'returned_size': 4097}, {'pid': 124},
                        {'seconds': 0}, {'seconds': -1},
                        {'microseconds': -1}, {'microseconds': 1000000}):
            with self.subTest(options=options), self.assertRaises(RuntimeError):
                self.identity_reader(**options)[0](123)

    def test_invalid_pid_and_unsupported_platform_do_not_query(self):
        read, calls = self.identity_reader()
        for pid in (0, -1, 0x80000000, True, '123'):
            with self.subTest(pid=pid), self.assertRaises(ValueError):
                read(pid)
        self.assertEqual(calls, [])
        with patch('process_memory.sys.platform', 'linux'), patch('process_memory.ctypes.CDLL') as load:
            with self.assertRaisesRegex(RuntimeError, '64-bit macOS'):
                birth_reader()
            load.assert_not_called()
        with patch('process_memory.sys.platform', 'darwin'), \
                patch('process_memory.ctypes.sizeof', return_value=4), \
                patch('process_memory.ctypes.CDLL') as load:
            with self.assertRaisesRegex(RuntimeError, '64-bit macOS'):
                birth_reader()
            load.assert_not_called()

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

    def test_environment_and_owned_pid_callback_without_guard(self):
        started = []
        result = run_captured([sys.executable, '-c', "import os; print(os.environ['BENCHMARK_TEST'])"],
                              env={**os.environ, 'BENCHMARK_TEST': 'explicit'}, on_started=started.append)
        self.assertEqual(result.stdout, 'explicit\n')
        self.assertEqual(len(started), 1)
        self.assertGreater(started[0], 0)

    def test_callback_failure_kills_and_reaps_owned_child_on_both_paths(self):
        for guarded in (False, True):
            child = Child()
            with tempfile.TemporaryDirectory() as directory, \
                    patch('process_memory.subprocess.Popen', return_value=child):
                with self.subTest(guarded=guarded), self.assertRaisesRegex(ValueError, 'callback failed'):
                    run_captured(['owned-test'], limit_bytes=1024 if guarded else None,
                                 samples=Path(directory) / 'memory.jsonl',
                                 on_started=lambda _: (_ for _ in ()).throw(ValueError('callback failed')))
            self.assertTrue(child.killed)
            self.assertEqual(child.returncode, -9)

    def test_parent_loss_before_launch_starts_no_child(self):
        with tempfile.TemporaryDirectory() as directory, patch('process_memory.subprocess.Popen') as launch:
            with self.assertRaisesRegex(RuntimeError, 'controller gone'):
                run_captured(['owned-test'], limit_bytes=1024, samples=Path(directory)/'memory.jsonl',
                             before_start=lambda: (_ for _ in ()).throw(RuntimeError('controller gone')))
            launch.assert_not_called()

    def test_parent_loss_while_waiting_kills_and_reaps_child(self):
        child = Child()
        with tempfile.TemporaryDirectory() as directory, \
                patch('process_memory.subprocess.Popen', return_value=child), \
                patch('process_memory.footprint_reader', return_value=lambda _:512):
            with self.assertRaisesRegex(RuntimeError, 'controller gone'):
                run_captured(['owned-test'], limit_bytes=1024, samples=Path(directory)/'memory.jsonl',
                             on_poll=lambda: (_ for _ in ()).throw(RuntimeError('controller gone')))
            self.assertTrue(child.killed)
            self.assertIsNotNone(child.returncode)

    @unittest.skipUnless(sys.platform == 'darwin', 'requires macOS libproc')
    def test_native_reader_observes_own_process(self):
        self.assertGreater(footprint_reader()(os.getpid()), 0)
        read_birth = birth_reader()
        self.assertGreater(read_birth(os.getpid()), 0)
        self.assertEqual(read_birth(os.getpid()), read_birth(os.getpid()))

    @unittest.skipUnless(sys.platform == 'darwin', 'requires macOS public sysctl')
    def test_native_identity_observes_owned_live_setuid_ps_then_reaped_exit(self):
        self.assertTrue(os.stat('/bin/ps').st_mode & stat.S_ISUID)
        # An oversized header fills our pipe, keeping only our ps child alive for both reads.
        child = subprocess.Popen(['/bin/ps', '-p', str(os.getpid()), '-o', 'pid=' + 'P' * 70000],
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            time.sleep(.03)
            self.assertIsNone(child.poll())
            read = birth_reader()
            identity = read(child.pid)
            self.assertGreater(identity, 0)
            self.assertEqual(read(child.pid), identity)
            output, errors = child.communicate(timeout=3)
            self.assertEqual(child.returncode, 0, errors.decode())
            self.assertGreater(len(output), 70000)
            with self.assertRaises(ProcessLookupError):
                read(child.pid)
        finally:
            if child.poll() is None:
                child.kill()
            child.communicate()

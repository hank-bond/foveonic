"""Identify owned macOS processes and guard their OS-accounted physical footprint."""

import ctypes
import json
import os
from pathlib import Path
import subprocess
import sys
import time


class RusageInfo(ctypes.Structure):
    """The stable RUSAGE_INFO_V0 layout from the macOS sys/resource.h SDK."""

    _fields_ = [('uuid', ctypes.c_uint8 * 16)] + [
        (name, ctypes.c_uint64) for name in (
            'user_time', 'system_time', 'pkg_idle_wkups', 'interrupt_wkups',
            'pageins', 'wired_size', 'resident_size', 'phys_footprint',
            'proc_start_abstime', 'proc_exit_abstime')]


class MemoryGuardExceeded(RuntimeError):
    """A sampled operational limit stopped an owned child, not an assertion pass."""

    def __init__(self, pid, observed, limit):
        self.pid, self.observed, self.limit = pid, observed, limit
        super().__init__(f'owned test process exceeded physical-footprint limit '
                         f'({observed} > {limit} bytes); killed PID {pid}')


def usage_reader():
    """Read the stable macOS process usage record, including its birth identity."""
    if sys.platform != 'darwin':
        raise RuntimeError('physical-footprint guards require macOS')
    library = ctypes.CDLL('/usr/lib/libproc.dylib', use_errno=True)
    read = library.proc_pid_rusage
    read.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_void_p]
    read.restype = ctypes.c_int

    def sample(pid):
        usage = RusageInfo()
        if read(pid, 0, ctypes.byref(usage)):
            error = ctypes.get_errno()
            raise OSError(error, os.strerror(error))
        return usage

    return sample


def footprint_reader():
    read = usage_reader()
    return lambda pid: read(pid).phys_footprint


class ProcessStartTime(ctypes.Structure):
    """The 64-bit Darwin timeval in the public extern_proc start-time union."""

    _fields_ = [('seconds', ctypes.c_int64), ('microseconds', ctypes.c_int32)]


class ProcessIdentity(ctypes.Structure):
    """Public macOS sys/proc.h extern_proc prefix, first in sysctl kinfo_proc."""

    # Apple's public 64-bit SDK ABI: sys/proc.h extern_proc and sys/sysctl.h kinfo_proc.
    # https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/proc.h
    _fields_ = [('start', ProcessStartTime), ('vmspace', ctypes.c_uint64),
                ('sigacts', ctypes.c_uint64), ('flags', ctypes.c_int32),
                ('status', ctypes.c_uint8), ('pid', ctypes.c_int32)]


def birth_reader():
    """Read public process start metadata without privileged task-resource access."""
    if sys.platform != 'darwin' or ctypes.sizeof(ctypes.c_void_p) != 8:
        raise RuntimeError('process birth identities require 64-bit macOS')
    library = ctypes.CDLL('/usr/lib/libSystem.B.dylib', use_errno=True)
    read = library.sysctl
    read.argtypes = [ctypes.POINTER(ctypes.c_int), ctypes.c_uint, ctypes.c_void_p,
                     ctypes.POINTER(ctypes.c_size_t), ctypes.c_void_p, ctypes.c_size_t]
    read.restype = ctypes.c_int

    def birth(pid):
        if type(pid) is not int or not 0 < pid <= 0x7fffffff:
            raise ValueError('process identity needs a positive pid_t')
        # Public sys/sysctl.h: CTL_KERN=1, KERN_PROC=14, KERN_PROC_PID=1.
        # The SDK's 64-bit extern_proc prefix is 48 bytes; full kinfo size is queried.
        name = (ctypes.c_int * 4)(1, 14, 1, pid)
        size = ctypes.c_size_t()

        def query(buffer):
            ctypes.set_errno(0)
            if read(name, 4, buffer, ctypes.byref(size), None, 0):
                error = ctypes.get_errno()
                raise OSError(error, f'process identity for PID {pid}: {os.strerror(error)}')

        query(None)
        if size.value == 0:
            raise ProcessLookupError(pid)
        minimum = ctypes.sizeof(ProcessIdentity)
        if not minimum <= size.value <= 65536:
            raise RuntimeError('process identity returned an invalid kinfo size')
        capacity = size.value
        buffer = ctypes.create_string_buffer(capacity)
        query(buffer)
        if size.value == 0:
            raise ProcessLookupError(pid)
        if not minimum <= size.value <= capacity:
            raise RuntimeError('process identity returned a short or oversized kinfo record')
        info = ProcessIdentity.from_buffer_copy(buffer)
        if info.pid != pid:
            raise RuntimeError('process identity returned a different PID')
        if info.status == 5:  # Public sys/proc.h SZOMB: awaiting collection by parent.
            raise ProcessLookupError(pid)
        if info.start.seconds <= 0 or not 0 <= info.start.microseconds < 1000000:
            raise RuntimeError('process identity returned an invalid start time')
        return info.start.seconds * 1000000 + info.start.microseconds

    return birth


def wait_guarded(process, timeout, limit_bytes, samples, *, reader=None, on_poll=None):
    """Stop an owned child above a sampled threshold, not a guaranteed peak ceiling."""
    if limit_bytes <= 0:
        raise ValueError('physical-footprint limit must be positive')
    read = reader or footprint_reader()
    deadline = time.monotonic() + timeout
    with Path(samples).open('x') as stream:
        while process.poll() is None:
            if on_poll is not None:
                on_poll()
            try:
                observed = read(process.pid)
            except OSError:
                if process.poll() is not None:
                    break
                raise
            exceeded = observed > limit_bytes
            stream.write(json.dumps({'monotonic_ns': time.monotonic_ns(),
                                     'pid': process.pid, 'physical_footprint_bytes': observed,
                                     'limit_bytes': limit_bytes, 'limit_exceeded': exceeded}) + '\n')
            stream.flush()
            if exceeded:
                process.kill()
                process.wait()
                raise MemoryGuardExceeded(process.pid, observed, limit_bytes)
            if time.monotonic() >= deadline:
                raise subprocess.TimeoutExpired(process.args, timeout)
            time.sleep(.1)
    return process.wait()


def run_captured(command, *, limit_bytes=None, samples=None, timeout=600, env=None,
                 on_started=None, before_start=None, on_poll=None):
    """Capture a child with optional environment, owned-PID notification and memory guard."""
    if on_poll is not None and limit_bytes is None:
        raise ValueError('a polled correctness child requires an explicit memory guard')
    if limit_bytes is None:
        if before_start is not None:
            before_start()
        if on_started is None:
            return subprocess.run(command, capture_output=True, text=True, timeout=timeout, env=env)
        process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=env)
        try:
            on_started(process.pid)
            output, errors = process.communicate(timeout=timeout)
            return subprocess.CompletedProcess(command, process.returncode, output, errors)
        finally:
            if process.poll() is None:
                process.kill()
            process.wait()
    if limit_bytes <= 0:
        raise ValueError('physical-footprint limit must be positive')
    # Files keep partial renderer evidence when the guard aborts a large run.
    output_path, error_path = Path(samples).with_suffix('.stdout'), Path(samples).with_suffix('.stderr')
    with output_path.open('x') as output, error_path.open('x') as errors:
        if before_start is not None:
            before_start()
        process = subprocess.Popen(command, stdout=output, stderr=errors, env=env)
        try:
            if on_started is not None:
                on_started(process.pid)
            returncode = wait_guarded(process, timeout, limit_bytes, samples, on_poll=on_poll)
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()
    return subprocess.CompletedProcess(command, returncode, output_path.read_text(), error_path.read_text())

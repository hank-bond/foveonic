"""Guard an owned macOS test process using OS-accounted physical footprint rather than RSS."""

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


def footprint_reader():
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
        return usage.phys_footprint

    return sample


def wait_guarded(process, timeout, limit_bytes, samples, *, reader=None):
    """Stop an owned child above a sampled threshold, not a guaranteed peak ceiling."""
    if limit_bytes <= 0:
        raise ValueError('physical-footprint limit must be positive')
    read = reader or footprint_reader()
    deadline = time.monotonic() + timeout
    with Path(samples).open('x') as stream:
        while process.poll() is None:
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
                raise RuntimeError(f'owned test process exceeded physical-footprint limit '
                                   f'({observed} > {limit_bytes} bytes); killed PID {process.pid}')
            if time.monotonic() >= deadline:
                raise subprocess.TimeoutExpired(process.args, timeout)
            time.sleep(.1)
    return process.wait()


def run_captured(command, *, limit_bytes=None, samples=None, timeout=600, env=None, on_started=None):
    """Capture a child with optional environment, owned-PID notification and memory guard."""
    if limit_bytes is None:
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
        process = subprocess.Popen(command, stdout=output, stderr=errors, env=env)
        try:
            if on_started is not None:
                on_started(process.pid)
            returncode = wait_guarded(process, timeout, limit_bytes, samples)
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()
    return subprocess.CompletedProcess(command, returncode, output_path.read_text(), error_path.read_text())

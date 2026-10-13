"""Record an owned native GUI session and inject explicit macOS pointer drags without changing app scheduling."""

import argparse
import ctypes
import hashlib
import json
import math
import os
from pathlib import Path
import signal
import subprocess
import sys
import time


def append_event(stream, **fields):
    stream.write(json.dumps({'monotonic_ns': time.monotonic_ns(), **fields}) + '\n')
    stream.flush()


def drag_points(start, end, y, steps, seconds):
    if steps < 2 or seconds <= 0 or not all(map(math.isfinite, (start, end, y, seconds))):
        raise ValueError('a drag needs finite coordinates, at least two points and positive duration')
    return [(seconds * i / (steps - 1), start + (end - start) * i / (steps - 1), y)
            for i in range(steps)]


def recording_environment(home, mode):
    env = {k: v for k, v in os.environ.items()
           if not k.startswith('FOVEONIC_GUI_SMOKE_') and k not in
           ['FOVEONIC_PROFILE', 'FOVEONIC_UI_TIMING', 'FOVEONIC_UI_GPU_PROFILE']}
    env.update(HOME=str(home), FOVEONIC_PROFILE=mode)
    return env


def record(args):
    directory = args.directory.resolve()
    directory.mkdir(parents=True, exist_ok=False)
    home = directory / 'home'
    home.mkdir()
    binary = args.binary.resolve(strict=True)
    fixture = args.fixture.resolve(strict=True)
    # Hash before startup/timing; source file-cache state is uncontrolled.
    binary_hash = hashlib.sha256(binary.read_bytes()).hexdigest()
    source_hash = hashlib.sha256(fixture.read_bytes()).hexdigest()
    with (directory / 'stderr.jsonl').open('x') as events:
        process = subprocess.Popen([str(binary), str(fixture)],
                                   env=recording_environment(home, args.mode),
                                   stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
                                   text=True, bufsize=1)
        manifest = {'pid': process.pid, 'binary': str(binary), 'fixture': str(fixture),
                    'binary_sha256': binary_hash, 'source_sha256': source_hash,
                    'home': str(home), 'started_monotonic_ns': time.monotonic_ns(),
                    'mode': args.mode, 'smoke_enabled': False, 'repaint_override': False}
        (directory / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
        print(json.dumps(manifest), flush=True)
        stopped = False

        def stop(_signum, _frame):
            nonlocal stopped
            stopped = True
            process.terminate()  # only the child this recorder owns

        previous = signal.signal(signal.SIGTERM, stop)
        try:
            for line in process.stderr:
                append_event(events, line=line.rstrip('\n'))
            process.wait()
        except KeyboardInterrupt:
            stop(None, None)
            process.wait()
        finally:
            signal.signal(signal.SIGTERM, previous)
            if process.poll() is None:
                stop(None, None)
                process.wait()
            append_event(events, kind='recorder_exit', returncode=process.returncode)
        if not stopped and process.returncode:
            raise SystemExit(process.returncode)


def drag(args):
    if sys.platform != 'darwin':
        raise RuntimeError('native pointer injection requires macOS')
    manifest = json.loads((args.directory / 'manifest.json').read_text())
    pid = manifest['pid']
    command = subprocess.check_output(['ps', '-p', str(pid), '-o', 'comm='], text=True).strip()
    if command != manifest['binary']:
        raise RuntimeError('recorded app is no longer running with the expected executable')
    cg = ctypes.CDLL('/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics')
    cf = ctypes.CDLL('/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation')
    cg.CGPreflightPostEventAccess.restype = ctypes.c_bool
    if not cg.CGPreflightPostEventAccess():
        raise RuntimeError('event-posting permission is missing; grant it before measurement')

    class Point(ctypes.Structure):
        _fields_ = [('x', ctypes.c_double), ('y', ctypes.c_double)]

    cg.CGEventCreateMouseEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint32, Point, ctypes.c_uint32]
    cg.CGEventCreateMouseEvent.restype = ctypes.c_void_p
    cg.CGEventPost.argtypes = [ctypes.c_uint32, ctypes.c_void_p]
    cf.CFRelease.argtypes = [ctypes.c_void_p]
    points = drag_points(args.start, args.end, args.y, args.steps, args.seconds)
    subprocess.run(['osascript', '-e', 'tell application "System Events" to set frontmost '
                    f'of first process whose unix id is {pid} to true'], check=True)
    time.sleep(.25)
    with (args.directory / 'pointer.jsonl').open('a') as events:
        def post(kind, x, y):
            event = cg.CGEventCreateMouseEvent(None, kind, Point(x, y), 0)
            if not event:
                raise RuntimeError('CGEventCreateMouseEvent failed')
            try:
                cg.CGEventPost(0, event)
                append_event(events, kind='pointer', event_type=kind, x=x, y=y)
            finally:
                cf.CFRelease(event)

        append_event(events, kind='drag_begin', label=args.label, pid=pid)
        post(5, args.start, args.y)
        post(1, args.start, args.y)
        began = time.monotonic()
        x, y = args.start, args.y
        try:
            for offset, x, y in points[1:]:
                time.sleep(max(0, began + offset - time.monotonic()))
                post(6, x, y)
        finally:
            post(2, x, y)
        append_event(events, kind='drag_end', label=args.label)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    capture = commands.add_parser('record')
    capture.add_argument('--binary', type=Path, required=True)
    capture.add_argument('--fixture', type=Path, required=True)
    capture.add_argument('--directory', type=Path, required=True)
    capture.add_argument('--mode', choices=['off', 'wall', 'gpu'], default='wall')
    capture.set_defaults(run=record)
    pointer = commands.add_parser('drag')
    pointer.add_argument('--directory', type=Path, required=True)
    pointer.add_argument('--start', type=float, required=True)
    pointer.add_argument('--end', type=float, required=True)
    pointer.add_argument('--y', type=float, required=True)
    pointer.add_argument('--steps', type=int, default=61)
    pointer.add_argument('--seconds', type=float, default=2.)
    pointer.add_argument('--label', required=True)
    pointer.set_defaults(run=drag)
    args = parser.parse_args()
    args.run(args)


if __name__ == '__main__':
    main()

"""Record shared native captures and inject explicit macOS pointer drags without changing app scheduling."""

import argparse
import ctypes
from contextlib import contextmanager
import hashlib
import json
import math
import os
import re
from pathlib import Path
import signal
import subprocess
import sys
import time
import uuid

from watch_workloads import require_quiet, concurrent_workloads


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
           if not k.startswith(('FOVEONIC_GUI_SMOKE_', 'FOVEONIC_GUI_SCENARIO_', 'FOVEONIC_DIAGNOSTICS_')) and k not in
           ['FOVEONIC_PROFILE', 'FOVEONIC_UI_TIMING', 'FOVEONIC_UI_GPU_PROFILE']}
    env.update(HOME=str(home), FOVEONIC_PROFILE=mode)
    return env


def source_tree_hash(root):
    """Hash nonignored Rust, shader, Cargo and bundled asset inputs before launch."""
    names = subprocess.check_output(
        ['git', '-C', str(root), 'ls-files', '--cached', '--others', '--exclude-standard', '-z']
    ).split(b'\0')
    paths = []
    for raw in names:
        if not raw:
            continue
        name = os.fsdecode(raw)
        path = Path(name)
        if (path.suffix in ('.rs', '.wgsl') or path.name in ('Cargo.toml', 'Cargo.lock')
                or name.startswith(('.cargo/', 'crates/foveonic/assets/',
                                    'crates/foveonic-gui/assets/'))):
            paths.append(name)
    digest = hashlib.sha256()
    for name in sorted(set(paths)):
        path = root / name
        # Preserve removal and symlink identity without following external targets.
        if path.is_symlink():
            data = os.fsencode(os.readlink(path))
            kind = b'link'
        elif path.is_file():
            data = path.read_bytes()
            kind = b'file'
        else:
            data = b''
            kind = b'missing'
        for part in (os.fsencode(name), kind, data):
            digest.update(len(part).to_bytes(8, 'big'))
            digest.update(part)
    return digest.hexdigest()


@contextmanager
def workload_monitor(output, cancelled=lambda: False):
    tool = Path(__file__).with_name('watch_workloads.py').resolve()
    process = subprocess.Popen([sys.executable, str(tool), str(output)], start_new_session=True)
    try:
        deadline = time.monotonic() + 5.
        while not output.exists() or output.stat().st_size == 0:
            if cancelled():
                raise KeyboardInterrupt
            if process.poll() is not None:
                raise RuntimeError('workload monitor exited before its first sample')
            if time.monotonic() >= deadline:
                raise RuntimeError('workload monitor did not produce its first sample')
            time.sleep(.01)
        if cancelled():
            raise KeyboardInterrupt
        yield tool
    finally:
        failed = process.poll() is not None
        if not failed:
            process.terminate()
        process.wait()
        if failed and sys.exc_info()[0] is None:
            raise RuntimeError('workload monitor exited during recording')


def record(args):
    namespace=getattr(args,'cache_namespace',None)
    if namespace is None:namespace='record-'+uuid.uuid4().hex
    if re.fullmatch(r'[A-Za-z0-9_-]{1,64}',namespace) is None:
        raise ValueError('cache namespace must be one ASCII alphanumeric, dash or underscore component (up to 64 characters)')
    directory = args.directory.resolve()
    directory.mkdir(parents=True, exist_ok=False)
    home = directory / 'home'
    home.mkdir()
    binary = args.binary.resolve(strict=True)
    fixture = args.fixture.resolve(strict=True)
    # Hashing reads the input and can warm the file cache. Do it before timing
    # and do not label the later app read as cold media access.
    binary_hash = hashlib.sha256(binary.read_bytes()).hexdigest()
    source_hash = hashlib.sha256(fixture.read_bytes()).hexdigest()
    source_tree = source_tree_hash(Path(__file__).resolve().parents[2])
    monitor_output = directory / 'workloads.jsonl'
    stopped = False
    process = None

    def stop(_signum, _frame):
        nonlocal stopped
        stopped = True
        if process is not None and process.poll() is None:
            process.terminate()  # only the child this recorder owns

    previous = signal.signal(signal.SIGTERM, stop)
    try:
        with workload_monitor(monitor_output, lambda: stopped) as monitor_tool:
            if stopped:
                return
            capture_parent = directory / 'captures'
            capture_parent.mkdir()
            env = recording_environment(home, args.mode)
            env['FOVEONIC_DIAGNOSTICS_DIR'] = str(capture_parent)
            env['FOVEONIC_DIAGNOSTICS_CACHE_NAMESPACE'] = namespace
            with (directory / 'stderr.txt').open('x') as errors, (directory / 'stdout.txt').open('x') as output:
                preflight = require_quiet()
                process = subprocess.Popen([str(binary), str(fixture)], env=env,
                                           stdout=output, stderr=errors, start_new_session=True)
                manifest = {'pid':process.pid,'binary':str(binary),'fixture':str(fixture),
                            'binary_sha256':binary_hash,'source_sha256':source_hash,
                            'source_tree_sha256':source_tree,
                            'source_tree_scope':'nonignored Rust, WGSL, Cargo and bundled asset inputs',
                            'workload_monitor':{'tool':str(monitor_tool),'output':str(monitor_output),'interval_seconds':.5},
                            'home':str(home),'startup_mode':args.mode,'capture_directory':str(capture_parent),'cache_namespace':namespace,
                            'smoke_enabled':False,'repaint_override':False,'preflight':preflight,
                            'instructions':'Use native Stop/Save to keep working, or close normally to save on exit; captures use the shared SDK recorder.'}
                (directory / 'launch.json').write_text(json.dumps(manifest,indent=2)+'\n')
                print(json.dumps(manifest),flush=True)
                try:
                    if stopped:stop(None,None)
                    process.wait()
                except KeyboardInterrupt:
                    stop(None,None)
                finally:
                    if process.poll() is None:stop(None,None)
                    process.wait()
        contention = concurrent_workloads(monitor_output, owned_pids=[process.pid])
        manifest.update(contention=contention, timing_valid=contention['checked'] and contention['uncontended'])
        captures=[]
        for saved in sorted(capture_parent.glob('*/manifest.json')):
            capture=saved.parent
            if not (capture/'trace.pftrace').is_file() or not (capture/'summary.json').is_file():continue
            data=json.loads(saved.read_text())
            data['application']['external_launch']=manifest
            saved.write_text(json.dumps(data,indent=2)+'\n')
            summary_path=capture/'summary.json'
            summary=json.loads(summary_path.read_text())
            summary.update(contention=contention,timing_valid=manifest['timing_valid'])
            summary_path.write_text(json.dumps(summary,indent=2)+'\n')
            captures.append(str(capture))
            if getattr(args,'trace_processor',None):
                from trace_analysis import analyze_capture
                analyze_capture(capture,args.trace_processor)
        manifest.update(returncode=process.returncode,captures=captures,capture_saved=bool(captures))
        (directory/'launch.json').write_text(json.dumps(manifest,indent=2)+'\n')
        if not stopped and process.returncode:raise SystemExit(process.returncode)
        if not stopped and args.mode!='off' and not captures:
            raise RuntimeError('no shared capture was saved; close normally or use native Stop/Save before quitting')

    except KeyboardInterrupt:
        pass
    finally:
        signal.signal(signal.SIGTERM, previous)


def drag(args):
    if sys.platform != 'darwin':
        raise RuntimeError('native pointer injection requires macOS')
    manifest = json.loads((args.directory / 'launch.json').read_text())
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
    capture.add_argument('--trace-processor', type=Path, help='explicit local tool for authoritative saved-trace analysis')
    capture.add_argument('--cache-namespace', help='explicit controlled reuse; default is a unique isolated record namespace')
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

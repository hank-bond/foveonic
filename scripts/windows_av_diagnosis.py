#!/usr/bin/env python3
"""Run a fixed, bounded matrix against the retained Windows core-test executable."""
import base64
import ctypes
from ctypes import wintypes
import datetime as dt
import hashlib
import json
import os
from pathlib import Path, PureWindowsPath
import platform
import re
import shutil
import subprocess
import sys
import time
import zipfile

ARTIFACT_SHA = '101445b53d3dc734f9250c39c28509a787bb14a6a76bd50695d9eacb28981c08'
EXE = 'foveonic-bbed161025ca942b.exe'
PDB = 'foveonic-bbed161025ca942b.pdb'
EXPECTED = {
    EXE: '95a7cf9f4f952c469137887343164c73126d622d85a7575c82460069100ec27e',
    PDB: '67c83e9c1f33f942d90983fbff20eee0d8d0fa168f9f1e858db13675ab3d704e',
    'dxc_2026_09_29.zip': 'ad31b1fc8443175d204f77a611fdb3ef2ec42759bdc2f1167368de24a4a7e7f1',
}
DLLS = {
    r'bin\x64\dxcompiler.dll': '0689e2fee0a1c9df8662a59680469f24e2a3ad7ce6bbbcc9284f7a83172bfc59',
    r'bin\x64\dxil.dll': '64be2368211d257a038a7d4ea6bb3e24c878eb8d588c7b8414fccf759b69c2ac',
}
PREFIX = 'nodes::color::weighted_white_balance::'
GPU_TESTS = [
    PREFIX + 'reduction_tests::candidate_scores_cover_all_candidates_and_only_first_quarter',
    PREFIX + 'reduction_tests::signed_black_reduction_preserves_samples_and_unequal_shields',
    PREFIX + 'reduction_tests::weighted_reduction_preserves_sparse_tail_and_boundary_weights',
    PREFIX + 'tests::signed_row_statistics_preserve_integer_offsets',
]
CELLS = [
    ('full_serial', ['--nocapture', '--test-threads=1']),
    ('white_balance_default', [PREFIX, '--nocapture']),
    ('white_balance_serial', [PREFIX, '--nocapture', '--test-threads=1']),
    ('diagnostics_default', ['diagnostics::tests::', '--nocapture']),
    ('full_without_four_gpu_tests', ['--nocapture'] + [arg for name in GPU_TESTS for arg in ('--skip', name)]),
    ('full_default_debugger', ['--nocapture']),
]


def digest(data):
    return hashlib.sha256(data).hexdigest()


def now():
    return dt.datetime.now(dt.timezone.utc).isoformat()


def stage(archive, root):
    blob = archive.read_bytes()
    if digest(blob) != ARTIFACT_SHA:
        raise ValueError('retained artifact ZIP checksum mismatch')
    with zipfile.ZipFile(archive) as z:
        names = z.namelist()
        if len(names) != len(set(names)) or any('/' in n or '\\' in n for n in names):
            raise ValueError('unexpected artifact ZIP layout')
        manifest = json.loads(z.read('SHA256SUMS.json'))
        listed = {}
        for row in manifest:
            name = PureWindowsPath(row['Path']).name
            if name in listed or name not in names:
                raise ValueError('invalid manifest entry')
            listed[name] = row['Hash'].lower()
        if set(names) != set(listed) | {'SHA256SUMS.json'}:
            raise ValueError('manifest does not cover artifact files')
        for name, expected in listed.items():
            if digest(z.read(name)) != expected:
                raise ValueError('manifest mismatch: ' + name)
        for name, expected in EXPECTED.items():
            data = z.read(name)
            if digest(data) != expected:
                raise ValueError('pinned checksum mismatch: ' + name)
            (root / name).write_bytes(data)
    with zipfile.ZipFile(root / 'dxc_2026_09_29.zip') as z:
        # ZipInfo normalizes native separators differently on Windows and macOS.
        members = {PureWindowsPath(info.filename).as_posix(): info for info in z.infolist()}
        if len(members) != len(z.infolist()):
            raise ValueError('duplicate normalized DXC member')
        for name, expected in DLLS.items():
            data = z.read(members[PureWindowsPath(name).as_posix()])
            if digest(data) != expected:
                raise ValueError('DXC DLL checksum mismatch: ' + name)
            (root / PureWindowsPath(name).name).write_bytes(data)


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n', encoding='utf-8')



# Toolhelp snapshots capture only descendant PIDs and selected graphics module identities.
class ProcessEntry(ctypes.Structure):
    _fields_ = [('size', wintypes.DWORD), ('usage', wintypes.DWORD),
                ('pid', wintypes.DWORD), ('heap', ctypes.c_size_t),
                ('module', wintypes.DWORD), ('threads', wintypes.DWORD),
                ('parent', wintypes.DWORD), ('priority', ctypes.c_long),
                ('flags', wintypes.DWORD), ('exe', wintypes.WCHAR * 260)]


class ModuleEntry(ctypes.Structure):
    _fields_ = [('size', wintypes.DWORD), ('module_id', wintypes.DWORD),
                ('pid', wintypes.DWORD), ('global_usage', wintypes.DWORD),
                ('process_usage', wintypes.DWORD), ('base', ctypes.c_void_p),
                ('base_size', wintypes.DWORD), ('handle', ctypes.c_void_p),
                ('name', wintypes.WCHAR * 256), ('path', wintypes.WCHAR * 260)]


def snapshot(flag, pid, entry_type, first, following):
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    kernel.CreateToolhelp32Snapshot.argtypes = [wintypes.DWORD, wintypes.DWORD]
    kernel.CreateToolhelp32Snapshot.restype = wintypes.HANDLE
    kernel.CloseHandle.argtypes = [wintypes.HANDLE]
    for function in (first, following):
        getattr(kernel, function).argtypes = [wintypes.HANDLE, ctypes.POINTER(entry_type)]
    handle = kernel.CreateToolhelp32Snapshot(flag, pid)
    if handle == ctypes.c_void_p(-1).value:
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        entry = entry_type()
        entry.size = ctypes.sizeof(entry)
        entries = []
        call = getattr(kernel, first)
        while call(handle, ctypes.byref(entry)):
            entries.append(entry_type.from_buffer_copy(entry))
            call = getattr(kernel, following)
        return entries
    finally:
        kernel.CloseHandle(handle)


def capture_processes(pid, result):
    try:
        processes = snapshot(2, 0, ProcessEntry, 'Process32FirstW', 'Process32NextW')
    except OSError as error:
        result.setdefault('observation_errors', []).append(str(error))
        return
    descendants = {pid}
    changed = True
    while changed:
        added = {p.pid for p in processes if p.parent in descendants} - descendants
        changed = bool(added)
        descendants.update(added)
    for process in processes:
        if process.pid in descendants and process.exe.lower() == EXE.lower():
            result['test_pid'] = process.pid
            break
    target = result['test_pid']
    if target is None:
        return
    allowed = {'dxcompiler.dll', 'dxil.dll', 'd3d12.dll', 'd3d12core.dll', 'd3d10warp.dll'}
    try:
        modules = snapshot(0x18, target, ModuleEntry, 'Module32FirstW', 'Module32NextW')
    except OSError as error:
        if len(result.setdefault('observation_errors', [])) < 20:
            result['observation_errors'].append(str(error))
        return
    for module in modules:
        if module.name.lower() not in allowed:
            continue
        if any(item['name'].lower() == module.name.lower() and item['path'] == module.path
               for item in result['graphics_modules']):
            continue
        identity = {'name': module.name, 'path': module.path, 'mapped_image_bytes': module.base_size}
        try:
            data = Path(module.path).read_bytes()
            identity['sha256'] = digest(data)
            identity['file_bytes'] = len(data)
        except OSError:
            identity['sha256'] = None
        result['graphics_modules'].append(identity)


def find_lldb():
    found = shutil.which('lldb')
    if found:
        return found
    installed = Path(os.environ.get('ProgramFiles', r'C:\Program Files')) / 'LLVM/bin/lldb.exe'
    return str(installed) if installed.is_file() else None


def event_log(pid, start, end, env):
    # Windows PowerShell joins arguments after -Command into command text.
    # Encode the complete query so PID and time values cannot change its parsing.
    query = r'''$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'
$p = PID_VALUE
$start = [DateTime]::Parse('START_VALUE').ToUniversalTime()
$end = [DateTime]::Parse('END_VALUE').ToUniversalTime()
try {
  $items = @(Get-WinEvent -FilterHashtable @{LogName='Application'; Id=1000; StartTime=$start; EndTime=$end} -ErrorAction Stop | ForEach-Object {
    $x = [xml]$_.ToXml(); $d = @($x.Event.EventData.Data | ForEach-Object { $_.'#text' })
    if ($d.Count -ge 9) {
      try { $id = if ($d[8] -match '^0x') { [Convert]::ToInt64($d[8].Substring(2),16) } else { [long]$d[8] } } catch { $id = -1 }
      if ($id -eq $p) { [pscustomobject]@{event_id=1000; time_utc=$_.TimeCreated.ToUniversalTime().ToString('o'); process_id=$id; fault_module=$d[3]; exception_code=$d[6]; fault_offset=$d[7]} }
    }
  }); [pscustomobject]@{status='queried'; matched_1000=$items} | ConvertTo-Json -Depth 4 -Compress
} catch {
  if ($_.FullyQualifiedErrorId -like 'NoMatchingEventsFound,*') {
    [pscustomobject]@{status='no_matching_events'; matched_1000=@()} | ConvertTo-Json -Compress
  } else {
    [pscustomobject]@{status='query_failed'; error_id=$_.FullyQualifiedErrorId; error=$_.Exception.Message} | ConvertTo-Json -Compress
  }
}'''
    query = query.replace('PID_VALUE', str(int(pid))).replace('START_VALUE', start).replace('END_VALUE', end)
    encoded = base64.b64encode(query.encode('utf-16le')).decode('ascii')
    try:
        events = subprocess.run(['powershell.exe', '-NoProfile', '-NonInteractive', '-EncodedCommand', encoded],
                                capture_output=True, text=True, timeout=10, check=False, env=env)
        if events.returncode:
            return {'status': 'query_failed', 'exit_code': events.returncode, 'stderr': events.stderr}
        result = json.loads(events.stdout)
        result['event_1001'] = 'not reliably PID-addressable'
        return result
    except (OSError, subprocess.TimeoutExpired, ValueError) as error:
        return {'status': 'query_unavailable', 'error': str(error)}


def cleanup_process(process, result):
    receipt = {'needed': process.poll() is None, 'taskkill_exit_code': None, 'parent_exited': False}
    result['cleanup'] = receipt
    try:
        if receipt['needed']:
            try:
                killed = subprocess.run(['taskkill', '/PID', str(process.pid), '/T', '/F'],
                                        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                        timeout=15, check=False)
                receipt['taskkill_exit_code'] = killed.returncode
            except (OSError, subprocess.TimeoutExpired) as error:
                receipt['taskkill_error'] = str(error)
            if process.poll() is None:
                process.kill()
                receipt['parent_kill_fallback'] = True
        process.wait(timeout=15)
    finally:
        receipt['parent_exited'] = process.poll() is not None


def debugger_outcome(stdout, result):
    exits = re.findall(r'Process (\d+) exited with status = (-?\d+)', stdout)
    stops = re.findall(r'stop reason = Exception (0x[0-9a-fA-F]+)', stdout)
    if exits:
        pid, code = exits[-1]
        result.update(test_pid=int(pid), target_state='exited', target_exit_code=int(code))
    elif stops:
        result.update(target_state='exception_stopped', target_exception_code=stops[-1].lower())
        launched = re.findall(r'Process (\d+) launched:', stdout)
        if launched:
            result['test_pid'] = int(launched[-1])
    else:
        result['target_state'] = 'unknown'


def run_cell(name, args, root, output, lldb, *, deadline=None, timeout_seconds=180):
    exe = root / EXE
    command = [str(exe), *args]
    debug = name == 'full_default_debugger' and lldb is not None
    if debug:
        command = [lldb, '--no-lldbinit', '--batch', '-o', 'image list', '-o', 'run',
                   '-k', 'thread backtrace all', '-k', 'image list', '--', str(exe), *args]
    env = {key: value for key, value in os.environ.items()
           if key.upper() not in ('GH_TOKEN', 'GITHUB_TOKEN')}
    env.update(RUST_MIN_STACK='8388608', FOVEONIC_GPU_TEST_TRACE='1')
    result = {'name': name, 'argv': command, 'test_argv': [str(exe), *args],
              'debugger': lldb if debug else ('unavailable' if name == 'full_default_debugger' else None),
              'start_utc': now(), 'timeout_seconds': timeout_seconds, 'timed_out': False,
              'test_pid': None, 'graphics_modules': [], 'target_exit_code': None,
              'debugger_exit_code': None, 'target_state': 'not_launched',
              'event_log': {'status': 'not_queried'}}
    if deadline is not None:
        timeout_seconds = min(timeout_seconds, deadline - time.monotonic() - 45)
        result['timeout_seconds'] = max(0, timeout_seconds)
    if timeout_seconds <= 0 or (name == 'full_default_debugger' and lldb is None):
        result['target_state'] = 'not_run_budget' if timeout_seconds <= 0 else 'not_run_debugger_unavailable'
        result['end_utc'] = now()
        write_json(output / (name + '.json'), result)
        return result
    process = None
    stdout_path = output / (name + '.stdout.log')
    try:
        with stdout_path.open('wb') as stdout, (output / (name + '.stderr.log')).open('wb') as stderr:
            process = subprocess.Popen(command, cwd=root, env=env, stdout=stdout, stderr=stderr)
            try:
                result['pid'] = process.pid
                if not debug:
                    result['test_pid'] = process.pid
                cell_deadline = time.monotonic() + timeout_seconds
                while process.poll() is None and time.monotonic() < cell_deadline:
                    capture_processes(process.pid, result)
                    time.sleep(0.5)
                result['timed_out'] = process.poll() is None
            finally:
                cleanup_process(process, result)
    except BaseException as error:
        result['controller_error'] = str(error)
        raise
    finally:
        result['end_utc'] = now()
        if process is not None:
            if debug:
                result['debugger_exit_code'] = process.returncode
                debugger_outcome(stdout_path.read_text(encoding='utf-8', errors='replace'), result)
                result['exit_note'] = 'Debugger status is separate from the inferior exit or exception stop.'
            else:
                result['target_exit_code'] = process.returncode
                result['target_state'] = 'exited'
            if result['timed_out'] or 'controller_error' in result:
                result['target_state'] = 'terminated_by_controller'
            if result['test_pid']:
                result['event_log'] = event_log(result['test_pid'], result['start_utc'], result['end_utc'], env)
        write_json(output / (name + '.json'), result)
    return result


def main():
    if len(sys.argv) != 3 or sys.platform != 'win32':
        raise SystemExit('usage on Windows: windows_av_diagnosis.py INPUT_ZIP EVIDENCE_DIR')
    deadline = time.monotonic() + 900
    archive, output = (Path(value).resolve() for value in sys.argv[1:])
    output.mkdir(parents=True, exist_ok=False)
    root = output.parent / 'windows-av-stage'
    root.mkdir(exist_ok=False)
    metadata = {'start_utc': now(), 'platform': platform.platform(),
                'image_os': os.environ.get('ImageOS'), 'image_version': os.environ.get('ImageVersion'),
                'artifact_id': 14224215814, 'artifact_sha256': ARTIFACT_SHA,
                'original_workspace_gate': 'failed', 'release_clearance': False,
                'original_image_version': '20261112.366.7',
                'control_budget_seconds': 900, 'job_timeout_minutes': 20,
                'release_gate': 'not assessed; diagnostic workflow completion is not a release pass'}
    write_json(output / 'run.json', metadata)
    stage(archive, root)
    lldb = find_lldb()
    metadata['lldb'] = lldb
    metadata['cdb'] = shutil.which('cdb')
    write_json(output / 'run.json', metadata)
    listed = run_cell('test_list', ['--list'], root, output, None, deadline=deadline)
    if listed['target_exit_code'] != 0 or listed['timed_out']:
        raise ValueError('test listing failed')
    lines = (output / 'test_list.stdout.log').read_text(encoding='utf-8', errors='replace').splitlines()
    tests = {line.removesuffix(': test') for line in lines if line.endswith(': test')}
    wb = {test for test in tests if test.startswith(PREFIX)}
    if wb != set(GPU_TESTS) | {PREFIX + 'tests::shader_contract_validates'}:
        raise ValueError('expected white-balance test names missing')
    sdk_tests = {
        'diagnostics::tests::sdk_capture_connects_two_threads_coalescing_counters_and_late_observations',
        'diagnostics::tests::sdk_ring_buffer_bounds_capture_and_unsupported_configs_are_rejected',
    }
    if not sdk_tests <= tests:
        raise ValueError('expected SDK diagnostics test names missing')
    metadata['white_balance_test_names'] = sorted(wb)
    metadata['sdk_diagnostics_test_names'] = sorted(sdk_tests)
    metadata['diagnostics_test_count'] = sum(test.startswith('diagnostics::tests::') for test in tests)
    write_json(output / 'run.json', metadata)
    for name, args in CELLS:
        run_cell(name, args, root, output, lldb, deadline=deadline)
    metadata['end_utc'] = now()
    write_json(output / 'run.json', metadata)


if __name__ == '__main__':
    main()

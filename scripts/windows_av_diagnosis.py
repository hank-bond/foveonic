#!/usr/bin/env python3
"""Run a fixed, bounded matrix against the retained Windows core-test executable."""
import argparse
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
import struct
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
CRASH_CELLS = [
    ('full_plain_default', ['--nocapture']),
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
        if process.pid in descendants and process.exe.lower() == PureWindowsPath(result['test_argv'][0]).name.lower():
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


def pe_identity(path):
    """Read bounded PE imports without loading a DLL into this process."""
    if path.stat().st_size > 256 * 1024 * 1024:
        raise ValueError('PE file exceeds inventory bound: ' + str(path))
    data = path.read_bytes()
    def unpack(fmt, offset):
        return struct.unpack_from(fmt, data, offset)
    if data[:2] != b'MZ':
        raise ValueError('missing DOS header: ' + str(path))
    pe = unpack('<I', 0x3c)[0]
    if data[pe:pe + 4] != b'PE\0\0':
        raise ValueError('missing PE header: ' + str(path))
    machine, sections = unpack('<HH', pe + 4)
    optional_size = unpack('<H', pe + 20)[0]
    optional = pe + 24
    magic = unpack('<H', optional)[0]
    if magic not in (0x10b, 0x20b) or sections > 96:
        raise ValueError('unsupported PE layout: ' + str(path))
    directories = optional + (112 if magic == 0x20b else 96)
    directory_count = unpack('<I', directories - 4)[0]
    image_base = unpack('<Q' if magic == 0x20b else '<I', optional + (24 if magic == 0x20b else 28))[0]
    header_size = unpack('<I', optional + 60)[0]
    ranges = [unpack('<IIII', optional + optional_size + index * 40 + 8) for index in range(sections)]
    def offset(rva):
        if rva < header_size and rva < len(data):
            return rva
        for virtual_size, address, raw_size, raw in ranges:
            if address <= rva < address + max(virtual_size, raw_size) and rva - address < raw_size:
                position = raw + rva - address
                if position < len(data):
                    return position
        raise ValueError('unmapped PE RVA: ' + str(path))
    imports = []
    for index, width, field in [(1, 20, 12), (13, 32, 4)]:
        if index >= directory_count:
            continue
        rva, size = unpack('<II', directories + index * 8)
        if not rva:
            continue
        position = offset(rva)
        for count in range(min(size // width, 2048)):
            entry = position + count * width
            if data[entry:entry + width] == bytes(width):
                break
            name_rva = unpack('<I', entry + field)[0]
            if index == 13 and not unpack('<I', entry)[0] & 1:
                name_rva -= image_base
            start = offset(name_rva)
            end = data.find(b'\0', start, start + 1024)
            if end < 0:
                raise ValueError('unterminated PE import: ' + str(path))
            name = data[start:end].decode('ascii')
            if PureWindowsPath(name).name != name:
                raise ValueError('unexpected PE import path: ' + name)
            imports.append({'name': name, 'delay_loaded': index == 13})
        else:
            raise ValueError('unterminated PE import table: ' + str(path))
    return {'name': path.name, 'path': str(path), 'sha256': digest(data), 'file_bytes': len(data),
            'machine': hex(machine), 'architecture': {0x8664: 'x64', 0x14c: 'x86', 0xaa64: 'arm64'}.get(machine, 'unknown'),
            'imports': imports}


def debugger_environment(path_additions=()):
    env = {key: value for key, value in os.environ.items()
           if key.upper() not in ('GH_TOKEN', 'GITHUB_TOKEN')}
    if path_additions:
        env['PATH'] = os.pathsep.join([*path_additions, env.get('PATH', '')])
    return env


def _dependency_inventory(lldb, root, output, deadline, inventory):
    llvm = Path(lldb).parent
    system = Path(os.environ['SystemRoot'])
    search = [llvm, system / 'System32', system, root]
    search.extend(Path(value) for value in os.environ.get('PATH', '').split(os.pathsep) if value)
    search = list(dict.fromkeys(path.resolve() for path in search if path.is_dir()))
    if len(search) > 80:
        raise ValueError('DLL search directory bound exceeded')
    python_roots = []
    if os.environ.get('RUNNER_TOOL_CACHE'):
        cached = Path(os.environ['RUNNER_TOOL_CACHE']) / 'Python'
        if cached.is_dir():
            python_roots.append(cached)
    extra = []
    visited = 0
    for cached in python_roots:
        for directory, children, _files in os.walk(cached):
            if time.monotonic() + 45 >= deadline:
                raise ValueError('DLL inventory budget exhausted')
            visited += 1
            if visited > 256:
                raise ValueError('Python runtime directory bound exceeded')
            parent = Path(directory)
            extra.append(parent)
            if len(parent.relative_to(cached).parts) >= 2:
                children.clear()
    inventory['python_cache_roots'] = [str(path) for path in python_roots]
    identities = {}
    pending = [Path(lldb)]
    def identity(path):
        key = str(path.resolve()).lower()
        if key not in identities:
            if len(identities) >= 128:
                raise ValueError('PE dependency file bound exceeded')
            identities[key] = pe_identity(path)
            inventory['files'].append(identities[key])
        return identities[key]
    scanned = set()
    while pending:
        if time.monotonic() + 45 >= deadline:
            raise ValueError('DLL inventory budget exhausted')
        path = pending.pop(0)
        key = str(path.resolve()).lower()
        if key in scanned:
            continue
        scanned.add(key)
        current = identity(path)
        if current['architecture'] != 'x64':
            inventory['errors'].append('non-x64 dependency: ' + str(path))
            continue
        if path.parent.resolve() == llvm.resolve():
            readobj = llvm / 'llvm-readobj.exe'
            if readobj.is_file():
                try:
                    probe = subprocess.run([str(readobj), '--coff-imports', '--file-headers', str(path)],
                                           capture_output=True, timeout=15, env=debugger_environment(), check=False)
                    current['llvm_readobj_exit_code'] = probe.returncode
                    (output / (path.name + '.imports.stdout.log')).write_bytes(probe.stdout)
                    (output / (path.name + '.imports.stderr.log')).write_bytes(probe.stderr)
                except (OSError, subprocess.TimeoutExpired) as error:
                    current['llvm_readobj_error'] = str(error)
        for imported in current['imports']:
            name = imported['name']
            row = {'importer': str(path), **imported}
            inventory['resolutions'].append(row)
            if name.lower().startswith(('api-ms-win-', 'ext-ms-win-')):
                row['resolution'] = 'Windows API-set contract'
                continue
            ordered = [llvm, system / 'System32', system, root,
                       *(Path(value) for value in inventory['debugger_path_additions']), *search]
            found = next((directory / name for directory in ordered if (directory / name).is_file()), None)
            if found is None:
                candidates = list(dict.fromkeys((directory / name).resolve() for directory in extra
                                                 if (directory / name).is_file()))
                row['off_path_candidates'] = [identity(candidate) for candidate in candidates]
                compatible = [candidate for candidate in candidates if identity(candidate)['architecture'] == 'x64']
                if len(compatible) != 1:
                    row['resolution'] = 'unresolved' if not compatible else 'ambiguous'
                    kind = 'delay-loaded DLL' if imported['delay_loaded'] else 'imported DLL'
                    inventory['errors'].append(row['resolution'] + ' ' + kind + ': ' + name)
                    continue
                found = compatible[0]
                parent = str(found.parent)
                if parent not in inventory['debugger_path_additions']:
                    inventory['debugger_path_additions'].append(parent)
                row['resolution'] = 'unique compatible existing runtime added to debugger PATH'
            else:
                row['resolution'] = 'existing loader search directory'
            row['resolved_path'] = str(found)
            resolved = identity(found)
            if found.parent.resolve() in (system.resolve(), (system / 'System32').resolve()):
                row['dependency_boundary'] = 'System DLL identity recorded; transitive OS dependencies are managed by Windows'
                if resolved['architecture'] != 'x64':
                    inventory['errors'].append('non-x64 system dependency: ' + str(found))
            else:
                pending.append(found)
    final_search = [llvm, system / 'System32', system, root,
                    *(Path(value) for value in inventory['debugger_path_additions']), *search]
    for row in inventory['resolutions']:
        if 'resolved_path' in row:
            final = next((directory / row['name'] for directory in final_search
                          if (directory / row['name']).is_file()), None)
            if final is None or final.resolve() != Path(row['resolved_path']).resolve():
                inventory['errors'].append('Runtime PATH changed an earlier resolution: ' + row['name'])
    inventory['status'] = 'passed' if not inventory['errors'] else 'blocked'


def dependency_inventory(lldb, root, output, deadline):
    inventory = {'status': 'inspecting', 'files': [], 'resolutions': [], 'debugger_path_additions': [],
                 'python_cache_roots': [], 'errors': [],
                 'delay_import_policy': 'Unresolved delay imports block this conservative preflight; that does not establish that they are needed at startup.'}
    try:
        _dependency_inventory(lldb, root, output, deadline, inventory)
    except (OSError, ValueError, struct.error) as error:
        inventory['status'] = 'blocked'
        inventory['errors'].append(str(error))
    finally:
        write_json(output / 'dependency-inventory.json', inventory)
    return inventory


def debugger_preflight(lldb, root, output, deadline):
    output.mkdir(exist_ok=False)
    result = {'status': 'blocked', 'core_controls_launched': 0, 'lldb': lldb,
              'debugger_path_additions': [], 'path_inherited_by_inferior': True}
    try:
        if not lldb:
            raise ValueError('LLDB is unavailable')
        inventory = dependency_inventory(lldb, root, output, deadline)
        result['debugger_path_additions'] = inventory['debugger_path_additions']
        if inventory['status'] != 'passed':
            raise ValueError('LLDB import closure is unresolved or ambiguous')
        env = debugger_environment(result['debugger_path_additions'])
        for name, args in [('version', ['--no-lldbinit', '--version']),
                           ('batch-version', ['--no-lldbinit', '--batch', '-o', 'version'])]:
            probe = subprocess.run([lldb, *args], cwd=root, env=env, capture_output=True, timeout=20, check=False)
            (output / (name + '.stdout.log')).write_bytes(probe.stdout)
            (output / (name + '.stderr.log')).write_bytes(probe.stderr)
            result[name + '_exit_code'] = probe.returncode
            if probe.returncode:
                raise ValueError('LLDB health command failed: ' + name)
        normal = output / 'normal.py'
        normal.write_text("import os\nassert not any(k.upper() in {'GH_TOKEN','GITHUB_TOKEN'} for k in os.environ)\nprint('owned debugger health normal')\n")
        crash = output / 'expected-av.py'
        crash.write_text("import ctypes\nfrom ctypes import wintypes\nk=ctypes.WinDLL('kernel32',use_last_error=True)\nk.SetErrorMode(3)\nk.CreateThread.argtypes=[ctypes.c_void_p,ctypes.c_size_t,ctypes.c_void_p,ctypes.c_void_p,wintypes.DWORD,ctypes.POINTER(wintypes.DWORD)]\nk.CreateThread.restype=wintypes.HANDLE\nk.WaitForSingleObject.argtypes=[wintypes.HANDLE,wintypes.DWORD]\nt=wintypes.DWORD()\nh=k.CreateThread(None,0,None,None,0,ctypes.byref(t))\nassert h\nk.WaitForSingleObject(h,10000)\n")
        for label, fixture in [('normal', normal), ('expected-av', crash)]:
            directory = output / label
            directory.mkdir()
            health = run_cell('lldb_health_' + label, [str(fixture)], root, directory, lldb,
                              deadline=deadline, timeout_seconds=25, executable=Path(sys.executable),
                              use_debugger=True, debugger_paths=result['debugger_path_additions'])
            text = (directory / ('lldb_health_' + label + '.stdout.log')).read_text(encoding='utf-8', errors='replace')
            gone = health['test_pid'] is not None and not any(entry.pid == health['test_pid'] for entry in
                        snapshot(2, 0, ProcessEntry, 'Process32FirstW', 'Process32NextW'))
            result[label] = {'target_state': health['target_state'], 'target_exit_code': health['target_exit_code'],
                             'debugger_exit_code': health['debugger_exit_code'], 'inferior_pid': health['test_pid'],
                             'inferior_gone': gone}
            if not gone or health['timed_out']:
                raise ValueError('LLDB fixture inferior did not exit: ' + label)
            if label == 'normal':
                if (health['target_state'] != 'exited' or health['target_exit_code'] != 0
                        or health['debugger_exit_code'] != 0 or 'owned debugger health normal' not in text):
                    raise ValueError('LLDB normal fixture failed')
            else:
                images = text.rsplit('(lldb) image list', 1)[-1]
                image_rows = re.findall(r'^\[\s*\d+\]\s+\S+\s+0x[0-9a-fA-F]+\s+.+', images, re.M)
                result[label]['post_crash_image_rows'] = len(image_rows)
                result[label]['backtrace_frames'] = len(re.findall(r'frame #\d+:', text))
                if (health['target_state'] != 'exception_stopped' or health.get('target_exception_code') != '0xc0000005'
                        or health['target_exit_code'] is not None or '(lldb) thread backtrace all' not in text
                        or not result[label]['backtrace_frames'] or not image_rows or 'kernel32.dll' not in images.lower()
                        or Path(sys.executable).name.lower() not in images.lower()):
                    raise ValueError('LLDB expected-AV stack or image capture failed')
        result['status'] = 'passed'
    except (OSError, ValueError, subprocess.TimeoutExpired, struct.error) as error:
        result['error'] = str(error)
    finally:
        write_json(output / 'result.json', result)
    return result


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


def run_cell(name, args, root, output, lldb, *, deadline=None, timeout_seconds=180,
             executable=None, use_debugger=False, debugger_paths=()):
    exe = executable if executable is not None else root / EXE
    command = [str(exe), *args]
    wants_debugger = use_debugger or name == 'full_default_debugger'
    debug = wants_debugger and lldb is not None
    if debug:
        command = [lldb, '--no-lldbinit', '--batch', '-o', 'image list', '-o', 'run',
                   '-k', 'thread backtrace all', '-k', 'image list', '--', str(exe), *args]
    env = debugger_environment(debugger_paths if debug else ())
    env.update(RUST_MIN_STACK='8388608', FOVEONIC_GPU_TEST_TRACE='1')
    result = {'name': name, 'argv': command, 'test_argv': [str(exe), *args],
              'debugger': lldb if debug else ('unavailable' if name == 'full_default_debugger' else None),
              'start_utc': now(), 'timeout_seconds': timeout_seconds, 'timed_out': False,
              'test_pid': None, 'graphics_modules': [], 'target_exit_code': None,
              'debugger_exit_code': None, 'target_state': 'not_launched',
              'event_log': {'status': 'not_queried'}}
    if debug:
        result['debugger_environment'] = {'path_additions': list(debugger_paths), 'inherited_by_inferior': True}
    if deadline is not None:
        timeout_seconds = min(timeout_seconds, deadline - time.monotonic() - 45)
        result['timeout_seconds'] = max(0, timeout_seconds)
    if timeout_seconds <= 0 or (wants_debugger and lldb is None):
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
    if sys.platform != 'win32':
        raise SystemExit('Windows is required')
    parser = argparse.ArgumentParser()
    parser.add_argument('archive', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--crash-only', action='store_true')
    args = parser.parse_args()
    deadline = time.monotonic() + 900
    archive, output = args.archive.resolve(), args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    root = output.parent / 'windows-av-stage'
    root.mkdir(exist_ok=False)
    metadata = {'start_utc': now(), 'platform': platform.platform(),
                'image_os': os.environ.get('ImageOS'), 'image_version': os.environ.get('ImageVersion'),
                'artifact_id': 14224215814, 'artifact_sha256': ARTIFACT_SHA,
                'original_workspace_gate': 'failed', 'release_clearance': False,
                'matrix': 'crash_only' if args.crash_only else 'isolation',
                'control_order': [name for name, _ in (CRASH_CELLS if args.crash_only else CELLS)],
                'workflow_commit': os.environ.get('GITHUB_SHA'),
                'prior_isolation_run': 43868573097,
                'cache_order_note': 'Prior isolation ran full_serial before white_balance_default and may have warmed persistent WARP/D3D caches. Crash-only runs full_plain_default as its first GPU control after CPU-only debugger health. Cache state is uncontrolled and unchanged.',
                'original_image_version': '20261112.366.7',
                'control_budget_seconds': 900, 'job_timeout_minutes': 20,
                'release_gate': 'not assessed; diagnostic workflow completion is not a release pass'}
    write_json(output / 'run.json', metadata)
    stage(archive, root)
    lldb = find_lldb()
    metadata['lldb'] = lldb
    metadata['cdb'] = shutil.which('cdb')
    write_json(output / 'run.json', metadata)
    debugger_paths = []
    if args.crash_only:
        preflight = debugger_preflight(lldb, root, output / 'lldb-preflight', deadline)
        metadata['debugger_preflight'] = preflight
        write_json(output / 'run.json', metadata)
        if preflight['status'] != 'passed':
            for name, cell_args in CRASH_CELLS:
                write_json(output / (name + '.json'), {'name': name, 'test_argv': [str(root / EXE), *cell_args],
                           'target_state': 'not_run_debugger_preflight', 'target_exit_code': None})
            metadata.update(end_utc=now(), core_controls_launched=0)
            write_json(output / 'run.json', metadata)
            raise RuntimeError('LLDB preflight blocked; no core controls launched')
        debugger_paths = preflight['debugger_path_additions']
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
    cells = CRASH_CELLS if args.crash_only else CELLS
    metadata['core_controls_launched'] = 0
    for name, cell_args in cells:
        result = run_cell(name, cell_args, root, output, lldb, deadline=deadline, debugger_paths=debugger_paths)
        if result.get('test_pid') is not None:
            metadata['core_controls_launched'] += 1
    metadata['end_utc'] = now()
    write_json(output / 'run.json', metadata)


if __name__ == '__main__':
    main()

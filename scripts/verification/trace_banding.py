"""Trace installed Merrill Stage 2 and actual column arguments on owned diagnostic inputs."""

import hashlib
import json
import os
from pathlib import Path
import struct

import lldb

OUT = Path(os.environ['FOVEONIC_BANDING_CAPTURE'])
PIN = '037419dc04bb1afe86dddad7a14888a1b36d6c00a0231ec50a6974a9963ce5e6'
STATE = {'status': 'pending', 'scope': 'Installed native Stage 2 replay with owned inputs and native algorithmic tiles/strips. Color scheduling uses one worker. Not an application export.', 'columns': [], 'mono_columns': []}
BASE = 0
BREAKS = {}
PENDING = {}
RETURNS = set()


def flush():
    (OUT / 'capture.json').write_text(json.dumps(STATE, indent=2) + '\n')


def register(frame, name):
    return frame.FindRegister(name).GetValueAsUnsigned()


def floating(frame, name):
    error = lldb.SBError()
    data = frame.FindRegister(name).GetData()
    value = data.GetFloat(error, 0) if name.startswith('s') else data.GetDouble(error, 0)
    if not error.Success():
        raise RuntimeError('cannot read floating register')
    return value


def memory(frame, address, size):
    if not address or not 0 < size <= 256 * 1024**2:
        raise ValueError('unexpected native memory extent')
    error = lldb.SBError()
    data = frame.GetThread().GetProcess().ReadMemory(address, size, error)
    if not error.Success() or len(data) != size:
        raise RuntimeError(str(error))
    return data


def image(frame, descriptor):
    _, address, width, height, stride, _ = struct.unpack('<QQIIII', memory(frame, descriptor, 32))
    if not 0 < width <= 6000 or not 0 < height <= 4000 or stride != width * 12:
        raise ValueError('unexpected native RGB descriptor')
    return {'width': width, 'height': height, 'stride': stride, 'sha256': hashlib.sha256(memory(frame, address, stride * height)).hexdigest()}


def mono_span(frame, descriptor, first, last):
    _, address, width, height, stride, _ = struct.unpack('<QQIIII', memory(frame, descriptor, 32))
    if not 0 < width <= 6000 or not 0 <= first <= last < height <= 4000 or stride != width * 4:
        raise ValueError('unexpected native mono descriptor/span')
    return {'width': width, 'height': height, 'stride': stride, 'first': first, 'last': last, 'sha256': hashlib.sha256(memory(frame, address + first * stride, (last - first + 1) * stride)).hexdigest()}


def remember_return(frame, target, collection, index, descriptor, span=None):
    address = register(frame, 'x30')
    if address not in RETURNS:
        point = target.BreakpointCreateByAddress(address)
        point.SetScriptCallbackFunction(__name__ + '.capture')
        BREAKS[point.GetID()] = 'column-return'
        RETURNS.add(address)
    thread = frame.GetThread().GetThreadID()
    if thread in PENDING:
        raise RuntimeError('overlapping column calls on one thread')
    PENDING[thread] = (collection, index, descriptor, span)


def effective(frame, address, mode):
    record = memory(frame, address, 0x1c0)
    (OUT / 'effective-noise.bin').write_bytes(record)
    STATE['effective'] = {'mode': mode, 'estimated_iso': struct.unpack_from('<f', record, 4)[0], 'reconstruction_scale': struct.unpack_from('<f', record, 0x18)[0], 'column_blend': struct.unpack_from('<f', record, 0xac)[0], 'gathered_window': struct.unpack_from('<I', record, 0xb0)[0], 'gathered_level': struct.unpack_from('<I', record, 0xb4)[0]}


def breakpoint(target, offset, label):
    point = target.BreakpointCreateByAddress(BASE + offset)
    point.SetScriptCallbackFunction(__name__ + '.capture')
    BREAKS[point.GetID()] = label


def capture(frame, location, _):
    global BASE
    try:
        target = frame.GetThread().GetProcess().GetTarget()
        label = BREAKS[location.GetBreakpoint().GetID()]
        if label == 'entry':
            module = frame.GetModule()
            native = Path(str(module.GetFileSpec()))
            digest = hashlib.sha256()
            with native.open('rb') as stream:
                for block in iter(lambda: stream.read(1024 * 1024), b''):
                    digest.update(block)
            if digest.hexdigest() != PIN:
                raise ValueError('native library identity changed')
            BASE = module.GetObjectFileHeaderAddress().GetLoadAddress(target)
            STATE['native_sha256'] = PIN
            request = register(frame, 'x1')
            STATE['input'] = image(frame, struct.unpack('<Q', memory(frame, request + 0x68, 8))[0])
            STATE['active_rect'] = struct.unpack('<4i', memory(frame, request + 0x90, 16))
            STATE['monochrome'] = memory(frame, request + 0x4c4, 1)[0]
            for offset, name in [(0x7f568, 'one-worker'), (0xaa3f4, 'tiled-entry'), (0x761d0, 'column'), (0x2fd3c, 'mono-entry'), (0x2f284, 'mono-column')]:
                breakpoint(target, offset, name)
        elif label == 'one-worker':
            if not frame.FindRegister('x0').SetValueFromCString('1'):
                raise RuntimeError('cannot select scheduling worker')
            STATE['worker_count'] = 1
        elif label == 'tiled-entry':
            effective(frame, register(frame, 'x4'), 'color')
            STATE['tiled_input'] = image(frame, register(frame, 'x1'))
        elif label == 'mono-entry':
            effective(frame, register(frame, 'x6'), 'monochrome')
            STATE['tiled_input'] = image(frame, register(frame, 'x2'))
        elif label == 'mono-column':
            descriptor = register(frame, 'x0')
            first, last = register(frame, 'w2'), register(frame, 'w3')
            call = {'before': mono_span(frame, descriptor, first, last), 'window': register(frame, 'w4'), 'gain': floating(frame, 's0'), 'estimated_iso': floating(frame, 's1'), 'level': struct.unpack('<I', memory(frame, register(frame, 'sp'), 4))[0]}
            index = len(STATE['mono_columns'])
            STATE['mono_columns'].append(call)
            remember_return(frame, target, 'mono_columns', index, descriptor, (first, last))
        elif label == 'column':
            descriptor = register(frame, 'x0')
            call = {'before': image(frame, descriptor), 'start': register(frame, 'x2'), 'end': register(frame, 'x3'), 'window': register(frame, 'x6'), 'monochrome': register(frame, 'x7'), 'floating_arguments': {n: floating(frame, n) for n in ['s0', 's1', 's2', 's3', 's4', 's5']}, 'trailing_u32': struct.unpack('<I', memory(frame, register(frame, 'sp'), 4))[0]}
            index = len(STATE['columns'])
            STATE['columns'].append(call)
            remember_return(frame, target, 'columns', index, descriptor)
        elif label == 'column-return':
            collection, index, descriptor, span = PENDING.pop(frame.GetThread().GetThreadID())
            STATE[collection][index]['after'] = mono_span(frame, descriptor, *span) if span else image(frame, descriptor)
        elif label == 'complete':
            if PENDING:
                raise RuntimeError('column return not observed')
            STATE['status'] = 'completed'
        elif label == 'assert':
            raise RuntimeError('native assertion reached')
        flush()
        return False
    except Exception as error:
        STATE.update(status='failed', error=repr(error))
        flush()
        print(error, flush=True)
        return True


def __lldb_init_module(debugger, _):
    if (OUT / 'capture.json').exists():
        raise FileExistsError('capture already exists')
    debugger.HandleCommand('settings set target.skip-prologue false')
    target = debugger.GetSelectedTarget()
    for symbol, label in [('_ZN14FIPEngine2_F2019priProcessF20Stage2EP20FIPERequestF20Stage2', 'entry'), ('_Z11AssertErrorPKcjS0_jj', 'assert'), ('fixture_complete', 'complete')]:
        point = target.BreakpointCreateByName(symbol)
        point.SetScriptCallbackFunction(__name__ + '.capture')
        BREAKS[point.GetID()] = label
    flush()

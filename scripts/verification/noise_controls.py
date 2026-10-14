"""Measure all Merrill native noise presets and boundaries without changing processing or reference files."""

import argparse
import ctypes as C
import hashlib
import itertools
import json
import math
from pathlib import Path
import struct

NATIVE_SHA256 = '037419dc04bb1afe86dddad7a14888a1b36d6c00a0231ec50a6974a9963ce5e6'
NATIVE = Path('/Applications/SIGMA_PhotoPro6.app/Contents/MonoBundle/Sensor/lib99_MerillDLLForMac.dylib')
RECORD_SIZE = 0x1a8
COLUMN_BYTES = range(0xac, 0xb8)
PRESETS = [0., .25, .5, .75, 1.]


def digest(path):
    with Path(path).open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def f32(value):
    return struct.unpack('<f', struct.pack('<f', value))[0]


def neighbor(value, direction):
    bits = struct.unpack('<I', struct.pack('<f', value))[0]
    return struct.unpack('<f', struct.pack('<I', bits + direction))[0]


def matrices(raw):
    """Read Float32 matrix records solely to establish safe native ISO ranges."""
    found = {}
    start = 0
    while start < len(raw):
        _, _, size, name_offset, payload = struct.unpack_from('<4s4I', raw, start)
        if size < 20 or start + size > len(raw):
            raise ValueError('invalid CAMF record extent')
        name = raw[start + name_offset:raw.index(0, start + name_offset)].decode()
        if name in ['ISONoiseSettings', 'AdditionalISONoiseSettings']:
            encoding, rank, offset = struct.unpack_from('<3I', raw, start + payload)
            if encoding != 3 or rank != 2:
                raise ValueError('native table fallback needs a separate bounds audit')
            rows, columns = [struct.unpack_from('<I', raw, start + payload + 12 + 12 * i)[0] for i in range(2)]
            if columns != 13 or rows < 2:
                raise ValueError('invalid ISO table')
            found[name] = [struct.unpack_from('<f', raw, start + offset + row * columns * 4)[0] for row in range(rows)]
        start += size
    if len(found) != 2:
        raise ValueError('both camera ISO tables are required for defined native requests')
    return found


class Guarded:
    def __init__(self, size):
        self.size = size
        self.storage = C.create_string_buffer(size + 128)
        C.memset(C.addressof(self.storage), 0xa5, 64)
        C.memset(C.addressof(self.storage) + 64 + size, 0xa5, 64)
        self.address = C.addressof(self.storage) + 64

    def bytes(self):
        self.check()
        return C.string_at(self.address, self.size)

    def check(self):
        if self.storage.raw[:64] != b'\xa5' * 64 or self.storage.raw[-64:] != b'\xa5' * 64:
            raise AssertionError('native buffer guard changed')


def bind(library, name, args):
    function = getattr(library, name)
    function.argtypes, function.restype = args, None
    return function


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--native', type=Path, default=NATIVE)
    parser.add_argument('--camf', action='append', required=True, help='camera-label=path to original decoded CAMF')
    parser.add_argument('--output', type=Path, required=True, help='new evidence directory')
    args = parser.parse_args()
    if digest(args.native) != NATIVE_SHA256:
        raise ValueError('installed native library identity changed; re-audit before calling pointers')
    cameras = []
    for value in args.camf:
        label, path = value.split('=', 1)
        path = Path(path).resolve()
        raw = path.read_bytes()
        tables = matrices(raw)
        cameras.append((label, path, raw, tables))
    args.output.mkdir(parents=True, exist_ok=False)
    report = {'status': 'running', 'scope': 'Installed Merrill version-three setup, primary calibration disabled, color and secondary monochrome; all discrete presets and defined ISO/control boundaries. Not pixel processing or application exports.', 'native_sha256': digest(args.native), 'harness_sha256': digest(__file__), 'cameras': [], 'cases': 0, 'groups': 0, 'changed_word_offsets': []}
    report_path = args.output / 'summary.json'
    report_path.write_text(json.dumps(report, indent=2) + '\n')
    library = C.CDLL(str(args.native))
    ctor = bind(library, '_ZN12FCamdata_F20C1EPKhj', [C.c_void_p, C.c_void_p, C.c_uint])
    destructor = bind(library, '_ZN12FCamdata_F20D1Ev', [C.c_void_p])
    string_ctor = bind(library, '_ZN15FUnicodeStr_F20C1EPKc', [C.c_void_p, C.c_char_p])
    string_destructor = bind(library, '_ZN15FUnicodeStr_F20D1Ev', [C.c_void_p])
    setup = bind(library, '_Z19SetUpNoiseParamsF20RN15FPixelProc2_F2011NoiseParamsEP12FCamdata_F20fffdddR15FUnicodeStr_F20bxbP23tagAdjustmentParameters', [C.c_void_p, C.c_void_p, C.c_float, C.c_float, C.c_float, C.c_double, C.c_double, C.c_double, C.c_void_p, C.c_bool, C.c_longlong, C.c_bool, C.c_void_p])
    changed = set()
    try:
        with (args.output / 'records.bin').open('xb') as records, (args.output / 'groups.jsonl').open('x') as groups:
            for label, path, raw, tables in cameras:
                # Additional-table pairs use the main estimate's brackets, but their
                # final knots can differ. Stop at the lower limit to avoid the
                # native out-of-bounds read.
                maximum = min(tables['ISONoiseSettings'][-1], tables['AdditionalISONoiseSettings'][-1], 6400.)
                knots = sorted(set([100., 800.] + tables['ISONoiseSettings'] + tables['AdditionalISONoiseSettings']))
                estimates = sorted(set(v for knot in knots for v in [neighbor(knot, -1), knot, neighbor(knot, 1)] if 0 < v <= maximum))
                camera = Guarded(128)
                raw_storage = C.create_string_buffer(raw)
                ctor(camera.address, raw_storage, len(raw))
                adjustment = Guarded(1024)
                C.memmove(adjustment.address + 0x28, struct.pack('<Q', 0x0002000100010000), 8)
                report['cameras'].append({'label': label, 'camf': str(path), 'camf_sha256': digest(path), 'tables': tables, 'estimates': estimates, 'native_maximum': maximum})
                try:
                    for resolution, mono in itertools.product(['HI', 'MED', 'LOW'], [False, True]):
                        name = Guarded(32)
                        string_ctor(name.address, resolution.encode())
                        try:
                            def measure(estimate, capture, chroma, luminance, banding):
                                state = Guarded(RECORD_SIZE)
                                setup(state.address, camera.address, estimate, capture, 2., chroma, luminance, banding, name.address, False, 3, mono, adjustment.address)
                                data = state.bytes()
                                index = min(4, max(0, int(banding * 4)))
                                base = ([.625, .5, .375, .25, .125] if resolution == 'HI' else [.875, .75, .625, .5, .375])[index]
                                gain = 1. if f32(estimate) < 800. else (struct.unpack('<f', struct.pack('<I', 0x3faaa993))[0] if resolution == 'HI' else f32(1.2))
                                expected = struct.pack('<fII', f32(base * gain), [15, 21, 27, 33, 39][index], index)
                                if data[0xac:0xb8] != expected:
                                    raise AssertionError(('unexpected native banding mapping', label, resolution, mono, estimate, banding, data[0xac:0xb8].hex(), expected.hex()))
                                position = report['cases']
                                records.write(data)
                                report['cases'] += 1
                                return position, data

                            for estimate, chroma, luminance in itertools.product(estimates, PRESETS, PRESETS):
                                # Exercise capture-ISO interpolation independently of the estimate.
                                capture = [100., 800., maximum][int(chroma * 4) % 3]
                                positions, states = zip(*(measure(estimate, capture, chroma, luminance, value) for value in PRESETS))
                                for state in states[1:]:
                                    offsets = {i for i, (a, b) in enumerate(zip(states[0], state)) if a != b}
                                    if not offsets.issubset(COLUMN_BYTES):
                                        raise AssertionError(('banding changed unrelated parameter bytes', sorted(offsets - set(COLUMN_BYTES))))
                                    changed.update(i // 4 * 4 for i in offsets)
                                groups.write(json.dumps({'camera': label, 'resolution': resolution, 'secondary_monochrome': mono, 'estimated_iso': estimate, 'capture_iso': capture, 'chroma': chroma, 'luminance': luminance, 'record_indices': positions}) + '\n')
                                report['groups'] += 1
                            for estimate, boundary in itertools.product([100., 800.], [.25, .5, .75, 1.]):
                                for value in [math.nextafter(boundary, -math.inf), boundary, math.nextafter(boundary, math.inf)]:
                                    position, _ = measure(estimate, 100., .5, .5, value)
                                    groups.write(json.dumps({'camera': label, 'resolution': resolution, 'secondary_monochrome': mono, 'estimated_iso': estimate, 'banding_boundary': value, 'record_indices': [position]}) + '\n')
                            for value in [-1., 2.]:
                                position, _ = measure(400., 100., .5, .5, value)
                                groups.write(json.dumps({'camera': label, 'resolution': resolution, 'secondary_monochrome': mono, 'banding_clipped': value, 'record_indices': [position]}) + '\n')
                        finally:
                            string_destructor(name.address)
                            name.check()
                    camera.check()
                    adjustment.check()
                    if raw_storage.raw[:len(raw)] != raw or digest(path) != hashlib.sha256(raw).hexdigest():
                        raise AssertionError('camera metadata changed')
                finally:
                    destructor(camera.address)
                    camera.check()
                report_path.write_text(json.dumps(report, indent=2) + '\n')
                print(label, report['cases'], 'complete guarded records', flush=True)
        report.update(status='passed', guards_intact=True, changed_word_offsets=[hex(v) for v in sorted(changed)], records_sha256=digest(args.output / 'records.bin'), groups_sha256=digest(args.output / 'groups.jsonl'))
    except Exception as error:
        report.update(status='failed', error=repr(error))
        raise
    finally:
        report_path.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({k: report[k] for k in ['status', 'cases', 'groups', 'changed_word_offsets']}), flush=True)


if __name__ == '__main__':
    main()

"""Retain guarded synthetic native color-column samples for all five banding presets."""

import argparse
import ctypes as C
import hashlib
import json
import math
from pathlib import Path
import struct

from noise_controls import NATIVE, NATIVE_SHA256, digest


class Image(C.Structure):
    _fields_ = [('vtable', C.c_void_p), ('data', C.POINTER(C.c_float)), ('width', C.c_uint32), ('height', C.c_uint32), ('stride', C.c_uint32), ('padding', C.c_uint32)]


def f32(value):
    return struct.unpack('<f', struct.pack('<f', value))[0]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input', type=Path, required=True, help='retained synthetic midpoint column fixture')
    parser.add_argument('--output', type=Path, required=True, help='new fixture file, never overwritten')
    parser.add_argument('--native', type=Path, default=NATIVE)
    args = parser.parse_args()
    if digest(args.native) != NATIVE_SHA256:
        raise ValueError('native identity changed')
    fixture = json.loads(args.input.read_text())
    width, height = fixture['width'], fixture['height']
    if (width, height) != (80, 112):
        raise ValueError('only the retained small synthetic fixture is supported')
    source = fixture['input']
    library = C.CDLL(str(args.native))
    consumer = getattr(library, '_Z15MatchColOffsetsP19FPixmapTemplate_F20I23FPixelClassFlt32RGB_F20ES2_jjP11FColorFloatPffffjb13FColorFloat32j')
    consumer.argtypes = [C.POINTER(Image), C.POINTER(Image), C.c_uint32, C.c_uint32, C.POINTER(C.c_double), C.POINTER(C.c_float), C.c_float, C.c_float, C.c_float, C.c_uint32, C.c_bool, C.c_float, C.c_float, C.c_float, C.c_uint32]
    consumer.restype = None
    model = (C.c_double * 9)(*[f32(v) for v in [25., 30., 15., .03, .04, .05, .0001, .0002, .00005]])
    table = (C.c_float * 2048)(*[f32(math.exp(f32(f32(i) * struct.unpack('<f', struct.pack('<I', 0xbb801002))[0]))) for i in range(2048)])
    guard, sentry = 64, f32(-19391.125)
    capacity = max(width * width, width * (height + 1)) * 3
    rows = []
    for preset in range(5):
        pixels = (C.c_float * (capacity + guard * 2))()
        scratch = (C.c_float * (capacity + guard * 2))()
        for storage in [pixels, scratch]:
            for i in range(guard):
                storage[i] = storage[guard + capacity + i] = sentry
        for i, value in enumerate(source):
            pixels[guard + i] = value
        address = lambda storage: C.cast(C.byref(storage, guard * 4), C.POINTER(C.c_float))
        image = Image(None, address(pixels), width, height, width * 12, 0)
        temporary = Image(None, address(scratch), width, height, width * 12, 0)
        window = 15 + preset * 6
        coefficient = [.625, .5, .375, .25, .125][preset]
        consumer(C.byref(image), C.byref(temporary), fixture['start'], fixture['end'], model, table, 8., coefficient, 100., window, False, 1., 1., 1., preset)
        for storage in [pixels, scratch]:
            if any(storage[i] != sentry or storage[guard + capacity + i] != sentry for i in range(guard)):
                raise AssertionError('native consumer crossed owned buffer guards')
        corrected = list(pixels)[guard:guard + len(source)]
        if not all(math.isfinite(v) for v in corrected):
            raise AssertionError('nonfinite native samples')
        rows.append({'preset': preset, 'coefficient': coefficient, 'window': window, 'corrected': corrected})
    result = {'reference': 'Installed Merrill color column consumer, guarded identical synthetic input. Independent native samples, not photographic application exports.', 'native_sha256': NATIVE_SHA256, 'harness_sha256': digest(__file__), 'input_fixture_sha256': digest(args.input), 'input_samples_sha256': hashlib.sha256(struct.pack(f'<{len(source)}f', *source)).hexdigest(), 'width': width, 'height': height, 'start': fixture['start'], 'end': fixture['end'], 'input': source, 'positions': rows}
    with args.output.open('x') as stream:
        json.dump(result, stream, separators=(',', ':'))
        stream.write('\n')
    print('five guarded native color presets retained:', args.output)


if __name__ == '__main__':
    main()

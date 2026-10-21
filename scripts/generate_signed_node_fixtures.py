"""Generate bounded native fixtures for signed chroma and direct root/detail operations."""

import argparse
import ctypes as C
import hashlib
import json
import math
from pathlib import Path

PIN = '99d6d9360f605f0eb83188e8d2acb81cb97d6866df5811ffd8cad9cefb0cb8e1'


class Image(C.Structure):
    _fields_ = [('vtable', C.c_void_p), ('data', C.c_void_p),
                ('width', C.c_uint32), ('height', C.c_uint32),
                ('stride', C.c_uint32), ('padding', C.c_uint32)]


class Size(C.Structure):
    _fields_ = [('width', C.c_int32), ('height', C.c_int32)]


class Color(C.Structure):
    _fields_ = [('red', C.c_double), ('green', C.c_double), ('blue', C.c_double)]


class Lookup(C.Structure):
    _fields_ = [('red', C.POINTER(C.c_int16)), ('green', C.POINTER(C.c_int16)),
                ('blue', C.POINTER(C.c_int16)), ('mask', C.c_uint32), ('size', C.c_uint32)]


class MonoLookup(C.Structure):
    _fields_ = [('data', C.c_void_p), ('mask', C.c_uint32), ('size', C.c_uint32)]


def chroma(library):
    pointer = C.POINTER(C.c_int16)
    lut = getattr(library, '_ZN15FPixelProc2_F1310GetTanhLUTERK11FColorFloatS2_')
    lut.argtypes = [C.POINTER(Color), C.POINTER(Color)]
    lut.restype = C.POINTER(Lookup)
    width, height = 17, 11
    values = [(i * 73) % 601 - 200 for i in range(width * height * 3)]
    cases = []
    for small in (True, False):
        amplitude, slope, shift, setting = [12., 17., 23.], [.8] * 3, 3, 4
        table = lut(C.byref(Color(*amplitude)), C.byref(Color(*slope)))
        name = '18ChromaBlurRowSmall' if small else '13ChromaBlurRow'
        fn = getattr(library, '_ZN15FPixelProc2_F13' + name + 'EPK17FPixelClass48SRGBPS0_11FColorFloatjPS3_jPKNS_17FCLUTTemplate_F13IsEEt')
        fn.argtypes = [pointer, pointer, Color, C.c_uint32, C.POINTER(pointer),
                       C.c_uint32, C.POINTER(Lookup), C.c_uint16]
        fn.restype = None
        pixels = (C.c_int16 * len(values))(*values)
        radius = 1 if small else 2
        backing = [(C.c_int16 * (width * 3))() for _ in range(2 * radius + 1)]
        ring = (pointer * len(backing))(*(C.cast(row, pointer) for row in backing))
        for y in range(height + radius):
            if y == radius:
                for j in range(radius):
                    C.memmove(ring[j], ring[radius], width * 6)
            src = C.cast(C.byref(pixels, min(y, height - 1) * width * 6), pointer)
            dst = C.cast(C.byref(pixels, (y - radius) * width * 6), pointer) if y >= radius else None
            fn(src, dst, Color(*amplitude), setting, ring, width, table, shift)
        cases.append(dict(support='binomial3' if small else 'box5', amplitude=amplitude,
                          slope=slope, setting=setting, shift=shift, expected=list(pixels)))
    return dict(width=width, height=height, input=values, cases=cases)


def root_detail(library, angle=0):
    width, height, crop = 17, 13, [2, 2, 15, 11]
    ow, oh = crop[2] - crop[0], crop[3] - crop[1]
    if angle in (90, 270):
        ow, oh = oh, ow
    values = [(i * 1709) % 24500 - 500 for i in range(width * height * 3)]
    source = (C.c_int16 * len(values))(*values)
    source_image = Image(None, C.addressof(source), width, height, width * 6, 0)
    root = [int((i / 4096 + .008000000000000002) ** (1 / 3) * 4096 + .5) for i in range(65536)]
    for signed in range(-4096, 0):
        v = max(0., math.fma(signed / 4096, 8.333333333333332, .2) * 4096 + 819.2) * .5
        root[signed & 65535] = int(v * v / 819.2 + .5)
    table = (C.c_int16 * len(root))(*root)
    lookup = MonoLookup(C.addressof(table), 65535, 65536)
    rgb, mono = (C.c_int16 * (ow * oh * 3))(), (C.c_int16 * (ow * oh))()
    destination = Image(None, C.addressof(rgb), ow, oh, ow * 6, 0)
    mono_image = Image(None, C.addressof(mono), ow, oh, ow * 2, 0)
    fn = getattr(library, '_ZN15FPixelProc2_F1318CropSizeRotateMoreEPK19FPixmapTemplate_F13I17FPixelClass48SRGBE5FRect5FSizeS6_S5_jPKNS_19FPixmapTemplate_F13IsEEPS2_PS0_I18FPixelClass16SMonoEdii')
    fn.argtypes = [C.c_void_p, C.c_void_p, Size, Size, C.c_void_p, C.c_uint,
                   C.c_void_p, C.c_void_p, C.c_void_p, C.c_double, C.c_int, C.c_int]
    fn.restype = C.c_double
    scale = fn(C.byref(source_image), (C.c_int * 4)(*crop), Size(width, height), Size(ow, oh),
               (C.c_int * 4)(0, 0, ow, oh), angle, C.byref(lookup), C.byref(destination),
               C.byref(mono_image), .001, 200, -120)
    unfiltered = list(mono)
    hp = getattr(library, '_ZN15FPixelProc2_F1316HiPassFilterMonoEP19FPixmapTemplate_F13I18FPixelClass16SMonoEd')
    hp.argtypes, hp.restype = [C.c_void_p, C.c_double], None
    hp(C.byref(mono_image), scale)
    return dict(width=width, height=height, crop=crop, scale=scale, angle=angle, input=values,
                rgb=list(rgb), mono=unfiltered, detail=list(mono))


def root_detail_rotated(library):
    return dict(cases=[root_detail(library, angle) for angle in (0, 90, 180, 270)])


def log_reconstruction(library):
    fn = getattr(library, '_ZN15FPixelProc2_F1320RemoveNoiseFIREdgeC1EP19FPixmapTemplate_F13I17FPixelClass48SRGBES3_ffffffffffPvPcRi')
    fn.argtypes = [C.c_void_p, C.c_void_p] + [C.c_float] * 10 + [C.c_void_p] * 3
    fn.restype = None
    cases = []
    for width, height, controls in [(16, 16, [1.] * 10),
                                    (17, 17, [.1,.2,.15,1.5,.55,5.,7.,.7,1.25,2.]),
                                    (32, 32, [.0011,.0019,.019,1.,.55,5.,7.,.7,.999876856803894,1.]),
                                    (48, 32, [.3,.15,.2,.75,.4,6.,4.,1.2,8.,.5])]:
        filtered = [(i * 127) % 33000 - 1000 if width == 48 else 1000 + (i * 37) % 5000 for i in range(width * height * 3)]
        original = [v + ((i % 7) - 3) * 100 for i, v in enumerate(filtered)]
        src = (C.c_int16 * len(filtered))(*filtered)
        dst = (C.c_int16 * len(original))(*original)
        source = Image(None, C.addressof(src), width, height, width * 6, 0)
        destination = Image(None, C.addressof(dst), width, height, width * 6, 0)
        work = (C.c_float * (144 * max(width, height)))()
        scratch = (C.c_uint32 * (1024 * 1024))()
        available = C.c_int(4 * 1024 * 1024)
        fn(C.byref(destination), C.byref(source), *controls, work, scratch, C.byref(available))
        cases.append(dict(width=width, height=height, controls=controls, original=original,
                          filtered=filtered, expected=list(dst)))
    return dict(cases=cases)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--library', type=Path, required=True)
    parser.add_argument('--kind', choices=['chroma', 'root-detail', 'root-detail-rotated', 'log-reconstruction'], required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if hashlib.sha256(args.library.read_bytes()).hexdigest() != PIN:
        parser.error('Reference implementation library identity does not match')
    if args.output.exists():
        parser.error('Output must be a new file')
    result = {'chroma': chroma, 'root-detail': root_detail, 'root-detail-rotated': root_detail_rotated, 'log-reconstruction': log_reconstruction}[args.kind](C.CDLL(str(args.library)))
    result['provenance'] = dict(library_sha256=PIN, generator_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                                scope='Bounded native signed row composition or direct crop and high-pass. Separate initialized buffers, finite controls, no complete camera pipeline parity claim.')
    args.output.write_text(json.dumps(result, separators=(',', ':')) + '\n')


if __name__ == '__main__':
    main()

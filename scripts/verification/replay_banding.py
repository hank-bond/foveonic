"""Replay all five native Merrill banding positions on identical inputs and retain consumer/output evidence."""

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import time

import numpy as np

from noise_controls import COLUMN_BYTES, NATIVE, NATIVE_SHA256, digest

ROOT = Path(__file__).resolve().parents[2]


def differences(path, baseline, dtype):
    left = np.memmap(path, dtype=dtype, mode='r')
    right = np.memmap(baseline, dtype=dtype, mode='r')
    if left.shape != right.shape:
        raise AssertionError('native result extent differs')
    maximum, total, changed = 0., 0., 0
    for start in range(0, left.size, 1024 * 1024):
        delta = left[start:start + 1024 * 1024].astype(np.float64) - right[start:start + 1024 * 1024]
        changed += int(np.count_nonzero(delta))
        maximum = max(maximum, float(np.max(np.abs(delta), initial=0.)))
        total += float(np.abs(delta).sum())
    return {'samples': int(left.size), 'changed_samples': changed, 'mean_absolute_difference': total / left.size, 'maximum_absolute_difference': maximum}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cases', type=Path, required=True, help='JSON list of explicit original CAMF/identical preprocessing inputs and capture rationals')
    parser.add_argument('--output', type=Path, required=True, help='new evidence directory')
    parser.add_argument('--native', type=Path, default=NATIVE)
    args = parser.parse_args()
    cases = json.loads(args.cases.read_text())
    if digest(args.native) != NATIVE_SHA256:
        raise ValueError('native library identity changed')
    if not cases or len({c['name'] for c in cases}) != len(cases):
        raise ValueError('case names must be distinct')
    args.output.mkdir(parents=True, exist_ok=False)
    output = args.output.resolve()
    worker = output / 'reconstruction'
    report = {'status': 'running', 'scope': 'Installed native Stage 2 replay, all five positions, identical input per case. This is consumer/pixel evidence, not original decoding, preprocessing variation, final TIFF encoding or application exports.', 'native_sha256': digest(args.native), 'harness_sha256': digest(__file__), 'tracer_sha256': digest(ROOT / 'scripts/verification/trace_banding.py'), 'worker_source_sha256': digest(ROOT / 'scripts/verification/reconstruction.cpp'), 'case_spec_sha256': digest(args.cases), 'cases': []}
    report_path = output / 'summary.json'
    def flush():
        report_path.write_text(json.dumps(report, indent=2) + '\n')
    flush()
    try:
        with (output / 'build.log').open('x') as log:
            subprocess.run(['xcrun', 'clang++', '-std=c++17', '-O2', '-g', str(ROOT / 'scripts/verification/reconstruction.cpp'), '-o', str(worker)], check=True, stdout=log, stderr=subprocess.STDOUT)
        report['worker_sha256'] = digest(worker)
        for case in cases:
            if any(v in case['name'] for v in ['/', '..']):
                raise ValueError('unsafe case label')
            source = Path(case['source']).resolve()
            camf = Path(case['camf']).resolve()
            pixels = Path(case['input']).resolve()
            if pixels.stat().st_size != 4928 * 3264 * 12:
                raise ValueError('full sensor input required')
            input_hash = digest(pixels)
            study = {'request': case, 'source_sha256': digest(source), 'camf_sha256': digest(camf), 'input_sha256': input_hash, 'positions': []}
            report['cases'].append(study)
            baseline = None
            for preset in [2, 0, 1, 3, 4]:
                directory = output / f"{case['name']}-{preset}"
                directory.mkdir()
                if shutil.disk_usage(directory).free < (10.25 * 1024**3 + 512 * 1024**2):
                    raise RuntimeError('disk reserve')
                env = os.environ.copy()
                for name in ['FOVEONIC_MONOCHROME', 'FOVEONIC_NOISE_RESOLUTION', 'FOVEONIC_NOISE_CHROMA', 'FOVEONIC_NOISE_LUMINANCE', 'FOVEONIC_NOISE_BANDING']:
                    env.pop(name, None)
                env.update(FOVEONIC_BANDING_CAPTURE=str(directory), FOVEONIC_NOISE_BANDING=str(preset / 4), FOVEONIC_NOISE_RESOLUTION=case.get('resolution', 'HI'), FOVEONIC_NOISE_CHROMA=str(case.get('chroma', .5)), FOVEONIC_NOISE_LUMINANCE=str(case.get('luminance', .5)))
                if case.get('monochrome', False):
                    env['FOVEONIC_MONOCHROME'] = '1'
                command = ['/usr/bin/lldb', '--batch', '-o', f'command script import {ROOT}/scripts/verification/trace_banding.py', '-o', 'run', '-k', 'bt all', '--', str(worker), str(args.native), str(camf), str(pixels), str(directory), *map(str, case['aperture']), *map(str, case['shutter'])]
                (directory / 'request.json').write_text(json.dumps({'command': command, 'environment': {k: v for k, v in env.items() if k.startswith('FOVEONIC_')}, 'input_sha256': input_hash}, indent=2) + '\n')
                started = time.monotonic()
                with (directory / 'lldb.log').open('x') as log:
                    subprocess.run(command, cwd=ROOT, env=env, check=True, stdout=log, stderr=subprocess.STDOUT, timeout=600)
                capture = json.loads((directory / 'capture.json').read_text())
                if capture['status'] != 'completed':
                    raise AssertionError((case['name'], preset, capture))
                effective = capture['effective']
                calls = sorted(capture['mono_columns'], key=lambda c: c['before']['first']) if case.get('monochrome') else capture['columns']
                if len(calls) != (8 if case.get('monochrome') else 16):
                    raise AssertionError('not every native algorithmic strip/tile was observed')
                windows = sorted(set(c['window'] for c in calls))
                if windows != [effective['gathered_window']]:
                    raise AssertionError(('consumer window differs from effective native settings', windows, effective))
                if not case.get('monochrome') and any(c['floating_arguments']['s1'] != effective['column_blend'] for c in calls):
                    raise AssertionError('color consumer did not use selected coefficient')
                if case.get('monochrome') and any(c['level'] != preset for c in calls):
                    raise AssertionError('mono consumer did not use selected level')
                row = {'preset': preset, 'seconds': time.monotonic() - started, 'capture': capture, 'rgb_sha256': digest(directory / 'detail-rgb.f32'), 'detail_sha256': digest(directory / 'detail-sharp.i16'), 'mode_contrast_sha256': digest(directory / 'mode-contrast.f64')}
                if baseline is None:
                    baseline = (directory, capture)
                else:
                    base_dir, base_capture = baseline
                    if capture['input'] != base_capture['input'] or capture['tiled_input'] != base_capture['tiled_input']:
                        raise AssertionError('preset changed earlier input pixels')
                    before = (directory / 'gathered.bin').read_bytes()
                    original = (base_dir / 'gathered.bin').read_bytes()
                    changes = {i for i, (a, b) in enumerate(zip(before, original)) if a != b}
                    if not changes.issubset(COLUMN_BYTES):
                        raise AssertionError(('gathered record changed outside banding fields', sorted(changes)))
                    row['rgb_difference_from_midpoint'] = differences(directory / 'detail-rgb.f32', base_dir / 'detail-rgb.f32', '<f4')
                    row['detail_difference_from_midpoint'] = differences(directory / 'detail-sharp.i16', base_dir / 'detail-sharp.i16', '<i2')
                    if not case.get('monochrome') and [c['before'] for c in calls] != [c['before'] for c in base_capture['columns']]:
                        raise AssertionError('preset changed raw tiles before column correction')
                study['positions'].append(row)
                flush()
                print(case['name'], preset, windows, round(row['seconds'], 2), 'seconds', flush=True)
            if digest(pixels) != input_hash or digest(source) != study['source_sha256'] or digest(camf) != study['camf_sha256']:
                raise AssertionError('original inputs changed')
        report['status'] = 'passed'
    except Exception as error:
        report.update(status='failed', error=repr(error))
        raise
    finally:
        flush()


if __name__ == '__main__':
    main()

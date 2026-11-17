"""Verify a packaged release reaches real native GUI smoke checkpoints on a desktop."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import struct
import subprocess
import sys
import zlib

ROOT = Path(__file__).resolve().parents[1]
TARGETS = ('preview', 'final', 'list', 'parade', 'waveform', 'zoom',
           'reordered', 'collapsed', 'sensor.layers', 'calibration.layers')


def digest(path):
    checksum = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            checksum.update(chunk)
    return checksum.hexdigest()


def check_png(data):
    """Validate the RGBA8 PNG chunks and scanline stream emitted by native smoke."""
    if data[:8] != b'\x89PNG\r\n\x1a\n':
        raise ValueError('missing PNG signature')
    offset, header, compressed, ended = 8, None, bytearray(), False
    while offset + 12 <= len(data):
        length = struct.unpack('>I', data[offset:offset + 4])[0]
        end = offset + length + 12
        if end > len(data):
            raise ValueError('truncated PNG chunk')
        kind = data[offset + 4:offset + 8]
        payload = data[offset + 8:end - 4]
        if zlib.crc32(data[offset + 4:end - 4]) != struct.unpack('>I', data[end - 4:end])[0]:
            raise ValueError('invalid PNG chunk checksum')
        if header is None:
            if kind != b'IHDR' or length != 13:
                raise ValueError('missing PNG image header')
            header = struct.unpack('>IIBBBBB', payload)
        elif kind == b'IDAT':
            compressed.extend(payload)
        elif kind == b'IEND':
            ended = length == 0 and end == len(data)
            break
        offset = end
    if not ended or not compressed:
        raise ValueError('missing PNG image data or end')
    width, height, depth, color, compression, filtering, interlace = header
    if not width or not height or (depth, color, compression, filtering, interlace) != (8, 6, 0, 0, 0):
        raise ValueError('expected a nonempty RGBA8 native capture')
    size = height * (1 + width * 4)
    if size > 256 * 1024 * 1024:
        raise ValueError('capture exceeds scanline validation limit')
    decoder = zlib.decompressobj()
    samples = decoder.decompress(compressed, size + 1)
    if (len(samples) != size or not decoder.eof or decoder.unused_data or decoder.unconsumed_tail
            or any(samples[row] > 4 for row in range(0, size, 1 + width * 4))):
        raise ValueError('invalid PNG scanline stream')


def captures(directory):
    for target in TARGETS:
        path = directory / (target + '.png')
        try:
            check_png(path.read_bytes())
        except (ValueError, zlib.error) as error:
            raise ValueError('invalid capture ' + path.name + ': ' + str(error)) from error
    return list(TARGETS)


def verify(binary, source, directory, timeout):
    binary = Path(binary).resolve(strict=True)
    source = Path(source).resolve(strict=True)
    directory = Path(directory).resolve()
    hashes = json.loads((ROOT / 'scripts/profiling/test_inputs.json').read_text())['sample_hashes']
    if source.name != 'blue-parking-sign.x3f' or digest(source) != hashes[source.name]:
        raise ValueError('requires the pinned blue-parking-sign.x3f example photograph')
    directory.mkdir(parents=True, exist_ok=False)
    inputs = directory / 'inputs'
    inputs.mkdir()
    staged = inputs / source.name
    shutil.copy2(source, staged)
    if digest(staged) != hashes[source.name]:
        raise ValueError('staged photograph does not match the pinned input')
    evidence = directory / 'evidence'
    evidence.mkdir()
    # Inherited review flags must not select another scenario or enlarge worker stacks.
    env = {name: value for name, value in os.environ.items()
           if not name.startswith('FOVEONIC_') and name != 'RUST_MIN_STACK'}
    env.update(FOVEONIC_GUI_SMOKE_DIR=str(evidence), FOVEONIC_GUI_SMOKE_DISK_CACHE_BYTES='0')
    report = {'outcome': 'failed', 'scenario': 'smoke-workspace',
              'platform': platform.platform(), 'architecture': platform.machine(),
              'binary_sha256': digest(binary), 'source_sha256': digest(staged),
              'exit_code': None, 'checkpoints': [], 'timeout_seconds': timeout}
    process = None
    try:
        with (directory / 'stdout.log').open('wb') as out, (directory / 'stderr.log').open('wb') as err:
            process = subprocess.Popen([str(binary), str(staged)], env=env,
                                       stdout=out, stderr=err)
            report['exit_code'] = process.wait(timeout=timeout)
        if report['exit_code'] != 0:
            raise ValueError('native GUI exited with code ' + str(report['exit_code']))
        report['checkpoints'] = captures(evidence)
        report['outcome'] = 'passed'
    except BaseException as error:
        if process is not None and process.poll() is None:
            process.kill()
            process.wait()
        report['failure'] = str(error) or type(error).__name__
        raise
    finally:
        (directory / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--source', type=Path, default=ROOT / '.local/example-photographs/blue-parking-sign.x3f')
    parser.add_argument('--directory', type=Path, required=True)
    parser.add_argument('--timeout', type=int, default=180)
    args = parser.parse_args()
    if args.timeout <= 0:
        parser.error('--timeout must be positive')
    try:
        report = verify(args.binary, args.source, args.directory, args.timeout)
    except (OSError, ValueError, subprocess.TimeoutExpired) as error:
        print(str(error), file=sys.stderr)
        return 1
    print(json.dumps(report, indent=2))
    return 0


if __name__ == '__main__':
    sys.exit(main())

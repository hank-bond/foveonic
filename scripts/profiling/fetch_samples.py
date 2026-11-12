"""Fetch pinned external RAW samples and verify cached originals without replacing them."""

import argparse
import json
import os
from pathlib import Path
import shutil
import tempfile
from urllib.error import URLError
from urllib.request import urlopen

from file_hash import digest

MANIFEST = Path(__file__).with_name('external_raw_samples.json')
DEFAULT_DIRECTORY = Path(__file__).resolve().parents[2] / '.local' / 'example-photographs'


def samples():
    records = json.loads(MANIFEST.read_text())['files']
    names = [record['file'] for record in records]
    if (not names or len(set(names)) != len(names)
            or any(not name or name in ('.', '..') or Path(name).name != name for name in names)):
        raise ValueError('sample manifest filenames must be distinct basenames')
    return records


def verify(path, sample):
    if path.stat().st_size != sample['size_bytes'] or digest(path) != sample['sha256']:
        raise ValueError(f"{path}: does not match the pinned size and SHA-256")


def sync(directory, records, offline=False):
    directory = Path(directory)
    missing = []
    # Check existing inputs before fetching anything. A mismatched local original
    # belongs to the caller and must not be silently repaired or overwritten.
    for sample in records:
        path = directory / sample['file']
        if path.exists() or path.is_symlink():
            verify(path, sample)
        else:
            missing.append(sample)
    if offline and missing:
        raise ValueError('missing external samples: ' + ', '.join(sample['file'] for sample in missing))
    if not missing:
        return 0
    directory.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.download-', dir=directory) as temporary:
        for sample in missing:
            staged = Path(temporary) / sample['file']
            print(f"Downloading {sample['file']}", flush=True)
            with urlopen(sample['url'], timeout=30) as response, staged.open('xb') as output:
                shutil.copyfileobj(response, output, length=1024 * 1024)
            verify(staged, sample)
            try:
                # Same-filesystem publication cannot overwrite another sync's file.
                os.link(staged, directory / sample['file'])
            except FileExistsError:
                verify(directory / sample['file'], sample)
    return len(missing)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--directory', type=Path, default=DEFAULT_DIRECTORY,
                        help='sample directory (default: repository .local/example-photographs)')
    parser.add_argument('--offline', action='store_true', help='verify cached samples without network access')
    args = parser.parse_args()
    try:
        records = samples()
        downloaded = sync(args.directory, records, args.offline)
    except (OSError, ValueError, URLError) as error:
        parser.exit(1, str(error) + '\n')
    print(f'Verified {len(records)} external RAW samples ({downloaded} downloaded) in {args.directory}')


if __name__ == '__main__':
    main()

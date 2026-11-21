"""Resolve tracked test recipes and explicit photo aliases into portable runner inputs."""

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

from file_hash import digest
from rust_test_inventory import MANIFEST, input_hashes, validate_inventory, verify_inputs

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
DEFINITIONS = HERE / 'test_inputs.json'
CASE_FILES = {'render_cases': 'render-cases.json', 'scenario_cases': 'scenario-cases.json',
              'choice_cases': 'choice-cases.json', 'regional_cases': 'regional-cases.json',
              'checkpoint_cases': 'checkpoint-cases.json', 'native_zoom_cases': 'native-zoom-cases.json'}


def prepare(source_dir, directory, definitions, external_samples, *, rust_inventory=None):
    source_dir, directory = Path(source_dir).resolve(), Path(directory).resolve()
    hashes = {sample['file']: sample['sha256'] for sample in external_samples}
    if hashes.keys() & definitions['sample_hashes'].keys():
        raise ValueError('test inputs duplicate an external sample identity')
    hashes.update(definitions['sample_hashes'])
    required_hashes = dict(hashes)
    if rust_inventory is not None:
        validate_inventory(rust_inventory)
        rust_hashes = input_hashes(rust_inventory, definitions, {'files': external_samples})
        for identity, names in rust_inventory['input_sets'].items():
            if not identity or identity in ('.', '..') or Path(identity).name != identity:
                raise ValueError('Rust fixture set names must be basenames')
            required_hashes.update((name, rust_hashes[name]) for name in names)
    aliases = definitions['aliases']
    for name in (*required_hashes, *aliases):
        if not name or name in ('.', '..') or Path(name).name != name:
            raise ValueError('test input names must be basenames')
    if aliases.keys() & hashes.keys() or any(source not in hashes for source in aliases.values()):
        raise ValueError('aliases must name distinct copies of declared originals')
    missing = [name for name in required_hashes if not (source_dir / name).is_file()]
    if missing:
        provided = [] if rust_inventory is None else [name for name in missing
                    if rust_inventory['additional_inputs'].get(name, {}).get('origin', {}).get('status') == 'provided_file']
        raise ValueError('missing declared test originals: ' + ', '.join(missing)
                         + ('; provide the exact inventory-pinned files: ' + ', '.join(provided) if provided else ''))
    for name, checksum in required_hashes.items():
        if digest(source_dir / name) != checksum:
            raise ValueError(f'{source_dir / name}: pinned test input SHA-256 mismatch')
        extra = None if rust_inventory is None else rust_inventory['additional_inputs'].get(name)
        if extra is not None and (source_dir / name).stat().st_size != extra['bytes']:
            raise ValueError(f'{source_dir / name}: pinned test input byte length mismatch')

    def resolve(name):
        if name in aliases:
            return str(directory / 'aliases' / name)
        if name in hashes:
            return str(source_dir / name)
        raise ValueError(f'undeclared test original: {name}')

    outputs = {
        filename: [{**case, 'source': resolve(case['source'])} for case in definitions[group]]
        for group, filename in CASE_FILES.items()
    }
    outputs['fixtures.json'] = [
        {**fixture, 'sources': [resolve(name) for name in fixture['sources']]}
        for fixture in definitions['fixtures']
    ]
    outputs['provenance.json'] = {'source_dir': str(source_dir), 'sample_hashes': hashes,
                                   'aliases': aliases, 'definitions': definitions}
    if rust_inventory is not None:
        outputs['provenance.json']['rust_inventory'] = rust_inventory
    # Generated declarations belong in a new directory, just like runner output.
    # Never replace another run's recipes or alias paths.
    directory.mkdir(parents=True, exist_ok=False)
    (directory / 'aliases').mkdir()
    for name, original in aliases.items():
        os.link(source_dir / original, directory / 'aliases' / name)
    if rust_inventory is not None:
        directories = {identity: directory / 'rust-fixtures' / identity
                       for identity in rust_inventory['input_sets']}
        for identity, destination in directories.items():
            destination.mkdir(parents=True, exist_ok=False)
            for name in rust_inventory['input_sets'][identity]:
                # Stream actual copies. Existing originals and their sidecars stay untouched.
                with (source_dir / name).open('rb') as source, (destination / name).open('xb') as target:
                    shutil.copyfileobj(source, target)
        outputs['rust-fixtures.json'] = verify_inputs(rust_inventory, rust_hashes, directories)
    for name, data in outputs.items():
        (directory / name).write_text(json.dumps(data, indent=2) + '\n')
    return {name: directory / name for name in outputs}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-dir', type=Path, default=ROOT / '.local/example-photographs')
    parser.add_argument('--directory', type=Path, default=ROOT / '.local/test-inputs',
                        help='new directory for resolved recipes and aliases')
    parser.add_argument('--offline', action='store_true', help='use verified cached originals')
    parser.add_argument('--rust-fixtures', action='store_true',
                        help='also copy exact Rust inventory fixture sets from verified cached originals')
    args = parser.parse_args()
    try:
        if args.directory.exists():
            raise ValueError(f'{args.directory}: choose a new generated input directory')
        flags = ['--offline'] if args.offline else []
        subprocess.run(['cargo', 'xtask', 'examples', 'sync', '--source-dir', str(args.source_dir.resolve()), *flags],
                       cwd=ROOT, check=True)
        subprocess.run([sys.executable, str(HERE / 'fetch_samples.py'), '--directory', str(args.source_dir.resolve()), *flags],
                       cwd=ROOT, check=True)
        definitions = json.loads(DEFINITIONS.read_text())
        external = json.loads((HERE / 'external_raw_samples.json').read_text())['files']
        inventory = json.loads(MANIFEST.read_text()) if args.rust_fixtures else None
        outputs = prepare(args.source_dir, args.directory, definitions, external, rust_inventory=inventory)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, str(error) + '\n')
    for name, path in outputs.items():
        print(f'{name}: {path}')


if __name__ == '__main__':
    main()

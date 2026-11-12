"""Stage owned test inputs and recipes without modifying the supplied originals."""

import hashlib
import json
import os
from pathlib import Path

from file_hash import digest


def stage_inputs(cases, directory):
    """Hard links isolate sidecars without copying RAW bytes or modifying caller files."""
    directory.mkdir(parents=True, exist_ok=False)
    inputs = []
    for index, case in enumerate(cases):
        source = Path(case['source'])
        staged = directory / f'{index:02}-{source.name}'
        try:
            os.link(source, staged)
        except OSError as error:
            raise ValueError('scenario inputs must support hard links into the owned run directory') from error
        record = {'source': str(source), 'staged_source': str(staged),
                  'sha256': digest(source), 'length': source.stat().st_size,
                  'modified_ns': source.stat().st_mtime_ns, 'label': case.get('label', source.name)}
        if 'settings' in case:
            # This is the maintained v1 sidecar format, validated by the actual app.
            sidecar = directory / 'foveonic' / (staged.name + '.foveonic.json')
            sidecar.parent.mkdir(exist_ok=True)
            payload = json.dumps({'format':'foveonic-editing','version':1,'settings':case['settings']}, indent=2).encode()
            if len(payload) > 64 * 1024:
                raise ValueError('declared scenario recipe exceeds the application sidecar limit')
            sidecar.write_bytes(payload)
            record['settings'] = case['settings']
            record['sidecar_sha256'] = hashlib.sha256(payload).hexdigest()
        else:
            record['settings'] = 'camera-defaults'
        inputs.append(record)
    return inputs

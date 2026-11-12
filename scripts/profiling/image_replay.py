"""Validate captured image identity and compare its pixels with a fresh renderer execution."""

import json
from pathlib import Path
import subprocess

from results import IMAGE_GEOMETRY_FIELDS, compare_images
from watch_workloads import require_quiet


def events(stdout):
    found = {}
    for line in stdout.splitlines():
        row = json.loads(line)
        if row['event'] in ('session', 'processing', 'validation'):
            if row['event'] in found:
                raise ValueError(f"duplicate {row['event']} event")
            found[row['event']] = row['data']
    if set(found) != {'session', 'processing', 'validation'}:
        raise ValueError('render did not report its settings, image scale and validation')
    return found


def replay_images(native_result, inputs, photos, final_action, binary, directory):
    """Compare checked final photos with their owned captured images and fresh renders."""
    captures = native_result.get('image_captures', [])
    declared = {item['staged_source']:item for item in inputs}
    if len({capture['source'] for capture in captures}) != len(inputs):
        raise ValueError('captured photo identities are missing or repeated')
    assertions = []
    for index, capture in enumerate(captures):
        source = capture['source']
        if source not in photos or source not in declared:
            raise ValueError('an image capture references an undeclared photo')
        photo, record = photos[source], declared[source]
        if (capture['task'], capture['revision']) != (photo['task'], photo['revision']):
            raise ValueError('an image capture is not the settled final frame')
        snapshot = capture['settings']
        effective = snapshot['settings']
        if snapshot['working_scale'] != photo['scale_divisor']:
            raise ValueError('captured image settings do not match the checked edited property and scale')
        actual_file = directory/'evidence'/f'image-{index:02}.f32'
        if Path(capture['file']) != actual_file:
            raise ValueError('image capture path does not match its owned evidence file')
        out = directory/f'fresh-{index:02}'
        golden_dir = directory/f'fresh-{index:02}-samples'
        snapshot_path = directory/f'fresh-{index:02}-settings.json'
        snapshot_path.write_text(json.dumps(snapshot, indent=2)+'\n')
        require_quiet()
        command = [str(binary), source, str(out), '--warm', '0',
                   '--settings-snapshot', str(snapshot_path), '--goldens', str(golden_dir)]
        fresh = subprocess.run(command, capture_output=True, text=True)
        (directory/f'fresh-{index:02}.jsonl').write_text(fresh.stdout)
        (directory/f'fresh-{index:02}.stderr').write_text(fresh.stderr)
        if fresh.returncode:
            raise ValueError(f'fresh render {index} exited with status {fresh.returncode}; inspect stderr')
        data = events(fresh.stdout)
        if (data['session']['settings'] != effective
                or data['processing']['effective_scale_divisor'] != photo['scale_divisor']
                or data['session']['adapter'] != native_result['hardware']['adapter']):
            raise ValueError('fresh render used different settings, scale or GPU adapter')
        expected_file = golden_dir/(Path(source).stem+'.f32')
        expected_geometry = {key:data['validation'][key] for key in IMAGE_GEOMETRY_FIELDS}
        actual_geometry = {key:capture[key] for key in IMAGE_GEOMETRY_FIELDS}
        assertions.append(compare_images(record['source'], final_action, expected_file, expected_geometry,
                                         actual_file, actual_geometry))
    return assertions

"""Export multi-photo host node spans to a Chrome trace without inventing GPU start times."""

import argparse
import json
from pathlib import Path

from summarize import parse_record


def timeline(records):
    rows = [r for record in records if (r := parse_record(record)) is not None]
    group_rows = [r for r in rows if r['event'] == 'editing_group']
    groups = {(r['mode'], r['round']): r for r in group_rows}
    if len(groups) != len(group_rows):
        raise ValueError('timeline requires one capture, not concatenated trials')
    modes = {r['mode'] for r in rows if r['event'] == 'editing_photo'}
    if len(modes) != 1:
        raise ValueError('timeline requires one profiling mode per capture')
    events = []
    offset_us = 0.
    for (mode, round_index), group in sorted(groups.items()):
        photos = [r for r in rows if r['event'] == 'editing_photo' and r['mode'] == mode and r['round'] == round_index]
        if len(photos) != group['photo_count'] or len({p['photo_index'] for p in photos}) != len(photos):
            raise ValueError('incomplete photo group')
        events.append({'name': 'edit group', 'ph': 'X', 'pid': 1, 'tid': 0,
                       'ts': offset_us, 'dur': group['completed_ms'] * 1000.,
                       'args': {k: group[k] for k in ['round', 'scale_divisor', 'workers', 'edit', 'photo_count']}})
        for photo in photos:
            thread = photo['photo_index'] + 1
            events.append({'name': photo['source'], 'ph': 'X', 'pid': 1, 'tid': thread,
                           'ts': offset_us + photo['started_ms'] * 1000.,
                           'dur': photo['completed_ms'] * 1000.,
                           'args': {'extent': photo['extent'], 'requested_scale_divisor': photo['requested_scale_divisor'], 'effective_scale_divisor': photo['effective_scale_divisor']}})
            for node in photo['profile'].get('nodes', []):
                if not node['executed'] or node.get('total_ms') is None:
                    continue
                events.append({'name': node['operation'], 'ph': 'X', 'pid': 1, 'tid': thread,
                               'ts': offset_us + (photo['profile_started_ms'] + node['started_ms']) * 1000.,
                               'dur': node['total_ms'] * 1000.,
                               'args': {k: node[k] for k in ['ordinal', 'host_milliseconds', 'publication_ms', 'completion_wait_ms', 'retirement_ms']}})
        # Separate relative group clocks with a visible gap. This gap is synthetic.
        offset_us += group['completed_ms'] * 1000. + 1000.
    if not events:
        raise ValueError('no completed editing groups')
    return {'traceEvents': events, 'displayTimeUnit': 'ms',
            'otherData': {'clock': 'host elapsed time', 'group_gap_ms': 1,
                          'note': 'Groups have independent clocks. GPU pass timestamps have no shared host origin and are not plotted.'}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('records', type=Path)
    parser.add_argument('destination', type=Path)
    args = parser.parse_args()
    records = [json.loads(line) for line in args.records.read_text().splitlines() if line.strip()]
    result = timeline(records)
    with args.destination.open('x') as file:
        json.dump(result, file)


if __name__ == '__main__':
    main()

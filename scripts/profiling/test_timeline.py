"""Check multi-photo timeline clocks, group completeness and missing diagnostic data."""

import unittest
from timeline import timeline


def record(event, data, mode='wall'):
    return {'foveonic_profile': 1, 'event': event, 'mode': mode, 'data': data}


def fixture(round_index=0, mode='wall'):
    node = {'executed': True, 'operation': 'Tone', 'ordinal': 4, 'started_ms': 2.,
            'host_milliseconds': .2, 'publication_ms': .1, 'completion_wait_ms': .5,
            'retirement_ms': .1, 'total_ms': 1.}
    return [record('editing_photo', {'round': round_index, 'photo_index': 0,
                'source': 'photo.x3f', 'started_ms': 1., 'completed_ms': 10.,
                'profile_started_ms': 1.5, 'extent': [8, 8], 'effective_quality': 'Thumb',
                'profile': {'nodes': [node, {**node, 'executed': False}]}}, mode),
            record('editing_group', {'round': round_index, 'photo_count': 1,
                'completed_ms': 12., 'workers': 1, 'quality': 'Thumb', 'edit': 'exposure'}, mode)]


class TimelineTests(unittest.TestCase):
    def test_profile_origin_and_group_clock_are_kept_separate(self):
        events = timeline(fixture() + fixture(1))['traceEvents']
        nodes = [e for e in events if e['name'] == 'Tone']
        self.assertEqual(len(nodes), 2)
        self.assertEqual(nodes[0]['ts'], 3500.)
        self.assertEqual(nodes[0]['dur'], 1000.)
        self.assertEqual(nodes[1]['ts'], 16500.)
        self.assertEqual(nodes[0]['args']['completion_wait_ms'], .5)

    def test_off_still_shows_photo_and_group_spans(self):
        records = fixture(mode='off')
        records[0]['data']['profile'] = {}
        self.assertEqual(len(timeline(records)['traceEvents']), 2)

    def test_incomplete_and_mixed_captures_are_rejected(self):
        with self.assertRaises(ValueError):
            timeline(fixture()[1:])
        with self.assertRaises(ValueError):
            timeline(fixture() + fixture(1, mode='gpu'))
        with self.assertRaises(ValueError):
            timeline(fixture() + fixture())


if __name__ == '__main__':
    unittest.main()

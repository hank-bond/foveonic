"""Check that interruption evidence cannot hide obsolete delivery or changed peer samples."""

import hashlib
from pathlib import Path
import tempfile
import unittest

from wide_stress import check_interruption


class WideInterruptionTests(unittest.TestCase):
    def fixture(self, root, injection='replace'):
        geometry = {'view': [0, 0, 1, 1]}
        snapshot = {'settings': {'exposure': 0.25}, 'working_scale': 8}
        expected = []
        for index in range(3):
            data = bytes([index]) * 12
            (root / f'{index}.f32').write_bytes(data)
            expected.append({'sha256': hashlib.sha256(data).hexdigest(), 'bytes': len(data),
                             'geometry': geometry, 'settings': snapshot})
        report = {'injection': injection, 'injection_peer_submitted': True,
                  'injected_ms': 1., 'group_fence_ms': 2., 'replacement_step_observed_ms': 3.,
                  'replacement_ms': 4., 'replacement_exact': True,
                  'geometry': [geometry] * 3, 'final_settings_snapshots': [snapshot] * 3,
                  'completion_ms': [(1, 2.5), (0, 4.), (2, 6.)],
                  'probe': {'unfinished_submitted_peers_at_interruption': [(0, 1)]}}
        return report, expected

    def test_already_encoded_peer_can_finish_before_replacement_but_remaining_set_cannot_block_it(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            report, expected = self.fixture(root)
            checked = check_interruption(report, root, expected)
            self.assertEqual(checked['peer_outputs_exact'], 2)
            report['replacement_ms'] = 7.
            with self.assertRaisesRegex(ValueError, 'after all remaining peers'):
                check_interruption(report, root, expected)

    def test_peer_corruption_and_absent_live_peer_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            report, expected = self.fixture(root)
            report['probe']['unfinished_submitted_peers_at_interruption'] = [(0, 0)]
            with self.assertRaisesRegex(ValueError, 'unfinished peer'):
                check_interruption(report, root, expected)
            report['probe']['unfinished_submitted_peers_at_interruption'] = [(0, 1)]
            (root / '1.f32').write_bytes(b'changed peer')
            with self.assertRaisesRegex(ValueError, 'samples, geometry or effective settings'):
                check_interruption(report, root, expected)

    def test_cancelled_photo_cannot_be_delivered(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            report, expected = self.fixture(root, 'cancel')
            report['outcomes'] = ['render superseded', 'complete', 'complete']
            report['completion_ms'] = [(1, 3.), (2, 4.)]
            with self.assertRaisesRegex(ValueError, 'obsolete photo retained'):
                check_interruption(report, root, expected)
            (root / '0.f32').unlink()
            self.assertEqual(check_interruption(report, root, expected)['peer_outputs_exact'], 2)


if __name__ == '__main__':
    unittest.main()

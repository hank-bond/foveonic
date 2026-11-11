"""Check shared preview assertions independently of either test driver."""

import unittest

from results import expected_preview_scale, render_result


class ResultTests(unittest.TestCase):
    def test_preview_policy_covers_supported_families_and_qualities(self):
        for family, thumb in [('Classic', 4), ('Merrill', 8), ('Quattro', 8)]:
            for quality, scale in [('full', 1), ('fast', 2), ('draft', 4), ('thumb', thumb)]:
                self.assertEqual(expected_preview_scale(quality, family), scale)

    def test_drivers_share_the_same_assertion_and_detect_wrong_scale(self):
        observation = {'step':'render', 'source':'photo.x3f', 'family':'Classic', 'scale_divisor':8}
        results = [render_result(driver, 'test', 'thumb', [observation], True)
                   for driver in ('headless', 'application')]
        for result in results:
            self.assertEqual(result['outcome'], 'failed')
            self.assertEqual(result['assertions'][0]['expected'], 4)
            self.assertEqual(result['assertions'][0]['actual'], 8)
            self.assertEqual(result['assertions'][0]['outcome'], 'failed')
        self.assertEqual(results[0]['assertions'], results[1]['assertions'])

    def test_missing_evidence_is_not_counted_as_a_pass(self):
        unknown = {'step':'render', 'source':'photo.x3f', 'family':None, 'scale_divisor':8}
        for observations in ([], [unknown], [{**unknown, 'family':'Merrill', 'scale_divisor':None}]):
            self.assertEqual(render_result('application', 'test', 'thumb', observations, True)['outcome'], 'incomplete')
        self.assertEqual(render_result('application', 'test', 'thumb', [], False, 'failed to load')['outcome'], 'failed')
        self.assertEqual(render_result('application', 'test', 'thumb', [], False)['failure'], 'the test driver failed')
        with self.assertRaisesRegex(ValueError, 'unknown preview quality'):
            render_result('application', 'test', 'unknown', [], True)


if __name__ == '__main__':
    unittest.main()

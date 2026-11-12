"""Check shared preview assertions independently of either test driver."""

import unittest
import tempfile
from pathlib import Path

from results import compare_images, expected_preview_scale, include_assertions, render_result


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

    def test_image_assertion_checks_samples_geometry_and_complete_storage(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            expected, actual = root/'expected.f32', root/'actual.f32'
            expected.write_bytes(bytes(range(24)))
            actual.write_bytes(expected.read_bytes())
            geometry = {'extent':[2,1], 'layout':'Rgb32Float', 'view_crop':[0,0,2,1],
                        'space':'test', 'linear_white':1, 'view_baked':False, 'view_extent':[2,1],
                        'view_rotation':0, 'endianness':'little'}
            def compare(actual_geometry=geometry):
                return compare_images('photo.x3f', 'edit', expected, geometry, actual, actual_geometry)
            self.assertEqual(compare()['outcome'], 'passed')
            incomplete = {key:value for key,value in geometry.items() if key != 'space'}
            self.assertEqual(compare_images('photo.x3f','edit',expected,incomplete,actual,incomplete)['outcome'],'failed')
            actual.write_bytes(bytes(range(23))+b'\xff')
            mismatch = compare()
            self.assertEqual(mismatch['outcome'], 'failed')
            self.assertEqual(mismatch['difference']['first_byte'], 23)
            actual.write_bytes(expected.read_bytes())
            self.assertEqual(compare({**geometry, 'view_crop':[0,0,1,1]})['outcome'], 'failed')
            expected.write_bytes(b'short'); actual.write_bytes(b'short')
            self.assertEqual(compare()['outcome'], 'failed')

    def test_added_assertion_fails_the_shared_result_without_hiding_prior_failure(self):
        observation = {'step':'render', 'source':'photo.x3f', 'family':'Merrill', 'scale_divisor':8}
        failed = {'name':'image_samples', 'outcome':'failed'}
        result = render_result('application', 'edit', 'thumb', [observation], True)
        include_assertions(result, [failed])
        self.assertEqual(result['outcome'], 'failed')
        self.assertEqual(result['failure'], 'image_samples assertion failed')
        result = render_result('application', 'edit', 'thumb', [], False, 'driver error')
        include_assertions(result, [failed])
        self.assertEqual(result['failure'], 'driver error')


if __name__ == '__main__':
    unittest.main()

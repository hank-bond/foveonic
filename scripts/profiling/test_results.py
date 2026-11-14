"""Check shared preview assertions independently of either test driver."""

import unittest
import tempfile
from pathlib import Path

from results import compare_images, expected_resolution_scale, include_assertions, render_result, expected_display_scale, observation_density


class ResultTests(unittest.TestCase):
    def test_preview_policy_covers_supported_families_and_qualities(self):
        for family, thumb in [('Classic', 4), ('Merrill', 8), ('Quattro', 8)]:
            for resolution, scale in [('full', 1), ('fast', 2), ('draft', 4), ('thumb', thumb)]:
                self.assertEqual(expected_resolution_scale(resolution, family), scale)

    def test_drivers_share_the_same_assertion_and_detect_wrong_scale(self):
        observation = {'step':'render', 'source':'photo.x3f', 'family':'Classic', 'scale_divisor':8}
        results = [render_result(driver, 'test', 'quality', [observation], True, resolution='thumb')
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
            self.assertEqual(render_result('application', 'test', 'quality', observations, True)['outcome'], 'incomplete')
        self.assertEqual(render_result('application', 'test', 'quality', [], False, 'failed to load')['outcome'], 'failed')
        self.assertEqual(render_result('application', 'test', 'quality', [], False)['failure'], 'the test driver failed')
        with self.assertRaisesRegex(ValueError, 'unknown processing quality'):
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
        observation = {'step':'render', 'source':'photo.x3f', 'family':'Merrill', 'processing_quality':'quality', 'display_density':0.1, 'scale_divisor':8}
        failed = {'name':'image_samples', 'outcome':'failed'}
        result = render_result('application', 'edit', 'quality', [observation], True)
        include_assertions(result, [failed])
        self.assertEqual(result['outcome'], 'failed')
        self.assertEqual(result['failure'], 'image_samples assertion failed')
        result = render_result('application', 'edit', 'quality', [], False, 'driver error')
        include_assertions(result, [failed])
        self.assertEqual(result['failure'], 'driver error')


class DisplayDemandTests(unittest.TestCase):
    def test_quality_and_speed_thresholds_are_independent_of_the_application_resolver(self):
        for mode,limit in [('quality',1.),('speed',1.5)]:
            for divisor in (8,4,2):
                self.assertEqual(expected_display_scale(mode,'Merrill',limit/divisor),divisor)
                self.assertEqual(expected_display_scale(mode,'Merrill',limit/divisor+0.0001),divisor//2)
        self.assertEqual(expected_display_scale('quality','Classic',0.01),4)
        self.assertIsNone(expected_display_scale('quality','Merrill',None))

    def test_raw_geometry_drives_density_and_same_policy_finer_images_remain_valid(self):
        observation={'step':'render','source':'a.x3f','family':'Merrill','processing_quality':'quality',
                     'display_size_points':[400,200],'native_extent':[4000,2000],'pixels_per_point':2.,
                     'display_density':0.01,'scale_divisor':2}
        self.assertEqual(observation_density(observation),0.2)
        result=render_result('application','initial','quality',[observation],True)
        self.assertEqual(result['assertions'][0]['expected'],4)
        self.assertEqual(result['outcome'],'passed')
        observation['processing_quality']='speed'
        self.assertEqual(render_result('application','initial','quality',[observation],True)['outcome'],'failed')
        observation['processing_quality']='quality'; observation['scale_divisor']=8
        self.assertEqual(render_result('application','initial','quality',[observation],True)['outcome'],'failed')

    def test_retired_mode_names_are_rejected(self):
        for mode in ('full','fast','draft','thumb','smart'):
            with self.assertRaises(ValueError):render_result('application','initial',mode,[],True)


if __name__ == '__main__':
    unittest.main()

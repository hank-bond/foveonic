"""Check that the comparison report separates warm renders from diagnostics and exports."""
import importlib.util
import json
import tempfile
from pathlib import Path
import unittest
from unittest import mock

SPEC = importlib.util.spec_from_file_location('color_field_report', Path(__file__).with_name('color_field_report.py'))
REPORT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(REPORT)


class SummaryTests(unittest.TestCase):
    def sample(self, elapsed, iteration, mode='wall'):
        return dict(event='render', scene='scene', variant='color-field', completed_ms=elapsed,
                    first=iteration == 0, iteration=iteration, mode=mode,
                    profile=dict(gpu=[dict(stage='solve', milliseconds=3.0)]),
                    settings=dict(exposure=-2), activity={}, sidecar_used=True)

    def test_warm_median_excludes_first_gpu_and_export(self):
        records = [self.sample(900, 0), self.sample(200, 1), self.sample(220, 2),
                   self.sample(800, 3, 'gpu'), dict(event='export', completed_ms=9999)]
        result = REPORT.summarize(records)['scene', 'color-field']
        self.assertEqual(result['first_ms'], 900)
        self.assertEqual(result['warm_median_ms'], 210)
        self.assertEqual(result['warm_samples'], 2)
        self.assertEqual(result['gpu_stages'], {'solve': 3.0})

    def test_first_only_does_not_invent_warm_samples(self):
        result = REPORT.summarize([self.sample(100, 0)])['scene', 'color-field']
        self.assertIsNone(result['warm_median_ms'])
        self.assertEqual(result['warm_samples'], 0)

    def test_errors_are_not_successful_renders(self):
        self.assertEqual(REPORT.summarize([dict(event='error', scene='scene')]), {})


class RegionTests(unittest.TestCase):
    def test_fifteen_registered_regions_have_valid_native_bounds(self):
        self.assertEqual(len(REPORT.PR_REGIONS['scenes']),11)
        self.assertEqual(sum(len(s['regions']) for s in REPORT.PR_REGIONS['scenes'].values()),15)
        for scene,entry in REPORT.PR_REGIONS['scenes'].items():
            self.assertEqual(len(REPORT.region_boxes(scene,entry['dimensions'])),len(entry['regions']))

    def test_no_arbitrary_fallback_crop(self):
        self.assertEqual(REPORT.region_boxes('_P2M0399',[4704,3136]),[])

    def test_dimensions_must_match_registration(self):
        with self.assertRaisesRegex(ValueError,'native oriented dimensions'):
            REPORT.region_boxes('_P2M0313',[4704,3136])
        regions=REPORT.region_boxes('_P2M0313',[3136,4704])
        self.assertEqual([r['label'] for r in regions],['Faces and skin','Pink swaddle'])


@unittest.skipUnless(importlib.util.find_spec('PIL'), 'install report Pillow dependencies')
class AssetTests(unittest.TestCase):
    def tiff(self, path, size=(32, 24)):
        from PIL import Image, ImageCms
        path.parent.mkdir(parents=True, exist_ok=True)
        Image.new('RGB', size, (140, 90, 70)).save(path, icc_profile=ImageCms.ImageCmsProfile(ImageCms.createProfile('sRGB')).tobytes())

    def dataset(self, root, area=None):
        settings = dict(highlight_mode='color_field', color_field=dict(approach='faithful'))
        if area:
            settings['color_field']['area'] = area
        records = [dict(event='session', adapter='test')]
        for i in range(2):
            records.append(dict(event='render', scene='_S', variant='color-field', completed_ms=10,
                                first=i == 0, iteration=i, mode='wall', profile=dict(gpu=[]),
                                settings=settings, activity={}, sidecar_used=False))
        root.mkdir(parents=True)
        (root / 'measurements.jsonl').write_text(''.join(json.dumps(r)+'\n' for r in records))
        self.tiff(root / '_S' / 'color-field.tiff')

    def test_staging_keeps_native_pixels_profiles_and_records_before_consuming_temporary(self):
        from PIL import Image
        with tempfile.TemporaryDirectory() as temp:
            source = Path(temp) / '_S' / '.staging-coarse-grid.tiff'
            self.tiff(source)
            expected = REPORT.sha256(source)
            result = REPORT.stage_preview(source, '_S', 'coarse-grid')
            self.assertFalse(source.exists())
            self.assertEqual(result['source_tiff_sha256'], expected)
            self.assertEqual(result['preview_bits'], 8)
            metadata = json.loads((source.parent / 'coarse-grid-assets.json').read_text())
            self.assertEqual(metadata, result)
            with Image.open(source.parent / 'images' / result['native_profile']) as image:
                self.assertEqual(image.size, (32, 24))
                self.assertTrue(image.info['icc_profile'])
            self.assertEqual(result['crops'], [])

    def test_staged_native_pixels_can_be_reframed_without_a_tiff_or_metadata_rewrite(self):
        from PIL import Image
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp)/'round'; self.dataset(root)
            source=root/'_S'/'color-field.tiff'; staging=source.with_name('.staging-color-field.tiff');source.rename(staging)
            REPORT.stage_preview(staging,'_S','color-field')
            metadata=source.with_name('color-field-assets.json'); recorded=metadata.read_bytes()
            region=dict(id='detail',label='Registered detail',box=[3,4,22,20],attachment='example',correlation=.99)
            entry=dict(dimensions=[32,24],regions=[region])
            with mock.patch.dict(REPORT.PR_REGIONS['scenes'],{'_S':entry}):
                REPORT.build(root,Path(temp)/'originals')
                value=json.loads((root/'report-data.json').read_text())['scenes']['_S']['variants']['color-field']
                first=root/value['crops'][0]['path']
                with Image.open(first) as image:
                    self.assertEqual(image.size,(19,16)); self.assertTrue(image.info['icc_profile'])
                region['box']=[3,4,25,22]
                REPORT.build(root,Path(temp)/'originals')
                value=json.loads((root/'report-data.json').read_text())['scenes']['_S']['variants']['color-field']
                self.assertNotEqual(root/value['crops'][0]['path'],first)
                with Image.open(root/value['crops'][0]['path']) as image:
                    self.assertEqual(image.size,(22,18))
                self.assertTrue(first.exists())
                self.assertEqual(metadata.read_bytes(),recorded)
                self.assertFalse(source.exists())

    def test_retained_tiff_is_never_consumed(self):
        with tempfile.TemporaryDirectory() as temp:
            source = Path(temp) / '_S' / 'coarse-grid.tiff'
            self.tiff(source)
            expected = REPORT.sha256(source)
            with self.assertRaises(ValueError):
                REPORT.stage_preview(source, '_S', 'coarse-grid')
            self.assertEqual(REPORT.sha256(source), expected)

    def test_legacy_roi_merge_keeps_raw_records_and_discloses_effective_area(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp); current=root/'current'; old=root/'old'; corpus=root/'originals'
            self.dataset(current, 'sensor'); self.dataset(old)
            raw=(old/'measurements.jsonl').read_bytes()
            REPORT.build(current, corpus, baseline=old)
            data=json.loads((current/'report-data.json').read_text())
            self.assertEqual(set(data['scenes']['_S']['variants']), {'color-field','roi-baseline'})
            value=data['scenes']['_S']['variants']['roi-baseline']
            self.assertEqual(value['settings']['color_field']['area'], 'active')
            self.assertNotIn('area',value['source_settings']['color_field'])
            self.assertEqual((old/'measurements.jsonl').read_bytes(),raw)

    def test_duplicate_recipe_rounds_are_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp); self.dataset(root/'a'); self.dataset(root/'b')
            with self.assertRaisesRegex(ValueError, 'Duplicate recipe'):
                REPORT.build(root/'a',root/'originals',supplements=[root/'b'])


if __name__ == '__main__':
    unittest.main()

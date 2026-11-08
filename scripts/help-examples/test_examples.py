"""Check safe manifests and explicit comparison generation without real GPU work."""

import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import examples


class ExampleTests(unittest.TestCase):
    def test_standalone_comparison_keeps_render_settings_and_clockwise_order(self):
        if importlib.util.find_spec('PIL') is None:
            self.skipTest('Pillow is required only for explicit comparison rendering')
        from PIL import Image, ImageCms
        colors = [(255, 0, 0), (0, 255, 0), (0, 0, 255),
                  (255, 255, 0), (0, 255, 255), (255, 0, 255)]
        profile = ImageCms.ImageCmsProfile(ImageCms.createProfile('sRGB')).tobytes()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source, renderer = root / 'source.x3f', root / 'renderer'
            source.write_bytes(b'original')
            renderer.write_bytes(b'frozen renderer')
            settings, output = root / 'settings.json', root / 'comparison.png'
            settings.write_text(json.dumps(dict(title='Test', base_args=['--wb', 'Sunlight'],
                variants=[dict(label=str(i + 1), args=['--exposure', str(i)]) for i in range(6)])))

            def render(command, **_):
                self.assertEqual(command[2], str(source))
                self.assertEqual(Path(command[0]).read_bytes(), b'frozen renderer')
                self.assertEqual(command[4:6], ['--wb', 'Sunlight'])
                Image.new('RGB', (8, 6), colors[int(command[-1])]).save(command[3], icc_profile=profile)

            with patch.object(examples.subprocess, 'run', side_effect=render) as calls:
                examples.compare(source, settings, renderer, output, 3, True, 80)
                self.assertEqual(calls.call_count, 6)
            with Image.open(output) as image:
                self.assertTrue(image.info['icc_profile'])
                pixels = list(image.getdata())
                positions = []
                for index, color in enumerate(colors):
                    first = pixels.index(color)
                    positions.append((first // image.width, first % image.width, index))
                self.assertEqual([index for _, _, index in sorted(positions)], [0, 1, 2, 5, 4, 3])
            self.assertEqual(source.read_bytes(), b'original')
            self.assertEqual({p.name for p in root.iterdir()},
                             {'source.x3f', 'renderer', 'settings.json', 'comparison.png'})
        with self.assertRaises(ValueError):
            examples.grid_order(5, 3, True)

    def test_manifests_reject_unsafe_names_and_invalid_comparisons(self):
        source = next(examples.CATALOG.glob('*/comparison.json'))
        manifest = json.loads(source.read_text())
        changes = [
            ('source', '../photo.x3f'),
            ('source', 'photo.json'),
            ('crop', [0, 0, 0, 10]),
            ('image_size', [0, 100]),
            ('variants', manifest['variants'][:1]),
            ('base_args', [42]),
        ]
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'comparison.json'
            for key, value in changes:
                invalid = copy.deepcopy(manifest)
                invalid[key] = value
                path.write_text(json.dumps(invalid))
                with self.subTest(field=key), self.assertRaises(ValueError):
                    examples.load_manifest(path)
            for image in ['../off.png', '/off.png', 'off.jpg']:
                invalid = copy.deepcopy(manifest)
                invalid['variants'][0]['image'] = image
                path.write_text(json.dumps(invalid))
                with self.subTest(image=image), self.assertRaises(ValueError):
                    examples.load_manifest(path)

    def test_checked_in_manifests_reference_existing_images(self):
        paths = list(examples.CATALOG.glob('*/comparison.json'))
        self.assertTrue(paths)
        for path in paths:
            data = examples.load_manifest(path)
            self.assertEqual(path.parent.name, data['id'])
            for variant in data['variants']:
                self.assertTrue((path.parent / variant['image']).is_file())

    def test_complementary_defect_examples_keep_the_companion_correction_on(self):
        for identifier, companion, flags in [
            ('bad-pixels', '--despeckle', ['--no-repair', '--repair']),
            ('speckles', '--repair', ['--no-despeckle', '--despeckle']),
        ]:
            with self.subTest(example=identifier):
                data = examples.load_manifest(examples.CATALOG / identifier / 'comparison.json')
                self.assertIn(companion, data['base_args'])
                self.assertNotIn('--reconstruction', data['base_args'])
                self.assertEqual(data['base_args'][data['base_args'].index('--sharpen') + 1], '0')
                self.assertEqual([v['args'] for v in data['variants']], [[flag] for flag in flags])
                left, top, right, bottom = data['crop']
                self.assertEqual([right - left, bottom - top], [64, 64])
                self.assertEqual(data['image_size'], [400, 400])

    def test_row_banding_brightens_both_sides_without_smoothing_or_resizing(self):
        data = examples.load_manifest(examples.CATALOG / 'row-banding' / 'comparison.json')
        args = data['base_args']
        self.assertEqual(args[args.index('--exposure') + 1], '3')
        self.assertNotIn('--reconstruction', args)
        for companion in ['--repair', '--despeckle', '--highlight-restoration']:
            self.assertIn(companion, args)
        self.assertEqual([v['args'] for v in data['variants']],
                         [['--no-row-correction'], ['--row-correction']])
        left, top, right, bottom = data['crop']
        self.assertEqual([right - left, bottom - top], data['image_size'])

    @unittest.skipUnless(importlib.util.find_spec('PIL'), 'Pillow is only needed for rendering')
    def test_render_candidates_require_write_to_update_assets(self):
        from PIL import Image, ImageCms
        profile = ImageCms.ImageCmsProfile(ImageCms.createProfile('sRGB')).tobytes()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            catalog = root / 'assets'
            directory = catalog / 'test'
            directory.mkdir(parents=True)
            source_dir = root / 'samples'
            source_dir.mkdir()
            (source_dir / 'test.x3f').write_bytes(b'fake source')
            renderer = root / 'renderer'
            renderer.write_bytes(b'fake renderer')
            manifest = dict(id='test', title='Test', description='Compare', note='Same crop',
                            source='test.x3f', crop=[0, 0, 2, 2], image_size=[4, 4],
                            base_args=['--wb', 'Sunlight'], variants=[
                                dict(id='off', label='Off', caption='Before', image='off.png', args=[]),
                                dict(id='on', label='On', caption='After', image='on.png', args=['--detail', '2'])])
            (directory / 'comparison.json').write_text(json.dumps(manifest))

            def render(command, **_):
                Image.new('RGB', (2, 2), 'red').save(command[3], icc_profile=profile)

            with patch.object(examples, 'ROOT', root), patch.object(examples, 'CATALOG', catalog), \
                    patch.object(examples.subprocess, 'run', side_effect=render):
                examples.render('test', source_dir, renderer, False)
                self.assertFalse((directory / 'off.png').exists())
                output = root / '.local/help-renders/test'
                self.assertEqual({p.name for p in output.iterdir()}, {'off.png', 'on.png', 'review.png'})
                examples.render('test', source_dir, renderer, True)
                for variant in manifest['variants']:
                    with Image.open(directory / variant['image']) as image:
                        self.assertEqual(image.size, (4, 4))
                self.assertEqual({p.name for p in directory.iterdir()}, {'comparison.json', 'off.png', 'on.png'})


if __name__ == '__main__':
    unittest.main()

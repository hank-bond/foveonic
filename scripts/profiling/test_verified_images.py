"""Check conservative raw-image removal, retained proofs and partial failures."""

import copy
import hashlib
import json
import os
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from results import compare_images, render_result
from verified_images import prune


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2)+'\n')


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


class VerifiedImagesTests(unittest.TestCase):
    def setUp(self):
        parent = Path(__file__).resolve().parents[2]/'.local'
        parent.mkdir(exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(prefix='verified-images-', dir=parent)
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name).resolve()

    def case(self, name='case'):
        root = self.base/name
        trial = root/'trial-00'
        geometry = {'extent':[1, 1], 'layout':'Rgb32Float', 'space':'rgb', 'linear_white':1,
                    'view_baked':False, 'view_extent':[1, 1], 'view_crop':[0, 0, 1, 1],
                    'view_rotation':0, 'endianness':'little'}
        checks, captures, inputs, samples = [], [], [], []
        for i in range(2):
            original = self.base/'originals'/f'photo-{i}.x3f'
            original.parent.mkdir(exist_ok=True)
            original.write_bytes(b'protected original')
            staged = trial/'inputs'/f'{i:02}-photo-{i}.x3f'
            staged.parent.mkdir(parents=True, exist_ok=True)
            staged.write_bytes(original.read_bytes())
            actual = trial/'evidence'/f'image-{i:02}.f32'
            expected = trial/f'fresh-{i:02}-samples'/(staged.stem+'.f32')
            for p in (actual, expected):
                p.parent.mkdir(parents=True, exist_ok=True)
                p.write_bytes(bytes([i])*12)
                samples.append(p)
            checks.append(compare_images(str(original), 'slider-drag', expected, geometry, actual, geometry))
            captures.append({**geometry, 'file':str(actual), 'source':str(staged),
                             'task':i+1, 'revision':2, 'settings':{'settings':{'exposure':i+.25, 'exposure_auto':False},
                                         'working_scale':8, 'processing_quality':'quality'}})
            inputs.append({'source':str(original), 'staged_source':str(staged), 'sha256':digest(original), 'settings':{'exposure':i, 'exposure_auto':False}})
        observations = [{'step':action, 'source':item['source'], 'source_sha256':item['sha256'],
                         'family':'Merrill', 'scale_divisor':8, 'processing_quality':'quality',
                         'requested_processing_quality':'quality', 'display_density':.1}
                        for action in ('initial-selection', 'slider-drag') for item in inputs]
        shared = render_result('application', 'slider-drag', 'quality', observations, True,
                               environment={'binary_sha256':'a'*64, 'fresh_binary_sha256':'b'*64, 'adapter':'GPU'})
        shared['assertions'].extend(checks)
        artifacts = {'test_result':{'path':str(trial/'evidence/test-result.json')},
                     'launch':str(trial/'launch.json'), 'result':str(trial/'evidence/scenario-result.json'),
                     'binary':str(self.base/'app'), 'binary_sha256':'a'*64,
                     'fresh_renderer':{'path':str(self.base/'renderer'), 'sha256':'b'*64}, 'source_tree_sha256':'c'*64}
        report = {'schema_version':1, 'scenario':'slider-drag', 'mode':'off', 'passed':True,
                  'workload_consistent':True, 'workload_key':{'identity':'fixed'}, 'trials_requested':1,
                  'photo_count':2, 'quality':'quality', 'trials':[{'passed':True, 'test_result':shared, 'artifacts':artifacts}]}
        scenario = {'name':'slider-drag', 'mode':'off', 'quality':'quality', 'photo_count':2,
                    'control_pointer':'/exposure', 'sources':[item['staged_source'] for item in inputs]}
        launch = {'inputs':inputs, 'scenario':scenario, 'binary':artifacts['binary'],
                  'binary_sha256':'a'*64, 'source_tree_sha256':'c'*64,
                  'fresh_renderer':copy.deepcopy(artifacts['fresh_renderer'])}
        initial = [{'source':item['staged_source'], 'settings':item['settings'],
                    'processing_quality':'quality'} for item in inputs]
        photos = [{'source':capture['source'], 'settings':capture['settings']['settings'],
                   'task':capture['task'], 'revision':capture['revision'], 'scale_divisor':8,
                   'processing_quality':'quality'} for capture in captures]
        native = {'passed':True, 'scenario':copy.deepcopy(scenario), 'hardware':{'adapter':'GPU'},
                  'image_captures':captures, 'steps':[{'action':'initial-selection', 'photos':copy.deepcopy(initial)},
                                                   {'action':'slider-drag', 'photos':copy.deepcopy(photos)}]}
        drag = {'pointer':'/exposure', 'paths':[p['source'] for p in photos], 'initial':[0, 1],
                'frames':[{'values':[.25, 1.25]}], 'errors':[]}
        fixture = SimpleNamespace(root=root, trial=trial, report=report, shared=shared,
                                  artifacts=artifacts, launch=launch, native=native, drag=drag, checks=checks, samples=samples)
        self.save(fixture)
        return fixture

    def save(self, f):
        write(f.trial/'launch.json', f.launch)
        write(f.trial/'evidence/scenario-result.json', f.native)
        write(f.trial/'evidence/slider-drag.json', f.drag)
        write(f.trial/'evidence/test-result.json', f.shared)
        f.artifacts['test_result']['sha256'] = digest(f.trial/'evidence/test-result.json')
        f.artifacts['result_sha256'] = digest(f.trial/'evidence/scenario-result.json')
        write(f.root/'regression.json', f.report)

    def preserved(self, f):
        self.assertTrue(all(p.exists() for p in f.samples))
        self.assertFalse((f.root/'sample-retention.json').exists())

    def test_success_removes_only_verified_images_and_retains_complete_proof(self):
        f = self.case()
        protected = [f.trial/'evidence/final.png', f.trial/'evidence/slider-failure.png',
                     f.trial/'evidence/unknown.f32', f.root/'goldens/baseline.f32', f.root/'run.log']
        for p in protected:
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_bytes(b'keep')
        metadata = {p:digest(p) for p in f.root.rglob('*.json')}
        receipt = prune(f.root)
        self.assertEqual(receipt['state'], 'complete')
        self.assertEqual(len(receipt['removed']), 4)
        self.assertEqual(len(receipt['images']), 2)
        self.assertTrue(all(not p.exists() for p in f.samples))
        self.assertTrue(all(p.read_bytes() == b'keep' for p in protected))
        self.assertEqual({p:digest(p) for p in metadata}, metadata)
        self.assertEqual(receipt, json.loads((f.root/'sample-retention.json').read_text()))
        self.assertEqual(receipt['regression_sha256'], digest(f.root/'regression.json'))
        for record in receipt['provenance']:
            self.assertEqual(digest(f.root/record['path']), record['sha256'])
        self.assertTrue(all(Path(i['source']).read_bytes() == b'protected original' for i in receipt['images']))

    def test_failed_incomplete_and_non_gui_reports_preserve_all_samples(self):
        changes = [lambda f:f.report.update(passed=False), lambda f:f.report.update(trials_requested=2),
                   lambda f:f.report.update(scenario='matrix'), lambda f:f.report.update(mode='wall'),
                   lambda f:f.report['trials'][0].update(passed=False),
                   lambda f:f.shared.update(outcome='failed'),
                   lambda f:f.shared['assertions'][0].update(outcome='unverified'),
                   lambda f:f.shared.update(assertions=[])]
        for i, change in enumerate(changes):
            with self.subTest(i=i):
                f = self.case(str(i));change(f);self.save(f)
                with self.assertRaises(ValueError):prune(f.root)
                self.preserved(f)

    def test_semantic_policy_identities_and_settled_frames_are_rechecked(self):
        changes = [lambda f:f.native['scenario'].update(quality='speed'),
                   lambda f:f.report.update(quality='speed'),
                   lambda f:f.artifacts.update(binary_sha256='d'*64),
                   lambda f:f.artifacts.update(source_tree_sha256='d'*64),
                   lambda f:f.shared['environment'].update(fresh_binary_sha256='d'*64),
                   lambda f:f.launch['fresh_renderer'].update(sha256='d'*64),
                   lambda f:f.native['image_captures'][0].update(task=999),
                   lambda f:f.native['image_captures'][0].update(revision=999),
                   lambda f:f.native['image_captures'][0]['settings'].update(working_scale=4),
                   lambda f:f.native['image_captures'][0]['settings'].update(processing_quality='speed'),
                   lambda f:f.native['image_captures'][0]['settings']['settings'].update(exposure=99),
                   lambda f:f.launch['inputs'][0]['settings'].update(exposure=99),
                   lambda f:f.drag['frames'][-1].update(values=[99, 99])]
        for i, change in enumerate(changes):
            with self.subTest(i=i):
                f = self.case(str(i));change(f);self.save(f)
                with self.assertRaises(ValueError):prune(f.root)
                self.preserved(f)

    def test_directory_receipt_barrier_failure_prevents_every_unlink(self):
        f = self.case()
        with patch('verified_images._sync_directory', side_effect=OSError('sync failed')):
            with self.assertRaises(OSError):prune(f.root)
        self.assertTrue(all(p.exists() for p in f.samples))
        receipt = json.loads((f.root/'sample-retention.json').read_text())
        self.assertEqual((receipt['state'], receipt['removed']), ('verified', []))

    def test_byte_corruption_and_truncation_are_rehashed_before_any_removal(self):
        for i, data in enumerate((b'wrong bytes!', b'short')):
            f = self.case(str(i));f.samples[-1].write_bytes(data)
            with self.assertRaises(ValueError):prune(f.root)
            self.preserved(f)

    def test_geometry_mismatch_and_missing_fields_preserve_all_samples(self):
        for i, mutate in enumerate((lambda g:g.update(view_rotation=1), lambda g:g.pop('space'))):
            f = self.case(str(i));mutate(f.checks[-1]['actual']['geometry']);self.save(f)
            with self.assertRaises(ValueError):prune(f.root)
            self.preserved(f)

    def test_unknown_baseline_and_external_paths_are_never_removed(self):
        for i, relative in enumerate(('goldens/baseline.f32', '../baseline.f32')):
            f = self.case(str(i));other = f.root/relative;other.parent.mkdir(exist_ok=True);other.write_bytes(bytes(12))
            f.checks[0]['expected']['file'] = str(other)
            self.save(f)
            with self.assertRaises(ValueError):prune(f.root)
            self.preserved(f);self.assertTrue(other.exists())

    def test_changed_native_or_shared_evidence_is_not_trusted(self):
        for i, name in enumerate(('scenario-result.json', 'test-result.json')):
            f = self.case(str(i));path = f.trial/'evidence'/name
            document = json.loads(path.read_text());document['extra'] = True;write(path, document)
            with self.assertRaises(ValueError):prune(f.root)
            self.preserved(f)

    def test_symlink_file_or_parent_is_rejected(self):
        for i, parent in enumerate((False, True)):
            f = self.case(str(i));p = f.samples[0]
            if parent:
                target = self.base/'moved';p.parent.rename(target)
                try:p.parent.symlink_to(target, target_is_directory=True)
                except OSError as error:self.skipTest(str(error))
            else:
                target = self.base/'external.f32';target.write_bytes(p.read_bytes());p.unlink()
                try:p.symlink_to(target)
                except OSError as error:self.skipTest(str(error))
            with self.assertRaises(ValueError):prune(f.root)
            self.preserved(f)

    def test_hardlinked_sample_is_preserved(self):
        f = self.case();os.link(f.samples[0], self.base/'linked.f32')
        with self.assertRaises(ValueError):prune(f.root)
        self.preserved(f)

    def test_receipt_write_failure_preserves_all_raw_files(self):
        f = self.case()
        with patch('verified_images._write', side_effect=OSError('disk full')):
            with self.assertRaises(OSError):prune(f.root)
        self.preserved(f)

    def test_receipt_exists_before_first_unlink_and_repeat_is_rejected(self):
        import verified_images
        f = self.case();unlink = Path.unlink;sync = verified_images._sync_directory;barriers = []
        def barrier(path):
            barriers.append(path)
            sync(path)
        def checked(path, *args, **kwargs):
            receipt = json.loads((f.root/'sample-retention.json').read_text())
            self.assertEqual(receipt['state'], 'verified')
            self.assertTrue(receipt['images'])
            self.assertTrue(barriers)
            return unlink(path, *args, **kwargs)
        with patch.object(Path, 'unlink', checked), patch('verified_images._sync_directory', side_effect=barrier):
            prune(f.root)
        self.assertEqual(len(barriers), 2)
        with self.assertRaisesRegex(ValueError, 'already exists'):prune(f.root)

    def test_partial_unlink_failure_is_recorded_and_never_resumed(self):
        f = self.case();unlink = Path.unlink;removed = []
        def fail_second(path, *args, **kwargs):
            if removed:raise OSError('cannot unlink second file')
            removed.append(path);return unlink(path, *args, **kwargs)
        with patch.object(Path, 'unlink', fail_second):
            with self.assertRaises(OSError):prune(f.root)
        receipt = json.loads((f.root/'sample-retention.json').read_text())
        self.assertEqual(receipt['state'], 'partial')
        self.assertEqual(receipt['removed'], [str(removed[0].relative_to(f.root))])
        self.assertEqual(sum(p.exists() for p in f.samples), 3)
        with self.assertRaises(ValueError):prune(f.root)

    def test_change_after_receipt_prevents_first_unlink(self):
        import verified_images
        f = self.case();write_receipt = verified_images._write
        def changed(path, value, **kwargs):
            write_receipt(path, value, **kwargs)
            if path.name == 'sample-retention.json':f.samples[-1].write_bytes(b'changed bytes')
        with patch('verified_images._write', side_effect=changed):
            with self.assertRaises(ValueError):prune(f.root)
        self.assertTrue(all(p.exists() for p in f.samples))
        receipt = json.loads((f.root/'sample-retention.json').read_text())
        self.assertEqual((receipt['state'], receipt['removed']), ('partial', []))


if __name__ == '__main__':
    unittest.main()

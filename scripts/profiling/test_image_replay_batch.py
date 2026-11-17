"""Check finite verifier requests, independent results and interrupted evidence."""

import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from correctness_cohort import CohortStopped
from image_replay import replay_images


class ImageReplayBatchTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        (self.root/'evidence').mkdir()
        self.geometry = {'extent': [1, 1], 'layout': 'Rgb32Float', 'space': 'test', 'linear_white': 1,
                         'view_baked': False, 'view_extent': [1, 1], 'view_crop': [0, 0, 1, 1],
                         'view_rotation': 0, 'endianness': 'little'}
        self.photos, self.inputs, captures = {}, [], []
        for index in range(2):
            source = str(self.root/f'{index}.x3f')
            settings = {'exposure': index+.5}
            capture = {**self.geometry, 'source': source, 'task': index+10, 'revision': 4,
                       'settings': {'settings': settings, 'working_scale': 8},
                       'file': str(self.root/'evidence'/f'image-{index:02}.f32')}
            Path(capture['file']).write_bytes(bytes(range(12)))
            captures.append(capture)
            self.photos[source] = {'task': index+10, 'revision': 4, 'scale_divisor': 8}
            self.inputs.append({'source': f'original-{index}', 'staged_source': source})
        self.native = {'hardware': {'adapter': 'GPU'}, 'image_captures': captures}

    def replay(self, **kwargs):
        return replay_images(self.native, self.inputs, self.photos, 'slider-drag', Path('/renderer'),
                             self.root, batch=True, **kwargs)

    def successful_batch(self, command, **kwargs):
        self.assertEqual(command[1], '--verify-batch')
        manifest = json.loads(Path(command[2]).read_text())
        kwargs['on_started'](1234)
        for index, request in enumerate(manifest['requests']):
            settings = json.loads(Path(request['settings_snapshot']).read_text())
            Path(request['golden']).parent.mkdir()
            Path(request['golden']).write_bytes(bytes(range(12)))
            records = [('verification_request_started', {'id': request['id'], 'pid':1234}),
                       ('session', {'source':request['source'], 'settings': settings['settings'], 'adapter': 'GPU'}),
                       ('processing', {'source':request['source'], 'execution':'fresh', 'effective_scale_divisor': 8}),
                       ('validation', {**self.geometry,'source':request['source']})]
            Path(request['events']).write_text('\n'.join(json.dumps({'event': event, 'data': data}) for event, data in records))
            receipt = {'schema_version': 1, 'batch_id': manifest['id'], 'request_id': request['id'],
                       'index': index, 'count': 2, 'source': request['source'],
                       'settings_snapshot': request['settings_snapshot'], 'golden': request['golden'],
                       'events': request['events'], 'execution': 'fresh', 'source_reopened': True,
                       'fresh_executor': True, 'timing_valid': False, 'passed': True, 'outcome': 'passed', 'pid': 1234}
            Path(request['terminal']).write_text(json.dumps(receipt))
        summary = {'event': 'verification_batch', 'data': {'id': manifest['id'], 'requests': 2,
                   'pid': 1234, 'passed': True, 'timing_valid': False}}
        return SimpleNamespace(returncode=0, stdout=json.dumps(summary), stderr='')

    def test_two_independent_photos_use_one_process_and_exact_checks(self):
        with patch('image_replay.require_quiet'), patch('image_replay.run_captured', side_effect=self.successful_batch) as run:
            checks = self.replay()
        self.assertEqual(run.call_count, 1)
        self.assertEqual([r['outcome'] for r in checks], ['passed', 'passed'])
        self.assertEqual([r['source'] for r in checks], ['original-0', 'original-1'])

    def test_parent_loss_marks_started_request_aborted_and_rest_unrun(self):
        def stopped(command, **kwargs):
            manifest = json.loads(Path(command[2]).read_text())
            Path(manifest['requests'][0]['events']).write_text('started\n')
            raise CohortStopped('parent gone')
        with patch('image_replay.require_quiet'), patch('image_replay.run_captured', side_effect=stopped):
            with self.assertRaises(CohortStopped):
                self.replay()
        receipts = [json.loads(p.read_text()) for p in sorted(self.root.glob('fresh-*-terminal.json'))]
        self.assertEqual([r['outcome'] for r in receipts], ['aborted', 'unrun'])
        self.assertTrue(all(r['passed'] is False for r in receipts))

    def test_corrupt_receipt_is_rejected(self):
        def corrupt(command, **kwargs):
            result = self.successful_batch(command, **kwargs)
            manifest = json.loads(Path(command[2]).read_text())
            path = Path(manifest['requests'][1]['terminal'])
            receipt = json.loads(path.read_text())
            receipt['request_id'] = 'wrong'
            path.write_text(json.dumps(receipt))
            return result
        with patch('image_replay.require_quiet'), patch('image_replay.run_captured', side_effect=corrupt):
            with self.assertRaisesRegex(ValueError, 'receipt'):
                self.replay()

    def test_wrong_geometry_remains_a_failed_exact_assertion(self):
        def wrong(command, **kwargs):
            self.geometry['view_rotation'] = 1
            return self.successful_batch(command, **kwargs)
        with patch('image_replay.require_quiet'), patch('image_replay.run_captured', side_effect=wrong):
            self.assertTrue(all(c['outcome'] == 'failed' for c in self.replay()))

    def test_stale_request_events_cannot_be_relabelled_by_a_current_terminal(self):
        def stale(command, **kwargs):
            result = self.successful_batch(command, **kwargs)
            manifest = json.loads(Path(command[2]).read_text())
            path = Path(manifest['requests'][1]['events'])
            records = [json.loads(line) for line in path.read_text().splitlines()]
            records[0]['data']['id'] = manifest['requests'][0]['id']
            path.write_text('\n'.join(map(json.dumps, records)))
            return result
        with patch('image_replay.require_quiet'), patch('image_replay.run_captured', side_effect=stale):
            with self.assertRaisesRegex(ValueError, 'events'):
                self.replay()

    def test_cohort_guard_callbacks_are_preserved(self):
        policy = SimpleNamespace(check_alive=lambda: None, check_quiet=lambda: None)
        with patch('image_replay.run_captured', side_effect=self.successful_batch) as run:
            self.replay(cohort=policy, max_memory_mb=2048)
        self.assertEqual(run.call_args.kwargs['before_start'], policy.check_alive)
        self.assertEqual(run.call_args.kwargs['on_poll'], policy.check_alive)
        self.assertEqual(run.call_args.kwargs['limit_bytes'], 2048*1024*1024)


if __name__ == '__main__':
    unittest.main()

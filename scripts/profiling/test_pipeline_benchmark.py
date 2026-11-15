"""Check fixed fresh-processing comparisons with bounded synthetic reports and no GPU execution."""

from contextlib import contextmanager
import copy
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from pipeline_benchmark import compare_reports, parse_run, record


def renderer_records(source, times, *, scale=8):
    settings = {'exposure': 0, 'processing_quality': 'quality', 'working_scale': scale}
    rows = [('session', {'frontend': 'completed_processing', 'render_only': True, 'adapter': 'test GPU',
                         'source': source, 'settings': settings, 'processing_quality': 'quality',
                         'scale_divisor': scale, 'family': 'Merrill'}),
            ('source_load', {'source': source, 'timings': {'total_ms': 50}, 'read_cache_state': 'uncontrolled'})]
    for iteration, value in enumerate(times):
        rows.append(('processing', {'source': source, 'settings': settings, 'processing_quality': 'quality',
                                    'effective_scale_divisor': scale, 'iteration': iteration, 'cold': iteration == 0,
                                    'execution': 'fresh', 'output': 'final', 'completed_ms': value,
                                    'render_call_ms': value / 2}))
    rows.append(('validation', {'source': source, 'outside_timing': True, 'digest': 'old-64-bit-digest',
                                'extent': [1, 2], 'layout': 'Rgb32Float', 'space': 'test', 'linear_white': 1,
                                'view_baked': False, 'view_extent': [1, 2], 'view_crop': [0, 0, 1, 2],
                                'view_rotation': 0, 'endianness': 'little'}))
    return [{'foveonic_profile': 1, 'mode': 'off', 'synchronized': False, 'event': event, 'data': data}
            for event, data in rows]


@contextmanager
def fake_monitor(path):
    path.write_text(json.dumps({'potential_workloads': [{'pid': 123, 'executable': 'profile_pipeline'}]}) + '\n')
    yield path


class PipelineBenchmarkTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.binary = self.root / 'profile_pipeline'
        self.binary.write_bytes(b'fixed executable identity')
        self.source = self.root / 'sample.x3f'
        self.source.write_bytes(b'bounded synthetic original')
        self.cases = [{'label': 'sample', 'family': 'merrill', 'source': str(self.source),
                       'resolutions': ['thumb'], 'processing_qualities': ['quality']}]
        self.times = [1000, 500, 10, 30, 20]
        self.pixel_data = bytes(range(24))
        self.mutations = lambda rows: rows
        self.addCleanup(patch.stopall)
        patch('pipeline_benchmark.require_quiet').start()
        patch('pipeline_benchmark.workload_monitor', fake_monitor).start()
        patch('pipeline_benchmark.run_captured', self.fake_run).start()

    def fake_run(self, command, *, limit_bytes, samples, env, on_started):
        self.assertEqual(limit_bytes, 5120 * 1024 * 1024)
        self.assertEqual(env['FOVEONIC_PROFILE'], 'off')
        self.assertIn('--render-only', command)
        self.assertEqual(command[command.index('--warm') + 1], str(len(self.times) - 1))
        on_started(123)
        destination = Path(command[command.index('--goldens') + 1])
        destination.mkdir(parents=True)
        (destination / 'sample.f32').write_bytes(self.pixel_data)
        rows = self.mutations(renderer_records(str(self.source), self.times))
        return subprocess.CompletedProcess(command, 0, '\n'.join(json.dumps(row) for row in rows), '')

    def capture(self, name='run', **options):
        return record(self.binary, self.cases, self.root / name, warmups=2, samples=3, **options)

    def test_warmups_are_exact_cold_and_load_stay_separate_and_sha_is_portable(self):
        report = self.capture(build_source_revision='declared-checkpoint')
        self.assertTrue(report['passed'])
        self.assertTrue(report['timing_valid'])
        row = report['cases'][0]
        self.assertEqual(row['warm_completed_ms'], [10, 30, 20])
        self.assertEqual([item['completed_ms'] for item in row['iterations']], self.times)
        self.assertEqual(row['source_load']['timings']['total_ms'], 50)
        self.assertEqual(row['summary_ms'], {'count': 3, 'median': 20, 'p95': 30, 'maximum': 30})
        self.assertEqual(len(row['pixels_sha256']), 64)
        self.assertFalse((self.root / 'run/floats/sample-thumb-quality/sample.f32').exists())
        self.assertEqual(json.loads((self.root / 'run/report.json').read_text()), report)
        self.assertEqual(report['build']['source_revision_identity'], 'caller-declared')

    def test_retention_is_explicit_and_failed_case_keeps_pixels_and_raw_output(self):
        self.capture(retain_floats=True)
        self.assertEqual((self.root / 'run/floats/sample-thumb-quality/sample.f32').read_bytes(), self.pixel_data)
        self.mutations = lambda rows: rows[:-1]
        report = self.capture('failed')
        self.assertFalse(report['passed'])
        self.assertIn('missing', report['cases'][0]['failure'])
        self.assertTrue((self.root / 'failed/floats/sample-thumb-quality/sample.f32').exists())
        self.assertTrue((self.root / 'failed/sample-thumb-quality.jsonl').is_file())

    def test_changed_original_during_run_cannot_receive_a_stale_identity(self):
        def changed_source(rows):
            self.source.write_bytes(b'changed during processing')
            return rows
        self.mutations = changed_source
        report = self.capture()
        self.assertFalse(report['passed'])
        self.assertIn('changed during', report['cases'][0]['failure'])
        self.assertTrue((self.root / 'run/floats/sample-thumb-quality/sample.f32').exists())

    def test_contention_invalidates_timing_without_changing_render_success(self):
        @contextmanager
        def contaminated(path):
            with fake_monitor(path):
                path.write_text(json.dumps({'potential_workloads': [
                    {'pid': 123, 'executable': 'profile_pipeline'}, {'pid': 124, 'executable': 'rustc'}]}) + '\n')
                yield path
        with patch('pipeline_benchmark.workload_monitor', contaminated):
            report = self.capture()
        self.assertTrue(report['passed'])
        self.assertFalse(report['timing_valid'])
        self.assertEqual(report['cases'][0]['contention']['other_workloads'], [{'pid': 124, 'executable': 'rustc'}])
        with self.assertRaisesRegex(ValueError, 'uncontended'):
            compare_reports(report, report)

    def test_budgets_and_numerical_equality_have_separate_outcomes(self):
        before = self.capture()
        self.times = [1000, 500, 11, 33, 22]
        after = self.capture('after')
        result = compare_reports(before, after)
        self.assertIsNone(result['timing_passed'])
        self.assertTrue(result['numerical_exact'])
        self.assertAlmostEqual(result['cases'][0]['median']['delta_percent'], 10)
        self.assertFalse(compare_reports(before, after, 5)['timing_passed'])
        self.assertTrue(compare_reports(before, after, 15)['timing_passed'])
        after['cases'][0]['pixels_sha256'] = 'f' * 64
        changed = compare_reports(before, after, 15)
        self.assertTrue(changed['timing_passed'])
        self.assertFalse(changed['numerical_exact'])

    def test_comparison_rejects_changed_workload_hardware_and_incomplete_samples(self):
        before = self.capture()
        changes = [lambda report: report['hardware'].update(host='other host'),
                   lambda report: report['measurement'].update(warmups=1),
                   lambda report: report['cases'][0].update(settings={'exposure': 1}),
                   lambda report: report['cases'][0].update(source_sha256='different original'),
                   lambda report: report['cases'][0].update(adapter='other GPU'),
                   lambda report: report['cases'][0]['geometry'].update(extent=[2, 1]),
                   lambda report: report.update(hardware={}),
                   lambda report: report['cases'][0]['warm_completed_ms'].pop(),
                   lambda report: report['cases'].append(copy.deepcopy(report['cases'][0]))]
        for change in changes:
            with self.subTest(change=change):
                after = copy.deepcopy(before)
                change(after)
                with self.assertRaises(ValueError):
                    compare_reports(before, after)

    def test_malformed_iterations_modes_and_actual_scale_are_rejected(self):
        case = {'source': str(self.source), 'processing_quality': 'quality', 'resolution': 'thumb'}
        changes = [lambda rows: rows.pop(2),
                   lambda rows: rows[2].update(mode='gpu'),
                   lambda rows: rows[2]['data'].update(iteration=99),
                   lambda rows: rows[2]['data'].update(execution='retained'),
                   lambda rows: rows[2]['data'].update(completed_ms=float('nan')),
                   lambda rows: rows[3]['data'].update(effective_scale_divisor=4),
                   lambda rows: rows[3]['data'].update(settings={'exposure': 9}),
                   lambda rows: rows[-1]['data'].update(outside_timing=False)]
        for change in changes:
            rows = renderer_records(str(self.source), self.times)
            change(rows)
            with self.subTest(change=change), self.assertRaises(ValueError):
                parse_run('\n'.join(json.dumps(row) for row in rows), case, 2, 3)

    def test_empty_warmups_and_owned_flag_injection_fail_before_launch(self):
        with self.assertRaisesRegex(ValueError, 'positive integers'):
            record(self.binary, self.cases, self.root / 'bad', warmups=0)
        self.cases[0]['options'] = ['--settings-snapshot', 'mutable.json']
        with self.assertRaisesRegex(ValueError, 'owns'):
            self.capture()
        self.cases[0]['options'] = ['--profile=wall']
        with self.assertRaisesRegex(ValueError, 'matrix-owned'):
            self.capture()


if __name__ == '__main__':
    unittest.main()

"""Check fixed fresh-processing comparisons with bounded synthetic reports and no GPU execution."""

from contextlib import contextmanager
import copy
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from pipeline_benchmark import build, compare_reports, parse_run, portable_report, record, summary


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
        self.memory = []
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
        if self.memory:
            Path(samples).write_text(''.join(json.dumps({'physical_footprint_bytes': value}) + '\n' for value in self.memory))
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

    def test_fixed_snapshot_uses_owned_scale_and_policy_and_checks_actual_settings(self):
        snapshot = {'settings': renderer_records(str(self.source), self.times)[0]['data']['settings'],
                    'working_scale': 1, 'processing_quality': 'speed', 'dehaze_estimate': True}
        self.cases[0]['settings_snapshot'] = snapshot
        before = copy.deepcopy(snapshot)
        report = self.capture()
        self.assertTrue(report['passed'])
        actual = json.loads((self.root / 'run/sample-thumb-quality.settings.json').read_text())
        self.assertEqual(actual['working_scale'], 8)
        self.assertEqual(actual['processing_quality'], 'quality')
        self.assertTrue(actual['dehaze_estimate'])
        self.assertEqual(snapshot, before)
        def changed_actual(rows):
            for row in rows:
                if 'settings' in row['data']:
                    row['data']['settings'] = {'unexpected': 'settings'}
            return rows
        self.mutations = changed_actual
        failed = self.capture('wrong-settings')
        self.assertFalse(failed['passed'])
        self.assertIn('declared snapshot', failed['cases'][0]['failure'])

    def test_portable_export_keeps_comparison_and_raw_observations_without_local_paths(self):
        report = self.capture(retain_floats=True)
        original = copy.deepcopy(report)
        exported = portable_report(report, 'a' * 64)
        self.assertNotIn(str(self.root), json.dumps(exported))
        self.assertEqual(report, original)
        result = compare_reports(report, exported)
        self.assertTrue(result['numerical_exact'])
        self.assertIsNone(result['timing_passed'])
        self.assertEqual(exported['cases'][0]['iterations'], report['cases'][0]['iterations'])
        self.assertEqual(exported['evidence']['source_report_sha256'], 'a' * 64)
        report['timing_valid'] = False
        with self.assertRaisesRegex(ValueError, 'uncontended'):
            portable_report(report, 'a' * 64)

    def test_runs_that_did_different_work_for_the_same_request_still_compare(self):
        before = self.capture()
        after = copy.deepcopy(before)
        after['cases'][0].update(settings={'exposure': 1}, effective_scale_divisor=4)
        row = compare_reports(before, after)['cases'][0]
        self.assertFalse(row['settings_equal'])
        self.assertEqual(row['delivered_scale'], {'before': 8, 'after': 4})
        after['cases'][0].update(resolution='draft')
        with self.assertRaises(ValueError):
            compare_reports(before, after)

    def test_only_shared_cases_compare_and_the_rest_are_named(self):
        before = self.capture()
        after = copy.deepcopy(before)
        added = copy.deepcopy(after['cases'][0])
        added['id'] = 'new-recipe-thumb-quality'
        after['cases'].append(added)
        after['cases_requested'] = 2
        result = compare_reports(before, after)
        self.assertEqual([row['id'] for row in result['cases']], ['sample-thumb-quality'])
        self.assertEqual(result['only_after'], ['new-recipe-thumb-quality'])
        self.assertEqual(result['only_before'], [])
        self.assertIn('Only in the second run: new-recipe-thumb-quality', summary(result))

    def test_a_timing_change_counts_only_when_the_samples_do_not_overlap(self):
        before = self.capture()
        self.times = [1000, 500, 12, 31, 22]
        overlapping = compare_reports(before, self.capture('overlapping'))
        self.assertFalse(overlapping['cases'][0]['beyond_spread'])
        self.times = [1000, 500, 5, 6, 7]
        faster = compare_reports(before, self.capture('faster'))
        self.assertTrue(faster['cases'][0]['beyond_spread'])
        self.assertIn('sample-thumb-quality: 20.0 -> 6.0 ms (-70%)', summary(faster))

    def test_changed_pixels_are_measured_and_identical_ones_are_not(self):
        before = self.capture()
        measured = []
        difference = {'samples': 6, 'different_samples': 3, 'nonfinite_samples': 0,
                      'max_absolute': 0.5, 'mean_absolute': 0.125, 'rms': 0.25}

        def measure(old, new):
            measured.append((old['id'], new['id']))
            return difference
        same = compare_reports(before, copy.deepcopy(before), measure=measure)
        self.assertEqual(measured, [])
        self.assertIn('Pixels: 1 identical, 0 changed.', summary(same))
        self.pixel_data = bytes(reversed(range(24)))
        changed = compare_reports(before, self.capture('changed'), measure=measure)
        self.assertEqual(measured, [('sample-thumb-quality', 'sample-thumb-quality')])
        self.assertEqual(changed['cases'][0]['numerical']['difference'], difference)
        self.assertIn('sample-thumb-quality: 50.0% of samples, max 0.5, mean 0.125', summary(changed))

    def test_peak_memory_is_recorded_and_large_changes_are_reported(self):
        self.memory = [2**30, 3 * 2**30, 2 * 2**30]
        before = self.capture()
        self.assertEqual(before['cases'][0]['peak_memory_bytes'], 3 * 2**30)
        self.memory = [3 * 2**30 + 2**28]
        steady = compare_reports(before, self.capture('steady'))
        self.assertIn('Peak memory: 0 of 1 cases changed', summary(steady))
        self.memory = [6 * 2**30]
        grown = compare_reports(before, self.capture('grown'))
        self.assertIn('sample-thumb-quality: 3.00 -> 6.00 GiB (+100%)', summary(grown))
        self.memory = []
        unsampled = compare_reports(before, self.capture('unsampled'))
        self.assertIsNone(unsampled['cases'][0]['peak_memory']['delta_percent'])

    def test_a_revision_is_exported_and_built_once_and_the_worktree_is_named_dirty(self):
        repository = self.root / 'repository'
        repository.mkdir()

        def git(*arguments):
            return subprocess.run(['git', '-c', 'user.name=t', '-c', 'user.email=t@example.invalid', *arguments],
                                  cwd=repository, check=True, capture_output=True, text=True).stdout.strip()
        git('init', '-q')
        (repository / 'Cargo.toml').write_text('first\n')
        git('add', '.')
        git('commit', '-q', '-m', 'first')
        first = git('rev-parse', 'HEAD')
        (repository / 'Cargo.toml').write_text('second\n')
        builds = []

        run = subprocess.run

        def cargo(command, *, cwd=None, **options):
            if command[0] != 'cargo':
                return run(command, cwd=cwd, **options)
            builds.append((Path(cwd), (Path(cwd) / 'Cargo.toml').read_text()))
            renderer = Path(cwd) / 'target/release/examples'
            renderer.mkdir(parents=True, exist_ok=True)
            (renderer / 'profile_pipeline').write_text('built')
            (renderer / 'profile_pipeline.exe').write_text('built')
        with patch('pipeline_benchmark.subprocess.run', cargo):
            binary, name = build(first, root=repository)
            self.assertEqual(name, first)
            self.assertEqual(builds, [(repository / '.local/runs/builds' / first, 'first\n')])
            self.assertTrue(binary.is_file())
            self.assertEqual(build(first, root=repository), (binary, first))
            self.assertEqual(len(builds), 1, 'a built revision is not built again')
            _, name = build('worktree', root=repository)
            self.assertEqual(name, first + '-dirty')
            self.assertEqual(builds[-1], (repository, 'second\n'))

    def test_comparison_rejects_changed_workload_hardware_and_incomplete_samples(self):
        before = self.capture()
        changes = [lambda report: report['hardware'].update(host='other host'),
                   lambda report: report['measurement'].update(warmups=1),
                   lambda report: report['cases'][0].update(source_sha256='different original'),
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
        with self.assertRaisesRegex(ValueError, 'matrix-owned'):
            self.capture()
        self.cases[0]['options'] = ['--profile=wall']
        with self.assertRaisesRegex(ValueError, 'matrix-owned'):
            self.capture()


if __name__ == '__main__':
    unittest.main()

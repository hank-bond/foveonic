"""Check pinned wide-probe inputs and separation of exactness from timing evidence."""

import json
from pathlib import Path
import tempfile
import unittest

from file_hash import digest
from wide import build_provenance, check_images, check_workload, inputs, parse_modes, summarize


class WideEvidenceTests(unittest.TestCase):
    def test_modes_reject_duplicates_and_serial_widths_above_one(self):
        self.assertEqual(parse_modes('serial:1,grouped:2'), (('serial', 1), ('grouped', 2)))
        for value in ('serial:2', 'grouped:0', 'grouped:2,grouped:2', 'unknown:1'):
            with self.assertRaises(ValueError):
                parse_modes(value)

    def test_frozen_binary_must_match_explicit_build_provenance(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary, record = root / 'profile_wide', root / 'build.json'
            binary.write_bytes(b'frozen program')
            data = {'binary_sha256': digest(binary), 'git_revision': 'base',
                    'source_tree_sha256': 'tree', 'build_command': 'cargo build', 'rustc_vv': 'rustc version'}
            record.write_text(json.dumps(data))
            self.assertEqual(build_provenance(binary, record), data)
            binary.write_bytes(b'different program')
            with self.assertRaisesRegex(ValueError, 'build provenance'):
                build_provenance(binary, record)

    def test_report_must_match_pinned_sources_and_off_profiling(self):
        source = {'source': '/photo/a.x3f', 'sha256': 'abc'}
        report = {'mode': 'grouped', 'width': 2, 'edit': 'exposure',
                  'completion_poll_interval_us': 1000, 'profile_mode': 'off',
                  'steps_per_photo': 1, 'cold_graph': False,
                  'sources': [source['source']], 'source_sha256': [source['sha256']]}
        check_workload(report, [source], 'exposure', 'grouped', 2, 1000)
        for changed in ({**report, 'profile_mode': 'wall'}, {**report, 'source_sha256': ['other']},
                        {**report, 'sources': ['/photo/b.x3f']}, {**report, 'sources': []}):
            with self.assertRaises(ValueError):
                check_workload(changed, [source], 'exposure', 'grouped', 2, 1000)

    def test_inputs_reject_changed_content_and_directory_traversal(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / 'a.x3f'
            source.write_bytes(b'approved original')
            manifest = root / 'inputs.json'
            row = {'file': source.name, 'sha256': digest(source)}
            manifest.write_text(json.dumps({'files': [row]}))
            self.assertEqual(inputs(root, manifest)[0]['source'], str(source.resolve()))
            source.write_bytes(b'different original')
            with self.assertRaisesRegex(ValueError, 'hash differs'):
                inputs(root, manifest)
            manifest.write_text(json.dumps({'files': [{**row, 'file': '../a.x3f'}]}))
            with self.assertRaisesRegex(ValueError, 'basenames'):
                inputs(root, manifest)

    def test_exactness_requires_complete_samples_geometry_settings_and_count(self):
        expected = [{'sha256': 'abc', 'geometry': {'crop': [0, 0, 2, 2]}, 'settings': {'exposure': 1}}]
        check_images(expected, expected)
        for changed in ([], [{**expected[0], 'sha256': 'def'}],
                        [{**expected[0], 'geometry': {'crop': [1, 0, 3, 2]}}],
                        [{**expected[0], 'settings': {'exposure': 2}}]):
            with self.assertRaises(ValueError):
                check_images(expected, changed)

    def test_warmups_and_contested_runs_do_not_enter_clean_distributions(self):
        def row(group, warmup=False, contested=False, passed=True):
            return {'edit': 'exposure', 'count': 2, 'mode': 'grouped', 'width': 2,
                    'warmup': warmup, 'passed': passed,
                    'steps_per_photo': 1, 'cold_graph': False,
                    'poll_us': 1000,
                    'contention': {'uncontended': not contested},
                    'group_ms': group, 'first_ms': group / 2,
                    'active_ms': group / 2, 'physical_peak_bytes': 1024, 'process_cpu_ms': 1}
        result = summarize([row(100, warmup=True), row(20), row(40), row(200, contested=True), row(999, passed=False)])
        self.assertEqual(len(result), 1)
        self.assertEqual(result[0]['trials'], 3)
        self.assertEqual(result[0]['uncontended_trials'], 2)
        self.assertEqual(result[0]['uncontended']['group_ms']['median'], 30)
        self.assertEqual(result[0]['all_observations']['group_ms']['median'], 40)


if __name__ == '__main__':
    unittest.main()

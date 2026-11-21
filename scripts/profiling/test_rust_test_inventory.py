"""Check exact ignored-test accounting, compiler discovery and reproducible command plans without a GPU."""

import copy
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from file_hash import digest
from rust_test_inventory import (MANIFEST, command_plan, compiler_artifacts, discover,
                                 ignored_names, input_hashes, reconcile,
                                 validate_inventory, verify_inputs)


def manifest():
    return {'schema_version': 1, 'scope': 'ignored only', 'fixture_policy': 'full copies',
            'suites': [{'id': 'core', 'target': 'core', 'kind': 'lib'}],
            'input_sets': {'standard': ['a.x3f'], 'merrill_high_only': ['a.x3f']},
            'additional_inputs': {},
            'environments': {'original': {'SOURCE': {'file': 'a.x3f'},
                                          'FIXTURES': {'directory': 'merrill_high_only'}},
                             'none': {}},
            'unset_environment': {'names': ['OPTIONAL_OUTPUT'],
                                  'prefixes': ['FOVEONIC_'], 'suffixes': ['_REFERENCE']},
            'groups': [{'id': 'originals', 'suite': 'core', 'environment': 'original',
                        'resource': 'gpu', 'one_test_per_process': False,
                        'tests': ['source::decode', 'render::exact']},
                       {'id': 'sdk', 'suite': 'core', 'environment': 'none',
                        'resource': 'gpu_sdk', 'one_test_per_process': True,
                        'tests': ['sdk::one', 'sdk::two']}],
            'excluded': [{'suite': 'core', 'test': 'research::study',
                          'reason': 'requires independent study data',
                          'requirements': 'STUDY_INPUT', 'source': 'src/study.rs'}]}


def discovered():
    return [{'kind': 'lib', 'target': 'core', 'executable': '/owned/core-tests',
             'binary_sha256': 'binary', 'ignored': ['source::decode', 'render::exact',
                                                    'sdk::one', 'sdk::two', 'research::study']}]


class InventoryTests(unittest.TestCase):
    def test_exact_required_and_excluded_names_reconcile(self):
        result = reconcile(manifest(), discovered())
        self.assertEqual(list(result), ['core'])
        extra = {'kind': 'bin', 'target': 'ordinary', 'ignored': []}
        self.assertEqual(reconcile(manifest(), [*discovered(), extra]), result)

    def test_unknown_missing_and_duplicate_discovery_fail(self):
        for mutation in ['unknown', 'missing_required', 'missing_excluded', 'duplicate', 'target', 'missing_target']:
            with self.subTest(mutation=mutation):
                rows = discovered()
                if mutation == 'unknown': rows[0]['ignored'].append('new::not_classified')
                elif mutation == 'missing_required': rows[0]['ignored'].remove('source::decode')
                elif mutation == 'missing_excluded': rows[0]['ignored'].remove('research::study')
                elif mutation == 'duplicate': rows[0]['ignored'].append('source::decode')
                elif mutation == 'target': rows.append({'kind': 'test', 'target': 'new', 'ignored': ['new_test']})
                else: rows = []
                with self.assertRaises(ValueError): reconcile(manifest(), rows)

    def test_classification_cannot_hide_duplicates_or_dependencies(self):
        for mutation in ['group', 'test', 'excluded_required', 'reason', 'requirements', 'suite', 'environment']:
            with self.subTest(mutation=mutation):
                document = manifest()
                if mutation == 'group': document['groups'].append(copy.deepcopy(document['groups'][0]))
                elif mutation == 'test': document['groups'][0]['tests'].append('source::decode')
                elif mutation == 'excluded_required': document['excluded'][0]['test'] = 'source::decode'
                elif mutation in ('reason', 'requirements'): document['excluded'][0][mutation] = ''
                elif mutation == 'suite': document['groups'][0]['suite'] = 'unknown'
                else: document['environments']['original']['SOURCE'] = {'file': 'unknown.x3f'}
                with self.assertRaises(ValueError): validate_inventory(document)

    def test_listing_is_strict_and_never_ignores_a_truncated_summary(self):
        self.assertEqual(ignored_names('a::one: test\na::two: test\n\n2 tests, 0 benchmarks\n'), ['a::one', 'a::two'])
        self.assertEqual(ignored_names('0 tests, 0 benchmarks\n'), [])
        for text in ['a: test\n', 'a: test\n2 tests, 0 benchmarks\n',
                     'a: test\na: test\n2 tests, 0 benchmarks\n',
                     '1 test, 1 benchmark\n', 'arbitrary output\n']:
            with self.subTest(text=text), self.assertRaises(ValueError): ignored_names(text)

    def test_discovery_only_invokes_libtest_listing_and_hashes_binary(self):
        with tempfile.TemporaryDirectory() as tmp:
            exe = Path(tmp) / 'test binary'; exe.write_bytes(b'compiled tests')
            artifact = {'kind': 'lib', 'target': 'core', 'executable': str(exe)}
            def run(command, **kwargs):
                self.assertEqual(command, [str(exe), '--ignored', '--list'])
                self.assertEqual(kwargs, {'capture_output': True, 'text': True, 'check': True, 'timeout': 30})
                return subprocess.CompletedProcess(command, 0, 'a: test\n1 test, 0 benchmarks\n', '')
            row = discover([artifact], run)[0]
            self.assertEqual(row['binary_sha256'], digest(exe))
            self.assertEqual(row['ignored'], ['a'])

    def test_compiler_reported_executable_not_guessed_target_directory(self):
        with tempfile.TemporaryDirectory() as tmp:
            exe = Path(tmp) / 'release/build/core/hash/out/tests'; exe.parent.mkdir(parents=True); exe.write_bytes(b'tests')
            row = {'reason': 'compiler-artifact', 'profile': {'test': True},
                   'package_id': 'path+file:///owned/core#0.8.4',
                   'target': {'name': 'core', 'kind': ['lib']}, 'executable': str(exe)}
            ordinary = {**row, 'profile': {'test': False}}
            text = '\n'.join(json.dumps(x) for x in [ordinary, row, row, {'reason': 'build-finished', 'success': True}])
            self.assertEqual(len(compiler_artifacts(text)), 1)
            self.assertEqual(compiler_artifacts(text)[0]['executable'], str(exe.resolve()))
            for bad in [text + '\n' + json.dumps({'reason': 'build-finished', 'success': False}),
                        text + '\n' + json.dumps({**row, 'package_id': 'different-package'}),
                        json.dumps(ordinary), 'not Cargo JSON']:
                with self.subTest(bad=bad[-90:]), self.assertRaises(ValueError): compiler_artifacts(bad)

    def test_plan_preserves_groups_env_overrides_and_sdk_process_boundaries(self):
        fixtures = {'standard': {'directory': '/owned/all'},
                    'merrill_high_only': {'directory': '/owned/merrill'}}
        plan = command_plan(manifest(), discovered(), fixtures,
                            {'PATH': '/runtime/dxc', 'FOVEONIC_DUMP': 'bad', 'PRIVATE_REFERENCE': 'bad'})
        self.assertFalse(plan['test_execution_performed'])
        self.assertEqual(plan['counts'], {'required': 4, 'excluded': 1, 'commands': 3})
        first = plan['commands'][0]
        self.assertEqual(first['tests'], ['source::decode', 'render::exact'])
        self.assertEqual(first['argv'], ['/owned/core-tests', 'source::decode', 'render::exact', '--ignored', '--exact', '--test-threads=1'])
        self.assertEqual(first['environment'], {'SOURCE': '/owned/all/a.x3f', 'FIXTURES': '/owned/merrill'})
        self.assertEqual(first['unset_environment'], ['FOVEONIC_DUMP', 'OPTIONAL_OUTPUT', 'PRIVATE_REFERENCE'])
        self.assertTrue(all(c['serial_per_adapter'] for c in plan['commands']))
        self.assertEqual([c['tests'] for c in plan['commands'][1:]], [['sdk::one'], ['sdk::two']])
        self.assertEqual(plan['excluded'][0]['requirements'], 'STUDY_INPUT')

    def test_literal_variants_replace_inherited_controls_after_environment_removal(self):
        document = manifest()
        document['environments']['original']['FOVEONIC_WIDE_RENDER_WIDTH'] = {'value': '2'}
        plan = command_plan(document, discovered(),
                            {'standard': {'directory': '/owned/all'},
                             'merrill_high_only': {'directory': '/owned/merrill'}},
                            {'FOVEONIC_WIDE_RENDER_WIDTH': 'serial'})
        command = plan['commands'][0]
        self.assertIn('FOVEONIC_WIDE_RENDER_WIDTH', command['unset_environment'])
        self.assertEqual(command['environment']['FOVEONIC_WIDE_RENDER_WIDTH'], '2')
        self.assertEqual(command['environment']['SOURCE'], '/owned/all/a.x3f')
        for value in ['', 2, None, {'file': 'a.x3f'}]:
            with self.subTest(value=value):
                document['environments']['original']['FOVEONIC_WIDE_RENDER_WIDTH'] = {'value': value}
                with self.assertRaises(ValueError):
                    validate_inventory(document)

    def test_truncated_or_merged_compiler_builds_cannot_establish_discovery(self):
        with tempfile.TemporaryDirectory() as tmp:
            exe = Path(tmp) / 'tests'; exe.write_bytes(b'tests')
            artifact = {'reason': 'compiler-artifact', 'profile': {'test': True},
                        'package_id': 'core', 'target': {'name': 'core', 'kind': ['lib']},
                        'executable': str(exe)}
            end = {'reason': 'build-finished', 'success': True}
            for messages in [[artifact], [artifact, end, end], [end, artifact]]:
                with self.subTest(messages=messages), self.assertRaises(ValueError):
                    compiler_artifacts('\n'.join(map(json.dumps, messages)))

    def test_exif_decisions_remain_visible_for_successful_checks(self):
        document = manifest(); document['groups'][0]['resource'] = 'gpu_exiftool'
        plan = command_plan(document, discovered(),
                            {'standard': {'directory': '/owned/all'},
                             'merrill_high_only': {'directory': '/owned/merrill'}}, {})
        self.assertIn('--nocapture', plan['commands'][0]['argv'])

    def test_fixture_copies_are_exact_and_not_mutated(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); directories = {}
            for name in ['standard', 'merrill_high_only']:
                directory = root / name; directory.mkdir(); (directory / 'a.x3f').write_bytes(b'original')
                directories[name] = directory
            hashes = {'a.x3f': digest(directories['standard'] / 'a.x3f')}
            result = verify_inputs(manifest(), hashes, directories)
            self.assertEqual(result['standard']['files'][0]['sha256'], hashes['a.x3f'])
            self.assertEqual((directories['standard'] / 'a.x3f').read_bytes(), b'original')
            for mutation in ['extra', 'missing', 'changed', 'symlink', 'hardlink']:
                path = directories['merrill_high_only'] / 'a.x3f'
                extra = directories['merrill_high_only'] / 'extra.x3f'
                if mutation == 'extra': extra.write_bytes(b'original')
                else:
                    path.unlink()
                    if mutation == 'changed': path.write_bytes(b'changed')
                    elif mutation == 'symlink': path.symlink_to(directories['standard'] / 'a.x3f')
                    elif mutation == 'hardlink': os.link(directories['standard'] / 'a.x3f', path)
                with self.subTest(mutation=mutation), self.assertRaises(ValueError): verify_inputs(manifest(), hashes, directories)
                if path.exists(): path.unlink()
                if extra.exists(): extra.unlink()
                path.write_bytes(b'original')

    def test_input_hash_sources_cannot_override_each_other(self):
        document = manifest(); sha = 'a' * 64
        self.assertEqual(input_hashes(document, {'sample_hashes': {'a.x3f': sha}}, {'files': []})['a.x3f'], sha)
        for definitions, external in [({'sample_hashes': {}}, {'files': []}),
                                      ({'sample_hashes': {'a.x3f': sha}}, {'files': [{'file': 'a.x3f', 'sha256': sha}]})]:
            with self.assertRaises(ValueError): input_hashes(document, definitions, external)

    def test_maintained_plan_cannot_inherit_reduced_correctness_coverage(self):
        document = json.loads(MANIFEST.read_text())
        suites = validate_inventory(document)
        found = [{'kind': suite['key'][0], 'target': suite['key'][1],
                  'executable': '/owned/'+name, 'binary_sha256': 'a'*64,
                  'ignored': sorted(suite['tests'])} for name, suite in suites.items()]
        plan = command_plan(document, found,
                            {'standard': {'directory': '/owned/all'},
                             'merrill_high_only': {'directory': '/owned/merrill'}},
                            {'WHITE_BALANCE_SCALES': '64', 'SMALL_PREVIEW_SCALES': '64',
                             'SMALL_PREVIEW_CASE': 'neutral', 'PATH': '/runtime/dxc'})
        for command in plan['commands']:
            self.assertTrue({'WHITE_BALANCE_SCALES', 'SMALL_PREVIEW_SCALES', 'SMALL_PREVIEW_CASE'}
                            <= set(command['unset_environment']))
            self.assertTrue({'WHITE_BALANCE_SCALES', 'SMALL_PREVIEW_SCALES', 'SMALL_PREVIEW_CASE'}
                            .isdisjoint(command['environment']))
            self.assertNotIn('PATH', command['unset_environment'])
        self.assertEqual(plan['counts']['required'], sum(len(group['tests']) for group in document['groups']))

    def test_maintained_inventory_keeps_release_selection_and_corrected_variants(self):
        document = json.loads(MANIFEST.read_text()); validate_inventory(document)
        self.assertEqual(sum(len(g['tests']) for g in document['groups']), 487)
        self.assertEqual(len(document['excluded']), 155)
        self.assertEqual(len(document['input_sets']['standard']), 27)
        groups = {name: g for g in document['groups'] for name in g['tests']}
        self.assertEqual(groups['source::tests::decode_original_merrill_files']['environment'], 'merrill_high_only')
        for suffix in ['classic_calibration_inspection_defers_color_selection', 'calibration_preparation_defers_color_metadata']:
            self.assertEqual(next(g for n, g in groups.items() if n.endswith('::' + suffix))['environment'], 'classic_dp2')
        self.assertEqual(sum(len(g['tests']) for g in document['groups'] if g['resource'] == 'gpu_sdk'), 5)

if __name__ == '__main__':
    unittest.main()

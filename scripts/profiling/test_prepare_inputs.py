"""Check declared test inputs preserve recipes, verify originals and generate explicit aliases."""

from contextlib import redirect_stdout, redirect_stderr
import copy
import io
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

from prepare_inputs import main, prepare
from rust_test_inventory import MANIFEST


class PrepareInputTests(unittest.TestCase):
    def fixture(self, directory):
        source = directory / 'originals'
        source.mkdir()
        (source / 'camera.x3f').write_bytes(b'original')
        checksum = hashlib.sha256(b'original').hexdigest()
        definitions = {'sample_hashes': {'camera.x3f': checksum},
                       'aliases': {'second.x3f': 'camera.x3f'},
                       'render_cases': [{'label': 'camera', 'source': 'camera.x3f', 'family': 'tiger'}],
                       'scenario_cases': [{'source': 'camera.x3f', 'settings': {'exposure': 0.15}}],
                       'checkpoint_cases': [{'source': 'camera.x3f', 'edits': [{'label': 'exposure', 'a': {'exposure': 0.1}, 'b': {'exposure': 0.2}}]}],
                       'regional_cases': [{'source': 'camera.x3f', 'settings': {'rotation': {'degrees': 17.}}}],
                       'choice_cases': [{'source': 'second.x3f', 'settings': {'white_balance': 'Sunlight'}}],
                       'fixtures': [{'name': 'tiger', 'family': 'tiger', 'sources': ['camera.x3f', 'second.x3f']}]}
        return source, definitions

    def rust_fixture(self, directory):
        source, definitions = self.fixture(directory)
        inventory = json.loads(MANIFEST.read_text())
        # Keep the maintained memberships and classifications, using tiny owned bytes.
        additional = {}
        for name in {name for names in inventory['input_sets'].values() for name in names}:
            payload = ('owned ' + name).encode()
            (source / name).write_bytes(payload)
            additional[name] = {**inventory['additional_inputs'].get(name, {}),
                                'sha256': hashlib.sha256(payload).hexdigest(), 'bytes': len(payload)}
        inventory['additional_inputs'] = additional
        return source, definitions, inventory

    def test_rust_fixtures_are_exact_independent_full_copies_with_inventory_provenance(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            source, definitions, inventory = self.rust_fixture(root)
            sidecar = source / 'foveonic/sidecar.json'
            sidecar.parent.mkdir()
            sidecar.write_text('original settings')
            outputs = prepare(source, root / 'generated', definitions, [], rust_inventory=inventory)
            receipts = json.loads(outputs['rust-fixtures.json'].read_text())
            for identity, expected in inventory['input_sets'].items():
                directory = root / 'generated/rust-fixtures' / identity
                self.assertEqual(set(p.name for p in directory.iterdir()), set(expected))
                self.assertEqual(receipts[identity]['directory'], str(directory))
                self.assertEqual([row['file'] for row in receipts[identity]['files']], expected)
                for name in expected:
                    copy_path, original = directory / name, source / name
                    self.assertEqual(copy_path.read_bytes(), original.read_bytes())
                    self.assertFalse(copy_path.is_symlink())
                    self.assertEqual(copy_path.stat().st_nlink, 1)
                    self.assertNotEqual(copy_path.stat().st_ino, original.stat().st_ino)
            self.assertEqual(json.loads(outputs['provenance.json'].read_text())['rust_inventory'], inventory)
            self.assertEqual(sidecar.read_text(), 'original settings')
            self.assertEqual((source / 'camera.x3f').read_bytes(), b'original')
            sentinel = root / 'generated/keep.json'
            sentinel.write_text('existing run')
            with self.assertRaises(FileExistsError):
                prepare(source, root / 'generated', definitions, [], rust_inventory=inventory)
            self.assertEqual(sentinel.read_text(), 'existing run')

    def test_missing_or_tampered_rust_inputs_fail_before_any_output(self):
        for mutation in ('missing_provided', 'changed', 'length', 'set_escape'):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary).resolve()
                source, definitions, inventory = self.rust_fixture(root)
                name = next(name for name, row in inventory['additional_inputs'].items()
                            if row.get('origin', {}).get('status') == 'provided_file')
                if mutation == 'missing_provided':
                    (source / name).unlink()
                elif mutation == 'changed':
                    (source / name).write_bytes(b'tampered')
                elif mutation == 'length':
                    inventory['additional_inputs'][name]['bytes'] += 1
                else:
                    inventory['input_sets']['../escape'] = inventory['input_sets']['standard']
                with self.assertRaises(ValueError) as error:
                    prepare(source, root / 'generated', definitions, [], rust_inventory=inventory)
                if mutation == 'missing_provided':
                    self.assertIn('provide the exact inventory-pinned files', str(error.exception))
                self.assertFalse((root / 'generated').exists())
                self.assertEqual((source / 'camera.x3f').stat().st_nlink, 1)

    def test_copied_rust_bytes_are_rehashed_before_receipts_are_written(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            source, definitions, inventory = self.rust_fixture(root)
            before = {p.name: p.read_bytes() for p in source.iterdir()}
            with patch('prepare_inputs.shutil.copyfileobj', side_effect=lambda source, target: target.write(b'tampered')):
                with self.assertRaisesRegex(ValueError, 'pinned fixture SHA-256 differs'):
                    prepare(source, root / 'generated', definitions, [], rust_inventory=inventory)
            self.assertFalse((root / 'generated/rust-fixtures.json').exists())
            self.assertEqual({p.name: p.read_bytes() for p in source.iterdir()}, before)

    def test_paths_and_aliases_preserve_declared_recipes_and_originals(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            source, definitions = self.fixture(root)
            sidecar = source / 'foveonic/camera.x3f.foveonic.json'
            sidecar.parent.mkdir()
            sidecar.write_text('{"settings":{"exposure":99}}')
            before = copy.deepcopy(definitions)
            outputs = prepare(source, root / 'generated', definitions, [])
            alias = root / 'generated/aliases/second.x3f'
            self.assertEqual(alias.stat().st_ino, (source / 'camera.x3f').stat().st_ino)
            self.assertEqual(alias.read_bytes(), b'original')
            scenarios = json.loads(outputs['scenario-cases.json'].read_text())
            self.assertEqual(scenarios[0]['settings'], {'exposure': 0.15})
            self.assertEqual(scenarios[0]['source'], str(source / 'camera.x3f'))
            fixed={'settings':{'exposure':-0.6},'dehaze_estimate':True,'working_scale':1,'processing_quality':'quality'}
            definitions['render_cases'][0]['settings_snapshot']=fixed
            additional=prepare(source,root/'generated-fixed',definitions,[])
            rendered=json.loads(additional['render-cases.json'].read_text())
            self.assertEqual(rendered[0]['settings_snapshot'],fixed)
            definitions['render_cases'][0].pop('settings_snapshot')
            checkpoints = json.loads(outputs['checkpoint-cases.json'].read_text())
            self.assertEqual(checkpoints[0]['edits'], definitions['checkpoint_cases'][0]['edits'])
            self.assertEqual(checkpoints[0]['source'], str(source / 'camera.x3f'))
            regional = json.loads(outputs['regional-cases.json'].read_text())
            self.assertEqual(regional[0]['settings']['rotation'], {'degrees': 17.})
            fixture = json.loads(outputs['fixtures.json'].read_text())[0]
            self.assertEqual(fixture['sources'], [str(source / 'camera.x3f'), str(alias)])
            self.assertEqual(definitions, before)
            self.assertEqual(set(source.iterdir()), {source / 'camera.x3f', sidecar.parent})
            self.assertEqual(sidecar.read_text(), '{"settings":{"exposure":99}}')

    def test_missing_or_changed_originals_fail_before_generating_anything(self):
        for original in (None, b'changed'):
            with self.subTest(original=original), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary).resolve()
                source, definitions = self.fixture(root)
                path = source / 'camera.x3f'
                if original is None:
                    path.unlink()
                else:
                    path.write_bytes(original)
                with self.assertRaises(ValueError):
                    prepare(source, root / 'generated', definitions, [])
                self.assertFalse((root / 'generated').exists())

    def test_existing_generated_recipes_are_not_overwritten(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            source, definitions = self.fixture(root)
            out = root / 'generated'
            out.mkdir()
            (out / 'recipes.json').write_text('my experiment')
            with self.assertRaises(FileExistsError):
                prepare(source, out, definitions, [])
            self.assertEqual((out / 'recipes.json').read_text(), 'my experiment')

    def test_invalid_aliases_and_undeclared_sources_do_not_create_output(self):
        for change in ('parent', 'cycle', 'shadow', 'unknown'):
            with self.subTest(change=change), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary).resolve()
                source, definitions = self.fixture(root)
                if change == 'parent':
                    definitions['aliases'] = {'../second.x3f': 'camera.x3f'}
                elif change == 'cycle':
                    definitions['aliases'] = {'second.x3f': 'second.x3f'}
                elif change == 'shadow':
                    definitions['aliases'] = {'camera.x3f': 'camera.x3f'}
                else:
                    definitions['scenario_cases'][0]['source'] = 'unknown.x3f'
                with self.assertRaises(ValueError):
                    prepare(source, root / 'generated', definitions, [])
                self.assertFalse((root / 'generated').exists())

    def test_external_identity_has_one_owner(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            source, definitions = self.fixture(root)
            external = [{'file': 'camera.x3f', 'sha256': definitions['sample_hashes'].pop('camera.x3f')}]
            prepare(source, root / 'generated', definitions, external)
            definitions['sample_hashes']['camera.x3f'] = external[0]['sha256']
            with self.assertRaisesRegex(ValueError, 'duplicate'):
                prepare(source, root / 'other-output', definitions, external)

    def test_command_syncs_both_sources_and_forwards_offline(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            source, definitions = self.fixture(root)
            declaration = root / 'definitions.json'
            declaration.write_text(json.dumps(definitions))
            (root / 'external_raw_samples.json').write_text('{"files":[]}')
            args = ['prepare_inputs.py', '--source-dir', str(source),
                    '--directory', str(root / 'generated'), '--offline']
            with patch('sys.argv', args), patch('prepare_inputs.DEFINITIONS', declaration), \
                    patch('prepare_inputs.HERE', root), patch('prepare_inputs.subprocess.run') as command, \
                    redirect_stdout(io.StringIO()):
                main()
            self.assertEqual(command.call_count, 2)
            self.assertEqual(command.call_args_list[0].args[0],
                             ['cargo', 'xtask', 'examples', 'sync', '--source-dir', str(source), '--offline'])
            self.assertEqual(command.call_args_list[1].args[0],
                             [sys.executable, str(root / 'fetch_samples.py'), '--directory', str(source), '--offline'])
            self.assertTrue((root / 'generated/fixtures.json').is_file())

    def test_existing_output_fails_before_starting_a_source_sync(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with patch('sys.argv', ['prepare_inputs.py', '--directory', str(root)]), \
                    patch('prepare_inputs.subprocess.run') as command:
                with redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as stopped:
                    main()
            self.assertEqual(stopped.exception.code, 1)
            command.assert_not_called()

    def test_rust_flag_reuses_the_existing_sync_phase_only(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            source, definitions, inventory = self.rust_fixture(root)
            declaration, manifest = root / 'definitions.json', root / 'inventory.json'
            declaration.write_text(json.dumps(definitions))
            manifest.write_text(json.dumps(inventory))
            (root / 'external_raw_samples.json').write_text('{"files":[]}')
            with patch('sys.argv', ['prepare_inputs.py', '--source-dir', str(source), '--offline',
                                     '--rust-fixtures', '--directory', str(root / 'generated')]), \
                    patch('prepare_inputs.DEFINITIONS', declaration), patch('prepare_inputs.MANIFEST', manifest), \
                    patch('prepare_inputs.HERE', root), patch('prepare_inputs.subprocess.run') as command, \
                    redirect_stdout(io.StringIO()):
                main()
            self.assertEqual(command.call_count, 2)
            self.assertTrue(all(call.args[0][-1] == '--offline' for call in command.call_args_list))
            self.assertTrue((root / 'generated/rust-fixtures.json').is_file())


if __name__ == '__main__':
    unittest.main()

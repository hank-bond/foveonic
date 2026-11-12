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
                       'choice_cases': [{'source': 'second.x3f', 'settings': {'white_balance': 'Sunlight'}}],
                       'fixtures': [{'name': 'tiger', 'family': 'tiger', 'sources': ['camera.x3f', 'second.x3f']}]}
        return source, definitions

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


if __name__ == '__main__':
    unittest.main()

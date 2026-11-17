"""Check dependency setup is explicit, repeatable and preserves existing input workflows."""

from contextlib import redirect_stdout
import io
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
import zipfile

import setup_tests as setup


class SetupTests(unittest.TestCase):
    @staticmethod
    def dxc_archive(include_dxil=True):
        data = io.BytesIO()
        with zipfile.ZipFile(data, 'w') as package:
            package.writestr('bin/x64/dxcompiler.dll', b'compiler')
            if include_dxil:
                package.writestr('bin/x64/dxil.dll', b'validator')
            package.writestr('ignored/other.dll', b'other')
        return data.getvalue()

    def test_windows_dxc_is_explicit_verified_and_reused_in_every_loader_directory(self):
        payload = self.dxc_archive()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive, target = root/'cache'/'dxc.zip', root/'custom-target'
            with patch('setup_tests.platform.system', return_value='Windows'), \
                    patch('setup_tests.platform.machine', return_value='AMD64'), \
                    patch('setup_tests.DXC_SHA256', hashlib.sha256(payload).hexdigest()), \
                    patch('setup_tests.urllib.request.urlopen', return_value=io.BytesIO(payload)) as download, \
                    redirect_stdout(io.StringIO()):
                with self.assertRaisesRegex(ValueError, 'Missing pinned'):
                    setup.prepare_dxc(archive=archive, target=target)
                download.assert_not_called()
                self.assertFalse(root.joinpath('cache').exists())
                setup.prepare_dxc(True, archive=archive, target=target)
                self.assertEqual(download.call_count, 1)
                for profile in ('debug', 'release'):
                    for suffix in ('', 'deps'):
                        self.assertEqual((target/profile/suffix/'dxcompiler.dll').read_bytes(), b'compiler')
                        self.assertEqual((target/profile/suffix/'dxil.dll').read_bytes(), b'validator')
                setup.prepare_dxc(archive=archive, target=target)
                setup.prepare_dxc(True, archive=archive, target=target)
                self.assertEqual(download.call_count, 1)

    def test_windows_dxc_rejects_corruption_and_never_overwrites_a_conflicting_file(self):
        payload = self.dxc_archive()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive, target = root/'dxc.zip', root/'target'
            archive.write_bytes(payload)
            with patch('setup_tests.platform.system', return_value='Windows'), \
                    patch('setup_tests.platform.machine', return_value='x86_64'), \
                    patch('setup_tests.DXC_SHA256', hashlib.sha256(payload).hexdigest()):
                with self.assertRaisesRegex(ValueError, 'Missing staged'):
                    setup.prepare_dxc(archive=archive, target=target)
                self.assertFalse(target.exists())
                conflict = target/'release'/'dxcompiler.dll'
                conflict.parent.mkdir(parents=True)
                conflict.write_bytes(b'unrelated')
                with self.assertRaisesRegex(ValueError, 'differs'):
                    setup.prepare_dxc(True, archive=archive, target=target)
                self.assertEqual(conflict.read_bytes(), b'unrelated')
                self.assertFalse((target/'debug').exists())
                archive.write_bytes(b'corrupt')
                with self.assertRaisesRegex(ValueError, 'checksum'):
                    setup.prepare_dxc(True, archive=archive, target=target)

    def test_windows_dxc_rejects_download_mismatch_and_incomplete_archive(self):
        payload = self.dxc_archive(False)
        with tempfile.TemporaryDirectory() as temporary:
            archive = Path(temporary)/'cache'/'dxc.zip'
            with patch('setup_tests.platform.system', return_value='Windows'), \
                    patch('setup_tests.platform.machine', return_value='AMD64'), \
                    patch('setup_tests.urllib.request.urlopen', side_effect=lambda *args, **kwargs: io.BytesIO(payload)):
                with self.assertRaisesRegex(ValueError, 'checksum'):
                    setup.prepare_dxc(True, archive=archive, target=Path(temporary)/'target')
                self.assertFalse(archive.exists())
                with patch('setup_tests.DXC_SHA256', hashlib.sha256(payload).hexdigest()):
                    with self.assertRaisesRegex(ValueError, 'incomplete'):
                        setup.prepare_dxc(True, archive=archive, target=Path(temporary)/'target')
                self.assertFalse((Path(temporary)/'target').exists())

    def test_dxc_target_directory_uses_cargo_metadata_and_non_windows_is_a_noop(self):
        result = subprocess.CompletedProcess([], 0, json.dumps({'target_directory':'/custom/cargo-target'}), '')
        with patch('setup_tests.subprocess.run', return_value=result) as run:
            self.assertEqual(setup.cargo_target_directory(), Path('/custom/cargo-target'))
            self.assertEqual(run.call_args.args[0], ['cargo', 'metadata', '--locked', '--offline',
                                                      '--no-deps', '--format-version', '1'])
        with patch('setup_tests.platform.system', return_value='Darwin'), \
                patch('setup_tests.urllib.request.urlopen') as download, \
                patch('setup_tests.cargo_target_directory') as target:
            setup.prepare_dxc(True)
            download.assert_not_called()
            target.assert_not_called()

    def test_dxc_pin_matches_the_windows_release_workflow(self):
        workflow = (setup.ROOT/'.github/workflows/windows-test.yml').read_text().lower()
        self.assertIn(setup.DXC_URL.lower(), workflow)
        self.assertIn(setup.DXC_SHA256, workflow)

    def test_windows_compiler_check_requires_clang_cl(self):
        seen = []
        def version(tool):
            seen.append(tool)
            return 'installed'
        with patch('setup_tests.platform.system', return_value='Windows'), \
                patch('setup_tests.version', side_effect=version), redirect_stdout(io.StringIO()):
            setup.check_tools()
        compiler = next(tool for tool in seen if tool.name == 'C/C++ compiler')
        self.assertEqual(compiler.command, ('clang-cl', '--version'))
    def test_shared_python_check_is_read_only_when_environment_is_missing(self):
        with tempfile.TemporaryDirectory() as temporary, patch('setup_tests.subprocess.run') as run:
            with self.assertRaisesRegex(ValueError, 'shared Python test environment'):
                setup.prepare_python(directory=Path(temporary) / 'missing')
            run.assert_not_called()

    def test_shared_python_dependencies_are_reused_when_ready(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            python = root / ('Scripts/python.exe' if setup.sys.platform == 'win32' else 'bin/python')
            python.parent.mkdir()
            python.touch()
            with patch('setup_tests.subprocess.run', return_value=subprocess.CompletedProcess([], 0)) as run, \
                    redirect_stdout(io.StringIO()):
                self.assertEqual(setup.prepare_python(True, root), python)
                setup.prepare_python(True, root)
            self.assertEqual(run.call_count, 2)
            self.assertTrue(all(call.args[0][1] == '-c' for call in run.call_args_list))

    def test_missing_python_dependency_requires_explicit_install_and_is_rechecked(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            python = root / ('Scripts/python.exe' if setup.sys.platform == 'win32' else 'bin/python')
            python.parent.mkdir()
            python.touch()
            with patch('setup_tests.subprocess.run', return_value=subprocess.CompletedProcess([], 1)) as run:
                with self.assertRaisesRegex(ValueError, 'Pillow'):
                    setup.prepare_python(directory=root)
                self.assertEqual(run.call_count, 1)
            with patch('setup_tests.subprocess.run', side_effect=[subprocess.CompletedProcess([], 1),
                       subprocess.CompletedProcess([], 0), subprocess.CompletedProcess([], 0)]) as run, \
                    redirect_stdout(io.StringIO()):
                setup.prepare_python(True, root)
            self.assertEqual(run.call_args_list[1].args[0],
                             [str(python), '-m', 'pip', 'install', '-r', str(setup.PYTHON_REQUIREMENTS)])
            self.assertEqual(run.call_count, 3)

    def run_check(self, versions, **options):
        with patch('setup_tests.version', side_effect=lambda tool: versions.get(tool.name)), \
                patch('setup_tests.subprocess.run') as run, redirect_stdout(io.StringIO()):
            setup.check_tools(**options)
            return run

    def test_check_is_read_only_and_repeatable(self):
        versions = {tool.name: 'installed' for tool in setup.TOOLS}
        for install in [False, True]:
            run = self.run_check(versions, install=install)
            run.assert_not_called()

    def test_missing_requirement_fails_without_installing(self):
        for missing in ('ExifTool', 'Git'):
            versions = {tool.name: 'installed' for tool in setup.TOOLS if tool.name != missing}
            with self.subTest(missing=missing), patch('setup_tests.subprocess.run') as run:
                with self.assertRaisesRegex(ValueError, missing):
                    self.run_check(versions)
                run.assert_not_called()

    def test_install_only_missing_tools_then_recheck(self):
        seen = set()
        def version(tool):
            return 'installed' if tool.name != 'ExifTool' or seen else None
        def run(command, **kwargs):
            self.assertEqual(command, ['brew', 'install', 'exiftool'])
            seen.add('installed')
        with patch('setup_tests.version', side_effect=version), \
                patch('setup_tests.platform.system', return_value='Darwin'), \
                patch('setup_tests.shutil.which', return_value='/tool'), \
                patch('setup_tests.subprocess.run', side_effect=run) as invoked, \
                redirect_stdout(io.StringIO()):
            setup.check_tools(install=True)
            setup.check_tools(install=True)
            self.assertEqual(invoked.call_count, 1)

    def test_missing_bootstrap_or_unsupported_platform_does_not_install(self):
        for missing, system in [('Rust 1.88+', 'Darwin'), ('Git', 'Darwin'), ('ExifTool', 'Linux')]:
            with self.subTest(missing=missing), \
                    patch('setup_tests.version', side_effect=lambda tool: None if tool.name == missing else 'ok'), \
                    patch('setup_tests.platform.system', return_value=system), \
                    patch('setup_tests.subprocess.run') as run:
                with self.assertRaises(ValueError):
                    setup.check_tools(install=True)
                run.assert_not_called()

    def test_failed_install_stays_failed(self):
        with patch('setup_tests.version', side_effect=lambda tool: None if tool.formula else 'ok'), \
                patch('setup_tests.platform.system', return_value='Darwin'), \
                patch('setup_tests.shutil.which', return_value='/tool'), \
                patch('setup_tests.subprocess.run', side_effect=subprocess.CalledProcessError(1, 'brew')):
            with self.assertRaises(subprocess.CalledProcessError):
                setup.check_tools(install=True)

    def test_images_reuse_existing_sync_and_hash_verification(self):
        with patch('setup_tests.subprocess.run') as run:
            setup.prepare_images(Path('/sample-cache'), True)
        self.assertEqual(run.call_count, 2)
        self.assertEqual(run.call_args_list[0].args[0],
                         ['cargo', 'run', '--locked', '--package', 'xtask', '--offline', '--',
                          'examples', 'sync', '--source-dir', '/sample-cache', '--offline'])
        self.assertEqual(run.call_args_list[1].args[0][-3:], ['--directory', '/sample-cache', '--offline'])

    def test_github_needed_only_for_online_images(self):
        versions = {tool.name: 'ok' for tool in setup.TOOLS}
        self.run_check(versions, images=True, offline=True)
        with self.assertRaisesRegex(ValueError, 'GitHub CLI'):
            self.run_check(versions, images=True)

    def test_components_use_the_repository_toolchain(self):
        installed = []
        def version(tool):
            return None if tool.component and not installed else 'ok'
        with patch('setup_tests.version', side_effect=version), \
                patch('setup_tests.shutil.which', return_value='/rustup'), \
                patch('setup_tests.subprocess.run', side_effect=lambda *a, **k: installed.append((a, k))), \
                redirect_stdout(io.StringIO()):
            setup.check_tools(install=True)
        self.assertEqual(installed[0][0][0], ['rustup', 'component', 'add', 'rustfmt', 'clippy'])
        self.assertEqual(installed[0][1]['cwd'], setup.ROOT)

    def test_successful_installer_must_make_tool_available(self):
        with patch('setup_tests.version', side_effect=lambda tool: None if tool.formula else 'ok'), \
                patch('setup_tests.platform.system', return_value='Darwin'), \
                patch('setup_tests.shutil.which', return_value='/tool'), \
                patch('setup_tests.subprocess.run'), redirect_stdout(io.StringIO()):
            with self.assertRaisesRegex(ValueError, 'Missing test requirements'):
                setup.check_tools(install=True)

    def test_offline_install_is_rejected_before_running_anything(self):
        with patch('sys.argv', ['setup_tests.py', '--install', '--offline']), \
                patch('setup_tests.check_tools') as check, \
                patch('setup_tests.prepare_images') as images, \
                patch('sys.stderr', new_callable=io.StringIO):
            with self.assertRaises(SystemExit) as result:
                setup.main()
            self.assertEqual(result.exception.code, 2)
            check.assert_not_called()
            images.assert_not_called()

    def test_old_rust_is_rejected(self):
        with patch('setup_tests.shutil.which', return_value='/rustc'), \
                patch('setup_tests.subprocess.run', return_value=subprocess.CompletedProcess([], 0, 'rustc 1.87.0', '')):
            self.assertIsNone(setup.version(setup.TOOLS[0]))



if __name__ == '__main__':
    unittest.main()

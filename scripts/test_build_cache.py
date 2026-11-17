"""Check cache compatibility inputs and checkout exports without running build tools."""

from contextlib import redirect_stdout
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import build_cache


class BuildCacheTests(unittest.TestCase):
    def test_fingerprint_is_deterministic_and_ignores_unrelated_environment(self):
        first, identity = build_cache.fingerprint('host', {'rustc':'one', 'native':'two'}, {}, 'helper')
        repeated, _ = build_cache.fingerprint('host', {'native':'two', 'rustc':'one'},
            {'PRIVATE_TOKEN':'hidden', 'FOVEONIC_BUILD_REVISION':'different-source'}, 'helper')
        self.assertEqual(first, repeated)
        self.assertEqual(set(identity['environment']), set(build_cache.ENVIRONMENT))
        self.assertNotIn('hidden', json.dumps(identity))

    def test_tools_platform_helper_and_every_selected_environment_change_invalidate(self):
        first, _ = build_cache.fingerprint('host', {'rustc':'one'}, {}, 'helper')
        variations = [('new-host', {'rustc':'one'}, {}, 'helper'),
                      ('host', {'rustc':'two'}, {}, 'helper'), ('host', {'rustc':'one'}, {}, 'new-helper')]
        variations += [('host', {'rustc':'one'}, {name:'changed'}, 'helper') for name in build_cache.ENVIRONMENT]
        for inputs in variations:
            with self.subTest(inputs=inputs):
                self.assertNotEqual(build_cache.fingerprint(*inputs)[0], first)

    def run_main(self, system, revision='a'*40, tool_failure=False):
        parent = Path(__file__).resolve().parents[1]/'.local'
        parent.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(prefix='build-cache-', dir=parent) as temporary:
            root = Path(temporary)
            output, environment = root/'output', root/'environment'
            output.write_text('existing=value\n');environment.write_text('EXISTING=1\n')
            selected = {'GITHUB_OUTPUT':str(output), 'GITHUB_ENV':str(environment), 'CARGO_INCREMENTAL':'0',
                        'ImageVersion':'one-image', 'PRIVATE_TOKEN':'hidden'}
            calls, printed = [], io.StringIO()
            def command(argv, **kwargs):
                calls.append(argv)
                if tool_failure:raise subprocess.CalledProcessError(1, argv)
                return revision+'\n' if argv == ['git','rev-parse','HEAD'] else ' '.join(argv)+' version\n'
            try:
                with patch.dict(os.environ, selected, clear=True), patch('build_cache.platform.system', return_value=system), \
                        patch('build_cache.platform.platform', return_value='host'), \
                        patch('build_cache.subprocess.check_output', side_effect=command), redirect_stdout(printed):
                    build_cache.main()
            except BaseException:
                self.assertEqual(output.read_text(), 'existing=value\n')
                self.assertEqual(environment.read_text(), 'EXISTING=1\n')
                raise
            exported = dict(line.split('=', 1) for line in output.read_text().splitlines())
            self.assertEqual(environment.read_text(), 'EXISTING=1\nFOVEONIC_BUILD_REVISION='+revision+'\n')
            self.assertEqual(exported['existing'], 'value')
            self.assertEqual(exported['revision'], revision)
            identity = json.loads(printed.getvalue())
            self.assertNotIn('hidden', printed.getvalue())
            self.assertEqual(identity['helper_sha256'], hashlib.sha256(Path(build_cache.__file__).read_bytes()).hexdigest())
            return calls, exported['fingerprint']

    def test_both_platforms_probe_original_tools_and_export_actual_checked_source(self):
        expected = {
            'Darwin': [['rustc','-vV'], ['xcodebuild','-version'],
                       ['xcrun','--sdk','macosx','--show-sdk-version'], ['clang','--version'],
                       ['cmake','--version'], ['git','rev-parse','HEAD']],
            'Windows': [['rustc','-vV'], ['clang-cl','--version'], ['cmake','--version'],
                        ['git','rev-parse','HEAD']],
        }
        for system in expected:
            with self.subTest(system=system):
                calls, value = self.run_main(system)
                self.assertEqual(calls, expected[system])
                self.assertEqual(value, self.run_main(system, 'b'*40)[1])

    def test_tool_failure_and_malformed_checkout_never_export_a_fingerprint(self):
        with self.assertRaises(subprocess.CalledProcessError):self.run_main('Darwin', tool_failure=True)
        for revision in ('short', 'a'*40+'\nINJECTED=value'):
            with self.subTest(revision=revision), self.assertRaises(ValueError):self.run_main('Windows', revision)

    def test_unsupported_platform_fails_before_probing_tools(self):
        with patch('build_cache.platform.system', return_value='Linux'), \
                patch('build_cache.subprocess.check_output') as command:
            with self.assertRaises(ValueError):build_cache.main()
            command.assert_not_called()


if __name__ == '__main__':
    unittest.main()

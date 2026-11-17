"""Check that process monitoring flags known executables rather than shell command text."""

import unittest
import json
from pathlib import Path
import tempfile
from unittest.mock import patch
from watch_workloads import concurrent_workloads, is_workload, require_quiet, running_workloads


class WorkloadTests(unittest.TestCase):
    def test_known_executables(self):
        for command in ['cargo test --release', '/usr/bin/vmmap -summary 123',
                        '/toolchain/bin/rustc --crate-name foveonic src/lib.rs',
                        '/other-worktree/Foveonic.app/Contents/MacOS/foveonic-gui',
                        '/tmp/target/release/examples/profile_pipeline photo.x3f result',
                        '/tmp/target/release/examples/profile_checkpoints cases.json report.jsonl',
                        '/tmp/target/release/examples/profile_noise_resources cases.json',
                        '/tmp/target/release/examples/profile_smart_preview photo.x3f',
                        '/tmp/target/release/deps/foveonic-abc --ignored']:
            self.assertTrue(is_workload(command), command)

    def test_shell_text_and_monitor_are_not_executing_work(self):
        self.assertFalse(is_workload('/bin/bash -c "sleep 3; vmmap -summary 123"'))
        self.assertFalse(is_workload('python3 scripts/profiling/watch_workloads.py output.jsonl'))
        self.assertFalse(is_workload('cargo --version'))

    def test_compiler_reported_build_directory_tests_are_workloads(self):
        self.assertTrue(is_workload('/tmp/target/release/build/foveonic/hash/out/foveonic-ef1b7166103d7e92 --ignored'))
        self.assertTrue(is_workload('/tmp/tests/foveonic_gui-1234567890abcdef'))
        self.assertFalse(is_workload('/tmp/foveonic-report-1234567890abcdef'))

    def test_preflight_uses_executables_and_preserves_paths_with_spaces(self):
        rows = '10 /tmp/another tree/Foveonic.app/Contents/MacOS/foveonic-gui\n11 /toolchain/rustc\n12 /bin/zsh\n'
        with patch('watch_workloads.subprocess.check_output', return_value=rows):
            found = running_workloads(exclude_pids=[11])
            self.assertEqual(found, [{'pid':10,'executable':'/tmp/another tree/Foveonic.app/Contents/MacOS/foveonic-gui'}])
            with self.assertRaisesRegex(RuntimeError, 'profiling postponed'):
                require_quiet()
        with patch('watch_workloads.subprocess.check_output', return_value='12 /bin/zsh\n'):
            self.assertEqual(require_quiet()['potential_workloads'], [])

    def test_monitor_excludes_only_the_owned_application_pid(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'workloads.jsonl'
            path.write_text(json.dumps({'potential_workloads':[
                {'pid':10,'executable':'foveonic-gui'}, {'pid':11,'executable':'rustc'}]})+'\n')
            report = concurrent_workloads(path, [10])
            self.assertFalse(report['uncontended'])
            self.assertEqual(report['other_workloads'], [{'pid':11,'executable':'rustc'}])


if __name__ == '__main__':
    unittest.main()

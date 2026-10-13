"""Check that process monitoring flags known executables rather than shell command text."""

import unittest
from watch_workloads import is_workload


class WorkloadTests(unittest.TestCase):
    def test_known_executables(self):
        for command in ['cargo test --release', '/usr/bin/vmmap -summary 123',
                        '/tmp/target/release/examples/profile_pipeline photo.x3f result',
                        '/tmp/target/release/deps/foveonic-abc --ignored']:
            self.assertTrue(is_workload(command), command)

    def test_shell_text_and_monitor_are_not_executing_work(self):
        self.assertFalse(is_workload('/bin/bash -c "sleep 3; vmmap -summary 123"'))
        self.assertFalse(is_workload('python3 scripts/profiling/watch_workloads.py output.jsonl'))
        self.assertFalse(is_workload('cargo --version'))


if __name__ == '__main__':
    unittest.main()

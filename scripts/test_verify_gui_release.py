"""Check release GUI verification rejects missing evidence, failures and stalled apps."""

import json
from pathlib import Path
import struct
import subprocess
import tempfile
import unittest
import zlib
from unittest.mock import Mock, patch

import verify_gui_release as smoke


class ReleaseSmokeTests(unittest.TestCase):
    def test_capture_evidence_requires_all_complete_checkpoints(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            def chunk(kind, payload):
                return (struct.pack('>I', len(payload)) + kind + payload
                        + struct.pack('>I', zlib.crc32(kind + payload)))
            png = (b'\x89PNG\r\n\x1a\n'
                   + chunk(b'IHDR', struct.pack('>IIBBBBB', 2, 2, 8, 6, 0, 0, 0))
                   + chunk(b'IDAT', zlib.compress((b'\x00' + b'\xff' * 8) * 2))
                   + chunk(b'IEND', b''))
            for target in smoke.TARGETS[:-1]:
                (root / (target + '.png')).write_bytes(png)
            with self.assertRaises(FileNotFoundError):
                smoke.captures(root)
            terminal = root / (smoke.TARGETS[-1] + '.png')
            terminal.write_bytes(png[:-1])
            with self.assertRaises(ValueError):
                smoke.captures(root)
            terminal.write_bytes(png[:45] + bytes([png[45] ^ 1]) + png[46:])
            with self.assertRaises(ValueError):
                smoke.captures(root)
            terminal.write_bytes(png)
            self.assertEqual(smoke.captures(root), list(smoke.TARGETS))

    def test_successful_exit_without_gui_evidence_is_a_failure(self):
        self.run_failure(0, FileNotFoundError, killed=False)

    def test_stack_overflow_exit_is_a_failure(self):
        self.run_failure(-1073741571, ValueError, killed=False)

    def test_timeout_stops_only_the_owned_process_and_records_failure(self):
        self.run_failure(subprocess.TimeoutExpired('app', 1), subprocess.TimeoutExpired, killed=True)

    def run_failure(self, result, expected, killed):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root / 'app'
            source = root / 'blue-parking-sign.x3f'
            binary.write_bytes(b'app')
            source.write_bytes(b'fixture')
            output = root / 'run'
            process = Mock()
            process.wait.side_effect = [result, 0] if isinstance(result, Exception) else None
            process.wait.return_value = result
            process.poll.return_value = None if killed else result
            with patch.object(smoke, 'digest', return_value='verified'), \
                    patch.object(smoke.platform, 'platform', return_value='test-platform'), \
                    patch.object(smoke.json, 'loads', return_value={'sample_hashes': {source.name: 'verified'}}), \
                    patch.object(smoke.subprocess, 'Popen', return_value=process) as launch, \
                    patch.dict(smoke.os.environ, {'FOVEONIC_GUI_SMOKE_MULTI': '1', 'RUST_MIN_STACK': '8388608'}):
                with self.assertRaises(expected):
                    smoke.verify(binary, source, output, 1)
            report = json.loads((output / 'result.json').read_text())
            self.assertEqual(report['outcome'], 'failed')
            self.assertIn('failure', report)
            self.assertEqual(process.kill.called, killed)
            env = launch.call_args.kwargs['env']
            self.assertNotIn('FOVEONIC_GUI_SMOKE_MULTI', env)
            self.assertNotIn('RUST_MIN_STACK', env)


if __name__ == '__main__':
    unittest.main()

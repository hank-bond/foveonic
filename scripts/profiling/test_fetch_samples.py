"""Check verified sample downloads preserve existing originals and discard incomplete inputs."""

from contextlib import redirect_stdout
import hashlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from urllib.error import URLError

from fetch_samples import samples, sync


def sample(name='sample.x3f', data=b'RAW sample bytes'):
    return {'file': name, 'url': 'https://raw.pixls.us/sample',
            'sha256': hashlib.sha256(data).hexdigest(), 'size_bytes': len(data)}


class FetchSampleTests(unittest.TestCase):
    def test_cached_original_is_verified_offline_without_touching_it(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            path = root / 'sample.x3f'
            path.write_bytes(b'RAW sample bytes')
            before = path.stat()
            with patch('fetch_samples.urlopen') as network:
                self.assertEqual(sync(root, [sample()], offline=True), 0)
            network.assert_not_called()
            self.assertEqual(path.stat().st_ino, before.st_ino)
            self.assertEqual(path.stat().st_mtime_ns, before.st_mtime_ns)

    def test_mismatched_original_stops_before_fetching_missing_inputs(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            original = root / 'sample.x3f'
            original.write_bytes(b'private edited original')
            with patch('fetch_samples.urlopen') as network:
                with self.assertRaisesRegex(ValueError, 'does not match'):
                    sync(root, [sample('missing.x3f'), sample()])
            network.assert_not_called()
            self.assertEqual(original.read_bytes(), b'private edited original')
            self.assertEqual(list(root.iterdir()), [original])

    def test_offline_missing_inputs_do_not_create_a_directory(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / 'missing-directory'
            with patch('fetch_samples.urlopen') as network:
                with self.assertRaisesRegex(ValueError, 'sample.x3f'):
                    sync(root, [sample()], offline=True)
            network.assert_not_called()
            self.assertFalse(root.exists())

    def test_download_is_verified_and_published_without_staging_debris(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / 'samples'
            with patch('fetch_samples.urlopen', return_value=io.BytesIO(b'RAW sample bytes')) as network:
                with redirect_stdout(io.StringIO()):
                    self.assertEqual(sync(root, [sample()]), 1)
            network.assert_called_once_with(sample()['url'], timeout=30)
            self.assertEqual((root / 'sample.x3f').read_bytes(), b'RAW sample bytes')
            self.assertEqual([path.name for path in root.iterdir()], ['sample.x3f'])

    def test_corrupt_or_truncated_download_never_becomes_an_original(self):
        for body in (b'short', b'BAD sample bytes'):
            with self.subTest(body=body), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                with patch('fetch_samples.urlopen', return_value=io.BytesIO(body)):
                    with redirect_stdout(io.StringIO()), self.assertRaises(ValueError):
                        sync(root, [sample()])
                self.assertEqual(list(root.iterdir()), [])

    def test_interrupted_stream_removes_partial_download(self):
        class BrokenResponse(io.BytesIO):
            def read(self, size=-1):
                if self.tell():
                    raise URLError('connection interrupted')
                return super().read(3)

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with patch('fetch_samples.urlopen', return_value=BrokenResponse(b'RAW sample bytes')):
                with redirect_stdout(io.StringIO()), self.assertRaises(URLError):
                    sync(root, [sample()])
            self.assertEqual(list(root.iterdir()), [])

    def test_publication_never_overwrites_a_concurrently_created_original(self):
        for body in (b'RAW sample bytes', b'another original'):
            with self.subTest(body=body), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                def response(*args, **kwargs):
                    (root / 'sample.x3f').write_bytes(body)
                    return io.BytesIO(b'RAW sample bytes')
                with patch('fetch_samples.urlopen', side_effect=response):
                    with redirect_stdout(io.StringIO()):
                        if body == b'RAW sample bytes':
                            self.assertEqual(sync(root, [sample()]), 1)
                        else:
                            with self.assertRaisesRegex(ValueError, 'does not match'):
                                sync(root, [sample()])
                self.assertEqual((root / 'sample.x3f').read_bytes(), body)
                self.assertEqual([path.name for path in root.iterdir()], ['sample.x3f'])

    def test_manifest_rejects_duplicate_and_parent_filenames(self):
        for records in ([], [sample(), sample()], [sample('../outside.x3f')]):
            with self.subTest(records=records), patch('fetch_samples.MANIFEST') as manifest:
                manifest.read_text.return_value = json.dumps({'files': records})
                with self.assertRaises(ValueError):
                    samples()


if __name__ == '__main__':
    unittest.main()

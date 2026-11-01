"""Sample settled macOS memory at explicit boundaries in an owned headless render run."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time


def digest(path):
    with path.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--cases', type=Path, required=True)
    parser.add_argument('--directory', type=Path, required=True)
    parser.add_argument('--route', choices=['async', 'staged', 'switch'], required=True)
    parser.add_argument('--repeats', type=int, default=3)
    args = parser.parse_args()
    if sys.platform != 'darwin':
        parser.error('memory capture requires macOS vmmap')
    if args.repeats < 1:
        parser.error('at least one repetition is required')
    directory = args.directory.resolve()
    directory.mkdir(parents=True, exist_ok=False)
    reports = directory / 'renders.jsonl'
    command = [str(args.binary.resolve(strict=True)), str(args.cases.resolve(strict=True)),
               args.route, 'off', str(args.repeats), '5000', str(reports)]
    environment = {key: value for key, value in os.environ.items()
                   if not key.startswith('FOVEONIC_GUI_SMOKE_') and key != 'FOVEONIC_PROFILE'}
    identities = {'binary_sha256': digest(args.binary), 'cases_sha256': digest(args.cases)}
    with (directory / 'stderr.txt').open('w') as errors, \
            (directory / 'samples.jsonl').open('w') as samples:
        process = subprocess.Popen(command, env=environment, stdout=subprocess.DEVNULL,
                                   stderr=errors)
        consumed = 0
        try:
            (directory / 'manifest.json').write_text(json.dumps({
                'pid': process.pid, 'command': command, 'mode': 'off', **identities,
                'sampling': 'vmmap after completion, outside render interval',
                'hold_ms': 5000,
            }, indent=2) + '\n')
            while process.poll() is None:
                lines = reports.read_text().splitlines(keepends=True) if reports.exists() else []
                for line in lines[consumed:]:
                    if not line.endswith('\n'):
                        break
                    event = json.loads(line)
                    began = time.monotonic()
                    snapshot = subprocess.run(['vmmap', '-summary', str(process.pid)],
                                              capture_output=True, text=True)
                    filename = f'{consumed:02}-{event["event"]}.txt'
                    (directory / filename).write_text(snapshot.stdout + snapshot.stderr)
                    summary = [line.strip() for line in snapshot.stdout.splitlines()
                               if line.startswith('Physical footprint')]
                    record = {'event': event['event'], 'iteration': event['data'].get('iteration'),
                              'snapshot': filename, 'summary': summary,
                              'sampling_ms': (time.monotonic() - began) * 1000,
                              'returncode': snapshot.returncode}
                    samples.write(json.dumps(record) + '\n')
                    samples.flush()
                    print(json.dumps(record), flush=True)
                    if snapshot.returncode:
                        raise RuntimeError('memory snapshot failed, inspect the raw capture')
                    if record['sampling_ms'] >= 5000:
                        raise RuntimeError('memory snapshot exceeded the hold interval')
                    consumed += 1
                time.sleep(.1)
            if process.returncode:
                raise SystemExit(process.returncode)
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait()


if __name__ == '__main__':
    main()

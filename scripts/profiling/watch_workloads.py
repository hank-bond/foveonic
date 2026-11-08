"""Check and record actual app, compiler and measurement processes that can contaminate timings."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import time


def is_workload(command):
    executable, _, arguments = command.strip().partition(' ')
    name = Path(executable).name
    if name == 'cargo':
        return any(verb in arguments.split() for verb in ['test', 'build', 'clippy', 'run'])
    return (is_workload_executable(executable)
            or '/deps/foveonic' in executable)


def is_workload_executable(executable):
    name = Path(executable).name
    return (name in {'cargo', 'rustc', 'clang', 'clang++', 'cc', 'cc1', 'ld',
                    'foveonic', 'foveonic-gui', 'Foveonic', 'vmmap', 'footprint',
                    'profile_pipeline', 'profile_editing', 'browser_pipeline'}
            or ('/deps/' in executable and name.startswith(('foveonic-', 'foveonic_gui-'))))


def running_workloads(exclude_pids=()):
    # comm identifies the executing program, preserving spaces in its path.
    # Arguments mentioning rustc/Foveonic are not evidence that either is running.
    rows = subprocess.check_output(['ps', '-axo', 'pid=,comm='], text=True).splitlines()
    excluded = {os.getpid(), *exclude_pids}
    found = []
    for row in rows:
        pid, _, executable = row.strip().partition(' ')
        executable = executable.strip()
        if pid and int(pid) not in excluded and is_workload_executable(executable):
            found.append({'pid': int(pid), 'executable': executable})
    return found


def require_quiet():
    found = running_workloads()
    if found:
        details = ', '.join(f"{item['pid']} {item['executable']}" for item in found)
        raise RuntimeError(f'profiling postponed while other workloads are running: {details}')
    return {'monotonic_ns': time.monotonic_ns(), 'potential_workloads': []}


def concurrent_workloads(path, owned_pids=()):
    """Report interference separately from behavioral correctness, excluding our child app."""
    samples = [json.loads(line) for line in Path(path).read_text().splitlines()]
    excluded = set(owned_pids)
    found = {(item['pid'], item['executable']) for sample in samples
             for item in sample['potential_workloads'] if item['pid'] not in excluded}
    return {'checked': True, 'uncontended': not found, 'samples': len(samples),
            'other_workloads': [{'pid': pid, 'executable': exe} for pid, exe in sorted(found)]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    with args.output.open('x') as output:
        while True:
            output.write(json.dumps({'monotonic_ns': time.monotonic_ns(),
                                     'potential_workloads': running_workloads()}) + '\n')
            output.flush()
            time.sleep(.5)


if __name__ == '__main__':
    main()

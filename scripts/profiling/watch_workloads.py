"""Flag known executing build, test and measurement programs without interpreting shell text as GPU contention."""

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
    return (name in ['vmmap', 'footprint', 'profile_pipeline']
            or '/deps/foveonic' in executable)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    with args.output.open('x') as output:
        while True:
            processes = subprocess.check_output(['ps', '-axo', 'pid=,command='], text=True).splitlines()
            candidates = []
            for process in processes:
                pid, _, command = process.strip().partition(' ')
                if int(pid) != os.getpid() and is_workload(command):
                    candidates.append(process.strip())
            output.write(json.dumps({'monotonic_ns': time.monotonic_ns(),
                                     'potential_workloads': candidates}) + '\n')
            output.flush()
            time.sleep(.5)


if __name__ == '__main__':
    main()

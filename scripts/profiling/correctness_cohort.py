"""Authorize owned macOS slider workers without weakening ordinary quiet-host checks."""

import json
import os
from pathlib import Path
import signal
import subprocess
import time
import uuid

from file_hash import digest
from process_memory import birth_reader
from watch_workloads import is_workload_executable


class CohortStopped(RuntimeError):
    """The controller stopped or this worker lost its authorization to continue."""


def process_snapshot():
    rows = subprocess.check_output(['ps', '-axo', 'pid=,ppid=,pgid=,state=,comm='], text=True)
    result = []
    for line in rows.splitlines():
        fields = line.strip().split(None, 4)
        if len(fields) == 5:
            pid, ppid, pgid, state, comm = fields
            result.append({'pid': int(pid), 'ppid': int(ppid), 'pgid': int(pgid), 'state': state, 'comm': comm})
    return result


def _session(row):
    return (row['pgid'] if 'pgid' in row else os.getpgid(row['pid']),
            row['sid'] if 'sid' in row else os.getsid(row['pid']))


def _same_birth(read, identity):
    try:
        return read(identity['pid']) == identity['birth']
    except ProcessLookupError:
        return False


def _worker_live(member, controller, rows, read):
    row = rows.get(member['pid'])
    if row is None or not _same_birth(read, member):
        return False
    try:
        return (row['ppid'] == controller['pid'] and _session(row) == (member['pid'], member['pid'])
                and _same_birth(read, member))
    except ProcessLookupError:
        return False


def _member_birth(row, group, read, group_reader):
    """Do not authorize a reused PID from an earlier process-group snapshot."""
    if row['pgid'] != group:
        return None
    before = read(row['pid'])
    return before if group_reader(row['pid']) == group and read(row['pid']) == before else None


def _scan(document, snapshot, read, anchors=None, group_reader=None):
    group_reader = group_reader or os.getpgid
    rows = {row['pid']: row for row in snapshot() if not row.get('state', '').startswith('Z')}
    live = {key: member for key, member in document['workers'].items()
            if _worker_live(member, document['controller'], rows, read)}
    binaries = {item['path'] for item in document['binaries']}
    binary_names = {Path(path).name for path in binaries}
    authorized, foreign = [], []
    for row in rows.values():
        try:
            owner = next((key for key, member in live.items() if row['pgid'] == member['pid']), None)
            birth = None if owner is None else _member_birth(row, live[owner]['pid'], read, group_reader)
        except ProcessLookupError:
            continue
        if birth is None:
            owner = None
        if owner is not None and anchors is not None:
            anchors.setdefault(owner, {})[row['pid']] = birth
        if Path(row['comm']).name not in binary_names and not is_workload_executable(row['comm']):
            continue
        # Protected unrelated processes need no filesystem or per-PID queries.
        try:
            executable = str(Path(row['comm']).resolve())
        except PermissionError:
            executable = None
        observed = {'pid': row['pid'], 'executable': row['comm']}
        if owner is not None and executable in binaries:
            authorized.append(observed)
        else:
            foreign.append(observed)
    receipt = {'monotonic_ns': time.monotonic_ns(), 'potential_workloads': foreign,
               'authorized_peers': authorized, 'timing_valid': False}
    if foreign:
        details = ', '.join(f"{item['pid']} {item['executable']}" for item in foreign)
        raise RuntimeError(f'correctness postponed while unrelated workloads are running: {details}')
    return receipt


class CohortPolicy:
    """Explicit worker capability; the descriptor is written only by its live parent."""

    def __init__(self, path, group_id, *, snapshot=None, birth=None, group_reader=None):
        self.path = Path(path)
        self.group_id = str(group_id)
        self.snapshot = snapshot or process_snapshot
        self.birth = birth or birth_reader()
        self.group_reader = group_reader
        self.pid = os.getpid()
        self.own_birth = self.birth(self.pid)
        self.run_id = None

    def _document(self):
        document = json.loads(self.path.read_text())
        if self.run_id is not None and document['id'] != self.run_id:
            raise RuntimeError('correctness cohort identity changed')
        if document['status'] != 'active' or not _same_birth(self.birth, document['controller']):
            raise CohortStopped('correctness controller is stopped or no longer alive')
        return document

    def check_alive(self):
        document = self._document()
        member = document['workers'].get(self.group_id)
        if member is None or member['pid'] != self.pid or member['birth'] != self.own_birth:
            raise CohortStopped('correctness worker is not registered with its birth identity')
        if os.getppid() != document['controller']['pid']:
            raise CohortStopped('correctness worker lost its controller parent')
        if (os.getpgid(self.pid), os.getsid(self.pid)) != (self.pid, self.pid):
            raise RuntimeError('correctness worker session changed')
        self.run_id = document['id']
        return document

    def wait_registered(self, timeout=5):
        deadline = time.monotonic() + timeout
        while True:
            document = self._document()
            if self.group_id in document['workers']:
                self.check_alive()
                return
            if time.monotonic() >= deadline:
                raise RuntimeError('correctness worker registration timed out')
            time.sleep(.02)

    def reference(self):
        document = self.check_alive()
        return {'run_id': document['id'], 'group_id': self.group_id,
                'controller': dict(document['controller']), 'descriptor': str(self.path),
                'worker': dict(document['workers'][self.group_id])}

    def check_quiet(self):
        return _scan(self.check_alive(), self.snapshot, self.birth, group_reader=self.group_reader)


class Controller:
    """Register a finite worker cohort and clean only birth-verified owned processes."""

    def __init__(self, directory, binary, renderer, groups_metadata=None, *, snapshot=None, birth=None, group_reader=None):
        self.snapshot = snapshot or process_snapshot
        self.birth = birth or birth_reader()
        self.group_reader = group_reader or os.getpgid
        self.path = Path(directory) / 'cohort.json'
        self.anchors = {}
        self.document = {'schema_version': 1, 'id': uuid.uuid4().hex, 'status': 'active',
                         'controller': {'pid': os.getpid(), 'birth': self.birth(os.getpid())},
                         'binaries': [{'path': str(Path(path).resolve()), 'sha256': digest(path)}
                                      for path in (binary, renderer)],
                         'expected_groups': groups_metadata if groups_metadata is not None else {},
                         'workers': {}}
        if self.path.exists():
            raise ValueError('correctness cohort descriptor already exists')
        self._save()

    def _save(self):
        temporary = self.path.with_suffix('.tmp')
        temporary.write_text(json.dumps(self.document, indent=2) + '\n')
        temporary.replace(self.path)

    def register(self, group_id, process):
        key = str(group_id)
        if (self.document['status'] != 'active' or key in self.document['workers']
                or any(member['pid'] == process.pid for member in self.document['workers'].values())):
            raise RuntimeError('correctness cohort is stopped or group already registered')
        row = next((row for row in self.snapshot() if row['pid'] == process.pid), None)
        if row is None or row['ppid'] != os.getpid() or _session(row) != (process.pid, process.pid):
            raise RuntimeError('correctness worker must be an owned session leader')
        member = {'pid': process.pid, 'birth': self.birth(process.pid), 'pgid': process.pid, 'sid': process.pid}
        self.document['workers'][key] = member
        self.anchors[key] = {process.pid: member['birth']}
        self._save()

    def observe(self):
        if not _same_birth(self.birth, self.document['controller']):
            raise RuntimeError('correctness controller birth identity changed')
        return _scan(self.document, self.snapshot, self.birth, self.anchors, self.group_reader)

    def check_quiet(self):
        return self.observe()

    def abort(self):
        self.document['status'] = 'aborted'
        self._save()

    def _survivors(self, key):
        member = self.document['workers'][key]
        known = self.anchors[key]
        result = []
        for row in self.snapshot():
            if row.get('state', '').startswith('Z'):
                continue
            try:
                same_group = row['pgid'] == member['pid']
                if same_group:
                    same_group = _member_birth(row, member['pid'], self.birth, self.group_reader) is not None
                anchored = row['pid'] in known and self.birth(row['pid']) == known[row['pid']]
            except ProcessLookupError:
                continue
            if same_group or anchored:
                result.append((row, anchored))
        return result

    def _signal_owned(self, key, signum):
        member = self.document['workers'][key]
        rows = {row['pid']: row for row in self.snapshot()}
        if _worker_live(member, self.document['controller'], rows, self.birth):
            # A live birth-verified session leader anchors the entire owned group.
            try:
                os.killpg(member['pid'], signum)
            except ProcessLookupError:
                pass
            return
        # Without the leader, signal only identities captured while it was alive.
        # Short-lived startup helpers can finish after their parent exits, before
        # a sample records them. Let them exit naturally rather than guessing ownership.
        deadline = time.monotonic() + 2
        survivors = self._survivors(key)
        for row, anchored in survivors:
            if anchored:
                try:
                    if _same_birth(self.birth, {'pid': row['pid'], 'birth': self.anchors[key][row['pid']]}):
                        os.kill(row['pid'], signum)
                except ProcessLookupError:
                    pass
        unknown = [row['pid'] for row, anchored in survivors if not anchored]
        grace = {'unanchored_pids': unknown, 'grace_seconds': 2, 'outcome': 'waiting'}
        if unknown:
            self.document.setdefault('cleanup', {})[key] = grace
        while any(not anchored for _, anchored in survivors):
            if time.monotonic() >= deadline:
                grace['outcome'] = 'blocked'
                self.abort()
                raise RuntimeError('cannot prove ownership of surviving correctness group; cleanup blocked')
            time.sleep(.02)
            survivors = self._survivors(key)
        grace['outcome'] = 'exited'

    def terminate(self, group_id, process, grace=10):
        key = str(group_id)
        member = self.document['workers'][key]
        if process.pid != member['pid']:
            raise RuntimeError('cleanup process does not match registered worker')
        if _same_birth(self.birth, member):
            try:
                os.kill(member['pid'], signal.SIGINT)
            except ProcessLookupError:
                pass
        try:
            process.wait(timeout=grace)
        except subprocess.TimeoutExpired:
            pass
        for signum in (signal.SIGTERM, signal.SIGKILL):
            if not self._survivors(key):
                break
            self._signal_owned(key, signum)
            try:
                process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                pass
        process.wait(timeout=5)
        self.unregister(key)

    def unregister(self, group_id):
        key = str(group_id)
        if key not in self.document['workers']:
            raise RuntimeError('correctness worker is not registered')
        if _same_birth(self.birth, self.document['workers'][key]):
            raise RuntimeError('cannot unregister a live correctness worker')
        if self._survivors(key):
            self._signal_owned(key, signal.SIGKILL)
            deadline = time.monotonic() + 2
            while self._survivors(key) and time.monotonic() < deadline:
                time.sleep(.02)
            if self._survivors(key):
                self.abort()
                raise RuntimeError('owned correctness children survived cleanup')
        del self.document['workers'][key]
        self.anchors.pop(key, None)
        self._save()

    def close(self):
        if self.document['workers']:
            raise RuntimeError('cannot close correctness cohort with registered workers')
        if self.document['status'] != 'aborted':
            self.document['status'] = 'closed'
        self._save()

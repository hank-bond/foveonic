"""Check cohort registration, foreign-workload rejection and birth-safe cleanup."""

import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from correctness_cohort import CohortPolicy, CohortStopped, Controller


class FakeProcess:
    def __init__(self, pid, world):
        self.pid, self.world = pid, world

    def wait(self, timeout=None):
        if self.pid in self.world.rows:
            raise subprocess.TimeoutExpired('worker', timeout)
        return -9


class World:
    def __init__(self, parent):
        self.rows, self.births, self.signals = {}, {}, []
        self.add(parent, 1, parent)

    def add(self, pid, ppid, group, comm='/usr/bin/python3'):
        self.rows[pid] = dict(pid=pid, ppid=ppid, pgid=group, sid=group, state='S', comm=comm)
        self.births[pid] = pid * 10

    def birth(self, pid):
        if pid not in self.rows:
            raise ProcessLookupError(pid)
        return self.births[pid]

    def snapshot(self):
        return list(self.rows.values())

    def group(self, pid):
        if pid not in self.rows:raise ProcessLookupError(pid)
        return self.rows[pid]['pgid']

    def kill(self, pid, signum):
        self.signals.append((pid, signum))
        if signum == signal.SIGKILL:
            self.rows.pop(pid, None)

    def killpg(self, group, signum):
        self.signals.append(('group', group, signum))
        if signum == signal.SIGKILL:
            for pid in list(self.rows):
                if self.rows[pid]['pgid'] == group:
                    self.rows.pop(pid)


class CorrectnessCohortTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)
        self.binary, self.renderer = self.directory/'foveonic-gui', self.directory/'profile_pipeline'
        self.binary.write_bytes(b'gui')
        self.renderer.write_bytes(b'renderer')
        self.parent = os.getpid()
        self.world = World(self.parent)
        self.controller = Controller(self.directory, self.binary, self.renderer,
                                     snapshot=self.world.snapshot, birth=self.world.birth, group_reader=self.world.group)
        self.worker = self.parent + 100000
        self.world.add(self.worker, self.parent, self.worker)
        self.process = FakeProcess(self.worker, self.world)

    def policy_context(self):
        return patch.multiple('correctness_cohort.os', getpid=lambda: self.worker,
                              getppid=lambda: self.parent, getpgid=lambda _: self.worker,
                              getsid=lambda _: self.worker)

    def test_registration_handshake_waits_for_parent_registration(self):
        def register(_):
            with patch('correctness_cohort.os.getpid', return_value=self.parent):
                self.controller.register('a', self.process)
        with self.policy_context():
            policy = CohortPolicy(self.controller.path, 'a', snapshot=self.world.snapshot, birth=self.world.birth, group_reader=self.world.group)
            with patch('correctness_cohort.time.sleep', side_effect=register) as sleep:
                policy.wait_registered()
            sleep.assert_called_once()
            self.assertEqual(policy.reference()['run_id'], self.controller.document['id'])

    def test_registration_timeout_and_wrong_parent(self):
        with self.policy_context():
            policy = CohortPolicy(self.controller.path, 'a', snapshot=self.world.snapshot, birth=self.world.birth, group_reader=self.world.group)
            with self.assertRaisesRegex(RuntimeError, 'timed out'):
                policy.wait_registered(timeout=0)
        self.world.rows[self.worker]['ppid'] = 999
        with self.assertRaisesRegex(RuntimeError, 'owned session leader'):
            self.controller.register('a', self.process)

    def test_exact_binary_in_registered_group_only(self):
        self.controller.register('a', self.process)
        self.world.add(self.worker+1, self.worker, self.worker, str(self.binary))
        receipt = self.controller.observe()
        self.assertEqual(len(receipt['authorized_peers']), 1)
        self.assertFalse(receipt['timing_valid'])
        self.world.add(self.worker+2, 1, self.worker+2, str(self.binary))
        with self.assertRaisesRegex(RuntimeError, 'unrelated workloads'):
            self.controller.observe()
        self.world.rows.pop(self.worker+2)
        self.world.add(self.worker+3, self.worker, self.worker, '/usr/bin/rustc')
        with self.assertRaisesRegex(RuntimeError, 'rustc'):
            self.controller.observe()

    def test_exited_zombie_is_neither_a_workload_nor_a_cleanup_target(self):
        self.controller.register('a', self.process)
        child = self.worker+1
        self.world.add(child, self.worker, self.worker, str(self.binary))
        self.world.rows[child]['state'] = 'Z'
        self.assertEqual(self.controller.observe()['authorized_peers'], [])
        self.world.rows.pop(self.worker)
        with patch('correctness_cohort.os.kill') as kill:
            self.controller.unregister('a')
        kill.assert_not_called()

    def test_declared_renamed_renderer_is_monitored_inside_and_outside_its_worker(self):
        self.controller.register('a', self.process)
        renamed=self.directory/'renderer-candidate'
        renamed.write_bytes(b'renderer')
        self.controller.document['binaries'].append({'path':str(renamed.resolve()),'sha256':'identity'})
        self.world.add(self.worker+1,self.worker,self.worker,str(renamed))
        self.assertEqual(self.controller.observe()['authorized_peers'][0]['pid'],self.worker+1)
        self.world.add(self.worker+2,1,self.worker+2,str(renamed))
        with self.assertRaisesRegex(RuntimeError,'unrelated workloads'):
            self.controller.observe()

    def test_protected_unrelated_process_needs_no_per_pid_or_path_query(self):
        self.controller.register('a', self.process)
        protected=self.worker+1
        self.world.add(protected,1,protected,'/protected/private/service')
        def group(pid):
            if pid==protected:raise PermissionError('protected system process')
            return self.world.group(pid)
        self.controller.group_reader=group
        with patch('correctness_cohort.Path.resolve',side_effect=PermissionError('protected path')):
            self.assertEqual(self.controller.observe()['authorized_peers'],[])
            self.assertEqual([row['pid'] for row,_ in self.controller._survivors('a')],[self.worker])
            self.world.rows[protected]['comm']='/protected/foveonic-gui'
            with self.assertRaisesRegex(RuntimeError,'unrelated workloads'):
                self.controller.observe()

    def test_stale_group_snapshot_never_anchors_or_kills_a_reused_pid(self):
        self.controller.register('a', self.process)
        child=self.worker+1
        self.world.add(child,self.worker,self.worker,str(self.binary))
        self.controller.group_reader=lambda pid: child if pid==child else self.world.group(pid)
        with self.assertRaisesRegex(RuntimeError,'unrelated workloads'):
            self.controller.observe()
        self.assertNotIn(child,self.controller.anchors['a'])
        self.world.rows.pop(self.worker)
        with patch('correctness_cohort.os.kill') as kill:
            self.controller.unregister('a')
        kill.assert_not_called()

    def test_worker_pid_reuse_removes_authorization(self):
        self.controller.register('a', self.process)
        self.world.add(self.worker+1, self.worker, self.worker, str(self.binary))
        self.world.births[self.worker] += 1
        with self.assertRaisesRegex(RuntimeError, 'unrelated workloads'):
            self.controller.observe()

    def test_controller_death_and_abort_fail_closed(self):
        self.controller.register('a', self.process)
        with self.policy_context():
            policy = CohortPolicy(self.controller.path, 'a', snapshot=self.world.snapshot, birth=self.world.birth, group_reader=self.world.group)
            policy.wait_registered()
            self.world.births[self.parent] += 1
            with self.assertRaisesRegex(CohortStopped, 'controller'):
                policy.check_alive()
            self.world.births[self.parent] -= 1
            self.controller.abort()
            with self.assertRaisesRegex(CohortStopped, 'controller'):
                policy.check_alive()
        with self.assertRaisesRegex(RuntimeError, 'stopped'):
            self.controller.register('b', self.process)

    def test_cleanup_escalates_and_reaps_owned_group(self):
        self.controller.register('a', self.process)
        self.world.add(self.worker+1, self.worker, self.worker, str(self.binary))
        self.controller.observe()
        with patch('correctness_cohort.os.kill', side_effect=self.world.kill), \
                patch('correctness_cohort.os.killpg', side_effect=self.world.killpg):
            self.controller.terminate('a', self.process, grace=0)
        self.assertIn((self.worker, signal.SIGINT), self.world.signals)
        self.assertIn(('group', self.worker, signal.SIGKILL), self.world.signals)
        self.assertFalse(self.controller.document['workers'])
        self.controller.close()
        self.assertEqual(json.loads(self.controller.path.read_text())['status'], 'closed')

    def test_leader_loss_uses_child_anchors(self):
        self.controller.register('a', self.process)
        child = self.worker+1
        self.world.add(child, self.worker, self.worker, str(self.binary))
        self.controller.observe()
        self.world.rows.pop(self.worker)
        with patch('correctness_cohort.os.kill', side_effect=self.world.kill), \
                patch('correctness_cohort.os.killpg') as killpg:
            self.controller.unregister('a')
        killpg.assert_not_called()
        self.assertEqual(self.world.signals, [(child, signal.SIGKILL)])

    def test_group_exit_between_identity_check_and_signal_is_harmless(self):
        self.controller.register('a', self.process)
        def exit_group(*_):
            self.world.rows.pop(self.worker)
            raise ProcessLookupError(self.worker)
        with patch('correctness_cohort.os.killpg', side_effect=exit_group):
            self.controller._signal_owned('a', signal.SIGTERM)
        self.controller.unregister('a')
        self.assertFalse(self.controller.document['workers'])

    def test_reused_child_pid_outside_group_is_never_signaled(self):
        self.controller.register('a', self.process)
        child = self.worker+1
        self.world.add(child, self.worker, self.worker, str(self.binary))
        self.controller.observe()
        self.world.rows.pop(self.worker)
        self.world.add(child, 1, child)
        self.world.births[child] += 1
        with patch('correctness_cohort.os.kill') as kill, patch('correctness_cohort.os.killpg') as killpg:
            self.controller.unregister('a')
        kill.assert_not_called()
        killpg.assert_not_called()

    def test_unobserved_orphan_blocks_cleanup_without_signal(self):
        self.controller.register('a', self.process)
        self.world.add(self.worker+1, self.worker, self.worker, str(self.binary))
        self.world.rows.pop(self.worker)
        with patch('correctness_cohort.os.kill') as kill, patch('correctness_cohort.os.killpg') as killpg, \
                patch('correctness_cohort.time.monotonic', side_effect=[0, 3]):
            with self.assertRaisesRegex(RuntimeError, 'cannot prove ownership'):
                self.controller.unregister('a')
        kill.assert_not_called()
        killpg.assert_not_called()
        self.assertEqual(self.controller.document['status'], 'aborted')
        self.assertEqual(self.controller.document['cleanup']['a']['outcome'], 'blocked')

    def test_unobserved_short_lived_helper_may_exit_naturally_before_cleanup(self):
        self.controller.register('a', self.process)
        child = self.worker+1
        self.world.add(child, self.worker, self.worker, '/usr/sbin/system_profiler')
        self.world.rows.pop(self.worker)
        with patch('correctness_cohort.os.kill') as kill, patch('correctness_cohort.os.killpg') as killpg, \
                patch('correctness_cohort.time.sleep', side_effect=lambda _: self.world.rows.pop(child)):
            self.controller.unregister('a')
        kill.assert_not_called()
        killpg.assert_not_called()
        self.assertFalse(self.controller.document['workers'])
        self.assertEqual(self.controller.document['cleanup']['a']['outcome'], 'exited')


if __name__ == '__main__':
    unittest.main()

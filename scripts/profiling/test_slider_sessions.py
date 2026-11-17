"""Check grouped case isolation, proof binding, deferred images and cleanup without a GPU."""

from contextlib import contextmanager, redirect_stdout
import copy
import io
import json
from pathlib import Path
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import MagicMock, patch

import behavioral
import scenarios
from slider_sessions import _progress_watchdog, group_jobs, run_group, validate_requests
from correctness_cohort import CohortStopped
from file_hash import digest
from worker_resources import resolve_worker_memory_mb


class SliderSessionTests(unittest.TestCase):
    def test_retention_sees_final_session_report_and_its_hash_remains_unchanged(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);requests=self.requests(root);proofs={}
            for request in requests:
                request['args'].prune_verified_images=True
            def retain(directory):
                path=directory/'regression.json';report=json.loads(path.read_text())
                self.assertTrue(report['passed']);self.assertFalse(report['timing_valid'])
                self.assertIn('session_reuse',report)
                proofs[path]=digest(path)
            with self.driver(), patch('verified_images.prune',side_effect=retain):
                result=run_group(requests,root/'session','pair')
            self.assertTrue(result['passed']);self.assertEqual(len(proofs),len(requests))
            self.assertTrue(all(digest(path)==checksum for path,checksum in proofs.items()))

    def requests(self, root):
        for name in ('app', 'renderer', 'a.x3f', 'b.x3f'):
            (root/name).write_bytes(name.encode())
        requests = []
        for index, pattern in enumerate(('forward', 'scrub')):
            name = f'case-{index}'; cases = root/(name+'-inputs.json')
            cases.write_text(json.dumps([{'source': str(root/source), 'settings': {'exposure': ev, 'exposure_auto': False}}
                                         for source, ev in [('a.x3f', 0.25), ('b.x3f', -0.25)]]))
            args = scenarios.argument_parser().parse_args([
                '--binary', str(root/'app'), '--cases', str(cases), '--directory', str(root/name),
                '--scenario', 'slider-drag', '--quality', 'quality', '--mode', 'off', '--trials', '1',
                '--disk-cache-bytes', '0', '--drag-pattern', pattern,
                '--compare-images', str(root/'renderer'), '--background'])
            requests.append({'case_id': name, 'args': args})
        return requests

    @contextmanager
    def driver(self, code=0, stop_after=None, corrupt=None, image_failure=False, wait_error=None,
               stubborn=False, scenario_change=None, exited=False):
        state = {'alive': False, 'monitor': False, 'envelopes': []}
        process = MagicMock(); process.pid = 321; process.poll.return_value = None
        def wait(timeout=None):
            if wait_error and timeout == .1:
                raise wait_error
            if stubborn and not process.kill.called:
                raise subprocess.TimeoutExpired('app', timeout)
            state['alive'] = False; process.poll.return_value = code
            return code
        process.wait.side_effect = wait
        @contextmanager
        def monitor(_path, **kwargs):
            state['monitor'] = True
            try: yield
            finally: state['monitor'] = False
        def launch(command, **kwargs):
            state['alive'] = True
            envelope = json.loads(Path(kwargs['env']['FOVEONIC_GUI_SCENARIO_MANIFEST']).read_text())
            state['envelopes'].append(envelope); queue = envelope['session']['cases']
            self.assertEqual(command[1], queue[0]['inputs'][0]['staged_source'])
            self.assertTrue(all(Path(c['evidence_directory']).is_dir() for c in queue))
            for index, case in enumerate(queue):
                if stop_after is not None and index > stop_after: break
                proof = {'id': envelope['session']['id'], 'case_id': case['case_id'], 'index': index,
                         'count': len(queue), 'pid': process.pid}
                if corrupt: proof = corrupt(index, proof)
                photos = [{'source': item['staged_source'], 'family': 'Merrill',
                           'processing_quality': case['scenario']['quality'], 'display_density': 0.1,
                           'scale_divisor': 8, 'settings': item['settings']} for item in case['inputs']]
                failed = stop_after is not None and index == stop_after
                native = {'passed': not failed, 'failure': 'controlled failure' if failed else None,
                          'memory_cache_bytes': case['scenario']['memory_cache_bytes'],
                          'disk_cache': {'enabled': False, 'max_bytes': 0}, 'session': proof,
                          'scenario': copy.deepcopy(case['scenario']),
                          'steps': [{'action': 'initial-selection', 'photos': photos}, {'action': 'slider-drag', 'photos': photos}]}
                if scenario_change and index == 0:
                    native['scenario'] = scenario_change(native['scenario'])
                (Path(case['evidence_directory'])/'scenario-result.json').write_text(json.dumps(native))
            if exited:
                state['alive'] = False
                process.poll.return_value = code
            return process
        def compare(*_args, **kwargs):
            self.assertFalse(state['alive'], 'fresh renderer began before app exit')
            self.assertFalse(state['monitor'], 'fresh renderer began before monitor exit')
            return [{'name': 'image_samples', 'outcome': 'failed' if image_failure else 'passed'}]
        with patch('scenarios.source_tree_hash', return_value='tree'), \
             patch('scenarios.require_quiet', return_value={}), \
             patch('scenarios.workload_monitor', side_effect=monitor), \
             patch('scenarios.concurrent_workloads', return_value={'checked': True, 'uncontended': True}), \
             patch('slider_sessions.subprocess.Popen', side_effect=launch) as popen, \
             patch('scenarios.compare_scenario_images', side_effect=compare) as images:
            yield state, process, popen, images

    def test_cohort_keeps_children_in_worker_session_and_invalidates_timings(self):
        policy = MagicMock()
        policy.reference.return_value = {'run_id':'owned-cohort','group_id':'pair'}
        policy.check_quiet.return_value = {'authorized_peers':[{'pid':99}]}
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); requests = self.requests(root)
            for request in requests:request['args'].max_memory_mb = 2048
            def guarded(process, timeout, limit, samples, on_poll):
                on_poll()
                return process.wait(timeout=timeout)
            with self.driver() as (_, _, popen, images), patch('scenarios.wait_guarded', side_effect=guarded):
                result = run_group(requests, root/'session', 'pair', cohort=policy)
            self.assertTrue(result['passed']); self.assertFalse(result['timing_valid'])
            self.assertFalse(popen.call_args.kwargs['start_new_session'])
            self.assertEqual(result['cohort']['run_id'], 'owned-cohort')
            self.assertTrue(policy.check_alive.called)
            self.assertTrue(all(call.kwargs['cohort'] is policy for call in images.call_args_list))

    def test_controller_loss_stops_owned_app_before_any_image_verification(self):
        policy = MagicMock()
        policy.check_quiet.return_value = {}
        policy.reference.return_value = {'run_id':'owned-cohort','group_id':'pair'}
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); requests = self.requests(root)
            for request in requests:request['args'].max_memory_mb = 2048
            def lost(*args, **kwargs):raise CohortStopped('controller no longer alive')
            with self.driver() as (_, process, _, images), patch('scenarios.wait_guarded', side_effect=lost):
                with self.assertRaises(CohortStopped):run_group(requests, root/'session', 'pair', cohort=policy)
            process.terminate.assert_called_once(); images.assert_not_called()
            result = json.loads((root/'session/session-result.json').read_text())
            self.assertEqual(result['status'], 'interrupted'); self.assertFalse(result['passed'])

    def test_groups_preserve_declared_selection_and_case_order_once(self):
        fixtures = [{'name': 'pair', 'family': 'merrill'}]
        cases = [{'name': name, 'pointer': '/exposure'} for name in ('first', 'second')]
        _, _, jobs = behavioral.select(fixtures, cases); groups = group_jobs(jobs)
        self.assertEqual(len(groups), 4)
        identity = lambda rows: [(f['name'], c['name'], t, c['processing_quality']) for f, c, t in rows]
        self.assertCountEqual(identity(jobs), identity([j for g in groups for j in g]))
        self.assertTrue(all([c['name'] for _, c, _ in g] == ['first', 'second'] for g in groups))
        self.assertTrue(all(len({(f['name'], t, c['processing_quality']) for f, c, t in g}) == 1 for g in groups))

    def test_full_manifest_guard_partition_preserves_every_case_and_serial_boundaries(self):
        fixtures = json.loads(Path(__file__).with_name('test_inputs.json').read_text())['fixtures']
        cases = json.loads(Path(__file__).with_name('slider_cases.json').read_text())
        _, _, original = behavioral.select(fixtures, cases)
        key = lambda job: (job[0]['name'], job[1]['name'], job[2], job[1]['processing_quality'])
        identity = lambda rows: [key(job) for job in rows]
        baseline = group_jobs(original)
        self.assertEqual((len(original), len(baseline)), (1440, 20))
        hints = {'quattro': 8192, 'tiger': 8192}
        next(case for case in cases if case['name'] == 'dehaze')['worker_memory_mb_by_family'] = hints
        _, _, jobs = behavioral.select(fixtures, cases)
        groups = group_jobs(jobs, concurrent=True)
        self.assertEqual(len(groups), 28)
        self.assertCountEqual(identity(original), identity([job for group in groups for job in group]))
        self.assertEqual(len(set(identity(jobs))), 1440)
        for group in groups:
            guards = {resolve_worker_memory_mb(fixture, case=case) for fixture, case, _ in group}
            self.assertEqual(len(guards), 1)
            selected = set(identity(group))
            self.assertEqual(identity(group), identity([job for job in jobs if key(job) in selected]))
        self.assertEqual([identity(g) for g in group_jobs(jobs)], [identity(g) for g in baseline])
        collapsed = group_jobs(jobs, concurrent=True, guard_mb=8192)
        self.assertEqual([identity(g) for g in collapsed], [identity(g) for g in baseline])

    def test_invalid_groups_are_rejected_before_staging(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); requests = self.requests(root)
            for field, value in [('mode', 'wall'), ('compare_images', None), ('trials', 2),
                                 ('folder_mode', 'browse'), ('save_on_exit', True), ('exit_with_pending', True), ('timeout', 0),
                                 ('max_ui_gap_ms', 20), ('theme', 'light'), ('quality', 'speed'),
                                 ('disk_cache_bytes', 1), ('memory_cache_bytes', 1073741824)]:
                changed = copy.deepcopy(requests); setattr(changed[1]['args'], field, value)
                with self.subTest(field=field), self.assertRaises(ValueError): validate_requests(changed, 'pair')
            changed = copy.deepcopy(requests); changed[1]['case_id'] = changed[0]['case_id']
            with self.assertRaisesRegex(ValueError, 'unique'): validate_requests(changed, 'pair')
            changed = copy.deepcopy(requests); changed[1]['args'].directory = changed[0]['args'].directory/'nested'
            with self.assertRaisesRegex(ValueError, 'disjoint'): validate_requests(changed, 'pair')
            with self.assertRaises(ValueError): validate_requests([], 'pair')
            for malformed in ([{}], [None], [requests[0], {}]):
                with self.assertRaises(ValueError): validate_requests(malformed, 'pair')
            self.assertFalse(requests[0]['args'].directory.exists())

    def test_one_app_keeps_each_fresh_gesture_and_initial_recipe(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); requests = self.requests(root); fresh = []
            for index, request in enumerate(requests):
                args = copy.deepcopy(request['args']); selected = scenarios.validated_inputs(args)
                fresh.append(scenarios.prepare_trial(args, selected, root/f'fresh-{index}', 'tree')['manifest'])
            with self.driver() as (state, process, popen, images): result = run_group(requests, root/'session', 'pair')
            self.assertTrue(result['passed']); self.assertFalse(result['timing_valid']); popen.assert_called_once()
            self.assertEqual(images.call_count, 2); queue = state['envelopes'][0]['session']['cases']
            self.assertEqual([c['case_id'] for c in queue], ['case-0', 'case-1'])
            self.assertEqual([c['scenario']['drag_pattern'] for c in queue], ['forward', 'scrub'])
            for index, (case, cold) in enumerate(zip(queue, fresh)):
                for key in ('control_pointer', 'drag_steps', 'drag_fraction', 'drag_pattern', 'light_theme', 'mixed_values', 'editing_section'):
                    self.assertEqual(case['scenario'][key], cold['scenario'][key])
                self.assertEqual([i['settings'] for i in case['inputs']], [i['settings'] for i in cold['inputs']])
                self.assertNotEqual(case['scenario']['directory'], cold['scenario']['directory'])
                report = json.loads((requests[index]['args'].directory/'regression.json').read_text())
                self.assertTrue(report['passed']); self.assertFalse(report['timing_valid'])
                self.assertEqual(report['session_reuse']['expected']['index'], index)
                self.assertEqual(report['session_reuse']['expected']['pid'], 321)
                self.assertEqual(report['group_elapsed_ms'], [])
            process.terminate.assert_not_called()

    def test_failed_first_case_and_missing_later_case_cannot_pass(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); requests = self.requests(root)
            with self.driver(stop_after=0) as (_, _, _, images): result = run_group(requests, root/'session', 'pair')
            self.assertFalse(result['passed']); self.assertEqual(len(result['cases']), 2)
            self.assertTrue(all(not row['passed'] for row in result['cases'])); images.assert_not_called()
            self.assertIn('scenario result', result['cases'][1]['failure'])
            self.assertFalse((requests[1]['args'].directory/'trial-00/evidence/scenario-result.json').exists())

    def test_image_failure_or_nonzero_exit_fails_group(self):
        for settings in ({'image_failure': True}, {'code': 7}):
            with self.subTest(settings=settings), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary); requests = self.requests(root)
                with self.driver(**settings): result = run_group(requests, root/'session', 'pair')
                self.assertFalse(result['passed']); self.assertTrue(all(not c['passed'] for c in result['cases']))
                shared = json.loads((requests[0]['args'].directory/'trial-00/evidence/test-result.json').read_text())
                self.assertEqual(shared['outcome'], 'failed')

    def test_case_proof_binds_session_case_order_count_and_pid(self):
        for key, value in [('id', 'other'), ('case_id', 'case-1'), ('index', 1), ('count', 99), ('pid', 999)]:
            with self.subTest(key=key), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary); requests = self.requests(root)
                def corrupt(index, proof): return {**proof, key: value} if index == 0 else proof
                # An app already exited before the first watchdog poll still gets all original assertions.
                with self.driver(corrupt=corrupt, exited=True): result = run_group(requests, root/'session', 'pair')
                self.assertFalse(result['passed']); self.assertFalse(result['cases'][0]['passed'])

    def test_case_identity_cannot_bless_another_gesture_or_configuration(self):
        changes = [('drag_pattern', 'scrub'), ('control_pointer', '/tone/contrast'),
                   ('quality', 'speed'), ('light_theme', True), ('editing_section', 'Saturation')]
        for key, value in changes+[('all', None)]:
            with self.subTest(key=key), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary); requests = self.requests(root)
                change = lambda scenario: {} if key == 'all' else {**scenario, key: value}
                with self.driver(scenario_change=change):
                    result = run_group(requests, root/'session', 'pair')
                self.assertFalse(result['passed']); self.assertFalse(result['cases'][0]['passed'])
                shared = json.loads((requests[0]['args'].directory/'trial-00/evidence/test-result.json').read_text())
                assertion = next(a for a in shared['assertions'] if a['name'] == 'slider_session_scenario')
                self.assertEqual(assertion['outcome'], 'failed')

    def test_competing_workload_cannot_bypass_quiet_guard(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); requests = self.requests(root)
            with self.driver() as (_, _, popen, images), patch('scenarios.require_quiet', side_effect=RuntimeError('other app')):
                with self.assertRaisesRegex(RuntimeError, 'other app'):
                    run_group(requests, root/'session', 'pair')
            popen.assert_not_called(); images.assert_not_called()
            self.assertFalse(json.loads((root/'session/session-result.json').read_text())['passed'])

    def test_interruption_and_timeout_reap_owned_app_and_preserve_failure(self):
        for error, stubborn in [(KeyboardInterrupt(), False), (subprocess.TimeoutExpired('app', 60), True)]:
            with self.subTest(error=type(error)), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary); requests = self.requests(root)
                ticks = [0]
                def clock():
                    ticks[0] += 600 if stubborn else 0
                    return ticks[0]
                with self.driver(wait_error=error, stubborn=stubborn) as (_, process, popen, images), \
                        patch('slider_sessions.time.monotonic', side_effect=clock):
                    with self.assertRaises(type(error)): run_group(requests, root/'session', 'pair')
                popen.assert_called_once(); process.terminate.assert_called_once(); images.assert_not_called()
                self.assertEqual(process.kill.call_count, int(stubborn))
                report = json.loads((root/'session/session-result.json').read_text()); self.assertFalse(report['passed'])
                self.assertEqual(report['status'], 'interrupted' if isinstance(error, KeyboardInterrupt) else 'failed')
                self.assertTrue((root/'session/session.json').exists())

    def test_stalled_exit_is_bounded_on_both_wait_paths_and_reaps_before_images(self):
        for guarded in (False, True):
            with self.subTest(guarded=guarded), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary); requests = self.requests(root); clock = [0.]
                for request in requests:
                    request['args'].timeout = 1
                    request['args'].max_memory_mb = 2048 if guarded else None
                policy = MagicMock() if guarded else None
                if policy is not None:
                    policy.reference.return_value = {}
                    policy.check_quiet.return_value = {}
                with self.driver() as (_, process, _, images):
                    def wait(timeout=None):
                        if timeout == .1:
                            clock[0] += .1
                            raise subprocess.TimeoutExpired('app', timeout)
                        process.poll.return_value = 0
                        return 0
                    process.wait.side_effect = wait
                    with patch('slider_sessions.time.monotonic', side_effect=lambda: clock[0]), \
                            patch('process_memory.time.sleep', side_effect=lambda duration: clock.__setitem__(0, clock[0] + duration)), \
                            patch('process_memory.footprint_reader', return_value=lambda _: 512):
                        with self.assertRaises(subprocess.TimeoutExpired):
                            run_group(requests, root/'session', 'pair', cohort=policy)
                    self.assertLess(clock[0], 1.2)
                    process.terminate.assert_called_once()
                    process.wait.assert_called_with(timeout=10)
                    images.assert_not_called()
                    if policy is not None:
                        self.assertGreater(policy.check_alive.call_count, 2)
                    self.assertFalse(json.loads((root/'session/session-result.json').read_text())['passed'])

    def test_post_spawn_report_failure_still_reaps_owned_app(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); requests = self.requests(root)
            original_write = Path.write_text
            def fail_running_receipt(path, text, *args, **kwargs):
                if path.name == 'session-result.tmp' and '"status": "running"' in text:
                    raise OSError('injected report write failure')
                return original_write(path, text, *args, **kwargs)
            with self.driver() as (_, process, popen, images), patch.object(Path, 'write_text', fail_running_receipt):
                with self.assertRaisesRegex(OSError, 'injected report'):
                    run_group(requests, root/'session', 'pair')
            popen.assert_called_once(); process.terminate.assert_called_once()
            process.wait.assert_called_with(timeout=10); images.assert_not_called()
            report = json.loads((root/'session/session-result.json').read_text())
            self.assertFalse(report['passed']); self.assertEqual(report['status'], 'failed')

    def test_opt_in_keeps_rows_and_stops_after_failed_group(self):
        for failed in (False, True):
            with self.subTest(failed=failed), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary); self.requests(root)
                fixture = {'name': 'pair', 'family': 'merrill', 'sources': [str(root/'a.x3f'), str(root/'b.x3f')]}
                cases = [{'name': name, 'pointer': '/exposure', 'section': 'Tone', 'pattern': pattern,
                          'recipes': [{'exposure': v, 'exposure_auto': False} for v in (0.25, -0.25)]}
                         for name, pattern in [('first', 'forward'), ('second', 'scrub')]]
                with self.driver(image_failure=failed) as (_, _, popen, _), redirect_stdout(io.StringIO()):
                    report = behavioral.run(root/'app', [fixture], cases, root/'matrix', compare_images=root/'renderer', reuse_session=True)
                self.assertEqual(report['passed'], not failed); self.assertFalse(report['timing_valid'])
                self.assertEqual(popen.call_count, 1 if failed else 4)
                if not failed:
                    self.assertEqual([(r['case'], r['theme'], r['processing_quality']) for r in report['results']],
                                     [(c['name'], t, c['processing_quality']) for _, c, t in behavioral.select([fixture], cases)[2]])
                self.assertEqual(len(report['results']), 2 if failed else 8)


class SessionProgressTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.clock = 0
        timer = patch('slider_sessions.time.monotonic', side_effect=lambda: self.clock)
        timer.start(); self.addCleanup(timer.stop)
        self.prepared = []
        for index in range(3):
            evidence = self.root / str(index)
            evidence.mkdir()
            self.prepared.append(({'case_id': f'case-{index}', 'args': SimpleNamespace(timeout=600)},
                                  {'evidence': evidence}))

    def path(self, index):
        return self.prepared[index][1]['evidence'] / 'scenario-result.json'

    def terminal(self, index, passed=True):
        return {'passed': passed, 'session': {'id': 'session', 'case_id': f'case-{index}',
                'index': index, 'count': 3, 'pid': 321}}

    def write(self, index, passed=True):
        self.path(index).write_text(json.dumps(self.terminal(index, passed)))

    def test_ordered_progress_consumed_once_renews_only_next_case(self):
        alive = MagicMock()
        check = _progress_watchdog(self.prepared, 'session', 321, alive)
        self.clock = 500; self.write(0); check()
        self.path(0).unlink()  # Completed evidence is never reread by the watchdog.
        self.clock = 900; self.write(2); check()  # Out-of-order terminal cannot advance case 1.
        self.clock = 1099; check()
        self.clock = 1100
        with self.assertRaises(subprocess.TimeoutExpired) as error:
            check()
        self.assertIn('case-1', str(error.exception.cmd))
        self.assertEqual(error.exception.timeout, 600)
        self.assertEqual(alive.call_count, 4)

    def test_missing_or_partial_json_never_renews_deadline(self):
        for partial in (None, '{', '{"session":'):
            with self.subTest(partial=partial):
                self.clock = 0
                if self.path(0).exists(): self.path(0).unlink()
                check = _progress_watchdog(self.prepared, 'session', 321)
                for instant in (100, 300, 599):
                    self.clock = instant
                    if partial is not None: self.path(0).write_text(partial)
                    check()
                self.clock = 600
                with self.assertRaises(subprocess.TimeoutExpired): check()

    def test_eighty_seven_case_group_still_times_out_first_stall_at_six_hundred_seconds(self):
        prepared = [({'case_id': f'case-{index}', 'args': SimpleNamespace(timeout=600)},
                     {'evidence': self.root / f'pending-{index}'}) for index in range(87)]
        check = _progress_watchdog(prepared, 'session', 321)
        self.clock = 599; check()
        self.clock = 600
        with self.assertRaises(subprocess.TimeoutExpired) as error: check()
        self.assertEqual(error.exception.timeout, 600)
        self.assertIn('case-0', str(error.exception.cmd))

    def test_completed_json_requires_exact_identity_and_boolean_passed(self):
        mutations = [('id', 'other'), ('case_id', 'case-1'), ('index', 1), ('index', False),
                     ('count', 9), ('pid', 999), ('pid', 321.), ('extra', True)]
        values = []
        for key, value in mutations:
            terminal = self.terminal(0); terminal['session'][key] = value; values.append(terminal)
        values += [{'passed': 1, 'session': self.terminal(0)['session']}, {}, [], None]
        for terminal in values:
            with self.subTest(terminal=terminal):
                self.path(0).write_text(json.dumps(terminal))
                check = _progress_watchdog(self.prepared, 'session', 321)
                with self.assertRaisesRegex(ValueError, 'terminal identity or passed flag'):
                    check()

    def test_terminal_can_arrive_after_partial_json_but_not_after_deadline(self):
        check = _progress_watchdog(self.prepared, 'session', 321)
        self.path(0).write_text('{'); self.clock = 500; check()
        self.write(0); self.clock = 599; check()
        self.write(1); self.clock = 1199
        with self.assertRaises(subprocess.TimeoutExpired): check()

    def test_partial_utf8_is_no_progress_until_complete_valid_terminal(self):
        check = _progress_watchdog(self.prepared, 'session', 321)
        self.path(0).write_bytes(b'{"note":"caf\xc3')
        self.clock = 500; check()
        terminal = {**self.terminal(0), 'note': 'caf\u00e9'}
        self.path(0).write_text(json.dumps(terminal, ensure_ascii=False), encoding='utf-8')
        self.clock = 599; check()
        self.clock = 1198; check()
        self.clock = 1199
        with self.assertRaises(subprocess.TimeoutExpired): check()

    def test_exit_grace_cannot_extend_original_sum_or_reread_terminals(self):
        check = _progress_watchdog(self.prepared, 'session', 321)
        for index, instant in enumerate((590, 1180, 1770)):
            self.clock = instant; self.write(index); check(); self.path(index).unlink()
        self.clock = 1799; check()
        self.clock = 1800
        with self.assertRaises(subprocess.TimeoutExpired) as error: check()
        self.assertIn('orderly-exit', str(error.exception.cmd))
        self.assertEqual(error.exception.timeout, 30)

    def test_failed_terminal_preserves_failure_and_only_allows_bounded_exit(self):
        check = _progress_watchdog(self.prepared, 'session', 321)
        self.clock = 50; self.write(0, passed=False); check()
        for index in range(3): self.write(index)
        self.clock = 649; check()
        self.clock = 650
        with self.assertRaises(subprocess.TimeoutExpired) as error: check()
        self.assertIn('orderly-exit', str(error.exception.cmd))

    def test_other_io_errors_and_controller_loss_are_not_ignored(self):
        check = _progress_watchdog(self.prepared, 'session', 321)
        with patch.object(Path, 'read_text', side_effect=PermissionError('denied')):
            with self.assertRaises(PermissionError): check()
        check = _progress_watchdog(self.prepared, 'session', 321,
                                   lambda: (_ for _ in ()).throw(CohortStopped('controller gone')))
        with self.assertRaises(CohortStopped): check()


if __name__ == '__main__': unittest.main()

"""Check bounded slider admission and complete finite-queue scheduling."""

import unittest

from worker_resources import resolve_worker_memory_mb, select_admissions, validate_admission


class WorkerResourcesTests(unittest.TestCase):
    def test_fixture_guard_and_global_override(self):
        fixture = {'name': 'light', 'worker_memory_mb': 2048}
        self.assertEqual(resolve_worker_memory_mb(fixture), 2048)
        self.assertEqual(resolve_worker_memory_mb(fixture, 4096), 4096)
        self.assertEqual(resolve_worker_memory_mb({'name': 'custom'}, 3072), 3072)

    def test_missing_or_invalid_guard_is_rejected(self):
        with self.assertRaisesRegex(ValueError, 'custom'):
            resolve_worker_memory_mb({'name': 'custom'})
        for value in (True, False, 0, -1, 2.5, '2048'):
            with self.subTest(value=value), self.assertRaises(ValueError):
                resolve_worker_memory_mb({'worker_memory_mb': value})
            with self.subTest(override=value), self.assertRaises(ValueError):
                resolve_worker_memory_mb({'worker_memory_mb': 2048}, value)

    def test_case_family_hint_precedence_keeps_custom_fixture_base_required(self):
        fixture = {'name': 'quattro', 'family': 'quattro', 'worker_memory_mb': 4096}
        case = {'worker_memory_mb_by_family': {'quattro': 8192, 'tiger': 8192}}
        self.assertEqual(resolve_worker_memory_mb(fixture, case=case), 8192)
        self.assertEqual(resolve_worker_memory_mb(fixture, 1024, case=case), 1024)
        self.assertEqual(resolve_worker_memory_mb({**fixture, 'family': 'classic'}, case=case), 4096)
        custom = {'name': 'custom', 'family': 'quattro'}
        with self.assertRaisesRegex(ValueError, 'custom'):
            resolve_worker_memory_mb(custom, case=case)
        self.assertEqual(resolve_worker_memory_mb(custom, 3072, case=case), 3072)

    def test_invalid_hints_are_not_hidden_by_a_global_override(self):
        fixture = {'name': 'quattro', 'family': 'quattro', 'worker_memory_mb': 4096}
        for hints in (None, [], {'unknown': 8192}, {'tiger': True}, {'quattro': 0}, {'classic': 2.5}):
            with self.subTest(hints=hints), self.assertRaises(ValueError):
                resolve_worker_memory_mb(fixture, 1024, case={'worker_memory_mb_by_family': hints})

    def test_validate_all_groups_before_admitting_any(self):
        for workers, budget, reservations in (
                (True, 8, [2]), (0, 8, [2]), (2, False, [2]),
                (2, 0, [2]), (2, 8, [2, 0]), (2, 8, [2, True]),
                (2, 8, [2, 9])):
            with self.subTest(workers=workers, budget=budget, reservations=reservations):
                with self.assertRaises(ValueError):
                    validate_admission(workers, budget, reservations)

    def test_first_fit_obeys_both_limits_and_preserves_inputs(self):
        reservations = [4, 4, 2, 2]
        pending, active = [1, 2, 3], [0]
        self.assertEqual(select_admissions(pending, active, reservations, 3, 8), [1])
        self.assertEqual(select_admissions(pending, active, reservations, 3, 6), [2])
        self.assertEqual(select_admissions(pending, active, reservations, 1, 8), [])
        self.assertEqual(pending, [1, 2, 3])
        self.assertEqual(active, [0])

    def test_finite_queue_eventually_admits_blocked_heavy_group_exactly_once(self):
        reservations = [2, 4, 2, 2, 4, 2]
        pending, active, completed = list(range(len(reservations))), [], []
        while pending or active:
            selected = select_admissions(pending, active, reservations, 3, 6)
            active.extend(selected)
            pending = [index for index in pending if index not in selected]
            self.assertLessEqual(len(active), 3)
            self.assertLessEqual(sum(reservations[index] for index in active), 6)
            self.assertTrue(active, 'a valid finite queue must make progress')
            completed.append(active.pop(0))
        self.assertEqual(sorted(completed), list(range(len(reservations))))
        self.assertEqual(len(completed), len(set(completed)))

    def test_invalid_queue_or_active_state_is_rejected(self):
        for pending, active, workers, budget in (
                ([0, 0], [], 2, 8), ([0], [0], 2, 8), ([True], [], 2, 8),
                ([-1], [], 2, 8), ([2], [], 2, 8), ([], [0, 1], 1, 8),
                ([], [0, 1], 2, 6)):
            with self.subTest(pending=pending, active=active, workers=workers, budget=budget):
                with self.assertRaises(ValueError):
                    select_admissions(pending, active, [4, 4], workers, budget)


if __name__ == '__main__':
    unittest.main()

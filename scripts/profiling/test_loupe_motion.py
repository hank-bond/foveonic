"""Reject absent, coalesced and stationary native loupe-motion evidence."""

import copy
import unittest
from loupe_motion import check_motion, check_return_coverage


class LoupeMotionTests(unittest.TestCase):
    def test_return_requires_retained_coverage_then_a_distinct_uncovered_result(self):
        evidence = {'single_whole': False, 'events': [
            {'observed': 'covered loupe withdrew held replacement', 'detail': [1, 1],
             'region': [0, 0, 100, 100], 'required': [10, 10, 90, 90]},
            {'observed': 'leaving covered loupe requested fresh detail', 'previous': [1, 1],
             'detail': [2, 2], 'region': [50, 0, 150, 100], 'required': [60, 10, 140, 90]}]}
        check_return_coverage(evidence)
        for change in ('missing', 'lost-image', 'same-image', 'already-covered', 'insufficient'):
            bad = copy.deepcopy(evidence)
            if change == 'missing': bad['events'].pop()
            elif change == 'lost-image': bad['events'][1]['previous'] = [3, 3]
            elif change == 'same-image': bad['events'][1]['detail'] = [1, 1]
            elif change == 'already-covered': bad['events'][1]['required'] = [60, 10, 90, 90]
            else: bad['events'][0]['required'] = [10, 10, 110, 90]
            with self.subTest(change=change), self.assertRaises(ValueError):
                check_return_coverage(bad)

    def test_motion_requires_both_sustained_controls(self):
        evidence = {'events': [{'motion': name, 'duration_seconds': 3, 'coverage': 'egui-input',
            'samples': [{'seconds': t, 'position': [x, 10]} for t, x in
                        [(0, 100), (0.75, 100+width), (2.25, 100-width), (3, 100)]]}
            for name, width in [('cached', 0.1), ('sweep', 50)]]}
        check_motion(evidence)
        for change in ['missing', 'coalesced', 'stationary', 'unordered', 'nonfinite', 'narrow']:
            bad = copy.deepcopy(evidence)
            row = bad['events'][0]
            if change == 'missing': bad['events'].pop()
            elif change == 'coalesced': row['duration_seconds'] = 0.02
            elif change == 'stationary':
                for sample in row['samples']: sample['position'][0] = 100
            elif change == 'unordered': row['samples'][1]['seconds'] = 0
            elif change == 'nonfinite': row['samples'][1]['position'][0] = float('nan')
            else: bad['events'][1]['samples'] = copy.deepcopy(row['samples'])
            with self.subTest(change=change), self.assertRaises(ValueError): check_motion(bad)

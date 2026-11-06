import copy
import unittest
from compare_raster_review import distance, score, mapped_source_box


def review():
    return {'schema': 1, 'kind': 'manual_raster_region_review', 'source': {'document_sha256': 'original'},
            'comparison': 'NFKC_then_remove_whitespace_for_optical_diagnostic_only',
            'regions': [{'id': 'first', 'source_pixels': [0, 0, 50, 30], 'text': '甲𠮷'},
                        {'id': 'second', 'source_pixels': [0, 40, 50, 70], 'text': '乙'}]}


def observation(text, box, *, tile=0, region=0, status='center_inside_core'):
    return {'tile': tile, 'region': region, 'text': text, 'source_pixels': box, 'core_status': status}


class RasterReviewTests(unittest.TestCase):
    def test_rotated_model_regions_are_scored_at_their_original_source_position(self):
        tile={'recognition_roi':[10,20,17,25]}
        matrices=([1,0,0,1,10,20],[0,1,-1,0,17,20],[-1,0,0,-1,17,25],[0,-1,1,0,10,25])
        boxes=([10,20,11,21],[16,20,17,21],[16,24,17,25],[10,24,11,25])
        for turn in range(4):
            tile['recognition_to_source']=matrices[turn]
            actual=mapped_source_box(tile,{'x0':0,'y0':0,'x1':1,'y1':1},turn)
            self.assertEqual(actual,boxes[turn])
        tile['recognition_to_source']=[0,-1,1,0,11,25]
        with self.assertRaisesRegex(ValueError,'normalization differs'):
            mapped_source_box(tile,{'x0':0,'y0':0,'x1':1,'y1':1},3)
        with self.assertRaisesRegex(ValueError,'outside'):
            mapped_source_box({'recognition_roi':[10,20,17,25]}, {'x0':0,'y0':0,'x1':8,'y1':1})

    def test_source_positions_and_nonbmp_scalars_determine_the_score(self):
        r = score(review(), [observation('甲𠮷', [1, 1, 20, 20]), observation('乙', [1, 41, 20, 60])])
        self.assertEqual(r['counts']['exact_review_regions'], 2)
        self.assertEqual(r['counts']['expected_scalars'], 3)
        self.assertEqual(r['counts']['edit_distance'], 0)
        self.assertFalse(r['coverage_verified'])

    def test_correct_spelling_elsewhere_does_not_repair_the_missing_source_region(self):
        r = score(review(), [observation('甲𠮷', [1, 41, 20, 60]), observation('乙', [1, 1, 20, 20])])
        self.assertEqual(r['counts']['exact_review_regions'], 0)
        self.assertGreater(r['counts']['edit_distance'], 0)

    def test_duplicates_are_counted_instead_of_selecting_a_correct_candidate(self):
        r = score(review(), [observation('甲𠮷', [1, 1, 20, 20]), observation('甲𠮷', [2, 1, 21, 20], region=1)])
        self.assertEqual(r['counts']['text_equal'], 0)
        self.assertEqual(r['regions'][0]['observed'], '甲𠮷\n甲𠮷')
        self.assertEqual(r['regions'][0]['edit_distance'], 2)
        self.assertTrue(r['regions'][1]['missing_observation'])

    def test_halo_context_and_outside_scope_never_supply_missing_text(self):
        r = score(review(), [observation('甲𠮷', [1, 1, 20, 20], status='halo_observation'),
                             observation('乙', [100, 100, 120, 120])])
        self.assertEqual(r['counts']['missing_observations'], 2)
        self.assertEqual(r['counts']['halo_observations'], 1)
        self.assertEqual(r['counts']['outside_review_scope_observations'], 1)

    def test_boundary_crossing_retains_literal_text_but_is_not_a_verified_region(self):
        r = score(review(), [observation('甲𠮷', [-1, 1, 20, 20])])
        self.assertTrue(r['regions'][0]['text_equal_under_declared_optical_normalization'])
        self.assertFalse(r['regions'][0]['exact_review_region'])

    def test_normalization_is_declared_and_does_not_modify_raw_observations(self):
        expected = review()
        expected['regions'][0]['text'] = 'Ａ①'
        candidate = observation(' A\n1 ', [1, 1, 20, 20])
        before = copy.deepcopy(candidate)
        result = score(expected, [candidate])
        self.assertEqual(candidate, before)
        self.assertTrue(result['regions'][0]['text_equal_under_declared_optical_normalization'])
        self.assertEqual(result['regions'][0]['observed'], ' A\n1 ')
        self.assertEqual(distance('甲乙', '甲𠮷丙'), 2)

    def test_invalid_review_overlap_is_rejected_even_without_any_predictions(self):
        r = review()
        r['regions'][1]['source_pixels'] = [1, 1, 20, 20]
        with self.assertRaisesRegex(ValueError, 'overlapping'):
            score(r, [])

    def test_a_wide_unmatched_region_is_reported_without_being_borrowed_into_a_mask(self):
        result = score(review(), [observation('乙', [0, 41, 100, 60])])
        self.assertEqual(result['counts']['missing_observations'], 2)
        self.assertEqual(result['counts']['unassigned_predictions_overlapping_review_masks'], 1)
        self.assertEqual(result['outside_review_scope'][0]['overlapping_review_masks'], ['second'])


if __name__ == '__main__':
    unittest.main()

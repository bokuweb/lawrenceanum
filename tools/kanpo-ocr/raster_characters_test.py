import copy
import json
import unittest
from PIL import Image, ImageDraw

from raster_characters import character_layer
from raster_layout import sha
from raster_orientation import observe_orientation


def character(text, offset, box, width=40, height=20):
    x, y, r, b = box
    return {'text': text, 'unicode_scalar_offset': offset, 'unicode_scalar_length': len(text),
            'quad': [[x/width, 1-y/height], [r/width, 1-y/height],
                     [r/width, 1-b/height], [x/width, 1-b/height]]}


def fixture():
    image = Image.new('RGBA', (40, 20), 'white'); draw = ImageDraw.Draw(image)
    draw.rectangle((4, 4, 9, 9), fill='black'); draw.rectangle((24, 4, 29, 9), fill='black')
    layout = {'source': {'document_sha256': 'a'*64}, 'canvas': {'pixel_width': 40, 'pixel_height': 20},
              'provenance': {'recognition_parent_sha256': 'b'*64},
              'cells': [{'id': 7, 'interior_source_pixels': [0, 0, 20, 20]},
                        {'id': 3, 'interior_source_pixels': [20, 0, 40, 20]}],
              'words': [{'id': 0, 'bounds_source_pixels': [4, 4, 30, 10]},
                        {'id': 1, 'bounds_source_pixels': [4, 4, 8, 10]}]}
    chars = [character('甲', 0, [2, 2, 12, 12]), character('乙', 1, [22, 2, 32, 12])]
    page = {'revision': 3, 'language_correction': False, 'observations': [
        {'bbox': [0, 0, 1, 1], 'candidates': [{'text': '甲乙', 'confidence': .1, 'characters': chars}]}]}
    return image, layout, page


def report(image, layout, page):
    return {'schema': 1, 'kind': 'source_derived_raster_orientation', 'source': layout['source'],
            'recognition_parent_sha256': layout['provenance']['recognition_parent_sha256'],
            'vision': {'page': page}, 'observation': observe_orientation(image, page)}


def build(image, layout, page):
    return character_layer(layout, image, json.dumps(report(image, layout, page)).encode())


class CharacterLayerTests(unittest.TestCase):
    def test_wide_candidate_splits_at_actual_source_cells_without_changing_its_text(self):
        image, layout, page = fixture(); before = copy.deepcopy((layout, page))
        layer = build(image, layout, page)
        self.assertEqual([f['text'] for f in layer['fragments']], ['甲', '乙'])
        self.assertEqual([f['cell'] for f in layer['fragments']], [7, 3])
        self.assertEqual([f['bounds_source_pixels'] for f in layer['fragments']], [[4, 4, 10, 10], [24, 4, 30, 10]])
        self.assertEqual([c['containing_words'] for c in layer['characters']], [[0], [0]])
        self.assertEqual(layer['characters'][0]['overlapping_words'], [1])
        self.assertEqual(layer['characters'][0]['ink_pixels'], 36)
        self.assertEqual((layout, page), before)

    def test_all_candidates_are_retained_even_when_confidence_favors_a_conflicting_reading(self):
        image, layout, page = fixture(); second = copy.deepcopy(page['observations'][0]['candidates'][0])
        second['text'] = '丙丁'; second['confidence'] = .99
        for c, text in zip(second['characters'], ['丙', '丁']): c['text'] = text
        page['observations'][0]['candidates'].append(second)
        layer = build(image, layout, page)
        self.assertEqual([c['text'] for c in layer['candidates']], ['甲乙', '丙丁'])
        self.assertEqual([f['text'] for f in layer['fragments']], ['甲', '乙', '丙', '丁'])
        self.assertEqual(layer['raw_report']['vision']['page'], page)
        self.assertEqual(layer['characters'][0]['support_sha256'], layer['characters'][2]['support_sha256'])

    def test_scalar_ranges_preserve_non_bmp_combining_marks_and_every_whitespace(self):
        image, layout, page = fixture(); candidate = page['observations'][0]['candidates'][0]
        pieces = ['𠮷\u3099', '\u3000 \n', '乙']
        candidate['text'] = ''.join(pieces); cursor = 0; candidate['characters'] = []
        for piece, box in zip(pieces, [[2,2,12,12], [12,2,20,12], [22,2,32,12]]):
            candidate['characters'].append(character(piece, cursor, box)); cursor += len(piece)
        layer = build(image, layout, page)
        self.assertEqual(''.join(f['text'] for f in layer['fragments']), candidate['text'])
        self.assertEqual([c['unicode_scalar_offset'] for c in layer['characters']], [0, 2, 5])
        self.assertEqual(layer['characters'][1]['geometry_status'], 'whitespace_position_unverified')
        self.assertIsNone(layer['characters'][1]['cell'])

    def test_unknown_character_geometry_is_preserved_without_borrowing_neighbor_positions(self):
        image, layout, page = fixture(); second = copy.deepcopy(page['observations'][0]['candidates'][0])
        del second['characters'][0]['quad']; page['observations'][0]['candidates'].append(second)
        layer = build(image, layout, page); bad = layer['characters'][2]
        self.assertEqual(bad['text'], '甲'); self.assertIsNone(bad['source_pixels']); self.assertIsNone(bad['cell'])
        self.assertTrue(any(u['characters'] == [2] and u['reason'] == 'invalid_or_unavailable_quad' for u in layer['unresolved']))

    def test_no_ink_is_not_a_reason_to_drop_the_character(self):
        image, layout, page = fixture(); image.paste('white', (22,2,32,12))
        layer = build(image, layout, page); c = layer['characters'][1]
        self.assertEqual(c['text'], '乙'); self.assertEqual(c['ink_pixels'], 0); self.assertIsNone(c['ink_bounds_source_pixels'])
        self.assertTrue(any(u['reason'] == 'no_copy_ink_in_character_rectangle' for u in layer['unresolved']))

    def test_nested_cells_use_unique_inner_geometry_while_competing_cells_stay_ambiguous(self):
        image, layout, page = fixture(); layout['cells'].append({'id': 8, 'interior_source_pixels': [3,3,11,11]})
        layer = build(image, layout, page); self.assertEqual(layer['characters'][0]['cell'], 8)
        layout['cells'].append({'id': 9, 'interior_source_pixels': [3,3,11,11]})
        layer = build(image, layout, page); self.assertIsNone(layer['characters'][0]['cell'])
        self.assertEqual(layer['characters'][0]['cells'], [8,9])

    def test_identical_spelling_in_different_source_positions_is_never_collapsed(self):
        image, layout, page = fixture(); candidate = page['observations'][0]['candidates'][0]
        candidate['text'] = '甲甲'; candidate['characters'][1]['text'] = '甲'
        layer = build(image, layout, page)
        self.assertEqual(len(layer['characters']), 2); self.assertEqual(len(layer['fragments']), 2)
        self.assertNotEqual(layer['characters'][0]['support_sha256'], layer['characters'][1]['support_sha256'])

    def test_invalid_scalar_ranges_in_a_non_top_candidate_are_rejected(self):
        image, layout, page = fixture(); second = copy.deepcopy(page['observations'][0]['candidates'][0])
        second['characters'][1]['unicode_scalar_offset'] = 0; page['observations'][0]['candidates'].append(second)
        with self.assertRaisesRegex(ValueError, 'scalar ranges'): build(image, layout, page)

    def test_foreign_report_copy_source_and_changed_raw_geometry_are_rejected(self):
        image, layout, page = fixture(); original = report(image, layout, page)
        for mutation in range(3):
            value = copy.deepcopy(original)
            if mutation == 0: value['source'] = {'document_sha256': 'c'*64}
            elif mutation == 1: value['recognition_parent_sha256'] = 'c'*64
            else: value['vision']['page']['observations'][0]['candidates'][0]['characters'][0]['quad'][0][0] += .1
            with self.assertRaises(ValueError): character_layer(layout, image, json.dumps(value).encode())

    def test_local_glyph_axes_remain_separate_on_a_mixed_orientation_image(self):
        image, layout, page = fixture(); layout['cells'] = []
        # The second glyph's right edge points down in source pixels. This is
        # glyph orientation, not a declaration about the line's reading flow.
        points = [[32,2], [32,12], [22,12], [22,2]]
        page['observations'][0]['candidates'][0]['characters'][1]['quad'] = [[x/40,1-y/20] for x,y in points]
        layer = build(image, layout, page)
        self.assertEqual([c['glyph_right_axis'] for c in layer['characters']], [0,1])
        self.assertEqual(len(layer['fragments']), 2)
        self.assertEqual(layer['raw_report']['observation']['status'], 'mixed_glyph_axes')

    def test_changed_copy_pixels_cannot_reuse_an_old_character_observation(self):
        image, layout, page = fixture(); raw = json.dumps(report(image, layout, page)).encode()
        changed = image.copy(); changed.putpixel((0,0), (0,0,0,255))
        with self.assertRaisesRegex(ValueError, 'observed source glyph geometry'):
            character_layer(layout, changed, raw)

    def test_whitespace_keeps_one_range_only_when_both_source_neighbors_agree(self):
        image, layout, page = fixture(); candidate = page['observations'][0]['candidates'][0]
        candidate['text'] = '甲 \u3000乙'; candidate['characters'][1]['unicode_scalar_offset'] = 3
        candidate['characters'].insert(1, character(' \u3000', 1, [12,2,22,12]))
        layer = build(image, layout, page)
        self.assertEqual([f['text'] for f in layer['fragments']], ['甲',' \u3000','乙'])
        self.assertIsNone(layer['fragments'][1]['bounds_source_pixels'])
        layout['cells'] = [{'id':7,'interior_source_pixels':[0,0,40,20]}]
        layer = build(image, layout, page)
        self.assertEqual([f['text'] for f in layer['fragments']], ['甲 \u3000乙'])
        self.assertEqual(layer['fragments'][0]['cell'], 7)
        self.assertIsNone(layer['characters'][1]['cell'])


if __name__ == '__main__': unittest.main()

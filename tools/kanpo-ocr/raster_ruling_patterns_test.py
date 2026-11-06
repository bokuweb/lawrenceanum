import unittest
from PIL import Image, ImageDraw
from raster_grid import observe
from raster_ruling_patterns import components, observe_dashes, recognition_mask


def ruled_pattern():
    image = Image.new('RGBA', (160, 170), 'white')
    draw = ImageDraw.Draw(image)
    draw.rectangle((10, 10, 141, 11), fill='black')
    draw.rectangle((10, 150, 141, 151), fill='black')
    draw.rectangle((140, 10, 141, 151), fill='black')
    for y in range(14, 150, 16):
        draw.rectangle((40, y, 41, y+6), fill='black')
    return image


def original_rules(image):
    return observe(image, minimum_run=30)


def repeated_cells(count=4, irregular=False):
    image = Image.new('RGBA', (160, 40*count+30), 'white')
    draw = ImageDraw.Draw(image)
    for y in range(10, 40*count+11, 40):
        draw.rectangle((10, y, 141, y+1), fill='black')
    draw.rectangle((140, 10, 141, 40*count+11), fill='black')
    for cell in range(count):
        for offset in (4, 14, 24, 34):
            start = 10+40*cell+offset
            if irregular and cell == count//2 and offset == 24:
                start += 4
            draw.rectangle((40, start, 41, start+3), fill='black')
    return image


def interrupted_border(cells=3, *, bad_cells=(1,), mismatched_last=False):
    image = Image.new('RGBA', (160, 140*cells+30), 'white')
    draw = ImageDraw.Draw(image)
    for y in range(10, 140*cells+11, 140):
        draw.rectangle((10, y, 141, y+1), fill='black')
    draw.rectangle((140, 10, 141, 140*cells+11), fill='black')
    for cell in range(cells):
        pitch = 20 if mismatched_last and cell == cells-1 else 16
        for offset in range(4, 139, pitch):
            y = 10+140*cell+offset
            length = 10 if cell in bad_cells and offset == 68 else 7
            draw.rectangle((40, y, 41, min(y+length-1, 140*(cell+1)+9)), fill='black')
    return image


class RulingPatternTests(unittest.TestCase):
    def test_matching_flanks_preserve_exceptional_ink_between_valid_local_patterns(self):
        image = interrupted_border()
        data = observe_dashes(image, original_rules(image))
        bridges = [c for c in data['candidates'] if c['pattern_kind'] == 'flanked_interval']
        self.assertEqual(len(bridges), 2)
        for c in bridges:
            self.assertEqual(c['span'], [152, 290])
            self.assertEqual(len(c['local_groups']), 2)
            self.assertEqual(c['unmodeled_support'], [[218, 228]])
            self.assertEqual(c['length_range'], [7, 7])
            for ci in c['flanking_candidates']:
                self.assertLess(ci, c['id'])
                self.assertNotEqual(data['candidates'][ci]['pattern_kind'], 'flanked_interval')
        mask, _ = recognition_mask(image, data)
        self.assertEqual(mask.getpixel((40, 160)), 255)
        self.assertEqual(mask.getpixel((40, 222)), 0)

    def test_missing_mismatched_or_unproven_flanks_do_not_extend_a_pattern(self):
        for source in (interrupted_border(cells=2),
                       interrupted_border(mismatched_last=True),
                       interrupted_border(cells=4, bad_cells=(1, 2))):
            candidates = observe_dashes(source, original_rules(source))['candidates']
            self.assertFalse(any(c['pattern_kind'] == 'flanked_interval' for c in candidates))

    def test_flanked_patterns_transform_without_changing_the_selected_source_components(self):
        image = interrupted_border()
        reference, _ = recognition_mask(image, observe_dashes(image, original_rules(image)))
        for operation in (Image.Transpose.ROTATE_90, Image.Transpose.ROTATE_180,
                          Image.Transpose.ROTATE_270, Image.Transpose.FLIP_LEFT_RIGHT,
                          Image.Transpose.FLIP_TOP_BOTTOM, Image.Transpose.TRANSPOSE,
                          Image.Transpose.TRANSVERSE):
            transformed = image.transpose(operation)
            mask, _ = recognition_mask(transformed, observe_dashes(transformed, original_rules(transformed)))
            self.assertEqual(mask.tobytes(), reference.transpose(operation).tobytes())
    def test_short_cells_need_repeated_adjacent_source_junctions(self):
        image = repeated_cells()
        data = observe_dashes(image, original_rules(image))
        self.assertEqual(len(data['candidates']), 2)
        for c in data['candidates']:
            self.assertEqual(c['pattern_kind'], 'adjacent_cells')
            self.assertEqual(len(c['cells']), 4)
            self.assertEqual(c['length_range'], [4, 4])
            self.assertEqual(c['pitch_range'], [10, 10])
            self.assertEqual(len(c['pattern_support']), 8)
            self.assertEqual(len(c['junctions']), 5)
        mask, _ = recognition_mask(image, data)
        self.assertEqual(mask.getpixel((40, 16)), 255)
        self.assertEqual(mask.getpixel((40, 50)), 0)
        for count in (1, 2):
            source = repeated_cells(count)
            self.assertEqual(observe_dashes(source, original_rules(source))['candidates'], [])

    def test_a_changed_cell_does_not_bridge_short_pattern_evidence(self):
        image = repeated_cells(irregular=True)
        self.assertEqual(observe_dashes(image, original_rules(image))['candidates'], [])
        image = repeated_cells()
        # An attached glyph remains a whole component, not a row of dashes.
        ImageDraw.Draw(image).rectangle((30, 27, 39, 30), fill='black')
        data = observe_dashes(image, original_rules(image))
        self.assertTrue(data['candidates'])
        mask, _ = recognition_mask(image, data)
        self.assertEqual(mask.getpixel((40, 28)), 0)

    def test_short_cell_patterns_keep_their_pixel_geometry_under_all_cardinal_bases(self):
        image = repeated_cells()
        reference, _ = recognition_mask(image, observe_dashes(image, original_rules(image)))
        for operation in (Image.Transpose.ROTATE_90, Image.Transpose.ROTATE_180,
                          Image.Transpose.ROTATE_270, Image.Transpose.FLIP_LEFT_RIGHT,
                          Image.Transpose.FLIP_TOP_BOTTOM, Image.Transpose.TRANSPOSE,
                          Image.Transpose.TRANSVERSE):
            transformed = image.transpose(operation)
            mask, _ = recognition_mask(transformed, observe_dashes(transformed, original_rules(transformed)))
            self.assertEqual(mask.tobytes(), reference.transpose(operation).tobytes())
        scaled = image.resize((320, image.height*2), Image.Resampling.NEAREST)
        mask, _ = recognition_mask(scaled, observe_dashes(scaled, original_rules(scaled)))
        self.assertEqual(mask.tobytes(), reference.resize(scaled.size, Image.Resampling.NEAREST).tobytes())
    def test_exact_component_runs_keep_diagonal_connections_and_separate_holes(self):
        mask = Image.new('L', (10, 10))
        draw = ImageDraw.Draw(mask)
        draw.rectangle((1, 1, 5, 5), outline=255)
        draw.point((3, 3), fill=255)
        draw.point((6, 6), fill=255)
        result = components(mask)
        self.assertEqual(len(result), 2)
        self.assertEqual(sorted(c['area'] for c in result), [1, 17])
        rebuilt = Image.new('L', mask.size)
        draw = ImageDraw.Draw(rebuilt)
        for component in result:
            for y, left, right in component['support']:
                draw.line((left, y, right-1, y), fill=255)
        self.assertEqual(rebuilt.tobytes(), mask.tobytes())

    def test_candidates_have_black_run_and_perpendicular_junction_witnesses(self):
        image = ruled_pattern()
        before = image.tobytes()
        data = observe_dashes(image, original_rules(image))
        self.assertEqual(len(data['candidates']), 2)
        for candidate in data['candidates']:
            self.assertEqual(candidate['axis'], 1)
            self.assertEqual(candidate['length_range'], [7, 7])
            self.assertEqual(candidate['pitch_range'], [16, 16])
            for a, b in candidate['support']:
                for y in range(a, b):
                    self.assertEqual(image.getpixel((candidate['cross'], y)), (0, 0, 0, 255))
            for junction in candidate['junctions']:
                for witness in junction['witnesses']:
                    self.assertEqual(image.getpixel(tuple(witness['pixel'])), (0, 0, 0, 255))
        mask, erased = recognition_mask(image, data)
        self.assertEqual(mask.getpixel((40, 18)), 255)
        self.assertEqual(mask.getpixel((40, 10)), 0)
        self.assertEqual(mask.getpixel((140, 30)), 0)
        self.assertEqual(image.tobytes(), before)
        self.assertTrue(erased['selected_components'])
        self.assertFalse(data['coverage_verified'])

    def test_unanchored_short_or_irregular_repetitions_are_not_rulings(self):
        for variant in ('no_junction', 'short', 'irregular'):
            image = ruled_pattern()
            draw = ImageDraw.Draw(image)
            if variant == 'no_junction':
                draw.rectangle((0, 0, 159, 12), fill='white')
                draw.rectangle((0, 150, 159, 169), fill='white')
            elif variant == 'short':
                draw.rectangle((35, 60, 45, 149), fill='white')
            else:
                draw.rectangle((40, 62, 41, 68), fill='white')
                draw.rectangle((40, 68, 41, 80), fill='black')
            self.assertEqual(observe_dashes(image, original_rules(image))['candidates'], [], variant)

    def test_connected_or_tall_glyph_components_near_the_line_are_protected(self):
        image = ruled_pattern()
        draw = ImageDraw.Draw(image)
        # This joins the short run at y46 without changing its scan at x40/41.
        draw.rectangle((32, 48, 39, 51), fill='black')
        data = observe_dashes(image, original_rules(image))
        self.assertTrue(data['candidates'])
        mask, _ = recognition_mask(image, data)
        self.assertEqual(mask.getpixel((40, 49)), 0)
        self.assertEqual(mask.getpixel((40, 82)), 255)
        tall = ruled_pattern()
        ImageDraw.Draw(tall).rectangle((40, 45, 40, 70), fill='black')
        data = observe_dashes(tall, original_rules(tall))
        self.assertTrue(data['candidates'])  # The x41 scan remains periodic.
        mask, _ = recognition_mask(tall, data)
        self.assertEqual(mask.getpixel((40, 55)), 0)

    def test_only_single_pixel_components_are_despeckled_without_a_pattern(self):
        image = Image.new('RGBA', (30, 30), 'white')
        draw = ImageDraw.Draw(image)
        draw.point((2, 2), fill='black')
        draw.rectangle((10, 10, 10, 11), fill='black')
        mask, details = recognition_mask(image, {'candidates': []})
        self.assertEqual(mask.getpixel((2, 2)), 255)
        self.assertEqual(mask.getpixel((10, 10)), 0)
        self.assertEqual(details['selected_components'][0]['reason'], 'isolated_one_pixel')

    def test_rotation_reflection_and_integer_scaling_preserve_the_observation(self):
        image = ruled_pattern()
        reference, _ = recognition_mask(image, observe_dashes(image, original_rules(image)))
        for operation in (Image.Transpose.ROTATE_90, Image.Transpose.ROTATE_180,
                          Image.Transpose.ROTATE_270, Image.Transpose.FLIP_LEFT_RIGHT,
                          Image.Transpose.FLIP_TOP_BOTTOM, Image.Transpose.TRANSPOSE,
                          Image.Transpose.TRANSVERSE):
            transformed = image.transpose(operation)
            actual, _ = recognition_mask(transformed, observe_dashes(transformed, original_rules(transformed)))
            self.assertEqual(actual.tobytes(), reference.transpose(operation).tobytes())
        scaled = image.resize((320, 340), Image.Resampling.NEAREST)
        actual, _ = recognition_mask(scaled, observe_dashes(scaled, original_rules(scaled)))
        self.assertEqual(actual.tobytes(), reference.resize((320, 340), Image.Resampling.NEAREST).tobytes())


if __name__ == '__main__':
    unittest.main()

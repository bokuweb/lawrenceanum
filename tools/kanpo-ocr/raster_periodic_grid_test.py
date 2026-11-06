import unittest
from PIL import Image, ImageDraw
from raster_grid import observe
from raster_ruling_patterns import observe_dashes
from raster_periodic_grid import periodic_faces


def form():
    image = Image.new('RGBA', (180, 190), 'white')
    draw = ImageDraw.Draw(image)
    for y in (10, 50, 90, 130, 170):
        draw.rectangle((10, y, 161, y+1), fill='black')
    for x in (10, 160):
        draw.rectangle((x, 10, x+1, 171), fill='black')
    for y in (10, 50, 90, 130):
        for offset in (4, 14, 24, 34):
            draw.rectangle((80, y+offset, 81, y+offset+3), fill='black')
    return image


def grid(image):
    original = observe(image, minimum_run=30)
    patterns = observe_dashes(image, original)
    return original, patterns, periodic_faces(original, patterns)


class PeriodicGridTests(unittest.TestCase):
    def test_short_connected_source_support_closes_faces_but_an_actual_white_gap_does_not(self):
        from raster_band_completion_test import interrupted
        for white_gap in (False,True):
            image=interrupted()
            draw=ImageDraw.Draw(image)
            for y in (10,80):draw.rectangle((10,y,145,y+1),fill='black')
            for x in (10,144):draw.rectangle((x,10,x+1,81),fill='black')
            if white_gap:draw.line((75,30,75,31),fill='white')
            original=observe(image,minimum_run=30)
            patterns=observe_dashes(image,original)
            result=periodic_faces(original,patterns,image=image)
            self.assertEqual(len(result['rectangular_cells']),1 if white_gap else 2)
            self.assertFalse(any(e['support_kind']=='inferred_periodic_span' for e in result['edges']))
            for node in result['intersections']:
                self.assertEqual(image.getpixel(tuple(node['witness_pixel'])),(0,0,0,255))
            if not white_gap:
                self.assertTrue(all(c['boundary_kind']=='includes_source_band_completion'
                                    for c in result['rectangular_cells']))

    def test_band_completion_cannot_use_patterns_from_another_source(self):
        image=form()
        original,patterns,_=grid(image)
        changed=image.copy();changed.putpixel((0,0),(0,0,0,255))
        with self.assertRaisesRegex(ValueError,'another source'):
            periodic_faces(original,patterns,image=changed)

    def test_dashed_candidates_split_cells_without_painting_original_white_gaps(self):
        image = form()
        before = image.tobytes()
        original, patterns, result = grid(image)
        self.assertEqual(len(original['rectangular_cells']), 4)
        self.assertEqual(len(result['rectangular_cells']), 8)
        self.assertTrue(all(c['boundary_kind'] == 'includes_inferred_periodic_spans'
                            for c in result['rectangular_cells']))
        self.assertEqual(image.getpixel((80, 22)), (255, 255, 255, 255))
        self.assertEqual(image.tobytes(), before)
        self.assertFalse(result['coverage_verified'])
        for node in result['intersections']:
            self.assertEqual(image.getpixel(tuple(node['witness_pixel'])), (0, 0, 0, 255))
        self.assertTrue(any(e['support_kind'] == 'inferred_periodic_span' for e in result['edges']))

    def test_solid_observations_are_unchanged_and_all_periodic_pixels_are_original(self):
        image = form()
        original, patterns, result = grid(image)
        for key, axis in (('horizontal', 0), ('vertical', 1)):
            for ri, r in enumerate(original[key]):
                self.assertEqual(result[key][ri], dict(r, support_kind='solid_source', source_rule=ri))
            for r in result[key]:
                for cross, a, b in r['support']:
                    for p in range(a, b):
                        self.assertEqual(image.getpixel((p, cross) if axis == 0 else (cross, p)),
                                         (0, 0, 0, 255))

    def test_an_unexplained_middle_cell_is_not_filled_by_two_separated_styles(self):
        image = form()
        ImageDraw.Draw(image).rectangle((80, 91, 81, 129), fill='white')
        original, patterns, result = grid(image)
        self.assertEqual(len(result['rectangular_cells']), len(original['rectangular_cells']))
        self.assertFalse(any(e['support_kind'] == 'inferred_periodic_span' for e in result['edges']))

    def test_local_groups_cannot_claim_an_entire_unmodeled_boundary(self):
        original, patterns, _ = grid(form())
        for c in patterns['candidates']:
            c['pattern_kind'] = 'flanked_interval'
        result = periodic_faces(original, patterns)
        self.assertEqual(len(result['rectangular_cells']), len(original['rectangular_cells']))
        self.assertTrue(all(u['reason'] == 'local_periodic_groups_are_not_a_complete_cell_border'
                            for u in result['unresolved']))

    def test_a_wide_stack_of_marks_cannot_become_a_thin_cell_boundary(self):
        image = form()
        draw = ImageDraw.Draw(image)
        for y in (10,50,90,130):
            for offset in (4,14,24,34):
                draw.rectangle((80,y+offset,90,y+offset+3),fill='black')
        original, patterns, result = grid(image)
        self.assertTrue(patterns['candidates'])
        self.assertEqual(len(result['rectangular_cells']),len(original['rectangular_cells']))
        self.assertTrue(any(u['reason']=='periodic_track_is_not_thin' for u in result['unresolved']))

    def test_a_periodic_extension_overlapping_a_solid_track_remains_unresolved(self):
        image = Image.new('RGBA',(180,270),'white')
        draw = ImageDraw.Draw(image)
        for y in range(10,251,40):
            draw.rectangle((10,y,161,y+1),fill='black')
        for x in (10,160):
            draw.rectangle((x,10,x+1,251),fill='black')
        draw.rectangle((80,10,81,91),fill='black')
        for y in (90,130,170,210):
            for offset in (4,14,24,34):
                draw.rectangle((80,y+offset,81,y+offset+3),fill='black')
        original, patterns, result = grid(image)
        self.assertTrue(patterns['candidates'])
        self.assertEqual(len(result['rectangular_cells']),len(original['rectangular_cells']))
        self.assertTrue(any(u['reason']=='periodic_and_solid_tracks_overlap' for u in result['unresolved']))

    def test_all_rotations_reflections_and_integer_scale_preserve_cell_geometry(self):
        image = form()
        _, _, reference = grid(image)
        reference_boxes = sorted(c['interior_pixels'] for c in reference['rectangular_cells'])
        for operation in (Image.Transpose.ROTATE_90, Image.Transpose.ROTATE_180,
                          Image.Transpose.ROTATE_270, Image.Transpose.FLIP_LEFT_RIGHT,
                          Image.Transpose.FLIP_TOP_BOTTOM, Image.Transpose.TRANSPOSE,
                          Image.Transpose.TRANSVERSE):
            moved = image.transpose(operation)
            _, _, result = grid(moved)
            expected = []
            for box in reference_boxes:
                mask = Image.new('1', image.size)
                x0,y0,x1,y1=box
                ImageDraw.Draw(mask).rectangle((x0,y0,x1-1,y1-1),fill=1)
                expected.append(list(mask.transpose(operation).getbbox()))
            self.assertEqual(sorted(c['interior_pixels'] for c in result['rectangular_cells']), sorted(expected))
        scaled = image.resize((image.width*2, image.height*2),Image.Resampling.NEAREST)
        _, _, result = grid(scaled)
        self.assertEqual(sorted(c['interior_pixels'] for c in result['rectangular_cells']),
                         [[2*v for v in b] for b in reference_boxes])


if __name__ == '__main__':
    unittest.main()

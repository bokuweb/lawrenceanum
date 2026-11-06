import hashlib
import json
import tempfile
import unittest
from pathlib import Path

from PIL import Image, ImageDraw
from raster_grid import crop_frame, observe, write_observation


def grid():
    image = Image.new('RGBA', (120, 100), 'white')
    draw = ImageDraw.Draw(image)
    draw.rectangle((10, 10, 110, 90), outline='black', width=1)
    draw.line((10, 50, 110, 50), fill='black')
    draw.line((60, 10, 60, 90), fill='black')
    return image


def interiors(result):
    return sorted(c['interior_pixels'] for c in result['rectangular_cells'])


class RasterGridTests(unittest.TestCase):
    def test_original_runs_and_pixel_witnesses_support_four_cells(self):
        image = grid()
        result = observe(image)
        self.assertEqual(interiors(result), [[11, 11, 60, 50], [11, 51, 60, 90],
                                             [61, 11, 110, 50], [61, 51, 110, 90]])
        self.assertEqual(len(result['intersections']), 9)
        self.assertEqual(len(result['edges']), 12)
        self.assertEqual(result['unresolved'], [])
        self.assertFalse(result['coverage_verified'])
        self.assertIsNone(result['article_assignment'])
        for axis in ('horizontal', 'vertical'):
            for rule in result[axis]:
                for cross, start, end in rule['support']:
                    for primary in range(start, end):
                        point = (primary, cross) if axis == 'horizontal' else (cross, primary)
                        self.assertEqual(image.getpixel(point), (0, 0, 0, 255))
        for node in result['intersections']:
            self.assertEqual(image.getpixel(tuple(node['witness_pixel'])), (0, 0, 0, 255))

    def test_a_missing_separator_yields_one_merged_face(self):
        image = grid()
        ImageDraw.Draw(image).line((60, 11, 60, 49), fill='white')
        self.assertEqual(interiors(observe(image)), [[11, 11, 110, 50],
                                                     [11, 51, 60, 90], [61, 51, 110, 90]])

    def test_a_one_pixel_hole_does_not_close_a_source_boundary(self):
        image = Image.new('RGBA', (100, 80), 'white')
        ImageDraw.Draw(image).rectangle((10, 10, 90, 70), outline='black')
        image.putpixel((50, 10), (255, 255, 255, 255))
        self.assertEqual(interiors(observe(image)), [])

    def test_a_missing_corner_is_not_inferred_from_bounding_boxes(self):
        image = Image.new('RGBA', (100, 80), 'white')
        ImageDraw.Draw(image).rectangle((10, 10, 90, 70), outline='black')
        image.putpixel((10, 10), (255, 255, 255, 255))
        result = observe(image)
        self.assertEqual(len(result['intersections']), 3)
        self.assertEqual(interiors(result), [])

    def test_dashed_edges_are_not_solid_lines(self):
        image = Image.new('RGBA', (120, 100), 'white')
        draw = ImageDraw.Draw(image)
        draw.line((10, 10, 110, 10), fill='black')
        draw.line((10, 90, 110, 90), fill='black')
        for y in range(10, 91, 8):
            draw.line((10, y, 10, min(90, y + 2)), fill='black')
            draw.line((110, y, 110, min(90, y + 2)), fill='black')
        result = observe(image, minimum_run=6)
        self.assertEqual(result['vertical'], [])
        self.assertEqual(interiors(result), [])

    def test_thick_solid_components_are_not_promoted_to_thin_rules(self):
        image = Image.new('RGBA', (100, 100), 'white')
        ImageDraw.Draw(image).rectangle((20, 20, 80, 80), fill='black')
        result = observe(image)
        self.assertEqual(result['horizontal'], [])
        self.assertEqual(result['vertical'], [])
        self.assertEqual(len(result['rejected_parallel_components']), 2)
        self.assertEqual(interiors(result), [])

    def test_gray_or_transparent_marks_are_not_exact_opaque_black(self):
        for color in ((1, 1, 1, 255), (0, 0, 0, 254), (0, 0, 0, 0)):
            image = Image.new('RGBA', (100, 100), 'white')
            ImageDraw.Draw(image).rectangle((10, 10, 90, 90), outline=color)
            result = observe(image)
            self.assertEqual(result['horizontal'], [])
            self.assertEqual(result['vertical'], [])

    def test_nonrectangular_enclosures_remain_explicit(self):
        image = Image.new('RGBA', (120, 100), 'white')
        draw = ImageDraw.Draw(image)
        draw.line([(10, 10), (110, 10), (110, 50), (60, 50), (60, 90), (10, 90), (10, 10)], fill='black')
        result = observe(image)
        self.assertEqual(interiors(result), [])
        self.assertEqual(len(result['bounded_faces']), 1)
        self.assertEqual(result['unresolved'], [{'reason': 'bounded_face_is_not_a_simple_rectangle', 'face': 0}])

    def test_rotations_reflections_and_integer_scaling_keep_source_geometry(self):
        original = grid()
        for operation in (Image.Transpose.FLIP_LEFT_RIGHT, Image.Transpose.FLIP_TOP_BOTTOM,
                          Image.Transpose.ROTATE_90, Image.Transpose.ROTATE_180,
                          Image.Transpose.ROTATE_270, Image.Transpose.TRANSPOSE,
                          Image.Transpose.TRANSVERSE):
            result = observe(original.transpose(operation))
            self.assertEqual(len(result['rectangular_cells']), 4)
            self.assertEqual(result['unresolved'], [])
        scaled = observe(original.resize((240, 200), Image.Resampling.NEAREST))
        self.assertEqual(interiors(scaled), [[22, 22, 120, 100], [22, 102, 120, 180],
                                             [122, 22, 220, 100], [122, 102, 220, 180]])

    def test_source_bytes_crops_hashes_and_integer_crop_frames_are_retained(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / 'input.png'
            image = grid()
            image.save(source)
            raw = source.read_bytes()
            result = write_observation(source, root / 'output')
            self.assertEqual((root / 'output/source.png').read_bytes(), raw)
            self.assertEqual(result['source']['PNG_sha256'], hashlib.sha256(raw).hexdigest())
            saved = json.loads((root / 'output/observation.json').read_bytes())
            self.assertEqual(saved, result)
            for cell in saved['rectangular_cells']:
                box, crop = cell['interior_pixels'], cell['crop']
                actual = (root / 'output' / crop['PNG']).read_bytes()
                self.assertEqual(hashlib.sha256(actual).hexdigest(), crop['PNG_sha256'])
                self.assertEqual(Image.open(root / 'output' / crop['PNG']).tobytes(), image.crop(box).tobytes())
                self.assertEqual(crop['recognition_to_source'], [1, 0, 0, 1, box[0], box[1]])
            with self.assertRaises(FileExistsError):
                write_observation(source, root / 'output')

    def test_white_canvas_and_invalid_thresholds(self):
        image = Image.new('RGB', (100, 80), 'white')
        result = observe(image)
        self.assertEqual(interiors(result), [])
        self.assertEqual(result['bounded_faces'], [])
        for value in (True, 0, 1, 2.5, '5'):
            with self.assertRaises(ValueError):
                observe(image, minimum_run=value)
            with self.assertRaises(ValueError):
                observe(image, minimum_aspect=value)

    def test_crop_frames_compose_parent_rotation_reflection_and_shear(self):
        for a, b, c, d in ((1, 0, 0, 1), (0, 1, -1, 0), (-1, 0, 0, -1),
                           (0, -1, 1, 0), (-1, 0, 0, 1), (1, 0, 0, -1),
                           (0, 1, 1, 0), (0, -1, -1, 0), (2, .25, -.5, 3)):
            frame = {'pixel_width': 120, 'pixel_height': 100, 'image_width': 300,
                     'image_height': 400, 'recognition_to_image': [a, b, c, d, 71, 83],
                     'image_to_user': [20, 0, 0, 30, 10, 11], 'rotate': 90,
                     'crop_box': [5, 6, 500, 600]}
            result = crop_frame(frame, [11, 13, 61, 73])
            self.assertEqual((result['pixel_width'], result['pixel_height']), (50, 60))
            for x, y in ((0, 0), (50, 0), (50, 60), (0, 60)):
                e, f = result['recognition_to_image'][4:]
                self.assertEqual([a*x + c*y + e, b*x + d*y + f],
                                 [a*(x+11) + c*(y+13) + 71, b*(x+11) + d*(y+13) + 83])
            for key in ('image_width', 'image_height', 'image_to_user', 'rotate', 'crop_box'):
                self.assertEqual(result[key], frame[key])
            self.assertEqual(frame['recognition_to_image'], [a, b, c, d, 71, 83])
        for box in ((-1, 0, 10, 10), (1, 1, 1, 5), (0, 0, 121, 100), (True, 0, 5, 5)):
            with self.assertRaises(ValueError):
                crop_frame(frame, box)


if __name__ == '__main__':
    unittest.main()

import copy
import json
import tempfile
import unittest
from pathlib import Path

from PIL import Image, ImageDraw
from prepare_raster import prepare, recognition_copy, sha, source_parts, verify_grid, windows
from raster_grid import observe, write_observation


def asset(root, *, matrix=(1, 0, 0, 1, 0, 0), source_image=None):
    if source_image is None:
        image = Image.new('RGBA', (120, 100), 'white')
        draw = ImageDraw.Draw(image)
        draw.rectangle((10, 10, 110, 90), outline='black')
        draw.line((60, 10, 60, 90), fill='black')
        draw.line((10, 50, 110, 50), fill='black')
        draw.rectangle((23, 24, 29, 36), fill='black')
        image.putpixel((35, 35), (0, 0, 0, 255))
    else:
        image = source_image.copy()
    source = root / 'input.png'
    image.save(source)
    directory = root / 'asset'
    write_observation(source, directory)
    pdf = root / 'original.pdf'
    pdf.write_bytes(b'original PDF test snapshot')
    frame = {'pixel_width': image.width, 'pixel_height': image.height, 'image_width': image.height,
             'image_height': image.width, 'recognition_to_image': list(matrix),
             'image_to_user': [12, 0, 0, 10, 8, 11], 'rotate': 90, 'crop_box': [0, 0, 200, 300]}
    frame_raw = (json.dumps(frame) + '\n').encode()
    (directory / 'source-placement.json').write_bytes(frame_raw)
    png_sha = sha(source.read_bytes())
    source_data = {'asset': 7, 'page': 2, 'draw': 3, 'document_sha256': sha(pdf.read_bytes()),
                   'encoded_sha256': sha(b'original encoded image'), 'source_PNG_sha256': png_sha,
                   'source_RGBA_sha256': sha(image.tobytes()), 'source_placement': 'source-placement.json',
                   'source_placement_sha256': sha(frame_raw), 'source_receipt': 'source-receipt.json'}
    receipt = {'document_sha256': source_data['document_sha256'], 'page': 2, 'draw': 3,
               'PNG_sha256': png_sha, 'RGBA_pixels_sha256': sha(image.tobytes()),
               'placement_sha256': sha(frame_raw),
               'source_draw': {'encoded_sha256': source_data['encoded_sha256']}}
    receipt_raw = (json.dumps(receipt) + '\n').encode()
    (directory / 'source-receipt.json').write_bytes(receipt_raw)
    source_data['source_receipt_sha256'] = sha(receipt_raw)
    bound = {'schema': 1, 'kind': 'unclassified_pdf_raster_ruling_faces', 'source': source_data,
             'observation': 'observation.json', 'observation_sha256': sha((directory / 'observation.json').read_bytes()),
             'coverage_verified': False, 'article_assignment': None, 'native_glyph_binding': None}
    (directory / 'pdf-binding.json').write_text(json.dumps(bound) + '\n')
    return directory, pdf, image


def coverage(width, height, boxes):
    counts = [0] * (width * height)
    for x0, y0, x1, y1 in boxes:
        for y in range(y0, y1):
            for x in range(x0, x1):
                counts[y * width + x] += 1
    return counts


class RasterPreparation(unittest.TestCase):
    def test_source_bound_orientation_rotates_only_child_pixels_and_composes_the_original_frame(self):
        from raster_orientation_test import sample,rotated
        from raster_orientation import observe_orientation,turn_matrix,turn_frame
        for turn in range(4):
            with tempfile.TemporaryDirectory() as name:
                root=Path(name);base,page=sample();image,page=rotated(base,page,turn)
                directory,pdf,_=asset(root,source_image=image,matrix=(0,1,-1,0,image.height,0))
                binding_raw=(directory/'pdf-binding.json').read_bytes();binding=json.loads(binding_raw)
                source={**binding['source'],'binding_sha256':sha(binding_raw),'observation_sha256':binding['observation_sha256']}
                orientation={'schema':1,'kind':'source_derived_raster_orientation','source':source,
                    'recognition_parent_sha256':sha((directory/'source.png').read_bytes()),
                    'vision':{'page':page},'observation':observe_orientation(image,page)}
                report=root/'orientation.json';report.write_text(json.dumps(orientation)+'\n')
                result=prepare(directory,pdf,root/'prepared',profile='identity',retain_window=True,
                               orientation_report=report,record_ink_support=True)
                self.assertEqual(result['settings']['recognition_quarter_turns'],(-turn)%4)
                self.assertEqual((root/'prepared'/'source.png').read_bytes(),(directory/'source.png').read_bytes())
                self.assertEqual(Image.open(root/'prepared'/'recognition-parent.png').tobytes(),image.tobytes())
                self.assertEqual(len(result['tiles']),1)
                tile=result['tiles'][0]
                self.assertEqual(Image.open(root/'prepared'/tile['PNG']).tobytes(),base.tobytes())
                self.assertEqual(tile['recognition_to_source'],turn_matrix(image.width,image.height,(-turn)%4))
                frame=json.loads((root/'prepared'/tile['placement']).read_bytes())
                original=json.loads((directory/'source-placement.json').read_bytes())
                self.assertEqual(frame,turn_frame(original,(-turn)%4))
                self.assertIn('raster_orientation.py',result['producer']['code_sha256'])
                orientation['observation']['quarter_turns_ccw']=(orientation['observation']['quarter_turns_ccw']+1)%4
                report.write_text(json.dumps(orientation)+'\n')
                with self.assertRaisesRegex(ValueError,'does not reproduce'):
                    prepare(directory,pdf,root/'forged',profile='identity',retain_window=True,orientation_report=report)

    def test_recorded_ink_reconstructs_all_actual_copy_pixels_without_changing_inputs(self):
        from source_gutters import ink_mask
        with tempfile.TemporaryDirectory() as name:
            root=Path(name);directory,pdf,image=asset(root)
            base=prepare(directory,pdf,root/'base',profile='median3-ruling-patterns')
            result=prepare(directory,pdf,root/'ink',profile='median3-ruling-patterns',record_ink_support=True)
            for member in ('source.png','recognition-parent.png','erasure-mask.png'):
                self.assertEqual((root/'base'/member).read_bytes(),(root/'ink'/member).read_bytes())
            self.assertEqual(result['tiles'],base['tiles'])
            raw=(root/'ink'/result['recognition_ink']['file']).read_bytes()
            self.assertEqual(sha(raw),result['recognition_ink']['sha256'])
            evidence=json.loads(raw);mask=Image.new('L',image.size);draw=ImageDraw.Draw(mask)
            for c in evidence['components']:
                for y,a,b in c['support']:draw.line((a,y,b-1,y),fill=255)
            parent=Image.open(root/'ink'/'recognition-parent.png').convert('RGBA')
            self.assertEqual(mask.tobytes(),ink_mask(parent).tobytes())
            self.assertEqual(evidence['recognition_RGBA_sha256'],sha(parent.tobytes()))

    def test_band_completion_is_explicit_and_keeps_the_recognition_parent_unchanged(self):
        from raster_band_completion_test import interrupted
        with tempfile.TemporaryDirectory() as name:
            root=Path(name)
            directory,pdf,_=asset(root,source_image=interrupted())
            with self.assertRaisesRegex(ValueError,'explicit periodic'):
                prepare(directory,pdf,root/'invalid',profile='identity',complete_ruling_bands=True)
            base=prepare(directory,pdf,root/'base',profile='median3-ruling-patterns',periodic_partition=True)
            result=prepare(directory,pdf,root/'completed',profile='median3-ruling-patterns',
                           periodic_partition=True,complete_ruling_bands=True)
            for member in ('source.png','recognition-parent.png','erasure-mask.png'):
                self.assertEqual((root/'base'/member).read_bytes(),(root/'completed'/member).read_bytes())
            self.assertTrue(result['settings']['complete_ruling_bands'])
            self.assertIn('raster_band_completion.py',result['producer']['code_sha256'])
            partition=json.loads((root/'completed'/result['partition_observation']['file']).read_bytes())
            self.assertTrue(partition['settings']['source_band_completion'])

    def test_periodic_partition_changes_geometry_without_changing_either_pixel_copy(self):
        from raster_periodic_grid_test import form
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            directory, pdf, source = asset(root, source_image=form(), matrix=(0, 1, -1, 0, 190, 0))
            original = prepare(directory, pdf, root/'solid', profile='median3-ruling-patterns', retain_window=True)
            result = prepare(directory, pdf, root/'periodic', profile='median3-ruling-patterns',
                             retain_window=True, periodic_partition=True)
            for member in ('source.png', 'source.pdf', 'source-observation.json',
                           'recognition-parent.png', 'erasure-mask.png'):
                self.assertEqual((root/'solid'/member).read_bytes(), (root/'periodic'/member).read_bytes())
            self.assertNotEqual(result['parts'], original['parts'])
            self.assertEqual(coverage(source.width, source.height, [t['core'] for t in result['tiles']]),
                             [1]*(source.width*source.height))
            self.assertEqual(result['settings']['partition_geometry'], 'periodic_ruling_candidates')
            raw = (root/'periodic'/result['partition_observation']['file']).read_bytes()
            self.assertEqual(sha(raw), result['partition_observation']['sha256'])
            self.assertEqual(len(json.loads(raw)['rectangular_cells']), 8)
            self.assertIn('raster_periodic_grid.py', result['producer']['code_sha256'])
            for t in result['tiles']:
                if t['status'] != 'prepared_unclassified':
                    continue
                frame = json.loads((root/'periodic'/t['placement']).read_bytes())
                x,y=t['recognition_roi'][:2]
                self.assertEqual(frame['recognition_to_image'], [0,1,-1,0,190-y,x])

    def test_candidate_interiors_and_complement_cover_each_pixel_once(self):
        cells = [{'id': 9, 'interior_pixels': [2, 3, 10, 8]},
                 {'id': 2, 'interior_pixels': [12, 1, 18, 11]},
                 {'id': 41, 'interior_pixels': [4, 11, 15, 16]}]
        before = copy.deepcopy(cells)
        parts = source_parts(20, 18, cells)
        self.assertEqual(coverage(20, 18, [p['box'] for p in parts]), [1] * 360)
        self.assertEqual(cells, before)
        self.assertEqual({p['cell'] for p in parts if 'cell' in p}, {2, 9, 41})
        tiles = windows(parts, core_size=5, halo=2)
        self.assertEqual(coverage(20, 18, [t['core'] for t in tiles]), [1] * 360)
        for tile in tiles:
            part = parts[tile['part']]['box']
            a, b, c, d = tile['core']
            x0, y0, x1, y1 = tile['window']
            self.assertTrue(part[0] <= x0 <= a < c <= x1 <= part[2])
            self.assertTrue(part[1] <= y0 <= b < d <= y1 <= part[3])
            self.assertLessEqual(x1-x0, 9)
            self.assertLessEqual(y1-y0, 9)

    def test_no_grid_still_plans_every_pixel_for_recognition(self):
        parts = source_parts(20, 18, [])
        self.assertEqual(parts, [{'role': 'outside_ruling_faces', 'box': [0, 0, 20, 18], 'id': 0}])
        self.assertEqual(coverage(20, 18, [t['core'] for t in windows(parts, core_size=7)]), [1] * 360)

    def test_invalid_or_duplicate_candidates_cannot_hide_pixels(self):
        cells = [{'id': 0, 'interior_pixels': [2, 2, 10, 10]}]
        for second in ({'id': 0, 'interior_pixels': [12, 2, 18, 10]},
                       {'id': 1, 'interior_pixels': [10, 10, 21, 15]},
                       {'id': 1, 'interior_pixels': [True, 10, 15, 15]}):
            with self.assertRaises(ValueError):
                source_parts(20, 20, cells + [second])
        for value in (0, -1, True, 1.5):
            with self.assertRaises(ValueError):
                windows([], core_size=value)

    def test_nested_frames_keep_the_inner_candidate_and_outer_remainder(self):
        cells = [{'id': 0, 'interior_pixels': [2, 2, 18, 18]},
                 {'id': 1, 'interior_pixels': [5, 5, 10, 10]}]
        parts = source_parts(20, 20, cells)
        self.assertEqual(coverage(20, 20, [p['box'] for p in parts]), [1] * 400)
        by_owner = {}
        for part in parts:
            owner = part.get('cell')
            b = part['box']
            by_owner[owner] = by_owner.get(owner, 0) + (b[2]-b[0])*(b[3]-b[1])
        self.assertEqual(by_owner, {None: 144, 0: 231, 1: 25})
        self.assertFalse(any(p['role'] == 'overlapping_ruling_faces' for p in parts))

    def test_partially_overlapping_frames_remain_ambiguous_without_pixel_loss(self):
        cells = [{'id': 0, 'interior_pixels': [2, 2, 10, 10]},
                 {'id': 1, 'interior_pixels': [9, 9, 15, 15]}]
        parts = source_parts(20, 20, cells)
        self.assertEqual(coverage(20, 20, [p['box'] for p in parts]), [1] * 400)
        ambiguous = [p for p in parts if p['role'] == 'overlapping_ruling_faces']
        self.assertEqual(len(ambiguous), 1)
        self.assertEqual(ambiguous[0]['box'], [9, 9, 10, 10])
        self.assertEqual(ambiguous[0]['cells'], [0, 1])
        repeated = [cells[0], dict(id=2, interior_pixels=cells[0]['interior_pixels'][:])]
        identical = [p for p in source_parts(20, 20, repeated) if p['role'] == 'overlapping_ruling_faces']
        self.assertEqual(len(identical), 1)
        self.assertEqual(identical[0]['cells'], [0, 2])

    def test_median_and_ruling_erasure_alter_only_the_declared_copy(self):
        image = Image.new('RGBA', (50, 50), 'white')
        draw = ImageDraw.Draw(image)
        draw.rectangle((5, 5, 45, 45), outline='black')
        draw.rectangle((20, 20, 24, 32), fill='black')
        image.putpixel((8, 8), (0, 0, 0, 255))
        raw = image.tobytes()
        observation = observe(image)
        unchanged, no_mask = recognition_copy(image, observation, 'identity')
        self.assertEqual(unchanged.tobytes(), raw)
        self.assertIsNone(no_mask.getbbox())
        changed, mask = recognition_copy(image, observation, 'median3-ruling-bands')
        self.assertEqual(image.tobytes(), raw)
        self.assertEqual(changed.getpixel((8, 8)), (255, 255, 255, 255))
        self.assertEqual(changed.getpixel((5, 20)), (255, 255, 255, 255))
        self.assertEqual(changed.getpixel((22, 25)), (0, 0, 0, 255))
        self.assertEqual(mask.getpixel((5, 20)), 255)
        self.assertEqual(mask.getpixel((22, 25)), 0)
        transparent = image.copy()
        transparent.putpixel((0, 0), (0, 0, 0, 0))
        with self.assertRaisesRegex(ValueError, 'opaque'):
            recognition_copy(transparent, observation, 'median3-ruling-bands')

    def test_ruling_boxes_are_recomputed_instead_of_trusting_a_modified_declaration(self):
        image = Image.new('RGBA', (50, 50), 'white')
        ImageDraw.Draw(image).rectangle((5, 5, 45, 45), outline='black')
        observation = observe(image)
        verify_grid(image, observation)
        observation['horizontal'][0]['band'][1] += 5
        with self.assertRaisesRegex(ValueError, 'reproduce'):
            verify_grid(image, observation)

    def test_preparation_retains_source_and_positions_all_crops_in_rotated_parent(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            directory, pdf, image = asset(root, matrix=(0, 1, -1, 0, 100, 0))
            before = {p.name: p.read_bytes() for p in directory.iterdir()}
            output = root / 'prepared'
            result = prepare(directory, pdf, output, profile='identity', core_size=35, halo=8)
            self.assertEqual(before, {p.name: p.read_bytes() for p in directory.iterdir()})
            self.assertEqual((output / 'source.pdf').read_bytes(), pdf.read_bytes())
            self.assertEqual((output / 'source.png').read_bytes(), before['source.png'])
            self.assertEqual(coverage(120, 100, [t['core'] for t in result['tiles']]), [1] * 12000)
            self.assertFalse(result['coverage_verified'])
            self.assertTrue(result['pixel_partition_verified'])
            self.assertIsNone(result['article_assignment'])
            self.assertEqual(Image.open(output / 'recognition-parent.png').tobytes(), image.tobytes())
            for tile in result['tiles']:
                if tile['status'] != 'prepared_unclassified':
                    self.assertEqual(tile['status'], 'recognition_copy_empty_unverified')
                    self.assertNotIn('text', tile)
                    continue
                box = tile['recognition_roi']
                self.assertEqual(Image.open(output / tile['PNG']).tobytes(), image.crop(box).tobytes())
                self.assertEqual(sha((output / tile['PNG']).read_bytes()), tile['PNG_sha256'])
                frame = json.loads((output / tile['placement']).read_bytes())
                self.assertEqual(frame['recognition_to_image'], [0, 1, -1, 0, 100-box[1], box[0]])
                self.assertEqual(sha((output / tile['placement']).read_bytes()), tile['placement_sha256'])
            with self.assertRaises(FileExistsError):
                prepare(directory, pdf, output, profile='identity')

    def test_changed_pdf_png_and_observation_do_not_publish_prepared_results(self):
        for member in ('pdf', 'source.png', 'observation.json', 'source-placement.json'):
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                directory, pdf, _ = asset(root)
                target = pdf if member == 'pdf' else directory / member
                target.chmod(0o600)
                target.write_bytes(b'changed source')
                with self.assertRaises(ValueError):
                    prepare(directory, pdf, root / 'prepared', profile='identity')
                self.assertFalse((root / 'prepared').exists())

    def test_periodic_profile_captures_its_source_observations_and_helper_code(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            directory, pdf, image = asset(root)
            before = (directory / 'source.png').read_bytes()
            output = root / 'prepared'
            result = prepare(directory, pdf, output, profile='median3-ruling-patterns')
            self.assertEqual((directory / 'source.png').read_bytes(), before)
            self.assertEqual((output / 'source.png').read_bytes(), before)
            details_raw = (output / result['ruling_patterns']['file']).read_bytes()
            self.assertEqual(sha(details_raw), result['ruling_patterns']['sha256'])
            details = json.loads(details_raw)
            self.assertEqual(details['original']['source_RGBA_sha256'], sha(image.tobytes()))
            self.assertFalse(details['original']['coverage_verified'])
            self.assertEqual(result['operations'][-1]['kind'], 'erase_bounded_periodic_ruling_components')
            code = result['producer']['code_sha256']
            self.assertIn('raster_ruling_patterns.py', code)
            for name, digest in code.items():
                self.assertEqual(sha((output / 'implementation' / name).read_bytes()), digest)

    def test_fixed_windows_do_not_change_size_when_preprocessing_removes_ink(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            directory, pdf, _ = asset(root)
            unchanged = prepare(directory, pdf, root / 'identity', profile='identity', retain_window=True,
                                core_size=35, halo=8)
            filtered = prepare(directory, pdf, root / 'filtered', profile='median3-ruling-patterns', retain_window=True,
                               core_size=35, halo=8)
            for result in (unchanged, filtered):
                self.assertEqual(result['settings']['recognition_crop'], 'source_window')
                for tile in result['tiles']:
                    if tile['status'] == 'prepared_unclassified':
                        self.assertEqual(tile['recognition_roi'], tile['window'])
            self.assertEqual([t['window'] for t in unchanged['tiles']], [t['window'] for t in filtered['tiles']])


if __name__ == '__main__':
    unittest.main()

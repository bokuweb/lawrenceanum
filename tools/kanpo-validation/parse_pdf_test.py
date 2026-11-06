import copy
import json
import importlib.util
from pathlib import Path
import tempfile
import unittest

from PIL import Image, ImageDraw
from parse_pdf import bind_decoded, bind_raster_grids, sha

grid_spec = importlib.util.spec_from_file_location('test_raster_grid', Path(__file__).parents[1] / 'kanpo-ocr/raster_grid.py')
grid_observer = importlib.util.module_from_spec(grid_spec)
grid_spec.loader.exec_module(grid_observer)


def source(directory):
    image = Image.new('RGBA', (2, 3), (0, 0, 0, 255))
    image.save(directory / 'image.png')
    payload = b'original encoded source pixels'
    declaration = {'object_ref': [8, 0], 'form_path': [], 'pixel_width': 2, 'pixel_height': 3,
                   'image_to_user': [2, 0, 0, 3, 10, 20], 'source_geometry_complete': True,
                   'declared_dictionary': {'type': 'dict', 'entries': []},
                   'clips': [], 'graphics_state_names_hex': []}
    frame = {'crop_box': [0, 0, 100, 100], 'rotate': 0, 'pixel_width': 2, 'pixel_height': 3}
    (directory / 'frame.json').write_text(json.dumps(frame))
    receipt = {'document_sha256': 'original', 'page': 0, 'draw': 0, 'visibility': 'unverified',
               'source_draw': {**declaration, 'encoded_bytes': len(payload), 'encoded_sha256': sha(payload)},
               'PNG_sha256': sha((directory / 'image.png').read_bytes()),
               'RGBA_pixels_sha256': sha(image.tobytes()),
               'placement_sha256': sha((directory / 'frame.json').read_bytes())}
    (directory / 'receipt.json').write_text(json.dumps(receipt))
    decoded = {'schema': 1, 'kind': 'intrinsic_pdf_image_sources', 'document_sha256': 'original',
               'rows': [{'page': 0, 'draw': 0, 'status': 'decoded', 'PNG': 'image.png',
                         'receipt': 'receipt.json', 'placement': 'frame.json',
                         'receipt_sha256': sha((directory / 'receipt.json').read_bytes())}]}
    analysis = {'assets': [{'id': 7, 'page': 0, 'draw': 0, 'document_sha256': 'original',
                            'encoded_bytes': list(payload), 'encoded_sha256': sha(payload),
                            'source': declaration, 'crop_box': frame['crop_box'], 'rotate': 0}]}
    return analysis, decoded


class DecodeBinding(unittest.TestCase):
    def test_editable_raster_candidates_must_match_the_independently_decoded_pixels_and_frame(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory=Path(temporary);analysis,decoded=source(directory);asset=analysis['assets'][0]
            receipt=json.loads((directory/'receipt.json').read_bytes());frame=json.loads((directory/'frame.json').read_bytes())
            raster={'schema':1,'kind':'source_derived_raster_layout','coverage_verified':False,
                    'source':{**{k:asset[k] for k in ('document_sha256','encoded_sha256','page','draw')},
                              'source_PNG_sha256':receipt['PNG_sha256'],'source_RGBA_sha256':receipt['RGBA_pixels_sha256']},
                    'canvas':{'pixel_width':2,'pixel_height':3},'source_frame':frame}
            asset['raster_layout']=raster
            result=bind_decoded(analysis,decoded,directory,'original')
            self.assertEqual(result[0]['raster_layout_source'],'original_decoded_pixels_and_placement_checked_text_unverified')
            for mutation in ('PNG','RGBA','frame','canvas'):
                changed=copy.deepcopy(analysis);layout=changed['assets'][0]['raster_layout']
                if mutation in ('PNG','RGBA'):layout['source'][f'source_{mutation}_sha256']='other pixels'
                elif mutation=='frame':layout['source_frame']['rotate']=90
                else:layout['canvas']['pixel_width']=3
                with self.assertRaisesRegex(ValueError,'independently decoded'):
                    bind_decoded(changed,decoded,directory,'original')

    def test_character_candidates_cannot_borrow_a_different_source_copy(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory=Path(temporary);analysis,decoded=source(directory);asset=analysis['assets'][0]
            receipt=json.loads((directory/'receipt.json').read_bytes());frame=json.loads((directory/'frame.json').read_bytes())
            raster={'schema':1,'kind':'source_derived_raster_layout','coverage_verified':False,
                    'source':{**{k:asset[k] for k in ('document_sha256','encoded_sha256','page','draw')},
                              'source_PNG_sha256':receipt['PNG_sha256'],'source_RGBA_sha256':receipt['RGBA_pixels_sha256']},
                    'canvas':{'pixel_width':2,'pixel_height':3},'source_frame':frame,
                    'provenance':{'recognition_parent_sha256':'parent'}}
            raster['character_layer']={'schema':1,'kind':'source_derived_raster_characters','coverage_verified':False,
                                      'source':copy.deepcopy(raster['source']),'canvas':copy.deepcopy(raster['canvas']),
                                      'recognition_parent_sha256':'parent'}
            asset['raster_layout']=raster;bind_decoded(analysis,decoded,directory,'original')
            for mutation in ('source','canvas','parent'):
                changed=copy.deepcopy(analysis);layer=changed['assets'][0]['raster_layout']['character_layer']
                if mutation=='source':layer['source']['source_PNG_sha256']='foreign'
                elif mutation=='canvas':layer['canvas']['pixel_width']=3
                else:layer['recognition_parent_sha256']='foreign'
                with self.assertRaisesRegex(ValueError,'character candidates differ'):
                    bind_decoded(changed,decoded,directory,'original')

    def test_only_matching_source_payload_declaration_placement_and_pixels_bind(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            analysis, decoded = source(directory)
            result = bind_decoded(analysis, decoded, directory, 'original')
            self.assertEqual(result[0]['asset'], 7)
            self.assertEqual(result[0]['visibility'], 'unverified')
            for mutation in ('foreign_pdf', 'changed_payload', 'changed_declaration', 'duplicate_draw'):
                a, d = copy.deepcopy(analysis), copy.deepcopy(decoded)
                if mutation == 'foreign_pdf':
                    d['document_sha256'] = 'other'
                elif mutation == 'changed_payload':
                    a['assets'][0]['encoded_bytes'][0] ^= 1
                elif mutation == 'changed_declaration':
                    a['assets'][0]['source']['image_to_user'][0] += 1
                else:
                    d['rows'].append(copy.deepcopy(d['rows'][0]))
                with self.assertRaises(ValueError, msg=mutation):
                    bind_decoded(a, d, directory, 'original')
            (directory / 'image.png').write_bytes(b'changed pixels')
            with self.assertRaisesRegex(ValueError, 'PNG changed'):
                bind_decoded(analysis, decoded, directory, 'original')

    def test_missing_failed_or_foreign_receipt_files_never_claim_decoded_pixels(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            analysis, decoded = source(directory)
            failed = copy.deepcopy(decoded)
            failed['rows'][0].update(status='failed', error='unsupported source')
            result = bind_decoded(analysis, failed, directory, 'original')
            self.assertEqual(result[0]['status'], 'failed')
            foreign = copy.deepcopy(decoded)
            foreign['rows'][0]['PNG'] = '../image.png'
            with self.assertRaisesRegex(ValueError, 'local receipt member'):
                bind_decoded(analysis, foreign, directory, 'original')
            decoded['rows'] = []
            with self.assertRaisesRegex(ValueError, 'inventories differ'):
                bind_decoded(analysis, decoded, directory, 'original')


def raster_source(directory, matrix=(1, 0, 0, 1, 0, 0)):
    analysis, decoded = source(directory)
    image = Image.new('RGBA', (120, 100), 'white')
    draw = ImageDraw.Draw(image)
    draw.rectangle((10, 10, 110, 90), outline='black')
    draw.line((10, 50, 110, 50), fill='black')
    draw.line((60, 10, 60, 90), fill='black')
    image.save(directory / 'image.png')
    frame = {'crop_box': [0, 0, 100, 100], 'rotate': 90, 'pixel_width': 120, 'pixel_height': 100,
             'image_width': 100, 'image_height': 120, 'recognition_to_image': list(matrix),
             'image_to_user': [2, 0, 0, 3, 10, 20]}
    (directory / 'frame.json').write_text(json.dumps(frame))
    receipt = json.loads((directory / 'receipt.json').read_bytes())
    receipt.update(PNG_sha256=sha((directory / 'image.png').read_bytes()),
                   placement_sha256=sha((directory / 'frame.json').read_bytes()),
                   RGBA_pixels_sha256=sha(image.tobytes()))
    receipt['source_draw'].update(pixel_width=100, pixel_height=120)
    (directory / 'receipt.json').write_text(json.dumps(receipt))
    decoded['rows'][0]['receipt_sha256'] = sha((directory / 'receipt.json').read_bytes())
    analysis['assets'][0]['source'].update(pixel_width=100, pixel_height=120)
    analysis['assets'][0]['rotate'] = 90
    return analysis, decoded


class RasterBinding(unittest.TestCase):
    def test_grid_candidates_keep_pdf_asset_occurrence_pixels_and_rotated_crop_placements(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            analysis, decoded = raster_source(directory, (0, 1, -1, 0, 100, 0))
            before = copy.deepcopy(analysis)
            output = directory / 'grids'
            result = bind_raster_grids(analysis, decoded, directory, output, 'original', grid_observer)
            self.assertEqual(analysis, before)
            self.assertEqual(result['rows'][0]['asset'], 7)
            self.assertEqual(result['rows'][0]['rectangular_cells'], 4)
            self.assertFalse(result['coverage_verified'])
            row = result['rows'][0]
            bound = json.loads((output / row['binding']).read_bytes())
            source_data = bound['source']
            self.assertEqual(source_data['encoded_sha256'], analysis['assets'][0]['encoded_sha256'])
            self.assertEqual((source_data['asset'], source_data['page'], source_data['draw']), (7, 0, 0))
            self.assertEqual(source_data['source_PNG_sha256'], sha((directory / 'image.png').read_bytes()))
            self.assertIsNone(bound['article_assignment'])
            self.assertIsNone(bound['native_glyph_binding'])
            self.assertEqual(bound['visibility'], 'unverified')
            snapshot = output / row['directory']
            self.assertEqual(sha((snapshot / bound['observation']).read_bytes()), bound['observation_sha256'])
            self.assertEqual((snapshot / 'source.png').read_bytes(), (directory / 'image.png').read_bytes())
            for cell in bound['cells']:
                crop_frame = json.loads((snapshot / cell['placement']).read_bytes())
                x, y, right, bottom = cell['source_interior_pixels']
                self.assertEqual(crop_frame['recognition_to_image'], [0, 1, -1, 0, 100-y, x])
                self.assertEqual([crop_frame['pixel_width'], crop_frame['pixel_height']], [right-x, bottom-y])
                self.assertEqual(sha((snapshot / cell['placement']).read_bytes()), cell['placement_sha256'])
                self.assertEqual(sha((snapshot / cell['PNG']).read_bytes()), cell['PNG_sha256'])

    def test_foreign_source_and_duplicate_asset_id_fail_before_observation(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            analysis, decoded = raster_source(directory)
            with self.assertRaisesRegex(ValueError, 'another PDF'):
                bind_raster_grids(analysis, decoded, directory, directory / 'foreign', 'other', grid_observer)
            self.assertFalse((directory / 'foreign').exists())
            duplicate = copy.deepcopy(analysis)
            duplicate['assets'].append(copy.deepcopy(duplicate['assets'][0]))
            with self.assertRaises(ValueError):
                bind_raster_grids(duplicate, decoded, directory, directory / 'duplicate', 'original', grid_observer)

    def test_failed_decode_is_retained_without_fabricated_empty_grid(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            analysis, decoded = raster_source(directory)
            decoded['rows'][0].update(status='failed', error='unsupported pixel filter')
            result = bind_raster_grids(analysis, decoded, directory, directory / 'grids', 'original', grid_observer)
            self.assertEqual(result['rows'], [{'asset': 7, 'status': 'failed', 'error': 'unsupported pixel filter'}])
            self.assertFalse((directory / 'grids/asset-0007').exists())

    def test_a_png_changed_after_binding_fails_with_raw_evidence_retained(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            analysis, decoded = raster_source(directory)

            class ChangingObserver:
                @staticmethod
                def write_observation(path, output):
                    image = Image.open(path).convert('RGBA')
                    image.putpixel((30, 30), (0, 0, 0, 255))
                    image.save(path)
                    return grid_observer.write_observation(path, output)

            with self.assertRaisesRegex(ValueError, 'source changed'):
                bind_raster_grids(analysis, decoded, directory, directory / 'grids', 'original', ChangingObserver)
            self.assertTrue((directory / 'grids/asset-0007/observation.json').exists())
            self.assertFalse((directory / 'grids/index.json').exists())

    def test_a_receipt_changed_during_observation_cannot_be_bound(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            analysis, decoded = raster_source(directory)

            class ChangingObserver:
                @staticmethod
                def write_observation(path, output):
                    result = grid_observer.write_observation(path, output)
                    (directory / 'receipt.json').write_text('{}')
                    return result

            with self.assertRaisesRegex(ValueError, 'receipt changed'):
                bind_raster_grids(analysis, decoded, directory, directory / 'grids', 'original', ChangingObserver)
            self.assertTrue((directory / 'grids/asset-0007/observation.json').exists())
            self.assertFalse((directory / 'grids/index.json').exists())


if __name__ == '__main__':
    unittest.main()

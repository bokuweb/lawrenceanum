import copy
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from PIL import Image,ImageDraw
from raster_layout import collect_words,assign_cells,cell_tables,cell_lines,layout_box,read_layout


def observation(text,box,core=None,confidence=.5):
    return {'tile':0,'region':0,'text':text,'confidence':confidence,'is_vertical':False,
            'source_pixels':box,'core':core or [0,0,20,20],'raw':{'text':text}}


def cell(i,box):
    return {'id':i,'interior_source_pixels':box,'frame_source_pixels':box,
            'frame_layout_pixels':box,'boundary_kind':'solid_source','words':[]}


def digest(data):return hashlib.sha256(data).hexdigest()


def write_json(path,value):
    raw=(json.dumps(value,ensure_ascii=False)+'\n').encode();path.write_bytes(raw);return digest(raw)


def captured_recognition(directory,change=None):
    """Synthetic protocol fixture; its text is not a recognition accuracy oracle."""
    prepared=directory/'prepared';prepared.mkdir();raw_dir=directory/'tile-00000-ocr';raw_dir.mkdir()
    parent=Image.new('RGBA',(20,20),'white');ImageDraw.Draw(parent).rectangle((2,4,5,9),fill='black')
    parent.save(prepared/'source.png');parent.save(prepared/'recognition-parent.png')
    child=parent.copy()
    if change=='foreign_pixels':ImageDraw.Draw(child).rectangle((12,12,15,15),fill='black')
    child.save(prepared/'tile.png');(prepared/'source.pdf').write_bytes(b'protocol-test-not-a-real-pdf')
    frame={'pixel_width':20,'pixel_height':20,'image_width':20,'image_height':20,
           'image_to_user':[20,0,0,20,0,0],'crop_box':[0,0,20,20],'rotate':0,
           'recognition_to_image':[1,0,0,1,0,0]}
    source={'document_sha256':digest((prepared/'source.pdf').read_bytes()),
            'source_PNG_sha256':digest((prepared/'source.png').read_bytes()),
            'source_RGBA_sha256':digest(parent.tobytes()),
            'source_placement_sha256':write_json(prepared/'source-placement.json',frame),
            'observation_sha256':write_json(prepared/'source-observation.json',{'rectangular_cells':[]})}
    tile_frame=copy.deepcopy(frame)
    if change=='foreign_frame':tile_frame['recognition_to_image'][4]=1
    tile={'id':0,'status':'prepared_unclassified','PNG':'tile.png',
          'PNG_sha256':digest((prepared/'tile.png').read_bytes()),'placement':'tile.placement.json',
          'placement_sha256':write_json(prepared/'tile.placement.json',tile_frame),
          'recognition_roi':[0,0,20,20],'core':[0,0,20,20],'recognition_to_source':[1,0,0,1,0,0]}
    prep={'source':source,'recognition_parent_sha256':digest((prepared/'recognition-parent.png').read_bytes()),
          'canvas':{'pixel_width':20,'pixel_height':20},'tiles':[tile]}
    prep_sha=write_json(prepared/'preparation.json',prep)
    region={'id':0,'text':'甲','confidence':.5,'is_vertical':False,'bbox':{'pixels':{'x0':0,'y0':0,'x1':10,'y1':15}}}
    trace={'body':'甲','stages':{'reading_order':[{'text':'甲','confidence':.5,'is_vertical':False,'bbox':[0,0,10,15]}]}}
    raw={'kind':'unclassified_ocr','layout_available':True,'article_assignment':None,'native_glyph_binding':None,
         'source':{'image':{'sha256':tile['PNG_sha256'],'bytes':len((prepared/'tile.png').read_bytes())}},
         'canvas':prep['canvas'],'text':'甲','recognition_regions':[region],
         'engine_result':{'trace':'trace.json','trace_sha256':write_json(raw_dir/'trace.json',trace)}}
    if change=='foreign_trace':raw['recognition_regions'][0]['confidence']=.9
    write_json(raw_dir/'observations.json',raw)
    projection=copy.deepcopy(raw);projection['recognition_regions'][0]['source_core']={'source_pixels':[0,0,10,15]}
    projection['source_derivation']={'preparation_sha256':prep_sha,'tile':0,
                                    'original_document_sha256':source['document_sha256'],
                                    'original_PNG_sha256':source['source_PNG_sha256']}
    projection['declared_placement']=tile_frame;write_json(directory/'projection.json',projection)
    row={'tile':0,'status':'recognized_unclassified','projection':'projection.json'}
    if change=='omitted_tile':row['status']='recognition_copy_empty_unverified'
    write_json(directory/'index.json',{'schema':1,'kind':'unclassified_source_derived_image_ocr',
                                       'coverage_verified':False,'source':source,'preparation_sha256':prep_sha,'rows':[row]})
    write_json(directory/'terminal.json',{'status':'complete','canonical_integration':False})


class RasterLayoutTests(unittest.TestCase):
    def test_optional_character_report_adds_ranges_without_replacing_any_word_observation(self):
        from raster_orientation import observe_orientation
        with tempfile.TemporaryDirectory() as temp:
            directory=Path(temp);captured_recognition(directory);before=read_layout(directory)
            page={'revision':3,'language_correction':False,'observations':[{'bbox':[0,0,1,1],'candidates':[
                {'text':'甲','confidence':.5,'characters':[{'text':'甲','unicode_scalar_offset':0,
                    'unicode_scalar_length':1,'quad':[[0,1],[.5,1],[.5,.25],[0,.25]]}]}]}]}
            parent=Image.open(directory/'prepared/recognition-parent.png').convert('RGBA')
            report={'schema':1,'kind':'source_derived_raster_orientation','source':before['source'],
                    'recognition_parent_sha256':before['provenance']['recognition_parent_sha256'],
                    'vision':{'page':page},'observation':observe_orientation(parent,page)}
            path=directory/'characters.json';write_json(path,report);after=read_layout(directory,path)
            layer=after.pop('character_layer');self.assertEqual(after,before)
            self.assertEqual(layer['characters'][0]['text'],'甲');self.assertEqual(layer['characters'][0]['containing_words'],[0])

    def test_captured_trace_pixels_and_frame_produce_positioned_editable_candidate(self):
        with tempfile.TemporaryDirectory() as temp:
            directory=Path(temp);captured_recognition(directory);layout=read_layout(directory)
        self.assertEqual(layout['words'][0]['text'],'甲')
        self.assertEqual(layout['words'][0]['bounds_source_pixels'],[2,4,6,10])
        self.assertEqual(layout['observations'][0]['raw']['confidence'],.5)
        self.assertEqual(layout['outside_words'],[0]);self.assertFalse(layout['coverage_verified'])

    def test_consistently_rehashed_foreign_child_pixels_are_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            directory=Path(temp);captured_recognition(directory,'foreign_pixels')
            with self.assertRaisesRegex(ValueError,'pixels do not reproduce'):read_layout(directory)

    def test_consistently_rehashed_foreign_child_frame_is_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            directory=Path(temp);captured_recognition(directory,'foreign_frame')
            with self.assertRaisesRegex(ValueError,'source pixel permutation'):read_layout(directory)

    def test_recognition_inventory_cannot_silently_drop_an_active_source_tile(self):
        with tempfile.TemporaryDirectory() as temp:
            directory=Path(temp);captured_recognition(directory,'omitted_tile')
            with self.assertRaisesRegex(ValueError,'silently omitted'):read_layout(directory)

    def test_raw_trace_confidence_cannot_be_rewritten_in_observations_and_projection(self):
        with tempfile.TemporaryDirectory() as temp:
            directory=Path(temp);captured_recognition(directory,'foreign_trace')
            with self.assertRaisesRegex(ValueError,'complete trace'):read_layout(directory)

    def test_equal_source_ink_and_equal_text_become_one_editable_word_with_all_aliases(self):
        mask=Image.new('L',(20,20));ImageDraw.Draw(mask).rectangle((2,4,5,9),fill=255)
        obs=[observation('甲',[0,0,10,15]),observation('甲',[1,3,8,12])];before=copy.deepcopy(obs)
        words,raw,unresolved=collect_words(obs,mask,20,20,0)
        self.assertEqual(obs,before);self.assertEqual(len(words),1)
        self.assertEqual(words[0]['text'],'甲');self.assertEqual(words[0]['alternatives'][0]['observations'],[0,1])
        self.assertEqual([r['word'] for r in raw],[0,0]);self.assertEqual(unresolved,[])

    def test_identical_spelling_at_two_positions_is_never_deduplicated(self):
        mask=Image.new('L',(20,20));draw=ImageDraw.Draw(mask)
        draw.rectangle((2,2,5,5),fill=255);draw.rectangle((12,12,15,15),fill=255)
        words,_,_=collect_words([observation('日',[0,0,8,8]),observation('日',[10,10,18,18])],mask,20,20,0)
        self.assertEqual(len(words),2);self.assertNotEqual(words[0]['support_sha256'],words[1]['support_sha256'])

    def test_conflicting_text_keeps_both_readings_without_using_confidence(self):
        mask=Image.new('L',(20,20));ImageDraw.Draw(mask).rectangle((2,4,5,9),fill=255)
        words,raw,unresolved=collect_words([observation('甲',[0,0,10,15],confidence=.99),
                                         observation('乙',[1,3,8,12],confidence=.01)],mask,20,20,0)
        self.assertIsNone(words[0]['text']);self.assertEqual(len(words[0]['alternatives']),2)
        self.assertEqual([r['confidence'] for r in raw],[.99,.01])
        self.assertEqual(unresolved[0]['reason'],'conflicting_text_for_identical_copy_ink')

    def test_halo_only_ink_does_not_create_an_owned_word_but_is_retained(self):
        mask=Image.new('L',(20,20));ImageDraw.Draw(mask).rectangle((2,4,5,9),fill=255)
        words,raw,unresolved=collect_words([observation('未校正',[0,0,20,20],core=[10,0,20,20])],mask,20,20,0)
        self.assertEqual(words,[]);self.assertEqual(raw[0]['text'],'未校正')
        self.assertEqual(raw[0]['ownership'],'context_ink');self.assertEqual(unresolved[0]['observations'],[0])

    def test_a_hollow_glyph_can_own_an_empty_central_core(self):
        mask=Image.new('L',(20,20));ImageDraw.Draw(mask).rectangle((0,0,9,9),outline=255)
        words,raw,_=collect_words([observation('O',[0,0,10,10],core=[4,2,6,8])],mask,20,20,0)
        self.assertEqual(len(words),1);self.assertEqual(raw[0]['ownership'],'owned_ink')
        self.assertFalse(mask.crop((4,2,6,8)).getbbox())

    def test_partly_overlapping_pixels_are_not_silently_stitched_or_deduplicated(self):
        mask=Image.new('L',(20,20));ImageDraw.Draw(mask).rectangle((2,4,12,9),fill=255)
        words,_,unresolved=collect_words([observation('甲乙',[0,0,9,15]),observation('乙丙',[6,0,16,15])],mask,20,20,0)
        self.assertEqual(len(words),2);self.assertEqual(unresolved[0]['reason'],'overlapping_word_support')

    def test_nested_cells_pick_unique_inner_geometry_and_partial_crossings_stay_unresolved(self):
        words=[{'id':0,'bounds_source_pixels':[4,4,6,6],'cell':None,'cells':[]},
               {'id':1,'bounds_source_pixels':[8,4,12,6],'cell':None,'cells':[]}]
        cells=[cell(8,[0,0,20,20]),cell(3,[3,3,7,7])]
        self.assertEqual(assign_cells(words,cells),[]);self.assertEqual([w['cell'] for w in words],[3,8])
        words=[{'id':0,'bounds_source_pixels':[8,4,12,6],'cell':None,'cells':[]}]
        unresolved=assign_cells(words,[cell(0,[0,0,10,10]),cell(1,[10,0,20,10])])
        self.assertEqual(unresolved[0]['reason'],'word_crosses_candidate_cell_boundary')
        self.assertIsNone(words[0]['cell'])

    def test_table_merges_are_geometric_and_missing_or_overlapping_slots_are_explicit(self):
        tables=cell_tables([cell(0,[0,0,20,10]),cell(1,[0,10,10,20]),cell(2,[10,10,20,20])])
        self.assertEqual(len(tables),1);self.assertEqual(tables[0]['slots'][0]['colspan'],2)
        self.assertEqual(tables[0]['uncovered_slots'],[])
        tables=cell_tables([cell(0,[0,0,10,10]),cell(1,[10,0,20,10]),cell(2,[0,10,10,20])])
        self.assertEqual(tables[0]['uncovered_slots'],[[1,1]])
        tables=cell_tables([cell(0,[0,0,20,20]),cell(1,[5,5,10,10])])
        self.assertTrue(tables[0]['overlapping_cells'])

    def test_line_order_uses_geometry_and_does_not_bridge_two_rows_through_a_tall_box(self):
        words=[{'id':0,'bounds_layout_pixels':[12,1,18,6],'writing_modes':[False],'text':'乙'},
               {'id':1,'bounds_layout_pixels':[1,0,8,7],'writing_modes':[False],'text':'甲'}]
        c=cell(0,[0,0,20,20]);c['words']=[0,1]
        lines=cell_lines(words,[c],True);self.assertEqual(lines[0]['words'],[1,0])
        words.append({'id':2,'bounds_layout_pixels':[0,10,8,17],'writing_modes':[False],'text':'丙'})
        words.append({'id':3,'bounds_layout_pixels':[9,0,11,18],'writing_modes':[False],'text':'?'});c['words']=[0,1,2,3]
        lines=cell_lines(words,[c],True);self.assertEqual(len(lines),1)
        self.assertEqual(lines[0]['status'],'reading_order_unresolved')

    def test_layout_coordinates_respect_the_glyph_basis_without_changing_source_bounds(self):
        self.assertEqual(layout_box([1,2,3,7],20,10,1),[2,17,7,19])
        self.assertEqual(layout_box([1,2,3,7],20,10,3),[3,1,8,3])


if __name__=='__main__':unittest.main()

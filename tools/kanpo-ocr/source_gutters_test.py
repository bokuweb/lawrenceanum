from pathlib import Path
import unittest,copy,json
from PIL import Image,ImageDraw
from source_gutters import component_bounds,plan
P=Path(__file__).resolve().parent

def path(x,y0=2,y1=18,width=.5):
    return {'stroke':True,'fill':False,'line_width':width,'subpaths':[{'points':[[x,y0],[x,y1]],'closed':False,'curved':False}]}
def fixture(count=2):
    image=Image.new('RGBA',(20*count,20),'white');draw=ImageDraw.Draw(image)
    for col in range(count):
        for y in [3,12]:draw.rectangle((20*col+3,y,20*col+16,y+3),fill='black')
    paths=[path(20*i)for i in range(1,count)]
    return image,paths,[[0,0],[image.width,0],[image.width,image.height],[0,image.height]]
class SourceGutterControls(unittest.TestCase):
    def test_eight_connected_components_preserve_exact_pixel_edge_bounds(self):
        mask=Image.new('L',(5,4),0);mask.putpixel((0,0),255);mask.putpixel((1,1),255);mask.putpixel((4,3),255)
        self.assertEqual(sorted(component_bounds(mask)),[[0,0,2,2],[4,3,5,4]])
    def test_all_white_has_no_ink_component(self):
        self.assertEqual(component_bounds(Image.new('L',(8,9),0)),[])
    def test_source_stroke_in_full_height_white_gutter_splits_without_losing_pixels(self):
        image,paths,quad=fixture();result=plan(image,paths,quad,'horizontal_ltr');self.assertEqual(len(result['cuts']),1)
        self.assertEqual(result['rectangles'],[[0,0,20,20],[20,0,40,20]])
        reconstructed=Image.new(image.mode,image.size)
        for box in result['rectangles']:reconstructed.paste(image.crop(box),(box[0],box[1]))
        self.assertEqual(reconstructed.tobytes(),image.tobytes())
    def test_number_of_crops_follows_source_geometry_for_two_three_and_seven_columns(self):
        for count in [2,3,7]:
            image,paths,quad=fixture(count);self.assertEqual(len(plan(image,paths,quad,'horizontal_ltr')['rectangles']),count)
    def test_a_white_gutter_without_a_source_stroke_does_not_invent_a_column(self):
        image,_,quad=fixture();self.assertEqual(plan(image,[],quad,'horizontal_ltr')['cuts'],[])
    def test_partial_stroke_that_misses_a_row_component_cannot_split_whole_image(self):
        image,_,quad=fixture();self.assertEqual(plan(image,[path(20,2,9)],quad,'horizontal_ltr')['cuts'],[])
    def test_header_or_footer_ink_outside_the_source_stroke_prevents_global_column_crops(self):
        for y in [0,19]:
            image,paths,quad=fixture();image.putpixel((8,y),(0,0,0,255));self.assertEqual(plan(image,paths,quad,'horizontal_ltr')['cuts'],[])
    def test_full_width_ink_bridge_is_not_cut_using_the_lower_column_stroke(self):
        image,paths,quad=fixture();ImageDraw.Draw(image).rectangle((3,3,36,4),fill='black');self.assertEqual(plan(image,paths,quad,'horizontal_ltr')['cuts'],[])
    def test_ink_in_the_stroke_band_blocks_the_cut_even_if_other_rows_are_white(self):
        image,paths,quad=fixture();image.putpixel((20,10),(254,254,254,255));self.assertEqual(plan(image,paths,quad,'horizontal_ltr')['cuts'],[])
    def test_transparent_black_is_not_painted_nonwhite_ink(self):
        image,paths,quad=fixture();image.putpixel((20,10),(0,0,0,0));self.assertEqual(len(plan(image,paths,quad,'horizontal_ltr')['cuts']),1)
    def test_partial_alpha_nonwhite_is_kept_and_can_block_a_cut(self):
        image,paths,quad=fixture();image.putpixel((20,10),(0,0,0,1));self.assertEqual(plan(image,paths,quad,'horizontal_ltr')['cuts'],[])
    def test_scale_and_translation_in_pdf_units_do_not_change_pixel_crops(self):
        for scale in [.125,1,8]:
            image,paths,quad=fixture();quad=[[x*scale-37,y*scale+91]for x,y in quad]
            paths[0]['line_width']*=scale;paths[0]['subpaths'][0]['points']=[[x*scale-37,y*scale+91]for x,y in paths[0]['subpaths'][0]['points']]
            self.assertEqual(plan(image,paths,quad,'horizontal_ltr')['rectangles'],[[0,0,20,20],[20,0,40,20]])
    def test_curved_filled_unstroked_clipped_or_nonfinite_paths_do_not_authorize_crops(self):
        for key,value in [('fill',True),('stroke',False),('clip',[0,0,50,20]),('line_width',float('inf'))]:
            image,paths,quad=fixture();paths[0][key]=value;self.assertEqual(plan(image,paths,quad,'horizontal_ltr')['cuts'],[])
        image,paths,quad=fixture();paths[0]['subpaths'][0]['curved']=True;self.assertEqual(plan(image,paths,quad,'horizontal_ltr')['cuts'],[])
    def test_polyline_segments_and_duplicate_strokes_use_one_gutter_cut(self):
        image,paths,quad=fixture();paths[0]['subpaths'][0]['points']=[[10,2],[20,2],[20,18]];paths*=2
        result=plan(image,paths,quad,'horizontal_ltr');self.assertEqual(len(result['cuts']),1);self.assertEqual(len(result['cuts'][0]['witnesses']),2)
    def test_no_hidden_writing_direction_or_oblique_axis_assumption(self):
        image,paths,quad=fixture()
        with self.assertRaisesRegex(ValueError,'writing'):plan(image,paths,quad,None)
        quad[1][1]=1;self.assertEqual(plan(image,paths,quad,'horizontal_ltr')['cuts'],[])
    def test_empty_source_image_is_retained_in_full(self):
        image,paths,quad=fixture();image.paste('white',(0,0,image.width,image.height));result=plan(image,paths,quad,'horizontal_ltr');self.assertEqual(result['cuts'],[]);self.assertEqual(result['rectangles'],[[0,0,40,20]])
    def test_planning_does_not_mutate_pixels_or_path_evidence(self):
        image,paths,quad=fixture();before=image.tobytes();old=copy.deepcopy(paths);plan(image,paths,quad,'horizontal_ltr');self.assertEqual(image.tobytes(),before);self.assertEqual(paths,old)


class ComponentOracle(unittest.TestCase):
    def test_all_three_by_three_masks_match_independent_pixel_flood_fill(self):
        for bits in range(512):
            remaining={(i%3,i//3)for i in range(9)if bits&(1<<i)};image=Image.new('L',(3,3),0)
            for pixel in remaining:image.putpixel(pixel,173)
            expected=[]
            while remaining:
                seed=remaining.pop();component={seed};pending=[seed]
                while pending:
                    x,y=pending.pop()
                    for dx in [-1,0,1]:
                        for dy in [-1,0,1]:
                            neighbor=(x+dx,y+dy)
                            if neighbor in remaining:remaining.remove(neighbor);component.add(neighbor);pending.append(neighbor)
                expected.append([min(x for x,y in component),min(y for x,y in component),max(x for x,y in component)+1,max(y for x,y in component)+1])
            self.assertEqual(sorted(component_bounds(image)),sorted(expected),bits)
if __name__=='__main__':unittest.main()

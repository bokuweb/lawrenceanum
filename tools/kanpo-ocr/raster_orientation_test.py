import copy
import unittest
from PIL import Image,ImageDraw
from raster_orientation import observe_orientation,right_axis,turn_image,turn_matrix,turn_frame


def sample():
    image=Image.new('RGBA',(60,40),'white');draw=ImageDraw.Draw(image)
    observations=[]
    for text,(x0,y0,x1,y1) in zip(('甲','乙\u0301'),((5,5,10,15),(30,20,36,30))):
        draw.rectangle((x0,y0,x1-1,y1-1),fill='black')
        quad=[[x0/60,1-y0/40],[x1/60,1-y0/40],[x1/60,1-y1/40],[x0/60,1-y1/40]]
        observations.append({'candidates':[{'text':text,'characters':[{'text':text,'unicode_scalar_offset':0,
            'unicode_scalar_length':len(text),'quad':quad}]}]})
    return image,{'revision':3,'language_correction':False,'observations':observations}


def rotated(image,page,turns):
    new=turn_image(image,turns);result=copy.deepcopy(page);w,h=image.size
    for ob in result['observations']:
        for c in ob['candidates'][0]['characters']:
            moved=[]
            for xx,yy in c['quad']:
                x,y=xx*w,(1-yy)*h
                nx,ny=((x,y),(y,w-x),(w-x,h-y),(h-y,x))[turns]
                moved.append([nx/new.width,1-ny/new.height])
            c['quad']=moved
    return new,result


class OrientationTests(unittest.TestCase):
    def test_all_four_glyph_bases_restore_exact_original_pixels(self):
        image,page=sample()
        for turns in range(4):
            moved,report=rotated(image,page,turns)
            observed=observe_orientation(moved,report)
            self.assertEqual(observed['glyph_right_axis'],(-turns)%4)
            self.assertEqual(turn_image(moved,observed['quarter_turns_ccw']).tobytes(),image.tobytes())
            self.assertFalse(observed['coverage_verified'])
        self.assertEqual(page,sample()[1])

    def test_upright_characters_in_a_vertical_line_keep_their_glyph_basis(self):
        image,page=sample()
        # Line order/character centers never decides the glyph's right axis.
        self.assertEqual(observe_orientation(image,page)['quarter_turns_ccw'],0)

    def test_mixed_geometry_is_not_resolved_by_majority_or_text(self):
        image,page=sample();turned=copy.deepcopy(page)
        character=turned['observations'][1]['candidates'][0]['characters'][0]
        quad=character['quad'];character['quad']=quad[1:]+quad[:1]
        mixed=copy.deepcopy(page)
        mixed['observations']=[page['observations'][0]]*4+[turned['observations'][1]]
        result=observe_orientation(image,mixed)
        self.assertIsNone(result['glyph_right_axis'])
        self.assertEqual(result['status'],'mixed_glyph_axes')

    def test_one_region_or_empty_source_ink_cannot_authorize_global_rotation(self):
        image,page=sample();page['observations']=page['observations'][:1]
        self.assertIsNone(observe_orientation(image,page)['glyph_right_axis'])
        _,page=sample();empty=Image.new('RGBA',image.size,'white')
        result=observe_orientation(empty,page)
        self.assertIsNone(result['glyph_right_axis'])
        self.assertEqual(len(result['unresolved']),2)

    def test_missing_degenerate_and_inconsistent_corner_geometry_stays_unresolved(self):
        for mutation in ('missing','flat','opposite'):
            image,page=sample();c=page['observations'][0]['candidates'][0]['characters'][0]
            if mutation=='missing':del c['quad']
            elif mutation=='flat':c['quad']=[[0,0]]*4
            else:c['quad'][2],c['quad'][3]=c['quad'][3],c['quad'][2]
            self.assertIsNone(observe_orientation(image,page)['glyph_right_axis'])
        self.assertIsNone(right_axis([[0,0],[1,1],[1,0],[0,1]],60,60))

    def test_foreign_character_offsets_and_corrected_model_outputs_are_rejected(self):
        image,page=sample();page['observations'][1]['candidates'][0]['characters'][0]['unicode_scalar_length']=1
        with self.assertRaisesRegex(ValueError,'ranges differ'):observe_orientation(image,page)
        image,page=sample();page['language_correction']=True
        with self.assertRaisesRegex(ValueError,'uncorrected'):observe_orientation(image,page)

    def test_rotation_matrices_map_every_child_pixel_and_compose_an_existing_quarter_turn(self):
        image=Image.new('RGBA',(7,5))
        for y in range(5):
            for x in range(7):image.putpixel((x,y),(x*30,y*40,x+y,255))
        frame={'pixel_width':7,'pixel_height':5,'recognition_to_image':[0,1,-1,0,5,0]}
        for turns in range(4):
            moved=turn_image(image,turns);a,b,c,d,e,f=turn_matrix(7,5,turns)
            actual=turn_frame(frame,turns);self.assertEqual((actual['pixel_width'],actual['pixel_height']),moved.size)
            for y in range(moved.height):
                for x in range(moved.width):
                    xx,yy=a*(x+.5)+c*(y+.5)+e,b*(x+.5)+d*(y+.5)+f
                    self.assertEqual(moved.getpixel((x,y)),image.getpixel((int(xx),int(yy))))
                    aa,bb,cc,dd,ee,ff=actual['recognition_to_image']
                    self.assertEqual([aa*(x+.5)+cc*(y+.5)+ee,bb*(x+.5)+dd*(y+.5)+ff],[5-yy,xx])


if __name__ == '__main__':
    unittest.main()

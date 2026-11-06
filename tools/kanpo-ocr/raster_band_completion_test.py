import unittest
from PIL import Image, ImageDraw
from raster_grid import observe, _covered
from raster_band_completion import complete_bands


def interrupted():
    image = Image.new('RGBA', (160, 100), 'white')
    draw = ImageDraw.Draw(image)
    # Both long seeded runs share a narrow band but are connected only by
    # an actual short run on the other scanline.
    draw.rectangle((10, 30, 70, 31), fill='black')
    draw.rectangle((80, 30, 145, 31), fill='black')
    draw.rectangle((41, 31, 99, 31), fill='white')
    draw.rectangle((65, 31, 85, 31), fill='black')
    return image


def complete(image):
    original = observe(image, minimum_run=30)
    return original, complete_bands(image, original)


class BandCompletionTests(unittest.TestCase):
    def test_overlapping_seed_bands_do_not_authorize_a_wider_completed_stroke(self):
        image=Image.new('RGBA',(160,100),'white');draw=ImageDraw.Draw(image)
        draw.line((10,30,70,30),fill='black');draw.line((10,31,40,31),fill='black')
        draw.line((80,31,145,31),fill='black');draw.line((100,32,145,32),fill='black')
        draw.line((65,31,70,31),fill='black');draw.line((65,32,85,32),fill='black')
        original,result=complete(image)
        self.assertEqual(len(original['horizontal']),2)
        self.assertEqual(len(result['horizontal']),2)
        self.assertTrue(all(r['support_kind']=='solid_source' for r in result['horizontal']))
        self.assertEqual(result['rejected_completions'][0]['band'],[30,33])

    def test_short_actual_support_joins_seeded_runs_without_filling_white_pixels(self):
        image = interrupted()
        before = image.tobytes()
        original, result = complete(image)
        self.assertEqual(len(original['horizontal']), 2)
        self.assertEqual(len(result['horizontal']), 1)
        r = result['horizontal'][0]
        self.assertEqual(r['source_rules'], [0, 1])
        self.assertEqual(r['support_kind'], 'source_band_completion')
        self.assertEqual(r['band'], [30, 32])
        self.assertEqual(r['span'], [10, 146])
        self.assertTrue(_covered(r, 10, 146))
        self.assertEqual(image.getpixel((75, 30)), (255, 255, 255, 255))
        self.assertEqual(image.tobytes(), before)
        for row,a,b in r['support']:
            self.assertEqual(image.crop((a,row,b,row+1)).getextrema(),
                             ((0,0),(0,0),(0,0),(255,255)))

    def test_a_true_white_gap_remains_two_tracks(self):
        image = interrupted()
        ImageDraw.Draw(image).line((75,30,75,31),fill='white')
        _, result = complete(image)
        self.assertEqual(len(result['horizontal']), 2)
        self.assertFalse(any(_covered(r,10,146) for r in result['horizontal']))

    def test_diagonal_touching_pixels_are_not_four_connected_support(self):
        image = interrupted()
        draw = ImageDraw.Draw(image)
        draw.rectangle((65,30,85,31),fill='white')
        draw.rectangle((65,30,74,30),fill='black')
        draw.rectangle((75,31,85,31),fill='black')
        _, result = complete(image)
        self.assertEqual(len(result['horizontal']), 2)

    def test_connected_ink_outside_an_observed_band_cannot_expand_it(self):
        image = interrupted()
        ImageDraw.Draw(image).rectangle((65,20,66,45),fill='black')
        _, result = complete(image)
        self.assertEqual(result['horizontal'][0]['band'],[30,32])
        self.assertTrue(all(30<=s[0]<32 for r in result['horizontal'] for s in r['support']))

    def test_unseeded_short_marks_stay_unclassified(self):
        image = interrupted()
        ImageDraw.Draw(image).line((150,31,155,31),fill='black')
        _, result = complete(image)
        self.assertEqual(len(result['horizontal']),1)
        self.assertEqual(result['horizontal'][0]['span'],[10,146])

    def test_rotations_reflections_and_scale_preserve_the_exact_support_mask(self):
        image = interrupted()
        def support_mask(source):
            _, data = complete(source)
            mask = Image.new('L', source.size)
            draw = ImageDraw.Draw(mask)
            for axis,key in ((0,'horizontal'),(1,'vertical')):
                for rule in data[key]:
                    for cross,a,b in rule['support']:
                        draw.line((a,cross,b-1,cross) if axis==0 else (cross,a,cross,b-1),fill=255)
            return mask
        reference = support_mask(image)
        for operation in (Image.Transpose.ROTATE_90, Image.Transpose.ROTATE_180,
                          Image.Transpose.ROTATE_270, Image.Transpose.FLIP_LEFT_RIGHT,
                          Image.Transpose.FLIP_TOP_BOTTOM, Image.Transpose.TRANSPOSE,
                          Image.Transpose.TRANSVERSE):
            self.assertEqual(support_mask(image.transpose(operation)).tobytes(),
                             reference.transpose(operation).tobytes())
        enlarged = image.resize((320,200),Image.Resampling.NEAREST)
        self.assertEqual(support_mask(enlarged).tobytes(),
                         reference.resize(enlarged.size,Image.Resampling.NEAREST).tobytes())


if __name__ == '__main__':
    unittest.main()

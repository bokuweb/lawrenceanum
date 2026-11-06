import copy
import unittest
from audit_paint_gaps import candidates


def fixture():
    def glyph(x, text):
        return dict(text=text, font=31, size=1., invisible=False,
                    x=x, y=0., end_x=x+1., end_y=0., bbox=[x, 0., x+1., 1.])
    return dict(glyphs=[glyph(0., 'arbitrary'), glyph(2., 'source')], paths=[
        dict(fill=True, stroke=False, line_width=0., clip=None,
             subpaths=[dict(points=[[1.2, .2], [1.8, .2], [1.8, .8], [1.2, .8]])])])


class PaintGaps(unittest.TestCase):
    def test_endpoint_only_curves_and_unknown_stroke_joins_do_not_claim_an_ink_hull(self):
        for curved, stroke in ((True, False), (False, True)):
            p = fixture()
            path = p['paths'][0]
            path['stroke'] = stroke
            path['subpaths'][0]['curved'] = curved
            self.assertEqual(candidates(p), [])

    def test_geometry_is_independent_of_text_scale_origin_and_writing_direction(self):
        for turn in range(4):
            for scale in [.125, 1., 64.]:
                for offset in [-128., 0., 256.]:
                    def point(x, y):
                        for _ in range(turn):
                            x, y = -y, x
                        return x*scale+offset, y*scale+offset
                    def bounds(b):
                        points = [point(b[x], b[y]) for x in (0, 2) for y in (1, 3)]
                        return [min(p[0] for p in points), min(p[1] for p in points),
                                max(p[0] for p in points), max(p[1] for p in points)]
                    p = fixture()
                    for g in p['glyphs']:
                        g['x'], g['y'] = point(g['x'], g['y'])
                        g['end_x'], g['end_y'] = point(g['end_x'], g['end_y'])
                        g['bbox'] = bounds(g['bbox'])
                        g['size'] *= scale
                    for path in p['paths']:
                        for sub in path['subpaths']:
                            sub['points'] = [point(*v) for v in sub['points']]
                    rows = candidates(p)
                    self.assertEqual(len(rows), 1)
                    self.assertEqual(rows[0]['gap'], bounds([1., 0., 2., 1.]))
                    self.assertIsNone(rows[0]['text'])
                    self.assertFalse(rows[0]['visibility_verified'])

    def test_owned_material_or_noncontained_paint_does_not_become_an_insertion(self):
        p = fixture()
        glyph = copy.deepcopy(p['glyphs'][0])
        glyph.update(font=100, x=1.2, end_x=1.8, bbox=[1.2, .1, 1.8, .9])
        p['glyphs'].append(glyph)
        self.assertEqual(candidates(p), [])
        p = fixture()
        p['paths'][0]['subpaths'][0]['points'][0][0] = -.1
        self.assertEqual(candidates(p), [])
        p = fixture()
        p['paths'].clear()
        self.assertEqual(candidates(p), [])

    def test_competing_native_successors_do_not_choose_one_by_text(self):
        p = fixture()
        p['glyphs'].append(copy.deepcopy(p['glyphs'][1]))
        self.assertEqual(candidates(p), [])


if __name__ == '__main__':
    unittest.main()

import hashlib
import json
from pathlib import Path
import tempfile
import unittest

import numpy as np
from PIL import Image
import read_native_glyphs as reader


class GlyphRecognitionControls(unittest.TestCase):
    def test_opposed_arcs_are_source_geometry_not_completed_model_text(self):
        mask = np.zeros((64, 48), dtype=bool)
        for y in range(6, 58):
            x = 5 + abs(y - 32) // 8
            mask[y, x] = mask[y, 47 - x] = True
        mask[20:44, 18:26] = True
        layout = reader.opposed_arcs(mask)
        self.assertIsNotNone(layout)
        for text, expected in [('ヲ）', '（ヲ）'), ('（ア）', '（ア）'), ('（ん', '（ん）')]:
            self.assertEqual(reader.decode_candidate(layout, [{'candidates': [{'text': text, 'confidence': 0.01}]}]), expected)
        for text in ['AA', '（１２）', '二十八', 'ヲ\x00', '']:
            self.assertIsNone(reader.decode_candidate(layout, [{'candidates': [{'text': text}]}]))
        self.assertIsNone(reader.decode_candidate(None, [{'candidates': [{'text': '（ア）'}]}]))
        self.assertIsNone(reader.decode_candidate(layout, []))
        # A frame, despite three connected components, supplies no opposed arcs.
        straight = np.zeros_like(mask)
        straight[6:58, 5] = straight[6:58, 42] = True
        straight[20:44, 18:26] = True
        self.assertIsNone(reader.opposed_arcs(straight))

    def test_all_cardinal_font_rotations_and_reflections_restore_actual_pixel_order(self):
        source = np.arange(4 * 7 * 3, dtype=np.uint8).reshape((4, 7, 3))
        for x in [(1, 0), (-1, 0), (0, 1), (0, -1)]:
            for down in [(1, 0), (-1, 0), (0, 1), (0, -1)]:
                if x[0] * down[0] + x[1] * down[1]:
                    continue
                points = [(x[0]*i+down[0]*j, x[1]*i+down[1]*j) for j in range(4) for i in range(7)]
                left, top = min(p[0] for p in points), min(p[1] for p in points)
                image = np.zeros((max(p[1] for p in points)-top+1, max(p[0] for p in points)-left+1, 3), dtype=np.uint8)
                for j in range(4):
                    for i in range(7):
                        a, b = x[0]*i+down[0]*j-left, x[1]*i+down[1]*j-top
                        image[b, a] = source[j, i]
                actual = reader.upright(Image.fromarray(image), {'font_x': x, 'font_y': [-n for n in down]})
                np.testing.assert_array_equal(np.asarray(actual), source)
        with self.assertRaises(ValueError):
            reader.upright(Image.fromarray(source), {'font_x': [1, 1], 'font_y': [0, -1]})

    def test_foreign_native_snapshot_is_rejected_before_rendering_or_ocr(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root/'source.pdf').write_bytes(b'not a PDF')
            (root/'native.json').write_text(json.dumps({'schema': 2, 'document_sha256': hashlib.sha256(b'other').hexdigest(), 'document_bytes': 9}))
            (root/'analysis.json').write_text(json.dumps({'native': {'pages': []}}))
            with self.assertRaisesRegex(ValueError, 'does not belong'):
                reader.run(root/'source.pdf', root/'native.json', root/'analysis.json', root/'output')
            self.assertFalse((root/'output').exists())


if __name__ == '__main__':
    unittest.main()

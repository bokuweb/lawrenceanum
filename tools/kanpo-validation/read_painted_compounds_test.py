import unittest
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
from unittest.mock import patch

import numpy as np

from read_painted_compounds import digest, enclosed_cells, feature, rank, recognize, run, trusted_glyph

ROOT = Path(__file__).resolve().parents[2]
EXTRACTOR = ROOT / 'target/debug/examples/native_pages'


def enclosed(inner):
    height, width = inner.shape
    mask = np.zeros((height, width + 40), dtype=bool)
    mask[:, 20:20 + width] = inner
    for y in range(height):
        x = 3 + round(10 * (2 * y / (height - 1) - 1) ** 2)
        mask[y, x:x + 2] = True
        mask[y, mask.shape[1] - x - 2:mask.shape[1] - x] = True
    return mask


def shapes():
    a = np.zeros((20, 20), dtype=bool)
    a[:, 9:12] = True
    a[9:12, :] = True
    b = np.zeros((20, 20), dtype=bool)
    b[:, :3] = True
    b[-3:, :] = True
    return a, b


def templates(masks):
    return {text: {**feature(mask), 'source': {'synthetic': text}} for text, mask in masks.items()}


class PaintedCompounds(unittest.TestCase):
    def test_arbitrary_counts_and_labels_are_learned_from_shape(self):
        a, b = shapes()
        for count in (2, 3, 5, 7):
            inner = np.zeros((count * 32, 24), dtype=bool)
            for i in range(count):
                inner[i * 32 + 6:i * 32 + 26, 2:22] = (a, b)[i % 2]
            mask = enclosed(inner)
            for mapping in ({'甲': a, '乙': b}, {'未知': a, '別字': b}):
                # Labels are opaque to the recognizer, including their lengths.
                keys = list(mapping)
                result = recognize(mask, templates(mapping))
                self.assertEqual(result['status'], 'candidate_unverified')
                self.assertEqual(result['text'], '（' + ''.join(keys[i % 2] for i in range(count)) + '）')
                self.assertEqual(len(result['layout']['cells']), count)

    def test_whole_disconnected_character_is_not_forced_into_multiple_characters(self):
        for count, label in ((2, '二'), (3, '三')):
            inner = np.zeros((count * 32, 24), dtype=bool)
            for i in range(count):
                inner[i * 32 + 15:i * 32 + 18, 2:22] = True
            one = np.ones((3, 20), dtype=bool)
            result = recognize(enclosed(inner), templates({label: inner, '一': one}))
            self.assertEqual(result['status'], 'whole_glyph_competes')
            self.assertNotIn('text', result)

    def test_rectangular_sides_missing_arcs_and_ambiguous_templates_fail_closed(self):
        a, _ = shapes()
        inner = np.zeros((64, 24), dtype=bool)
        inner[6:26, 2:22] = a
        inner[38:58, 2:22] = a
        mask = enclosed(inner)
        result = recognize(mask, templates({'甲': a, '乙': a}))
        self.assertEqual(result['status'], 'ambiguous_glyph')
        self.assertNotIn('text', result)
        self.assertEqual(recognize(mask, {})['status'], 'ambiguous_glyph')
        table = np.zeros_like(mask)
        table[:, 3:5] = True
        table[:, -5:-3] = True
        table[:, 20:44] = inner
        self.assertIsNone(enclosed_cells(table))
        self.assertIsNone(enclosed_cells(inner))
        self.assertIsNone(enclosed_cells(np.zeros_like(mask)))

    def test_unknown_invisible_or_overlapped_font_labels_do_not_train(self):
        g = {'text': '任', 'bbox': [0, 0, 10, 10], 'vertical': True,
             'invisible': False, 'font': 0}
        p = {'glyphs': [g], 'fonts': [{'ordering': 'Japan1', 'has_to_unicode': False}], 'paths': []}
        self.assertTrue(trusted_glyph(p, 0, (100, 100)))
        p['fonts'][0]['ordering'] = 'Identity'
        self.assertFalse(trusted_glyph(p, 0, (100, 100)))
        p['fonts'][0]['has_to_unicode'] = True
        self.assertTrue(trusted_glyph(p, 0, (100, 100)))
        p['glyphs'].append(dict(g))
        self.assertFalse(trusted_glyph(p, 0, (100, 100)))
        p['glyphs'].pop()
        p['paths'] = [{'fill': True, 'stroke': False, 'line_width': 0,
                       'subpaths': [{'points': [[2, 2], [8, 8]]}]}]
        self.assertFalse(trusted_glyph(p, 0, (100, 100)))
        p['paths'] = []
        g['invisible'] = True
        self.assertFalse(trusted_glyph(p, 0, (100, 100)))

    def test_normalization_retains_topology_and_is_independent_of_whitespace(self):
        a, b = shapes()
        model = templates({'甲': a, '乙': b})
        for origin in (0, 7, 13):
            mask = np.pad(a, ((origin, 20), (origin, 30)))
            self.assertEqual(rank(mask, model)[0]['text'], '甲')
            self.assertEqual(rank(mask, model)[0]['distance'], 0)

    def test_foreign_unbound_or_wrong_sized_snapshots_are_rejected_before_rendering(self):
        raw = b'captured PDF input; never a valid PDF to render'
        valid = {'schema': 2, 'kind': 'native_page_snapshot',
                 'document_sha256': digest(raw), 'document_bytes': len(raw), 'pages': []}
        for change in ({'schema': 1}, {'kind': 'other'}, {'document_sha256': '0' * 64},
                       {'document_bytes': len(raw) + 1}):
            snapshot = {**valid, **change}
            with tempfile.TemporaryDirectory() as directory:
                base = Path(directory)
                original = base / 'source.pdf'
                original.write_bytes(raw)
                extractor = base / 'native_pages'
                extractor.write_bytes(b'unexecuted source extractor')
                with patch('read_painted_compounds.subprocess.run',
                           return_value=SimpleNamespace(stdout=json.dumps(snapshot).encode())):
                    with self.assertRaisesRegex(ValueError, 'captured PDF bytes'):
                        run(original, extractor, base / 'output')


@unittest.skipUnless(EXTRACTOR.is_file(), 'build kanpo-pdf --example native_pages first')
class OriginalSourceCompounds(unittest.TestCase):
    def test_all_original_compounds_against_source_review(self):
        fixtures = ROOT / 'crates/kanpo-parser/tests/data/workbook'
        review = json.loads((fixtures / 'painted-compound-review.json').read_text())
        with tempfile.TemporaryDirectory() as directory:
            report = run(fixtures / review['fixture'], EXTRACTOR, Path(directory) / 'output')
        self.assertEqual(report['source']['document_sha256'], review['document_sha256'])
        self.assertFalse(report['canonical_integration'])
        keys = ('page', 'before_glyph', 'after_glyph', 'text')
        self.assertEqual([{key: row[key] for key in keys} for row in report['observations']],
                         review['observations'])
        self.assertTrue(all(row['status'] == 'candidate_unverified'
                            and not row['visibility_verified'] for row in report['observations']))

    def test_original_thin_rules_are_not_recognized_as_text(self):
        pdf = ROOT / 'crates/kanpo-parser/tests/data/workbook/case-13.pdf'
        with tempfile.TemporaryDirectory() as directory:
            report = run(pdf, EXTRACTOR, Path(directory) / 'output')
        self.assertEqual(len(report['observations']), 3)
        self.assertTrue(all(row['status'] == 'unclassified_geometry'
                            and row['text'] is None for row in report['observations']))


if __name__ == '__main__':
    unittest.main()

import unittest
from unittest.mock import patch
from types import SimpleNamespace
from compare_references import publication_headings, publication_identity, reference_fields


class PublicationHeadingTests(unittest.TestCase):
    def test_only_explicit_line_initial_identifiers_are_used(self):
        self.assertEqual(publication_headings([
            "○ 任意省 告示 第三百七十三号", "〇任意省告示第三百七十三号",
            "本文中の〇別省令第九号", "○参考（任意省三七三）",
            "○任意省令第１２号　次の本文", "〇任意省令第三号。本文",
        ]), {"任意省告示第三百七十三号", "任意省令第12号", "任意省令第三号"})

    def test_different_identifiers_do_not_match_despite_same_publisher(self):
        listed=publication_headings(["〇任意省告示第三百七十三号"])
        linked=publication_headings(["○任意省告示第二百二十四号"])
        self.assertTrue(listed.isdisjoint(linked))
        self.assertFalse(publication_headings(["本文だけ"]));

    def test_unresolved_glyphs_do_not_prove_a_different_publication(self):
        self.assertEqual(publication_identity({'\ue1e7甲\ue225乙令第六号'}, {'甲・乙令第六号'}), 'unverified')
        self.assertEqual(publication_identity({'甲令第三号'}, {'甲令第四号'}), 'mismatch')
        self.assertEqual(publication_identity({'甲令第三号'}, {'甲令第三号'}), 'common_heading')
        self.assertEqual(publication_identity({'甲令第三号', '\U000f0000'}, {'甲令第三号'}), 'common_heading')


class ReferenceContextTests(unittest.TestCase):
    def read(self, body, continuation=False):
        with patch('compare_references.subprocess.run', return_value=SimpleNamespace(stdout=body.encode())):
            return reference_fields('synthetic.rtf', continuation)

    def table(self, left, right, header=False):
        return '<table>'+('<tr><td>改正後</td><td>改正前</td></tr>' if header else '')+f'<tr><td><p>{left}</p></td><td><p>{right}</p></td></tr></table>'

    def test_explicit_header_establishes_roles_and_cell_paragraphs_do_not_end_them(self):
        fields, continuation, warnings = self.read(self.table('甲', '乙', True)+self.table('丙', '丁'))
        self.assertEqual(fields, ['甲丙','乙丁']);self.assertTrue(continuation);self.assertEqual(warnings, [])

    def test_ordinary_prose_ends_context_before_a_normal_two_column_table(self):
        fields, continuation, warnings = self.read('<p>追加する本文。</p>'+self.table('見出し','説明'), True)
        self.assertEqual(fields, ['','']);self.assertFalse(continuation)
        self.assertEqual(warnings, ['Table without a proven comparison header'])

    def test_prose_between_tables_cannot_reclassify_a_later_ordinary_table(self):
        fields, continuation, warnings = self.read(self.table('甲','乙',True)+'<p>次の独立した本文。</p>'+self.table('行名','値'))
        self.assertEqual(fields, ['甲','乙']);self.assertFalse(continuation);self.assertEqual(len(warnings),1)

    def test_composition_and_continuation_controls_preserve_an_existing_context(self):
        fields, continuation, warnings = self.read('<p>〈組方向コード＝１１〉（ｔ 天上：１）</p><p>〈表継続〉</p>'+self.table('甲','乙'),True)
        self.assertEqual(fields, ['甲','乙']);self.assertTrue(continuation);self.assertEqual(warnings,[])

    def test_a_continuation_marker_without_a_prior_header_cannot_invent_roles(self):
        fields, continuation, warnings = self.read('<p>〈表継続〉</p>'+self.table('甲','乙'))
        self.assertEqual(fields, ['','']);self.assertFalse(continuation);self.assertEqual(len(warnings),1)


if __name__ == "__main__":
    unittest.main()

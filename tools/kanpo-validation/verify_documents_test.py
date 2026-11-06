import copy
import unittest
from verify_documents import verify


def fixture(text='甲乙'):
    source = {'blocks': [{'kind': 'paragraph', 'runs': [{'text': text, 'underline': False}]}]}
    target = {'blocks': [{'kind': 'paragraph', 'runs': [{'kind': 'text', 'text': text, 'underline': False}]}]}
    return {'runtime': {'document_sha256': 'original', 'native_glyph_engine': 'none'},
            'native': {'articles': [{'document': source}], 'pages': [{'words': [{'id': 0, 'status': 'used'}],
                'regions': [{'role': 'body', 'lines': [{'text': text, 'words': [0]}]}]}]},
            'documents': [target], 'painted_text': None, 'assets': [], 'placements': [], 'page_placements': [],
            'unresolved': [], 'underlines': {'parallel_bindings': [], 'source_issues': [], 'unresolved_fields': []},
            'ruby': {'bindings': [], 'unresolved': []}, 'frames': {'bindings': [], 'unresolved': []},
            'baseline_shifts': {'bindings': [], 'unresolved': []},
            'native_glyphs': {'bindings': [], 'source_issues': []}}


class CompleteStreams(unittest.TestCase):
    def test_complete_stream_is_conserved_and_silent_text_change_is_rejected(self):
        model = fixture()
        self.assertTrue(verify(model)['all_checks_passed'])
        model['documents'][0]['blocks'][0]['runs'][0]['text'] = '甲丙'
        with self.assertRaisesRegex(AssertionError, 'complete native field'):
            verify(model)

    def test_only_the_original_proved_frame_character_can_be_removed(self):
        model = fixture('□０')
        model['documents'][0]['blocks'][0]['runs'][0]['text'] = '０'
        model['frames']['bindings'] = [{'source_document_sha256': 'original', 'document': 0,
                                       'field': 'paragraph', 'offset': 0, 'length': 1, 'text': '０',
                                       'removed_offset': 0, 'removed_length': 1, 'removed_text': '□'}]
        self.assertTrue(verify(model)['all_checks_passed'])
        changed = copy.deepcopy(model)
        changed['frames']['bindings'][0]['source_document_sha256'] = 'another source'
        with self.assertRaises(AssertionError):
            verify(changed)
        model['frames']['bindings'][0]['removed_offset'] = 1
        with self.assertRaises(AssertionError):
            verify(model)

    def test_duplicate_word_ownership_or_missing_page_asset_cannot_pass(self):
        model = fixture()
        model['native']['pages'][0]['regions'][0]['lines'][0]['words'].append(0)
        with self.assertRaises(AssertionError):
            verify(model)
        model = fixture()
        model['page_placements'] = [{'asset': 0, 'page': 0, 'kind': 'page_surface'}]
        with self.assertRaises(AssertionError):
            verify(model)


if __name__ == '__main__':
    unittest.main()

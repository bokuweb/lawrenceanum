import copy
import io
import unittest
from pypdf import PdfWriter
from pypdf.generic import DecodedStreamObject, NameObject
from verify_native_paint import verify


class IndependentPaint(unittest.TestCase):
    def fixture(self):
        writer = PdfWriter()
        page = writer.add_blank_page(100, 100)
        stream = DecodedStreamObject()
        stream.set_data(b'[3 2] 1 d 2 w 10 10 m 80 10 l S')
        page[NameObject('/Contents')] = writer._add_object(stream)
        output = io.BytesIO()
        writer.write(output)
        num = lambda v: {'type': 'int', 'value': v}
        command = lambda op, args: {'operator_hex': op.encode().hex(),
                                    'operands': [num(v) for v in args], 'matrices': []}
        draw = {'commands': [command('m', [10, 10]), command('l', [80, 10])],
                'state': [{'operator_hex': '64', 'operands': [{'type': 'array', 'values': [num(3), num(2)]}, num(1)], 'resource': None},
                          {'operator_hex': '77', 'operands': [num(2)], 'resource': None}],
                'operator_hex': '53', 'matrices': [], 'clips': []}
        model = {'page_graphics': [{'paths': [draw], 'crop_box': [0, 0, 100, 100], 'rotate': 0}]}
        return output.getvalue(), model

    def test_independent_original_and_replayed_dash_have_identical_pixels(self):
        pdf, model = self.fixture()
        self.assertTrue(verify(pdf, model, 2)['all_pixels_equal'])

    def test_a_lost_dash_or_changed_path_cannot_pass_the_pixel_gate(self):
        pdf, model = self.fixture()
        changed = copy.deepcopy(model)
        changed['page_graphics'][0]['paths'][0]['state'].pop(0)
        self.assertFalse(verify(pdf, changed, 2)['all_pixels_equal'])
        changed = copy.deepcopy(model)
        changed['page_graphics'][0]['paths'][0]['commands'][1]['operands'][0]['value'] = 70
        self.assertFalse(verify(pdf, changed, 2)['all_pixels_equal'])


if __name__ == '__main__':
    unittest.main()

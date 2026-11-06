#!/usr/bin/env python3
"""Independently render original PDF paths and the parsed construction/state.

Text and images are excluded on both sides. This audits vector preservation,
including dashes, Bezier controls, clipping, paint rules and scoped state; it does
not certify text reading order or image compositing. No expected manuscript is
read. Requires pypdf, pypdfium2 and Pillow.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path

from pypdf import PdfReader, PdfWriter
from pypdf.generic import ContentStream, DecodedStreamObject, NameObject
import pypdfium2 as pdfium

TEXT = {b'BT', b'ET', b'Tf', b'Tc', b'Tw', b'Tz', b'TL', b'Tr', b'Ts',
        b'Td', b'TD', b'Tm', b'T*', b'Tj', b'TJ', b"'", b'"'}
PAINT = {b'S', b's', b'f', b'F', b'f*', b'B', b'B*', b'b', b'b*'}


def name(raw):
    return b'/' + b''.join(bytes([c]) if 33 <= c <= 126 and c not in b'()<>[]{}/%#'
                           else f'#{c:02X}'.encode() for c in raw)


def number(value):
    return format(value, '.17g').encode() if isinstance(value, float) else str(value).encode()


def operand(value):
    kind = value['type']
    if kind in ('real', 'int'):
        return number(value['value'])
    if kind == 'name':
        return name(bytes.fromhex(value['hex']))
    if kind == 'string':
        return b'<' + value['hex'].encode() + b'>'
    if kind == 'array':
        return b'[' + b' '.join(operand(v) for v in value['values']) + b']'
    if kind == 'dict':
        return b'<<' + b' '.join(name(bytes.fromhex(k)) + b' ' + operand(v)
                                for k, v in value['entries']) + b'>>'
    if kind == 'bool':
        return b'true' if value['value'] else b'false'
    if kind == 'null':
        return b'null'
    raise ValueError(f'Cannot replay unresolved resource {kind}')


def operator(item):
    return b' '.join([*(operand(v) for v in item['operands']),
                      bytes.fromhex(item['operator_hex'])]) + b'\n'


def matrix(values):
    return b' '.join(number(v) for v in values) + b' cm\n'


def construct(commands):
    # The current path is outside q/Q's graphics state. Each command uses its
    # original CTM, including a cm between m and l in the same path.
    return b''.join(b'q\n' + b''.join(matrix(m) for m in c['matrices']) + operator(c) + b'Q\n' for c in commands)


def page_to_user(point, crop, rotate):
    x, y = point
    x0, y0, x1, y1 = crop
    if rotate == 90:
        return y + x0, x + y0
    if rotate == 180:
        return x1 - x, y + y0
    if rotate == 270:
        return x1 - y, y1 - x
    return x + x0, y1 - y


def replay(page, resources):
    result = bytearray()
    for draw in page['paths']:
        result.extend(b'q\n')
        for clip in draw['clips']:
            if clip['origin'] == 'form_bbox':
                points = [page_to_user(p, page['crop_box'], page['rotate']) for p in clip['page_quad']]
                for i, point in enumerate(points):
                    result.extend(b' '.join(number(v) for v in point) + (b' m\n' if i == 0 else b' l\n'))
                result.extend(b'h\n')
            else:
                result.extend(construct(clip['commands']))
            result.extend(b'W* n\n' if clip['rule'] == 'evenodd' else b'W n\n')
        for state in draw['state']:
            op = bytes.fromhex(state['operator_hex'])
            if state['resource'] is not None:
                # Local resource declarations are embedded under fresh names;
                # a Form's /A must never resolve to its caller's unrelated /A.
                if op != b'gs':
                    raise ValueError('Scoped colour/pattern resources need a resource adapter')
                key = f'/KP{len(resources)}'
                # Parse a native dictionary with the independent PDF parser.
                from pypdf.generic import read_object
                resources[NameObject(key)] = read_object(io.BytesIO(operand(state['resource'])), None)
                result.extend(key.encode() + b' gs\n')
            else:
                result.extend(operator(state))
        result.extend(construct(draw['commands']))
        result.extend(b''.join(matrix(m) for m in draw['matrices']))
        result.extend(bytes.fromhex(draw['operator_hex']) + b'\nQ\n')
    return bytes(result)


def pdf_page(reader, source, program, ext_states=None):
    from pypdf.generic import DictionaryObject
    writer = PdfWriter()
    # Annotation objects are outside this page-content paint audit. Some
    # original files also contain malformed encrypted annotation strings.
    page = writer.add_page(source, excluded_keys=("/Annots",))
    content = DecodedStreamObject()
    content.set_data(program)
    page[NameObject('/Contents')] = writer._add_object(content)
    if ext_states is not None:
        page[NameObject('/Resources')] = DictionaryObject({NameObject('/ExtGState'): ext_states})
    stream = io.BytesIO()
    writer.write(stream)
    return stream.getvalue()


def pixels(pdf_bytes, scale):
    doc = pdfium.PdfDocument(pdf_bytes)
    page = doc[0]
    try:
        bitmap = page.render(scale=scale)
        try:
            return bitmap.to_pil().convert('RGBA')
        finally:
            bitmap.close()
    finally:
        page.close()
        doc.close()


def verify(pdf, analysis, scale=1):
    from pypdf.generic import DictionaryObject
    reader = PdfReader(io.BytesIO(pdf))
    if len(reader.pages) != len(analysis['page_graphics']):
        raise ValueError('page count mismatch')
    rows = []
    for i, (page, parsed) in enumerate(zip(reader.pages, analysis['page_graphics'])):
        content = page.get_contents()
        original = ContentStream(content, reader)
        if any(op in {b'Do', b'INLINE IMAGE'} for _, op in original.operations):
            # Image XObjects can be omitted, but Forms may themselves paint
            # paths. Do not silently certify a page whose Form was skipped.
            xobjects = page.get('/Resources', {}).get('/XObject', {})
            for args, op in original.operations:
                if op == b'Do' and xobjects[args[0]].get_object().get('/Subtype') != '/Image':
                    raise ValueError('Original Form paths require recursive independent flattening')
        original.operations = [(args, op) for args, op in original.operations
                               if op not in TEXT | {b'Do', b'INLINE IMAGE'}]
        count = sum(op in PAINT for _, op in original.operations)
        if count != len(parsed['paths']):
            raise ValueError(f'page {i}: independent path count {count} != {len(parsed["paths"])}')
        expected = pdf_page(reader, page, original.get_data())
        states = DictionaryObject()
        actual = pdf_page(reader, page, replay(parsed, states), states)
        a, b = pixels(expected, scale), pixels(actual, scale)
        if a.size != b.size:
            raise ValueError('rendered dimensions differ')
        import numpy as np
        av, bv = np.asarray(a), np.asarray(b)
        changed = int(np.any(av != bv, axis=2).sum())
        rows.append({'page': i, 'paths': count, 'different_pixels': changed,
                     'original_RGBA_sha256': hashlib.sha256(a.tobytes()).hexdigest(),
                     'parsed_RGBA_sha256': hashlib.sha256(b.tobytes()).hexdigest()})
    return {'scope': 'original_vector_paint_only', 'excluded_page_keys': ['/Annots'],
            'scale': scale, 'pages': rows,
            'all_pixels_equal': all(r['different_pixels'] == 0 for r in rows)}


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument('pdf', type=Path)
    cli.add_argument('analysis', type=Path)
    cli.add_argument('output', type=Path)
    cli.add_argument('--scale', type=float, default=1)
    args = cli.parse_args()
    report = verify(args.pdf.read_bytes(), json.loads(args.analysis.read_bytes()), args.scale)
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    print('pages', len(report['pages']), 'equal', report['all_pixels_equal'])
    return 0 if report['all_pixels_equal'] else 1


if __name__ == '__main__':
    raise SystemExit(main())

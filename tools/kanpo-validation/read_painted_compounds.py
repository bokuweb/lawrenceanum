#!/usr/bin/env python3
"""Read enclosed vertical compound paint using this PDF's mapped native glyphs.

This opt-in source-font recognizer emits observations, not native Words. It has
no manuscript/expected-text input and never changes kanpo-parser's Analysis.
Unknown shapes, ambiguous partitions and whole-glyph alternatives are retained.
Requires Pillow, numpy and pypdfium2; no model download or network access.
"""
import argparse
from contextlib import closing
import hashlib
import importlib.metadata
import io
import json
from pathlib import Path
import subprocess
import tempfile

import numpy as np
from PIL import Image
import pypdfium2

from audit_paint_gaps import candidates, overlaps, positive, paint_bounds


def digest(data):
    return hashlib.sha256(data).hexdigest()


def components(mask):
    """Eight-connected foreground, with half-open integer pixel bounds."""
    ink = {(int(y), int(x)) for y, x in zip(*np.where(mask))}
    result = []
    while ink:
        point = min(ink)
        ink.remove(point)
        queue, points = [point], []
        while queue:
            y, x = queue.pop()
            points.append((y, x))
            for dy in (-1, 0, 1):
                for dx in (-1, 0, 1):
                    neighbor = y + dy, x + dx
                    if neighbor in ink:
                        ink.remove(neighbor)
                        queue.append(neighbor)
        ys, xs = zip(*points)
        result.append({'box': (min(xs), min(ys), max(xs) + 1, max(ys) + 1),
                       'points': points})
    return result


def normalize(mask):
    ys, xs = np.where(mask)
    if not len(xs):
        return None
    tight = mask[min(ys):max(ys) + 1, min(xs):max(xs) + 1]
    return np.asarray(Image.fromarray(tight).resize((64, 64), Image.Resampling.NEAREST))


def distance(mask):
    """Exact Chebyshev distance to foreground in the normalized bitmap."""
    reached = mask.copy()
    if not reached.any():
        raise ValueError('empty template')
    result = np.zeros(mask.shape, dtype=np.uint8)
    step = 0
    while not reached.all():
        padded = np.pad(reached, 1)
        expanded = reached.copy()
        for dy in (-1, 0, 1):
            for dx in (-1, 0, 1):
                expanded |= padded[1 + dy:65 + dy, 1 + dx:65 + dx]
        step += 1
        result[expanded & ~reached] = step
        reached = expanded
    return result


def feature(mask):
    normalized = normalize(mask)
    if normalized is None:
        return None
    return {'mask': normalized, 'distance': distance(normalized),
            'components': len(components(normalized))}


def rank(mask, templates):
    own = feature(mask)
    if own is None:
        return []
    scores = []
    for label, template in templates.items():
        if own['components'] != template['components']:
            continue
        score = (np.mean(template['distance'][own['mask']])
                 + np.mean(own['distance'][template['mask']])) / 2
        scores.append({'text': label, 'distance': float(score),
                       'template': template['source']})
    return sorted(scores, key=lambda s: (s['distance'], s['text']))


def enclosed_cells(mask):
    """Two opposed arcs and a unique uncut equal-pitch vertical lattice.

    Pixel phase can give the two arcs different heights. Every inner component's
    centre must be supported by both arcs; no distance tolerance is used.
    A single native character may also have disconnected strokes: the caller
    must compare the whole inner glyph before treating these as several cells.
    """
    parts = components(mask)
    if len(parts) < 4:
        return None
    left = min(parts, key=lambda c: c['box'][0])
    right = max(parts, key=lambda c: c['box'][2])
    if left is right or left['box'][2] >= right['box'][0]:
        return None
    inner = [c for c in parts if c is not left and c is not right]
    if not all(left['box'][2] <= c['box'][0] < c['box'][2] <= right['box'][0]
               and all(a['box'][1] < (c['box'][1] + c['box'][3]) / 2 < a['box'][3]
                       for a in (left, right)) for c in inner):
        return None
    for arc, sign in ((left, -1), (right, 1)):
        top, bottom = arc['box'][1], arc['box'][3]
        rows = {}
        for y, x in arc['points']:
            rows.setdefault(y, []).append(x)
        if len(rows) != bottom - top:
            return None
        mean = lambda y: sum(rows[y]) / len(rows[y])
        middle = (top + bottom - 1) // 2
        if not all(sign * mean(middle) > sign * mean(y) for y in (top, bottom - 1)):
            return None
    top = min(left['box'][1], right['box'][1])
    bottom = max(left['box'][3], right['box'][3])
    height = bottom - top
    maximum = max(c['box'][3] - c['box'][1] for c in inner)
    partitions = []
    for count in range(2, height // maximum + 1):
        pitch = height // count
        cells = [[] for _ in range(count)]
        for part in inner:
            a, z = part['box'][1], part['box'][3]
            slot = (a - top) // pitch
            if slot < 0 or slot >= count or z - top > (slot + 1) * pitch:
                break
            cells[slot].append(part)
        else:
            if all(cells):
                partitions.append((pitch, cells))
    if len(partitions) != 1:
        return None
    pitch, cells = partitions[0]
    boxes = [[left['box'][2], top + i * pitch, right['box'][0], top + (i + 1) * pitch]
             for i in range(len(cells))]
    return {'arcs': [list(left['box']), list(right['box'])], 'cells': boxes,
            'inner': [left['box'][2], top, right['box'][0], bottom]}


def crop(mask, box):
    x0, y0, x1, y1 = box
    return mask[y0:y1, x0:x1]


def recognize(mask, templates):
    layout = enclosed_cells(mask)
    if layout is None:
        return {'status': 'unclassified_geometry'}
    cells = [rank(crop(mask, box), templates) for box in layout['cells']]
    whole = rank(crop(mask, layout['inner']), templates)
    evidence = {'layout': layout, 'cells': [s[:2] for s in cells],
                'whole_glyph_alternatives': whole[:2]}
    if any(not s or len(s) > 1 and s[0]['distance'] == s[1]['distance'] for s in cells):
        return {'status': 'ambiguous_glyph', **evidence}
    mean = sum(s[0]['distance'] for s in cells) / len(cells)
    if whole and whole[0]['distance'] <= mean:
        return {'status': 'whole_glyph_competes', **evidence}
    return {'status': 'candidate_unverified',
            'text': '（' + ''.join(s[0]['text'] for s in cells) + '）', **evidence}


def render(page, box, scale):
    width, height = page.get_size()
    x0, y0, x1, y1 = box
    if not positive(box) or not (0 <= x0 < x1 <= width and 0 <= y0 < y1 <= height):
        raise ValueError('source crop is outside the actual PDF page')
    bitmap = page.render(scale=scale, crop=(x0, height - y1, width - x1, y0))
    try:
        image = bitmap.to_pil().convert('L')
        buffer = io.BytesIO()
        image.save(buffer, format='PNG')
        return np.asarray(image) < 128, buffer.getvalue(), image.size
    finally:
        bitmap.close()


def trusted_glyph(page, index, size):
    glyph = page['glyphs'][index]
    font = page['fonts'][glyph['font']]
    text, box = glyph['text'], glyph['bbox']
    width, height = size
    return (not glyph['invisible'] and glyph['vertical'] and len(text) == 1
            and '\u4e00' <= text <= '\u9fff'
            and (font['ordering'] == 'Japan1' or font['has_to_unicode'])
            and positive(box) and 0 <= box[0] < box[2] <= width
            and 0 <= box[1] < box[3] <= height
            and not any(i != index and not other['invisible'] and overlaps(box, other['bbox'])
                        for i, other in enumerate(page['glyphs']))
            and all((not path['fill'] and not path['stroke'])
                    or (b := paint_bounds(path)) is not None and not overlaps(box, b)
                    for path in page['paths']))


def run(pdf, extractor, output, scale=16):
    raw = pdf.read_bytes()
    extractor_bytes = extractor.read_bytes()
    scripts = {name: Path(__file__).with_name(name).read_bytes()
               for name in ('read_painted_compounds.py', 'audit_paint_gaps.py')}
    output.mkdir(parents=True, exist_ok=False)
    implementation = output / 'implementation'
    implementation.mkdir()
    for name, data in scripts.items():
        (implementation / name).write_bytes(data)
    # Both interpreters consume the same captured bytes, regardless of a later
    # edit to the user-supplied path. No shell or document text is executed.
    with tempfile.TemporaryDirectory(prefix='kanpo-compound-') as directory:
        original = Path(directory) / 'original.pdf'
        original.write_bytes(raw)
        executable = Path(directory) / 'native_pages'
        executable.write_bytes(extractor_bytes)
        executable.chmod(0o500)
        process = subprocess.run([str(executable), str(original)],
                                 capture_output=True, check=True)
    (output / 'native-pages.json').write_bytes(process.stdout)
    snapshot = json.loads(process.stdout)
    if (snapshot.get('schema') != 2 or snapshot.get('kind') != 'native_page_snapshot'
            or snapshot.get('document_sha256') != digest(raw)
            or snapshot.get('document_bytes') != len(raw)):
        raise ValueError('native snapshot does not bind the captured PDF bytes')
    document = pypdfium2.PdfDocument(raw)
    try:
        pages = snapshot['pages']
        inventory = snapshot['xobject_inventory']
        if len(document) != len(pages) or len(inventory) != len(pages):
            raise ValueError('interpreters disagree on page count')
        observations = []
        pending = []
        for pi, page in enumerate(pages):
            with closing(document[pi]) as source:
                for ci, gap in enumerate(candidates(page)):
                    before = page['glyphs'][gap['before_glyph']]
                    after = page['glyphs'][gap['after_glyph']]
                    row = {'page': pi, **gap}
                    if inventory[pi] != {'image_draw_count': 0, 'capture_gap_count': 0}:
                        row['status'] = 'opaque_image_source'
                    elif (before['end_x'] != before['x'] or before['end_y'] <= before['y']
                            or after['end_x'] != after['x'] or after['end_y'] <= after['y']):
                        row['status'] = 'unsupported_axis'
                    else:
                        mask, png, size = render(source, gap['gap'], scale)
                        name = f'page-{pi}-gap-{ci}.png'
                        (output / name).write_bytes(png)
                        row['render'] = {'file': name, 'png_sha256': digest(png),
                                         'pixel_size': size, 'box': gap['gap']}
                        if enclosed_cells(mask) is None:
                            row['status'] = 'unclassified_geometry'
                        else:
                            pending.append((row, mask))
                    observations.append(row)
        templates = {}
        for pi, page in enumerate(pages) if pending else []:
            # Rendering would otherwise mix the mapped glyph with an unrelated
            # raster source. Unknown image capture is not evidence of no image.
            if inventory[pi] != {'image_draw_count': 0, 'capture_gap_count': 0}:
                continue
            with closing(document[pi]) as source:
                for gi, glyph in enumerate(page['glyphs']):
                    if glyph['text'] in templates or not trusted_glyph(page, gi, source.get_size()):
                        continue
                    mask, png, size = render(source, glyph['bbox'], scale)
                    own = feature(mask)
                    if own is not None:
                        own['source'] = {'page': pi, 'glyph': gi, 'font': glyph['font'],
                                         'bbox': glyph['bbox'], 'png_sha256': digest(png),
                                         'pixel_size': size}
                        templates[glyph['text']] = own
        for row, mask in pending:
            row.update(recognize(mask, templates))
        report = {'schema': 1, 'kind': 'source_font_compound_observations',
                  'canonical_integration': False, 'coverage_verified': False,
                  'source': {'document_sha256': digest(raw), 'document_bytes': len(raw)},
                  'implementation': {'script_sha256': digest(scripts['read_painted_compounds.py']),
                                     'gap_script_sha256': digest(scripts['audit_paint_gaps.py']),
                                     'extractor_sha256': digest(extractor_bytes),
                                     'libraries': {n: importlib.metadata.version(n)
                                                   for n in ('numpy', 'Pillow', 'pypdfium2')}},
                  'rendering': {'scale': scale, 'ink_luminance_below': 128,
                                'normalization': [64, 64], 'rotation': 'native_down_only'},
                  'templates': {label: v['source'] for label, v in templates.items()},
                  'observations': observations}
        if any(Path(__file__).with_name(name).read_bytes() != data for name, data in scripts.items()):
            raise ValueError('recognition source code changed during execution')
        (output / 'observations.json').write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n')
        return report
    finally:
        document.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('pdf', type=Path)
    parser.add_argument('--extractor', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    report = run(args.pdf, args.extractor, args.output)
    print(json.dumps([{'page': r['page'], 'status': r['status'], 'text': r.get('text')}
                      for r in report['observations']], ensure_ascii=False))


if __name__ == '__main__':
    main()

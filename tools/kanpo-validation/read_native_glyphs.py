#!/usr/bin/env python3
"""Explicit source-bound recognition of isolated unmapped native glyphs.

The parser supplies candidates; no law/word/CID lookup or manuscript is read.
The current optical scope is one kana inside two physically opposed arcs.
Apple Vision is optional, local to macOS, and never a Unicode certificate.
"""
import argparse
import hashlib
import importlib.metadata
import json
import math
from pathlib import Path
import subprocess

import numpy as np
from PIL import Image
import pypdfium2


def sha(data):
    return hashlib.sha256(data).hexdigest()


def components(mask):
    remaining = set(zip(*np.where(mask)))
    found = []
    while remaining:
        first = min(remaining)
        remaining.remove(first)
        queue, pixels = [first], []
        while queue:
            y, x = queue.pop()
            pixels.append((int(y), int(x)))
            for dy in (-1, 0, 1):
                for dx in (-1, 0, 1):
                    point = y + dy, x + dx
                    if point in remaining:
                        remaining.remove(point)
                        queue.append(point)
        ys, xs = zip(*pixels)
        found.append({'bounds': [min(xs), min(ys), max(xs) + 1, max(ys) + 1], 'pixels': pixels})
    return found


def opposed_arcs(mask):
    parts = components(mask)
    if len(parts) < 3:
        return None
    left = min(parts, key=lambda p: p['bounds'][0])
    right = max(parts, key=lambda p: p['bounds'][2])
    if left is right or left['bounds'][2] >= right['bounds'][0]:
        return None
    inner = [p for p in parts if p is not left and p is not right]
    for p in inner:
        a, b, c, d = p['bounds']
        if not (left['bounds'][2] <= a < c <= right['bounds'][0]
                and all(arc['bounds'][1] < (b + d) / 2 < arc['bounds'][3] for arc in (left, right))):
            return None
    for arc, direction in ((left, -1), (right, 1)):
        rows = {}
        for y, x in arc['pixels']:
            rows.setdefault(y, []).append(x)
        top, bottom = arc['bounds'][1], arc['bounds'][3]
        if len(rows) != bottom - top:
            return None
        mean = lambda y: sum(rows[y]) / len(rows[y])
        mid = (top + bottom - 1) // 2
        if not all(direction * mean(mid) > direction * mean(y) for y in (top, bottom - 1)):
            return None
    return {'arcs': [left['bounds'], right['bounds']], 'inner': [
        min(p['bounds'][0] for p in inner), min(p['bounds'][1] for p in inner),
        max(p['bounds'][2] for p in inner), max(p['bounds'][3] for p in inner)]}


def decode_candidate(layout, observations):
    if layout is None:
        return None
    # Optical parentheses may be omitted by OCR. Their existence comes from
    # the original two arcs, not from completing a guessed manuscript string.
    if len(observations) != 1 or not observations[0].get('candidates'):
        return None
    text = observations[0]['candidates'][0]['text'].strip('()（）')
    if len(text) != 1 or not ('ぁ' <= text <= 'ゖ' or 'ァ' <= text <= 'ヺ'):
        return None
    return '（' + text + '）'


def upright(image, candidate):
    x, y = tuple(candidate['font_x']), tuple(-v for v in candidate['font_y'])
    transforms = {
        ((1, 0), (0, 1)): None,
        ((-1, 0), (0, 1)): Image.Transpose.FLIP_LEFT_RIGHT,
        ((1, 0), (0, -1)): Image.Transpose.FLIP_TOP_BOTTOM,
        ((-1, 0), (0, -1)): Image.Transpose.ROTATE_180,
        ((0, 1), (-1, 0)): Image.Transpose.ROTATE_90,
        ((0, -1), (1, 0)): Image.Transpose.ROTATE_270,
        ((0, 1), (1, 0)): Image.Transpose.TRANSPOSE,
        ((0, -1), (-1, 0)): Image.Transpose.TRANSVERSE,
    }
    if (x, y) not in transforms:
        raise ValueError('original font axes are not cardinal and orthogonal')
    transform = transforms[x, y]
    return image.copy() if transform is None else image.transpose(transform)


def run(pdf, native, analysis, output, engine='vision'):
    raw = pdf.read_bytes()
    snapshot_bytes = native.read_bytes()
    snapshot = json.loads(snapshot_bytes)
    model = json.loads(analysis.read_bytes())
    if (snapshot.get('schema') != 2 or snapshot.get('document_sha256') != sha(raw)
            or snapshot.get('document_bytes') != len(raw)
            or len(snapshot['pages']) != len(model['native']['pages'])):
        raise ValueError('native snapshot does not belong to this original PDF')
    output.mkdir(parents=True, exist_ok=False)
    script = Path(__file__).read_bytes()
    (output / 'read_native_glyphs.py').write_bytes(script)
    swift_path = Path(__file__).with_name('recognize_glyphs.swift')
    swift = swift_path.read_bytes()
    captured_swift = output / 'recognize_glyphs.swift'
    captured_swift.write_bytes(swift)
    captured_swift.chmod(0o400)
    candidates = model['native_glyphs']['candidates']
    entries, images = [], []
    with pypdfium2.PdfDocument(raw) as document:
        page_index, bitmap = None, None
        for index, candidate in enumerate(candidates):
            pi, gi = candidate['page'], candidate['glyph']
            page = snapshot['pages'][pi]
            glyph = page['glyphs'][gi]
            if (candidate['source_document_sha256'] != sha(raw)
                    or candidate['bounds'] != glyph['bbox'] or candidate['font'] != glyph['font']
                    or candidate['cid'] != glyph['cid'] or glyph['invisible']):
                raise ValueError('unresolved candidate differs from original native glyph')
            scale = 12
            if pi != page_index:
                if tuple(document[pi].get_size()) != (page['width'], page['height']):
                    raise ValueError('renderer and native page dimensions differ')
                bitmap = document[pi].render(scale=scale).to_pil().convert('RGB')
                page_index = pi
            a, b, c, d = candidate['bounds']
            box = [math.floor(a * scale), math.floor(b * scale), math.ceil(c * scale), math.ceil(d * scale)]
            if not (0 <= box[0] < box[2] <= bitmap.width and 0 <= box[1] < box[3] <= bitmap.height):
                raise ValueError('original glyph crop escapes its page')
            original_image = bitmap.crop(box)
            original_path = output / f'glyph-{index}-source.png'
            original_image.save(original_path)
            image = upright(original_image, candidate)
            path = output / f'glyph-{index}.png'
            image.save(path)
            layout = opposed_arcs(np.asarray(image.convert('L')) < 128)
            entries.append({'candidate': candidate, 'layout': layout, 'PNG': path.name,
                            'PNG_sha256': sha(path.read_bytes()), 'pixel_box': box, 'scale': scale})
            entries[-1].update({'source_PNG': original_path.name, 'source_PNG_sha256': sha(original_path.read_bytes())})
            images.append(path.resolve())
    engine_result = {'outputs': []}
    if images:
        if engine != 'vision':
            raise ValueError('unsupported explicit native glyph recognition engine')
        result = subprocess.run(['swift', str(captured_swift.resolve()), *map(str, images)],
                                capture_output=True, check=True)
        engine_result = json.loads(result.stdout)
        if len(engine_result['outputs']) != len(entries):
            raise ValueError('optical result inventory differs')
    (output / 'engine.json').write_text(json.dumps(engine_result, ensure_ascii=False, indent=2) + '\n')
    observations = []
    for entry, result in zip(entries, engine_result['outputs']):
        if Path(result['path']).resolve() != (output / entry['PNG']).resolve():
            raise ValueError('optical result belongs to a different source crop')
        text = decode_candidate(entry['layout'], result['observations'])
        entry['engine_observation'] = result
        entry['text'] = text
        entry['status'] = 'candidate_unverified' if text is not None else 'unrecognized'
        if text is not None:
            observations.append({'candidate': entry['candidate'], 'text': text,
                                 'status': 'candidate_unverified', 'render': entry})
    if Path(__file__).read_bytes() != script or swift_path.read_bytes() != swift:
        raise ValueError('recognition implementation changed during execution')
    report = {'schema': 1, 'kind': 'native_glyph_observations',
              'source': {'document_sha256': sha(raw), 'document_bytes': len(raw)},
              'native_snapshot_sha256': sha(snapshot_bytes), 'script_sha256': sha(script),
              'swift_sha256': sha(swift), 'engine': engine_result.get('os'),
              'libraries': {name: importlib.metadata.version(name) for name in ('pypdfium2', 'Pillow', 'numpy')},
              'entries': entries, 'observations': observations, 'coverage_verified': False}
    (output / 'observations.json').write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n')
    return report


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('pdf', type=Path)
    parser.add_argument('--native', type=Path, required=True)
    parser.add_argument('--analysis', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    report = run(args.pdf, args.native, args.analysis, args.output)
    print(json.dumps({'candidates': len(report['entries']), 'readings': len(report['observations'])}))

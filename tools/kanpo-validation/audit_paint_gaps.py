#!/usr/bin/env python3
"""Find source paint between native glyph advances, without recognizing its text.

Input is the native_pages example's original Page JSON. Results are conservative
audit candidates, not OCR output, text insertions, or a coverage certification.
Neither expected text nor a law/page-specific rule participates in detection.
"""
import argparse
import bisect
import json
import math
from pathlib import Path


def positive(b):
    return len(b) == 4 and all(math.isfinite(v) for v in b) and b[0] < b[2] and b[1] < b[3]


def overlaps(a, b):
    return a[0] < b[2] and b[0] < a[2] and a[1] < b[3] and b[1] < a[3]


def contains(a, b):
    return a[0] <= b[0] and b[2] <= a[2] and a[1] <= b[1] and b[3] <= a[3]


def axis(g):
    d = [g['end_x'] - g['x'], g['end_y'] - g['y']]
    if not all(math.isfinite(v) for v in d) or g['invisible'] or not positive(g['bbox']):
        return None
    if d[0] == 0 and d[1] != 0:
        return 1, 1 if d[1] > 0 else -1
    if d[1] == 0 and d[0] != 0:
        return 0, 1 if d[0] > 0 else -1
    return None


def paint_bounds(path):
    if not (path['fill'] or path['stroke']):
        return None
    # Native SubPath keeps curve endpoints, not Bezier controls. Their hull is
    # not an ink bound. Likewise a general stroked join can exceed half-width.
    if any(s.get('curved', False) or path['stroke'] and len(s['points']) > 2
           and not s.get('rect', False) for s in path['subpaths']):
        return None
    points = [p for s in path['subpaths'] for p in s['points']]
    if not points or not all(len(p) == 2 and all(math.isfinite(v) for v in p) for p in points):
        return None
    width = path['line_width'] if path['stroke'] else 0
    if not math.isfinite(width) or width < 0:
        return None
    # Straight fill vertices and axis-aligned rectangle/single-segment strokes.
    # Clipping and final visibility are not certified here.
    b = [min(p[0] for p in points) - width / 2,
         min(p[1] for p in points) - width / 2,
         max(p[0] for p in points) + width / 2,
         max(p[1] for p in points) + width / 2]
    return b if positive(b) else None


def candidates(page):
    glyphs = page['glyphs']
    groups = {}
    for i, g in enumerate(glyphs):
        a = axis(g)
        if a is not None and math.isfinite(g['size']) and g['size'] > 0:
            groups.setdefault((g['font'], g['size'], a), []).append(i)
    paint = [(i, b) for i, p in enumerate(page['paths']) if (b := paint_bounds(p)) is not None]
    out = []
    for (_, _, (primary, sign)), ids in groups.items():
        cross = 1 - primary
        key = ('x', 'y')[primary]
        ids.sort(key=lambda i: glyphs[i][key] * sign)
        starts = [glyphs[i][key] * sign for i in ids]
        for before in ids:
            a = glyphs[before]
            end = a['end_' + key] * sign
            after = None
            for pos in range(bisect.bisect_right(starts, end), len(ids)):
                j = ids[pos]
                b = glyphs[j]
                lo = max(a['bbox'][cross], b['bbox'][cross])
                hi = min(a['bbox'][cross + 2], b['bbox'][cross + 2])
                if lo < hi:
                    if after is not None and starts[pos] == after[0]:
                        after = None
                        break
                    if after is not None:
                        break
                    after = starts[pos], j, lo, hi
            if after is None:
                continue
            start, successor, lo, hi = after
            gap = [0.] * 4
            gap[cross], gap[cross + 2] = lo, hi
            gap[primary], gap[primary + 2] = sorted([end * sign, start * sign])
            if not positive(gap):
                continue
            paths = [(i, b) for i, b in paint if contains(gap, b)]
            if not paths:
                continue
            if any(not g['invisible'] and positive(g['bbox']) and overlaps(gap, g['bbox']) for g in glyphs):
                continue
            bounds = [min(b[k] for _, b in paths) for k in (0, 1)] + [max(b[k] for _, b in paths) for k in (2, 3)]
            out.append({'before_glyph': before, 'after_glyph': successor, 'gap': gap,
                        'paint_bounds': bounds, 'path_ids': [i for i, _ in paths],
                        'text': None, 'visibility_verified': False})
    return sorted(out, key=lambda r: (r['before_glyph'], r['after_glyph']))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('native_pages', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    snapshot = json.loads(args.native_pages.read_text())
    pages = snapshot if isinstance(snapshot, list) else snapshot['pages']
    report = {'schema': 1, 'kind': 'unclassified_native_paint_gaps',
              'source': None if isinstance(snapshot, list) else {
                  'document_sha256': snapshot['document_sha256'],
                  'document_bytes': snapshot['document_bytes']},
              'coverage_verified': False,
              'pages': [{'page': i, 'candidates': candidates(p),
                         'unbounded_paint_paths': [j for j, path in enumerate(p['paths'])
                                                   if (path['fill'] or path['stroke'])
                                                   and paint_bounds(path) is None]}
                        for i, p in enumerate(pages)]}
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n')
    print([len(p['candidates']) for p in report['pages']])


if __name__ == '__main__':
    main()

"""Observe axis-aligned ruling support and bounded faces in original RGBA pixels.

This is an image observation, not a table/text completeness certificate. In
particular, broken or dashed boundaries are not silently closed. All rulings
retain their exact supporting maximal black runs, including their thickness.
"""
import argparse
import hashlib
import json
import math
import re
from io import BytesIO
from pathlib import Path

from PIL import Image, ImageChops


def _black_mask(image):
    r, g, b, a = image.convert('RGBA').split()
    mask = r.point(lambda value: 255 if value == 0 else 0)
    for channel in (g, b):
        mask = ImageChops.darker(mask, channel.point(lambda value: 255 if value == 0 else 0))
    return ImageChops.darker(mask, a.point(lambda value: 255 if value == 255 else 0))


def _tracks(mask, minimum, aspect, axis):
    if axis == 1:
        mask = mask.transpose(Image.Transpose.TRANSPOSE)
    width, height = mask.size
    pixels = mask.tobytes()
    segments, parents = [], []
    previous = []

    def root(index):
        while parents[index] != index:
            parents[index] = parents[parents[index]]
            index = parents[index]
        return index

    for cross in range(height):
        current = []
        cursor = 0
        for match in re.finditer(b'\xff+', pixels[cross * width:(cross + 1) * width]):
            start, end = match.span()
            if end - start < minimum:
                continue
            index = len(segments)
            segments.append([cross, start, end])
            parents.append(index)
            # Only adjacent scanlines with shared actual black pixels join.
            while cursor < len(previous) and previous[cursor][1] <= start:
                cursor += 1
            other = cursor
            while other < len(previous) and previous[other][0] < end:
                parents[root(previous[other][2])] = root(index)
                other += 1
            current.append((start, end, index))
        previous = current
    groups = {}
    for index, segment in enumerate(segments):
        groups.setdefault(root(index), []).append(segment)
    accepted, rejected = [], []
    for support in groups.values():
        band = [min(s[0] for s in support), max(s[0] for s in support) + 1]
        span = [min(s[1] for s in support), max(s[2] for s in support)]
        entry = {'axis': axis, 'band': band, 'span': span, 'support': support}
        if span[1] - span[0] >= aspect * (band[1] - band[0]):
            accepted.append(entry)
        else:
            rejected.append({**entry, 'reason': 'parallel_support_is_not_thin'})
    accepted.sort(key=lambda rule: (rule['band'], rule['span']))
    return accepted, rejected


def _intersection(horizontal, vertical):
    for y, x0, x1 in horizontal['support']:
        for x, y0, y1 in vertical['support']:
            if x0 <= x < x1 and y0 <= y < y1:
                return [x, y]
    return None


def _covered(rule, start, end):
    cursor = start
    for a, b in sorted((s[1], s[2]) for s in rule['support']):
        if a > cursor:
            return False
        cursor = max(cursor, b)
        if cursor >= end:
            return True
    return False


def _faces(horizontal, vertical, *, covered=_covered):
    nodes, edges, unresolved = [], [], []
    along = {0: {}, 1: {}}
    for hi, h in enumerate(horizontal):
        for vi, v in enumerate(vertical):
            witness = _intersection(h, v)
            if witness is None:
                continue
            index = len(nodes)
            nodes.append({'id': index, 'horizontal': hi, 'vertical': vi,
                          'point': [sum(v['band']) / 2, sum(h['band']) / 2],
                          'witness_pixel': witness})
            along[0].setdefault(hi, []).append(index)
            along[1].setdefault(vi, []).append(index)
    for axis, rules in ((0, horizontal), (1, vertical)):
        for ri, ids in along[axis].items():
            ids.sort(key=lambda i: nodes[i]['point'][axis])
            for a, b in zip(ids, ids[1:]):
                start, end = nodes[a]['point'][axis], nodes[b]['point'][axis]
                if start == end:
                    unresolved.append({'reason': 'coincident_graph_vertices', 'nodes': [a, b]})
                elif covered(rules[ri], start, end):
                    edges.append({'id': len(edges), 'axis': axis, 'rule': ri, 'nodes': [a, b]})
    adjacency = {n['id']: [] for n in nodes}
    by_pair = {}
    for edge in edges:
        a, b = edge['nodes']
        adjacency[a].append(b)
        adjacency[b].append(a)
        by_pair[(a, b)] = by_pair[(b, a)] = edge['id']

    def direction(a, b):
        x, y = nodes[a]['point']
        xx, yy = nodes[b]['point']
        if y == yy and x < xx:
            return 0
        if x == xx and y < yy:
            return 1
        if y == yy and xx < x:
            return 2
        if x == xx and yy < y:
            return 3
        raise ValueError('Non-cardinal source ruling graph')

    for a, neighbors in adjacency.items():
        neighbors.sort(key=lambda b: direction(a, b))
        directions = [direction(a, b) for b in neighbors]
        if len(set(directions)) != len(directions):
            unresolved.append({'reason': 'ambiguous_graph_direction', 'node': a})
    if unresolved:
        return nodes, edges, [], [], unresolved
    visited, faces, cells = set(), [], []
    for edge in edges:
        for a, b in (edge['nodes'], edge['nodes'][::-1]):
            if (a, b) in visited:
                continue
            start, walk, walk_edges = (a, b), [], []
            while (a, b) not in visited:
                visited.add((a, b))
                walk.append(a)
                walk_edges.append(by_pair[(a, b)])
                neighbors = adjacency[b]
                next_node = neighbors[(neighbors.index(a) - 1) % len(neighbors)]
                a, b = b, next_node
            if (a, b) != start:
                raise ValueError('Inconsistent source ruling graph traversal')
            points = [nodes[n]['point'] for n in walk]
            area2 = sum(p[0] * q[1] - q[0] * p[1] for p, q in zip(points, points[1:] + points[:1]))
            if area2 <= 0:
                continue
            face = {'id': len(faces), 'nodes': walk, 'edges': walk_edges, 'area_pixels': area2 / 2}
            faces.append(face)
            corners = [p for prev, p, nxt in zip(points[-1:] + points[:-1], points, points[1:] + points[:1])
                       if not (prev[0] == p[0] == nxt[0] or prev[1] == p[1] == nxt[1])]
            if len(set(walk)) != len(walk) or len(corners) != 4:
                unresolved.append({'reason': 'bounded_face_is_not_a_simple_rectangle', 'face': face['id']})
                continue
            hs = sorted({edges[e]['rule'] for e in walk_edges if edges[e]['axis'] == 0},
                        key=lambda i: horizontal[i]['band'])
            vs = sorted({edges[e]['rule'] for e in walk_edges if edges[e]['axis'] == 1},
                        key=lambda i: vertical[i]['band'])
            if len(hs) != 2 or len(vs) != 2:
                unresolved.append({'reason': 'rectangle_needs_four_unambiguous_ruling_tracks', 'face': face['id']})
                continue
            top, bottom = (horizontal[i] for i in hs)
            left, right = (vertical[i] for i in vs)
            roi = [left['band'][1], top['band'][1], right['band'][0], bottom['band'][0]]
            if roi[0] >= roi[2] or roi[1] >= roi[3]:
                unresolved.append({'reason': 'rectangle_has_no_interior_pixels', 'face': face['id']})
                continue
            cells.append({'id': len(cells), 'face': face['id'], 'interior_pixels': roi,
                          'horizontal_rules': hs, 'vertical_rules': vs})
    return nodes, edges, faces, cells, unresolved


def observe(image, *, minimum_run=None, minimum_aspect=10):
    """Return candidate geometry before recognition, without inspecting text."""
    width, height = image.size
    if width <= 0 or height <= 0:
        raise ValueError('Expected a nonempty source raster')
    if minimum_run is None:
        minimum_run = max(2, (min(width, height) + 19) // 20)
    if type(minimum_run) is not int or minimum_run < 2:
        raise ValueError('Expected an integer minimum run of at least two pixels')
    if type(minimum_aspect) is not int or minimum_aspect < 2:
        raise ValueError('Expected an integer minimum ruling aspect of at least two')
    mask = _black_mask(image)
    horizontal, rejected_h = _tracks(mask, minimum_run, minimum_aspect, 0)
    vertical, rejected_v = _tracks(mask, minimum_run, minimum_aspect, 1)
    nodes, edges, faces, cells, unresolved = _faces(horizontal, vertical)
    return {'schema': 1, 'kind': 'unclassified_raster_ruling_faces',
            'canvas': {'pixel_width': width, 'pixel_height': height, 'origin': 'top_left_pixel_edges'},
            'settings': {'opaque_black_only': True, 'minimum_run': minimum_run,
                         'minimum_run_default_rule': 'max(2,ceil(min(width,height)/20))',
                         'minimum_aspect': minimum_aspect, 'gap_closing': False},
            'horizontal': horizontal, 'vertical': vertical,
            'rejected_parallel_components': rejected_h + rejected_v,
            'intersections': nodes, 'edges': edges, 'bounded_faces': faces,
            'rectangular_cells': cells, 'unresolved': unresolved,
            'coverage_verified': False, 'article_assignment': None, 'visibility': 'unverified'}


def write_observation(source, output, *, minimum_run=None, minimum_aspect=10):
    source, output = Path(source), Path(output)
    raw = source.read_bytes()
    image = Image.open(BytesIO(raw)).convert('RGBA')
    result = observe(image, minimum_run=minimum_run, minimum_aspect=minimum_aspect)
    output.mkdir()
    (output / 'source.png').write_bytes(raw)
    (output / 'source.png').chmod(0o400)
    sha = lambda data: hashlib.sha256(data).hexdigest()
    result['source'] = {'PNG_sha256': sha(raw), 'RGBA_sha256': sha(image.tobytes()),
                        'source_filename': 'source.png', 'pillow': Image.__version__,
                        'producer_sha256': sha(Path(__file__).read_bytes())}
    for cell in result['rectangular_cells']:
        x0, y0, x1, y1 = cell['interior_pixels']
        crop = image.crop((x0, y0, x1, y1))
        name = f"cell-{cell['id']:04}.png"
        crop.save(output / name)
        cell['crop'] = {'PNG': name, 'PNG_sha256': sha((output / name).read_bytes()),
                        'RGBA_sha256': sha(crop.tobytes()),
                        'recognition_to_source': [1, 0, 0, 1, x0, y0]}
    (output / 'observation.json').write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n')
    return result


def crop_frame(frame, box):
    """Compose an original pixel crop with the declared image placement."""
    width, height = frame['pixel_width'], frame['pixel_height']
    if (type(width) is not int or type(height) is not int or width <= 0 or height <= 0
            or len(box) != 4 or any(type(value) is not int for value in box)):
        raise ValueError('Invalid source crop canvas')
    x0, y0, x1, y1 = box
    if not (0 <= x0 < x1 <= width and 0 <= y0 < y1 <= height):
        raise ValueError('Source crop is outside the parent image')
    matrix = frame['recognition_to_image']
    if (len(matrix) != 6 or any(type(v) not in (int, float) or not math.isfinite(v) for v in matrix)):
        raise ValueError('Invalid source crop transform')
    a, b, c, d, e, f = matrix
    result = {**frame, 'pixel_width': x1 - x0, 'pixel_height': y1 - y0,
              'recognition_to_image': [a, b, c, d, a * x0 + c * y0 + e, b * x0 + d * y0 + f]}
    if not all(math.isfinite(v) for v in result['recognition_to_image']):
        raise ValueError('Unrepresentable crop transform')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source')
    parser.add_argument('--output', required=True)
    parser.add_argument('--minimum-run', type=int)
    parser.add_argument('--minimum-aspect', type=int, default=10)
    args = parser.parse_args()
    result = write_observation(args.source, args.output, minimum_run=args.minimum_run,
                               minimum_aspect=args.minimum_aspect)
    print(json.dumps({'rules': len(result['horizontal']) + len(result['vertical']),
                      'rectangular_cells': len(result['rectangular_cells']),
                      'unresolved': result['unresolved'], 'coverage_verified': False}))


if __name__ == '__main__':
    main()

"""Source-only periodic ruling candidates and declared OCR-copy cleanup.

Candidates retain actual black runs and perpendicular ruling witnesses. They
are not text/checkbox classifications or proof of semantic table boundaries.
Only a separate recognition copy is changed; original pixels remain available.
"""
import hashlib
import re

from PIL import Image, ImageDraw
from raster_grid import _black_mask
from source_gutters import ink_mask


def components(mask):
    """Exact nonzero 8-connected components, including all supporting row runs."""
    if mask.mode != 'L':
        raise ValueError('Expected an L-mode component mask')
    width, height = mask.size
    raw = mask.tobytes()
    parents, ranks, spans = [], [], []
    previous = []

    def root(i):
        while parents[i] != i:
            parents[i] = parents[parents[i]]
            i = parents[i]
        return i

    def join(a, b):
        a, b = root(a), root(b)
        if a == b:
            return
        if ranks[a] < ranks[b]:
            a, b = b, a
        parents[b] = a
        if ranks[a] == ranks[b]:
            ranks[a] += 1

    for y in range(height):
        current, cursor = [], 0
        for match in re.finditer(b'[^\x00]+', raw[y*width:(y+1)*width]):
            x0, x1 = match.span()
            i = len(spans)
            spans.append([y, x0, x1])
            parents.append(i)
            ranks.append(0)
            while cursor < len(previous) and previous[cursor][1] < x0:
                cursor += 1
            j = cursor
            while j < len(previous) and previous[j][0] <= x1:
                join(i, previous[j][2])
                j += 1
            current.append((x0, x1, i))
        previous = current
    grouped = {}
    for i, span in enumerate(spans):
        grouped.setdefault(root(i), []).append(span)
    result = []
    for support in grouped.values():
        bounds = [min(s[1] for s in support), min(s[0] for s in support),
                  max(s[2] for s in support), max(s[0] for s in support)+1]
        result.append({'id': len(result), 'bounds': bounds, 'area': sum(s[2]-s[1] for s in support),
                       'support': support})
    return result


def observe_dashes(image, rulings):
    """Observe repeated runs between consecutive actual perpendicular rulings."""
    mask = _black_mask(image)
    candidates = []
    for axis, parallel, perpendicular in ((0, 'horizontal', 'vertical'), (1, 'vertical', 'horizontal')):
        widths = [r['band'][1]-r['band'][0] for r in rulings[parallel]]
        if not widths:
            continue
        thickness, quantum = max(widths), min(widths)
        minimum = thickness + 1
        scan = mask if axis == 0 else mask.transpose(Image.Transpose.TRANSPOSE)
        width, height = scan.size
        raw = scan.tobytes()
        for cross in range(height):
            candidate_start = len(candidates)
            runs = [m.span() for m in re.finditer(b'\xff+', raw[cross*width:(cross+1)*width])
                    if m.end()-m.start() >= minimum]
            if len(runs) < 6:
                continue
            junctions = []
            for index, rule in enumerate(rulings[perpendicular]):
                witnesses = [(row, a, b) for row, a, b in rule['support'] if a <= cross < b]
                if not witnesses:
                    continue
                row = witnesses[0][0]
                witness = [row, cross] if axis == 0 else [cross, row]
                if junctions and rule['band'][0] <= junctions[-1]['band'][1]:
                    junctions[-1]['band'][1] = max(junctions[-1]['band'][1], rule['band'][1])
                    junctions[-1]['witnesses'].append({'rule': index, 'pixel': witness})
                else:
                    junctions.append({'band': rule['band'][:], 'witnesses': [{'rule': index, 'pixel': witness}]})
            intervals, raw_intervals = [], []
            for before, after in zip(junctions, junctions[1:]):
                left, right = before['band'][1], after['band'][0]
                support = [(max(a, left), min(b, right)) for a, b in runs if a < right and left < b]
                support = [(a, b) for a, b in support if b-a >= minimum]
                raw_intervals.append({'span': [left, right], 'support': [list(s) for s in support],
                                      'junctions': [before, after]})
                if len(support) < 4:
                    intervals.append(None)
                    continue
                # The first and last runs can be truncated by their junction.
                # Standalone candidates require four further complete runs;
                # shorter cells contribute evidence only through a chain.
                middle = support[1:-1]
                lengths = [b-a for a, b in middle]
                pitches = [b[0]-a[0] for a, b in zip(middle, middle[1:])]
                gaps = [b[0]-a[1] for a, b in zip(middle, middle[1:])]
                if (max(lengths)-min(lengths) > thickness or max(pitches)-min(pitches) > thickness
                        or min(gaps) <= quantum or middle[0][0]-left > 2*max(pitches)
                        or right-middle[-1][1] > 2*max(pitches)):
                    intervals.append(None)
                    continue
                interval = {'span': [left, right], 'support': [list(s) for s in support],
                            'pattern_support': [list(s) for s in middle],
                            'length_range': [min(lengths), max(lengths)],
                            'pitch_range': [min(pitches), max(pitches)],
                            'junctions': [before, after]}
                intervals.append(interval)
                if len(support) < 6:
                    continue
                candidates.append({'id': len(candidates), 'axis': axis, 'cross': cross,
                                   'pattern_kind': 'single_interval',
                                   'span': [left, right], 'support': [list(s) for s in support],
                                   'pattern_support': [list(s) for s in middle],
                                   'length_range': [min(lengths), max(lengths)],
                                   'pitch_range': [min(pitches), max(pitches)],
                                   'junctions': [before, after], 'stroke_thickness': thickness,
                                   'quantum': quantum})
            # A dashed cell border can restart its phase at each junction.
            # Observe consecutive cells together without joining across an
            # unexplained cell or weakening the complete-run requirement.
            chains = []
            chain = []
            for interval in intervals + [None]:
                if interval is not None and chain:
                    lengths = [v for entry in chain+[interval] for v in entry['length_range']]
                    pitches = [v for entry in chain+[interval] for v in entry['pitch_range']]
                    if max(lengths)-min(lengths) > thickness or max(pitches)-min(pitches) > thickness:
                        chains.append(chain[:])
                        while chain:
                            chain = chain[1:]
                            lengths = [v for entry in chain+[interval] for v in entry['length_range']]
                            pitches = [v for entry in chain+[interval] for v in entry['pitch_range']]
                            if max(lengths)-min(lengths) <= thickness and max(pitches)-min(pitches) <= thickness:
                                break
                if interval is None:
                    if chain:
                        chains.append(chain)
                        chain = []
                else:
                    chain.append(interval)
            for chain in chains:
                if len(chain) < 3 or not any(len(entry['support']) < 6 for entry in chain):
                    continue
                support = [run for entry in chain for run in entry['support']]
                pattern_support = [run for entry in chain for run in entry['pattern_support']]
                if len(pattern_support) < 6:
                    continue
                lengths = [v for entry in chain for v in entry['length_range']]
                pitches = [v for entry in chain for v in entry['pitch_range']]
                candidates.append({'id': len(candidates), 'axis': axis, 'cross': cross,
                                   'pattern_kind': 'adjacent_cells', 'cells': chain,
                                   'span': [chain[0]['span'][0], chain[-1]['span'][1]],
                                   'support': support, 'pattern_support': pattern_support,
                                   'length_range': [min(lengths), max(lengths)],
                                   'pitch_range': [min(pitches), max(pitches)],
                                   'pitch_basis': 'within_each_cell_phase_can_restart',
                                   'junctions': [chain[0]['junctions'][0]]+[e['junctions'][1] for e in chain],
                                   'stroke_thickness': thickness, 'quantum': quantum})
            # Preserve exceptional runs, rather than rejecting a whole long
            # border because one run touches noise. The same observed style
            # must already meet BOTH of this cell's source junctions.
            anchors = candidates[candidate_start:]
            for interval in raw_intervals:
                before, after = interval['junctions']
                preceding = [c for c in anchors if c['junctions'][-1]['band'] == before['band']]
                following = [c for c in anchors if c['junctions'][0]['band'] == after['band']]
                if not preceding or not following:
                    continue
                for first in preceding:
                    for last in following:
                        if (max(first['length_range'][0], last['length_range'][0]) > min(first['length_range'][1], last['length_range'][1])
                                or max(first['pitch_range'][0], last['pitch_range'][0]) > min(first['pitch_range'][1], last['pitch_range'][1])):
                            continue
                        lengths = [min(first['length_range'][0], last['length_range'][0]),
                                   max(first['length_range'][1], last['length_range'][1])]
                        pitches = [min(first['pitch_range'][0], last['pitch_range'][0]),
                                   max(first['pitch_range'][1], last['pitch_range'][1])]
                        groups, group = [], []
                        for run in interval['support'] + [None]:
                            fits = run is not None and lengths[0] <= run[1]-run[0] <= lengths[1]
                            if fits and group:
                                step, gap = run[0]-group[-1][0], run[0]-group[-1][1]
                                fits = pitches[0] <= step <= pitches[1] and gap > quantum
                                if not fits:
                                    groups.append(group)
                                    group = []
                                    fits = True  # This run can start a new local group.
                            if not fits:
                                if group:
                                    groups.append(group)
                                    group = []
                            else:
                                group.append(run)
                        groups = [g for g in groups if len(g) >= 4]
                        support = [run for g in groups for run in g]
                        if len(support) < 4:
                            continue
                        selected = {tuple(s) for s in support}
                        observed_pitches = [b[0]-a[0] for g in groups for a, b in zip(g, g[1:])]
                        candidates.append({'id': len(candidates), 'axis': axis, 'cross': cross,
                                           'pattern_kind': 'flanked_interval',
                                           'span': interval['span'], 'support': support, 'pattern_support': support,
                                           'length_range': [min(b-a for a, b in support), max(b-a for a, b in support)],
                                           'pitch_range': [min(observed_pitches), max(observed_pitches)],
                                           'pitch_basis': 'local_groups_with_observed_flanking_style',
                                           'local_groups': groups, 'flanking_candidates': [first['id'], last['id']],
                                           'unmodeled_support': [s for s in interval['support'] if tuple(s) not in selected],
                                           'junctions': interval['junctions'], 'stroke_thickness': thickness,
                                           'quantum': quantum})
    return {'schema': 1, 'kind': 'unclassified_periodic_ruling_observations',
            'source_RGBA_sha256': hashlib.sha256(image.convert('RGBA').tobytes()).hexdigest(),
            'candidates': candidates, 'coverage_verified': False}


def recognition_mask(image, observations):
    """Remove only isolated pixels or bounded, short, separable components.

    A glyph connected to a candidate stripe, or extending beyond the observed
    dash length, remains intact. No character spelling or OCR output is used.
    """
    mask = Image.new('L', image.size)
    draw = ImageDraw.Draw(mask)
    selected = []
    for component in components(ink_mask(image)):
        box, area = component['bounds'], component['area']
        candidates = []
        if area != 1:
            for candidate in observations['candidates']:
                axis = candidate['axis']
                cross, radius = candidate['cross'], candidate['stroke_thickness']
                low, high = candidate['span']
                primary = [box[0], box[2]] if axis == 0 else [box[1], box[3]]
                normal = [box[1], box[3]] if axis == 0 else [box[0], box[2]]
                if (normal[0] < cross-radius or normal[1] > cross+radius+1
                        or primary[0] < low or primary[1] > high
                        or primary[1]-primary[0] > candidate['length_range'][1]+radius):
                    continue
                if not any(max(primary[0], a) < min(primary[1], b) for a, b in candidate['support']):
                    continue
                candidates.append(candidate['id'])
        if area != 1 and not candidates:
            continue
        for y, x0, x1 in component['support']:
            draw.line((x0, y, x1-1, y), fill=255)
        selected.append({**component, 'reason': 'isolated_one_pixel' if area == 1 else 'bounded_periodic_component',
                         'candidate_ids': candidates})
    return mask, {'schema': 1, 'kind': 'recognition_copy_component_erasure',
                  'recognition_input_RGBA_sha256': hashlib.sha256(image.convert('RGBA').tobytes()).hexdigest(),
                  'selected_components': selected, 'coverage_verified': False}

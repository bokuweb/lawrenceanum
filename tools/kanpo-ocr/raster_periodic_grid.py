"""Candidate faces with explicitly inferred periodic boundaries.

Actual source pixels still witness every vertex. Only traversal along an
observed periodic span bridges white gaps. These faces are hypotheses, not
closed black components or certified semantic table cells.
"""
import copy
import hashlib
from raster_grid import _covered, _faces


def periodic_faces(rulings, patterns, *, image=None):
    candidates = patterns['candidates']
    if image is not None:
        from raster_band_completion import complete_bands
        if hashlib.sha256(image.convert('RGBA').tobytes()).hexdigest() != patterns['source_RGBA_sha256']:
            raise ValueError('Periodic observations belong to another source image')
        completion = complete_bands(image, rulings)
        rules = {key: completion[key] for key in ('horizontal', 'vertical')}
    else:
        rules = {key: [dict(copy.deepcopy(r), support_kind='solid_source', source_rule=i)
                       for i, r in enumerate(rulings[key])]
                 for key in ('horizontal', 'vertical')}
    unresolved = copy.deepcopy(completion['rejected_completions']) if image is not None else []
    for axis, key in ((0, 'horizontal'), (1, 'vertical')):
        entries = []
        for c in candidates:
            if c['axis'] != axis:
                continue
            if c['pattern_kind'] == 'flanked_interval':
                unresolved.append({'reason': 'local_periodic_groups_are_not_a_complete_cell_border',
                                   'candidates': [c['id']]})
                continue
            entries.append({'candidate': c, 'band': [c['cross'], c['cross']+1],
                            'span': [c['junctions'][0]['band'][0], c['junctions'][-1]['band'][1]]})
        parents = list(range(len(entries)))

        def root(i):
            while parents[i] != i:
                parents[i] = parents[parents[i]]
                i = parents[i]
            return i

        for i, a in enumerate(entries):
            for j, b in enumerate(entries[:i]):
                if (max(a['band'][0], b['band'][0]) <= min(a['band'][1], b['band'][1])
                        and max(a['span'][0], b['span'][0]) < min(a['span'][1], b['span'][1])):
                    parents[root(i)] = root(j)
        groups = {}
        for i, entry in enumerate(entries):
            groups.setdefault(root(i), []).append(entry)
        for group in groups.values():
            cs = [e['candidate'] for e in group]
            band = [min(e['band'][0] for e in group), max(e['band'][1] for e in group)]
            spans = sorted(e['span'] for e in group)
            extent = [min(s[0] for s in spans), max(s[1] for s in spans)]
            ids = sorted(c['id'] for c in cs)
            if band[1]-band[0] > max(c['stroke_thickness'] for c in cs):
                unresolved.append({'reason': 'periodic_track_is_not_thin', 'candidates': ids})
                continue
            # Overlapping solid/dashed tracks need a separate source union
            # proof. Retain that ambiguity instead of placing two boundaries.
            conflicting = [i for i, r in enumerate(rules[key])
                           if r['support_kind'] != 'inferred_periodic_span'
                           and max(band[0], r['band'][0]) < min(band[1], r['band'][1])
                           and max(extent[0], r['span'][0]) < min(extent[1], r['span'][1])]
            if conflicting:
                unresolved.append({'reason': 'periodic_and_solid_tracks_overlap',
                                   'axis': axis, 'candidates': ids, 'solid_rules': conflicting})
                continue
            support = set()
            for c in cs:
                support.update((c['cross'], a, b) for a, b in c['support'])
                for junction in c['junctions']:
                    for witness in junction['witnesses']:
                        x, y = witness['pixel']
                        support.add((y, x, x+1) if axis == 0 else (x, y, y+1))
            rules[key].append({'axis': axis, 'band': band, 'span': extent,
                               'support': [list(s) for s in sorted(support)],
                               'support_kind': 'inferred_periodic_span', 'candidates': ids,
                               'logical_spans': spans})

    def covered(rule, start, end):
        if rule['support_kind'] in ('solid_source', 'source_band_completion'):
            return _covered(rule, start, end)
        cursor = start
        for a, b in rule['logical_spans']:
            if a > cursor:
                return False
            cursor = max(cursor, b)
            if cursor >= end:
                return True
        return False

    h, v = rules['horizontal'], rules['vertical']
    nodes, edges, faces, cells, graph_unresolved = _faces(h, v, covered=covered)
    for edge in edges:
        rule = (h if edge['axis'] == 0 else v)[edge['rule']]
        edge['support_kind'] = rule['support_kind']
    for cell in cells:
        face = faces[cell['face']]
        kinds = {edges[e]['support_kind'] for e in face['edges']}
        cell['boundary_kind'] = ('includes_inferred_periodic_spans' if 'inferred_periodic_span' in kinds
                                 else 'includes_source_band_completion' if 'source_band_completion' in kinds
                                 else 'solid_source')
    return {'schema': 1, 'kind': 'unclassified_periodic_ruling_faces',
            'canvas': copy.deepcopy(rulings['canvas']),
            'source_RGBA_sha256': patterns['source_RGBA_sha256'],
            'settings': {'vertices': 'actual_source_black_intersections',
                         'periodic_edges': 'declared_between_witnessed_source_junctions',
                         'solid_gap_closing': False, 'source_band_completion': image is not None},
            'horizontal': h, 'vertical': v, 'intersections': nodes, 'edges': edges,
            'bounded_faces': faces, 'rectangular_cells': cells,
            'unresolved': unresolved+graph_unresolved, 'coverage_verified': False,
            'article_assignment': None, 'native_glyph_binding': None}

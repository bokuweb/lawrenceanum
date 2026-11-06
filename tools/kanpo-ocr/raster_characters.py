"""Preserve source-bound character ranges and split candidates at physical cells.

All returned model candidates remain alternatives. Geometry associates a range
with source ink, cells and word observations; it never chooses a spelling.
"""
import copy
import json
import math
from pathlib import Path

from raster_layout import contains, ink_runs, overlap, pixel_box, sha, support_bounds
from raster_orientation import observe_orientation, right_axis
from source_gutters import ink_mask


def innermost(bounds, cells):
    covering = [c for c in cells if contains(c['interior_source_pixels'], bounds)]
    return [c['id'] for c in covering if not any(
        c['interior_source_pixels'] != other['interior_source_pixels']
        and contains(c['interior_source_pixels'], other['interior_source_pixels']) for other in covering)]


def union_bounds(boxes):
    boxes = [b for b in boxes if b is not None]
    return ([min(b[0] for b in boxes), min(b[1] for b in boxes),
             max(b[2] for b in boxes), max(b[3] for b in boxes)] if boxes else None)


def character_groups(characters, refs):
    """Keep raw whitespace inside a range only when both spatial neighbors agree."""
    groups, pending, previous = [], [], None
    for ref in refs:
        item = characters[ref]
        if not item['text'].strip():
            pending.append(ref)
            continue
        key = (item['cell'], item['glyph_right_axis'], item['geometry_status'])
        if groups and key == previous:
            groups[-1].extend(pending + [ref])
        else:
            if pending:
                groups.append(pending)
            groups.append([ref])
        pending, previous = [], key
    if pending:
        groups.append(pending)
    return groups


def character_layer(layout, parent, report_raw):
    """Use only captured source pixels, model ranges and physical ruling cells."""
    report = json.loads(report_raw)
    if (report.get('schema') != 1 or report.get('kind') != 'source_derived_raster_orientation'
            or report.get('source') != layout['source']
            or report.get('recognition_parent_sha256') != layout['provenance']['recognition_parent_sha256']):
        raise ValueError('Character report belongs to another source or recognition copy')
    page = report['vision']['page']
    if observe_orientation(parent, page) != report['observation']:
        raise ValueError('Character report differs from observed source glyph geometry')
    width, height = parent.size
    if layout['canvas'] != {'pixel_width': width, 'pixel_height': height}:
        raise ValueError('Character copy differs from the original source canvas')
    mask = ink_mask(parent)
    characters, candidates, fragments, unresolved = [], [], [], []
    for oi, observation in enumerate(page['observations']):
        for rank, candidate in enumerate(observation['candidates']):
            text, confidence = candidate.get('text'), candidate.get('confidence')
            if (not isinstance(text, str) or type(confidence) not in (int, float)
                    or not math.isfinite(confidence) or not 0 <= confidence <= 1
                    or not isinstance(candidate.get('characters'), list)):
                raise ValueError('Invalid raw character candidate')
            cursor, pieces, refs = 0, [], []
            for ci, raw in enumerate(candidate['characters']):
                piece = raw.get('text')
                if (not isinstance(piece, str) or not piece
                        or raw.get('unicode_scalar_offset') != cursor
                        or raw.get('unicode_scalar_length') != len(piece)):
                    raise ValueError('Character scalar ranges differ from the complete candidate')
                item = {'id': len(characters), 'observation': oi, 'candidate': rank, 'character': ci,
                        'unicode_scalar_offset': cursor, 'unicode_scalar_length': len(piece), 'text': piece,
                        'quad_source_pixels': None, 'source_pixels': None, 'glyph_right_axis': None,
                        'ink_pixels': 0, 'ink_bounds_source_pixels': None, 'support_sha256': None,
                        'cells': [], 'cell': None, 'containing_words': [], 'overlapping_words': [],
                        'geometry_status': 'invalid_or_unavailable_quad'}
                basis = right_axis(raw.get('quad'), width, height)
                if basis is not None:
                    points = basis['quad_source_pixels']
                    box = [max(0, math.floor(min(p[0] for p in points))),
                           max(0, math.floor(min(p[1] for p in points))),
                           min(width, math.ceil(max(p[0] for p in points))),
                           min(height, math.ceil(max(p[1] for p in points)))]
                    if pixel_box(box, width, height):
                        runs = ink_runs(mask, box)
                        bounds = support_bounds(runs)
                        item.update(quad_source_pixels=points, source_pixels=box,
                                    glyph_right_axis=basis['axis'],
                                    ink_pixels=sum(b-a for y, a, b in runs),
                                    ink_bounds_source_pixels=bounds,
                                    support_sha256=sha(json.dumps(runs, separators=(',', ':')).encode()) if runs else None,
                                    geometry_status='source_character_quad_candidate')
                        if bounds is not None and piece.strip():
                            owners = innermost(bounds, layout['cells'])
                            item['cells'] = owners
                            item['cell'] = owners[0] if len(owners) == 1 else None
                            item['containing_words'] = [w['id'] for w in layout['words']
                                                        if contains(w['bounds_source_pixels'], bounds)]
                            # Both supports sample the same mask. Containment of
                            # the complete ink box implies containment of every
                            # sampled character pixel. Partial overlaps stay separate.
                            for word in layout['words']:
                                if word['id'] in item['containing_words']:
                                    continue
                                common = [max(bounds[0], word['bounds_source_pixels'][0]),
                                          max(bounds[1], word['bounds_source_pixels'][1]),
                                          min(bounds[2], word['bounds_source_pixels'][2]),
                                          min(bounds[3], word['bounds_source_pixels'][3])]
                                if overlap(bounds, word['bounds_source_pixels']) and ink_runs(mask, common):
                                    item['overlapping_words'].append(word['id'])
                if not piece.strip():
                    item['geometry_status'] = 'whitespace_position_unverified'
                if item['geometry_status'] != 'source_character_quad_candidate':
                    unresolved.append({'reason': item['geometry_status'], 'characters': [item['id']], 'fragments': []})
                elif not item['ink_pixels']:
                    unresolved.append({'reason': 'no_copy_ink_in_character_rectangle', 'characters': [item['id']], 'fragments': []})
                elif len(item['cells']) > 1:
                    unresolved.append({'reason': 'ambiguous_character_cell_containment', 'characters': [item['id']], 'fragments': []})
                elif item['cell'] is None and any(overlap(c['interior_source_pixels'], item['ink_bounds_source_pixels']) for c in layout['cells']):
                    unresolved.append({'reason': 'character_crosses_candidate_cell_boundary', 'characters': [item['id']], 'fragments': []})
                refs.append(item['id']); characters.append(item); pieces.append(piece); cursor += len(piece)
            if ''.join(pieces) != text:
                raise ValueError('Character ranges do not reconstruct the complete candidate')
            # Whitespace is retained without inventing character coordinates.
            # Matching spatial neighbors can keep it inside the same raw range;
            # cross-cell and leading/trailing whitespace stays unpositioned.
            groups = character_groups(characters, refs)
            fragment_refs = []
            for group in groups:
                members = [characters[i] for i in group]
                supported = [c for c in members if c['text'].strip()]
                fragment = {'id': len(fragments), 'observation': oi, 'candidate': rank,
                            'characters': group, 'text': ''.join(c['text'] for c in members),
                            'cell': supported[0]['cell'] if supported else None,
                            'glyph_right_axis': supported[0]['glyph_right_axis'] if supported else None,
                            'bounds_source_pixels': union_bounds([c['ink_bounds_source_pixels'] for c in supported]),
                            'status': 'model_sequence_fragment_candidate'}
                fragment_refs.append(fragment['id']); fragments.append(fragment)
            candidates.append({'observation': oi, 'candidate': rank, 'text': text,
                               'confidence': confidence, 'characters': refs, 'fragments': fragment_refs})
    helper_names = ['raster_characters.py', 'raster_layout.py', 'raster_orientation.py', 'source_gutters.py']
    return {'schema': 1, 'kind': 'source_derived_raster_characters',
            'source': copy.deepcopy(layout['source']), 'canvas': copy.deepcopy(layout['canvas']),
            'recognition_parent_sha256': layout['provenance']['recognition_parent_sha256'],
            'report_sha256': sha(report_raw), 'raw_report': report,
            'implementation_sha256': {name: sha(Path(__file__).with_name(name).read_bytes()) for name in helper_names},
            'characters': characters, 'candidates': candidates, 'fragments': fragments, 'unresolved': unresolved,
            'association_policy': 'complete_copy_ink_bounds_in_unique_innermost_source_cell',
            'fragment_policy': 'raw_character_ranges_split_at_supported_cell_or_axis_changes',
            'coverage_verified': False, 'visibility': 'unverified'}

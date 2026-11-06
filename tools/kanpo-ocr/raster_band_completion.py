"""Recover short actual black support inside already observed ruling bands.

No white gap is closed and no band grows toward neighboring text. Components
join only through shared pixels on adjacent scanlines (four-connectivity).
Only components seeded by an original long-run ruling become candidates.
"""
import copy
import hashlib
from PIL import Image
from raster_grid import _black_mask, _tracks


def complete_bands(image, rulings):
    if [image.width, image.height] != [rulings['canvas']['pixel_width'], rulings['canvas']['pixel_height']]:
        raise ValueError('Source ruling canvas differs from the original image')
    result = {}
    rejected = []
    mask = _black_mask(image)
    for axis, key in ((0, 'horizontal'), (1, 'vertical')):
        scan = mask if axis == 0 else mask.transpose(Image.Transpose.TRANSPOSE)
        groups = []
        for ri, rule in sorted(enumerate(rulings[key]), key=lambda entry: entry[1]['band']):
            lo, hi = rule['band']
            if not (0 <= lo < hi <= scan.height):
                raise ValueError('Original ruling band is outside its source canvas')
            if groups and lo < groups[-1]['band'][1]:
                groups[-1]['band'][1] = max(groups[-1]['band'][1], hi)
                groups[-1]['seeds'].append(ri)
            else:
                groups.append({'band': [lo, hi], 'seeds': [ri]})
        observed = []
        for group in groups:
            lo, hi = group['band']
            thin, other = _tracks(scan.crop((0, lo, scan.width, hi)), 1, 2, 0)
            components = thin + other
            lookup = {}
            for ci, component in enumerate(components):
                for row, a, b in component['support']:
                    lookup[(row+lo, a, b)] = ci
            seeded = {}
            for ri in group['seeds']:
                original = rulings[key][ri]
                roots = {lookup.get(tuple(s)) for s in original['support']}
                if len(roots) != 1 or None in roots:
                    raise ValueError('Original ruling support does not belong to one source component')
                seeded.setdefault(roots.pop(), []).append(ri)
            for ci, ids in seeded.items():
                component = components[ci]
                support = [[row+lo, a, b] for row, a, b in component['support']]
                band = [component['band'][0]+lo, component['band'][1]+lo]
                span = component['span'][:]
                maximum_width = max(rulings[key][i]['band'][1]-rulings[key][i]['band'][0] for i in ids)
                if band[1]-band[0] > maximum_width:
                    rejected.append({'reason': 'connected_completion_exceeds_original_stroke_width',
                                     'axis': axis, 'source_rules': sorted(ids), 'band': band,
                                     'span': span, 'support': support, 'observed_band': [lo, hi]})
                    observed.extend(dict(copy.deepcopy(rulings[key][i]),support_kind='solid_source',source_rule=i)
                                    for i in ids)
                    continue
                if (len(ids) == 1 and support == rulings[key][ids[0]]['support']
                        and band == rulings[key][ids[0]]['band'] and span == rulings[key][ids[0]]['span']):
                    observed.append(dict(copy.deepcopy(rulings[key][ids[0]]),
                                         support_kind='solid_source', source_rule=ids[0]))
                else:
                    observed.append({'axis': axis, 'band': band, 'span': span, 'support': support,
                                     'support_kind': 'source_band_completion',
                                     'source_rules': sorted(ids), 'observed_band': [lo, hi]})
        result[key] = sorted(observed, key=lambda r: (r['band'], r['span']))
    return {'schema': 1, 'kind': 'source_pixel_supported_ruling_band_completion',
            'source_RGBA_sha256': hashlib.sha256(image.convert('RGBA').tobytes()).hexdigest(),
            'canvas': copy.deepcopy(rulings['canvas']), 'connectivity': 4,
            'gap_closing': False, 'band_growth': False, **result,
            'rejected_completions': rejected,
            'coverage_verified': False}

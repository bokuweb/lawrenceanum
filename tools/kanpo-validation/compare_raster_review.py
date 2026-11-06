"""Source-position evaluation of manual raster text reviews, never parser input."""
import argparse
import hashlib
import json
import math
import unicodedata
from pathlib import Path


def normalized(text):
    return ''.join(c for c in unicodedata.normalize('NFKC', text) if not c.isspace())


def distance(a, b):
    previous = list(range(len(b) + 1))
    for i, ca in enumerate(a, 1):
        current = [i]
        for j, cb in enumerate(b, 1):
            current.append(min(previous[j] + 1, current[-1] + 1, previous[j-1] + (ca != cb)))
        previous = current
    return previous[-1]


def ordered_box(box):
    return (isinstance(box, list) and len(box) == 4
            and all(type(x) in (int, float) and math.isfinite(x) for x in box)
            and box[0] < box[2] and box[1] < box[3])


def inside(outer, inner):
    return outer[0] <= inner[0] and outer[1] <= inner[1] and inner[2] <= outer[2] and inner[3] <= outer[3]


def score(review, predictions):
    if review.get('schema') != 1 or review.get('kind') != 'manual_raster_region_review':
        raise ValueError('Expected manual source-position raster review')
    if review.get('comparison') != 'NFKC_then_remove_whitespace_for_optical_diagnostic_only':
        raise ValueError('Unrecognized manual optical comparison policy')
    regions = review['regions']
    if (not regions or len({r['id'] for r in regions}) != len(regions)
            or any(not ordered_box(r['source_pixels']) or not isinstance(r['text'], str) for r in regions)):
        raise ValueError('Invalid manual review regions')
    for i, region in enumerate(regions):
        a = region['source_pixels']
        for other in regions[:i]:
            b = other['source_pixels']
            if max(a[0], b[0]) < min(a[2], b[2]) and max(a[1], b[1]) < min(a[3], b[3]):
                raise ValueError('Ambiguous overlapping manual review masks')
    assigned = [[] for _ in regions]
    outside, halo = [], []
    for prediction in predictions:
        if prediction['core_status'] not in ('center_inside_core', 'halo_observation'):
            raise ValueError('Invalid source core ownership status')
        if prediction['core_status'] == 'halo_observation':
            halo.append(prediction)
            continue
        box = prediction['source_pixels']
        if not ordered_box(box) or not isinstance(prediction['text'], str):
            raise ValueError('Invalid source-owned OCR observation')
        x, y = (box[0]+box[2])/2, (box[1]+box[3])/2
        owners = [i for i, r in enumerate(regions)
                  if r['source_pixels'][0] <= x < r['source_pixels'][2]
                  and r['source_pixels'][1] <= y < r['source_pixels'][3]]
        if len(owners) > 1:
            raise ValueError('Ambiguous overlapping manual review masks')
        if not owners:
            overlapping = [r['id'] for r in regions
                           if max(box[0], r['source_pixels'][0]) < min(box[2], r['source_pixels'][2])
                           and max(box[1], r['source_pixels'][1]) < min(box[3], r['source_pixels'][3])]
            outside.append(dict(prediction, overlapping_review_masks=overlapping))
        else:
            assigned[owners[0]].append(prediction)
    results = []
    for region, observed in zip(regions, assigned):
        # Keep every observation. Do not select the spelling closest to gold,
        # deduplicate matching strings, or borrow text from another position.
        observed.sort(key=lambda p: (p['source_pixels'][1], p['source_pixels'][0], p['tile'], p['region']))
        text = '\n'.join(p['text'] for p in observed)
        expected, actual = normalized(region['text']), normalized(text)
        geometry = all(inside(region['source_pixels'], p['source_pixels']) for p in observed)
        results.append({'id': region['id'], 'source_pixels': region['source_pixels'],
                        'expected': region['text'], 'observed': text, 'observations': observed,
                        'expected_scalars': len(expected), 'edit_distance': distance(expected, actual),
                        'text_equal_under_declared_optical_normalization': actual == expected,
                        'all_observations_inside_review_mask': geometry,
                        'exact_review_region': actual == expected and geometry,
                        'missing_observation': not observed})
    return {'schema': 1, 'kind': 'source_position_raster_review_comparison', 'scope': review.get('scope'),
            'comparison': review['comparison'], 'source': review['source'], 'regions': results,
            'counts': {'review_regions': len(results),
                       'text_equal': sum(r['text_equal_under_declared_optical_normalization'] for r in results),
                       'exact_review_regions': sum(r['exact_review_region'] for r in results),
                       'missing_observations': sum(r['missing_observation'] for r in results),
                       'expected_scalars': sum(r['expected_scalars'] for r in results),
                       'edit_distance': sum(r['edit_distance'] for r in results),
                       'outside_review_scope_observations': len(outside),
                       'unassigned_predictions_overlapping_review_masks': sum(bool(p['overlapping_review_masks']) for p in outside),
                       'halo_observations': len(halo)},
            'assessment_limit': 'Position-based review, not raw OCR character accuracy. A missing observation means no source-owned region center entered that mask; text may exist in a wider unmatched region. No expected substring is borrowed to repair its geometry.',
            'outside_review_scope': outside, 'coverage_verified': False}


def local(directory, name):
    if not isinstance(name, str) or Path(name).name != name or name in ('', '.', '..'):
        raise ValueError('Expected a local raster comparison receipt member')
    return directory / name


def mapped_source_box(tile, pixels, turns=0):
    if type(turns) is not int or not 0<=turns<=3:
        raise ValueError('Invalid declared recognition quarter turn')
    x,y,right,bottom=tile['recognition_roi'];w,h=right-x,bottom-y
    expected=([1,0,0,1,x,y],[0,1,-1,0,right,y],[-1,0,0,-1,right,bottom],[0,-1,1,0,x,bottom])[turns]
    matrix=tile.get('recognition_to_source',expected if turns==0 else None)
    if matrix!=expected:
        raise ValueError('OCR normalization differs from the declared source crop')
    values=[pixels[k] for k in ('x0','y0','x1','y1')]
    cw,ch=(h,w) if turns%2 else (w,h)
    u0,v0,u1,v1=values
    if (any(type(v) is not int for v in values) or not (0<=u0<u1<=cw and 0<=v0<v1<=ch)):
        raise ValueError('OCR region is outside its normalized source crop')
    a,b,c,d,e,f=matrix
    points=[(a*u+c*v+e,b*u+d*v+f) for u,v in ((u0,v0),(u1,v0),(u1,v1),(u0,v1))]
    return [min(p[0] for p in points),min(p[1] for p in points),max(p[0] for p in points),max(p[1] for p in points)]


def compare(review_path, directory):
    directory = Path(directory)
    review_raw = Path(review_path).read_bytes()
    review = json.loads(review_raw)
    index_raw = (directory / 'index.json').read_bytes()
    index = json.loads(index_raw)
    if (index.get('schema') != 1 or index.get('kind') != 'unclassified_source_derived_image_ocr'
            or index.get('coverage_verified') is not False):
        raise ValueError('Expected original source-derived OCR index')
    for key, value in review['source'].items():
        if index['source'].get(key) != value:
            raise ValueError('Manual review and OCR belong to different source images: ' + key)
    prep_raw = (directory / 'prepared/preparation.json').read_bytes()
    digest = hashlib.sha256(prep_raw).hexdigest()
    if digest != index['preparation_sha256']:
        raise ValueError('Captured OCR preparation differs from the index')
    prep = json.loads(prep_raw)
    if prep['source']!=index['source']:
        raise ValueError('OCR preparation belongs to another source image')
    if review['canvas'] != prep['canvas']:
        raise ValueError('Manual review and source preparation canvases differ')
    if [r['tile'] for r in index['rows']] != [t['id'] for t in prep['tiles']]:
        raise ValueError('OCR index does not retain the complete declared tile inventory')
    turns=prep.get('settings',{}).get('recognition_quarter_turns',0)
    if type(turns) is not int or not 0<=turns<=3 or (turns and not prep.get('orientation')):
        raise ValueError('Invalid source orientation declaration')
    if prep.get('orientation'):
        orientation_raw=local(directory/'prepared',prep['orientation']['file']).read_bytes()
        if hashlib.sha256(orientation_raw).hexdigest()!=prep['orientation']['sha256']:
            raise ValueError('Captured source orientation changed')
        orientation=json.loads(orientation_raw)
        if (orientation.get('source')!=prep['source'] or orientation.get('recognition_parent_sha256')!=prep['recognition_parent_sha256']
                or orientation.get('observation',{}).get('quarter_turns_ccw')!=turns):
            raise ValueError('Source orientation belongs to another image or recognition copy')
    predictions = []
    failures = []
    for row in index['rows']:
        if row['status'] != 'recognized_unclassified':
            if row['status'] != 'recognition_copy_empty_unverified':
                failures.append(row)
            continue
        obs = json.loads(local(directory, row['projection']).read_bytes())
        derivation = obs['source_derivation']
        if (derivation['preparation_sha256'] != digest or derivation['tile'] != row['tile']
                or derivation['original_document_sha256'] != review['source']['document_sha256']
                or derivation['original_PNG_sha256'] != review['source']['source_PNG_sha256']):
            raise ValueError('OCR observation changed its original source identity')
        tile = prep['tiles'][row['tile']]
        if tile['id'] != row['tile'] or derivation['recognition_roi'] != tile['recognition_roi']:
            raise ValueError('OCR region does not match the declared source crop')
        if prep.get('orientation') and (derivation.get('orientation_sha256')!=prep['orientation']['sha256']
                or derivation.get('recognition_quarter_turns')!=turns
                or derivation.get('recognition_to_source')!=tile.get('recognition_to_source')):
            raise ValueError('OCR orientation differs from its declared source crop')
        for region in obs['recognition_regions']:
            core = region['source_core']
            if core['status'] == 'invalid_recognition_box':
                failures.append({'tile': row['tile'], 'region': region['id'], 'reason': 'invalid_recognition_box'})
                continue
            box = region['bbox']['pixels']
            actual = mapped_source_box(tile,box,turns)
            if actual != core['source_pixels']:
                raise ValueError('OCR source pixel mapping differs from its crop')
            a, b, c, d = tile['core']
            cx, cy = (actual[0]+actual[2])/2, (actual[1]+actual[3])/2
            expected_core = 'center_inside_core' if a <= cx < c and b <= cy < d else 'halo_observation'
            if core['status'] != expected_core:
                raise ValueError('OCR core ownership differs from original pixel geometry')
            predictions.append({'tile': row['tile'], 'region': region['id'], 'text': region['text'],
                                'source_pixels': actual, 'core_status': core['status']})
    result = score(review, predictions)
    result['review_sha256'] = hashlib.sha256(review_raw).hexdigest()
    result['ocr_index_sha256'] = hashlib.sha256(index_raw).hexdigest()
    result['failed_or_invalid_observations'] = failures
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('review', type=Path)
    parser.add_argument('ocr_directory', type=Path)
    parser.add_argument('--output', required=True, type=Path)
    options = parser.parse_args()
    result = compare(options.review, options.ocr_directory)
    with options.output.open('x') as file:
        file.write(json.dumps(result, ensure_ascii=False, indent=2) + '\n')
    print(json.dumps(result['counts']))
    return 0 if (result['counts']['exact_review_regions'] == result['counts']['review_regions']
                 and not result['failed_or_invalid_observations']) else 2


if __name__ == '__main__':
    raise SystemExit(main())

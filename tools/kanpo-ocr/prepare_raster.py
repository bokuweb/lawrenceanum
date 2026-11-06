"""Prepare declared OCR copies and pixel-complete source partitions.

The original PDF, PNG, ruling observations and receipts remain unchanged.
Geometry is decided before OCR. A transformed empty region is not a verified
blank source, and geometrically closed faces are not semantic table cells.
"""
import argparse
import copy
import hashlib
import json
from io import BytesIO
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter
import raster_grid
import raster_ruling_patterns
import source_gutters
from raster_grid import crop_frame, observe
from source_gutters import ink_mask


def sha(data):
    return hashlib.sha256(data).hexdigest()


def local(directory, name):
    if not isinstance(name, str) or Path(name).name != name or name in ('', '.', '..'):
        raise ValueError('Expected a local raster receipt member')
    return directory / name


def source_parts(width, height, cells):
    """Partition all pixels, retaining nested and overlapping candidate identities."""
    if type(width) is not int or type(height) is not int or width <= 0 or height <= 0:
        raise ValueError('Expected positive integer source dimensions')
    candidates = []
    ids = set()
    for cell in cells:
        box = cell['interior_pixels']
        if (type(cell['id']) is not int or cell['id'] < 0 or cell['id'] in ids
                or not isinstance(box, list) or len(box) != 4
                or any(type(v) is not int for v in box)
                or not (0 <= box[0] < box[2] <= width and 0 <= box[1] < box[3] <= height)):
            raise ValueError('Invalid or duplicate source cell occurrence')
        ids.add(cell['id'])
        candidates.append({'id': cell['id'], 'box': box[:]})
    cuts = sorted({0, height, *[y for part in candidates for y in (part['box'][1], part['box'][3])]})
    active = {}
    parts = []
    contains = lambda a, b: a[0] <= b[0] and a[1] <= b[1] and b[2] <= a[2] and b[3] <= a[3]
    for top, bottom in zip(cuts, cuts[1:]):
        covered = [p for p in candidates if p['box'][1] <= top and bottom <= p['box'][3]]
        edges = sorted({0, width, *[x for p in covered for x in (p['box'][0], p['box'][2])]})
        intervals = []
        for left, right in zip(edges, edges[1:]):
            covering = [p for p in covered if p['box'][0] <= left and right <= p['box'][2]]
            inner = [p for p in covering if not any(p['box'] != q['box'] and contains(p['box'], q['box']) for q in covering)]
            owners = tuple(sorted(p['id'] for p in inner))
            if intervals and intervals[-1][1] == left and intervals[-1][2] == owners:
                intervals[-1] = (intervals[-1][0], right, owners)
            else:
                intervals.append((left, right, owners))
        next_active = {}
        for left, right, owners in intervals:
            key = (left, right, owners)
            if key in active:
                part = active.pop(key)
                part['box'][3] = bottom
            else:
                part = {'box': [left, top, right, bottom]}
                if not owners:
                    part['role'] = 'outside_ruling_faces'
                elif len(owners) == 1:
                    part.update(role='ruling_face_candidate', cell=owners[0])
                else:
                    part.update(role='overlapping_ruling_faces', cells=list(owners))
            next_active[key] = part
        parts.extend(active.values())
        active = next_active
    parts.extend(active.values())
    assert sum((p['box'][2]-p['box'][0]) * (p['box'][3]-p['box'][1]) for p in parts) == width * height
    parts.sort(key=lambda p: (p['box'][1], p['box'][0], p['box'][3], p['box'][2]))
    return [dict(part, id=i) for i, part in enumerate(parts)]


def windows(parts, *, core_size=768, halo=128):
    if type(core_size) is not int or core_size <= 0 or type(halo) is not int or halo < 0:
        raise ValueError('Expected positive integer core size and nonnegative halo')
    result = []
    for part in parts:
        left, top, right, bottom = part['box']
        for y in range(top, bottom, core_size):
            for x in range(left, right, core_size):
                end_x, end_y = min(right, x+core_size), min(bottom, y+core_size)
                result.append({'id': len(result), 'part': part['id'], 'role': part['role'],
                               **({'cell': part['cell']} if 'cell' in part else {}),
                               **({'cells': part['cells']} if 'cells' in part else {}),
                               'core': [x, y, end_x, end_y],
                               'window': [max(left, x-halo), max(top, y-halo),
                                          min(right, end_x+halo), min(bottom, end_y+halo)]})
    return result


def verify_grid(image, observation):
    """Recompute geometry so a receipt cannot declare arbitrary erasure boxes."""
    settings = observation['settings']
    expected = observe(image, minimum_run=settings['minimum_run'], minimum_aspect=settings['minimum_aspect'])
    actual = copy.deepcopy(observation)
    actual.pop('source', None)
    for cell in actual.get('rectangular_cells', []):
        cell.pop('crop', None)
    if actual != expected:
        raise ValueError('Ruling observations do not reproduce from original pixels')


def recognition_copy(image, observation, profile, *, details=None):
    if profile not in ('identity', 'median3-ruling-bands', 'median3-ruling-patterns'):
        raise ValueError('Unsupported raster recognition profile')
    mask = Image.new('L', image.size)
    if profile == 'identity':
        return image.copy(), mask
    if image.getchannel('A').getextrema() != (255, 255):
        raise ValueError('Median/ruling preprocessing requires an opaque source canvas')
    filtered = image.filter(ImageFilter.MedianFilter(3))
    draw = ImageDraw.Draw(mask)
    for key in ('horizontal', 'vertical'):
        for rule in observation[key]:
            low, high = rule['band']
            start, end = rule['span']
            # Inclusive Pillow rectangle ends correspond to a one-pixel
            # expansion of the half-open original band and span.
            box = (start-1, low-1, end, high) if key == 'horizontal' else (low-1, start-1, high, end)
            draw.rectangle(box, fill=255)
    filtered.paste((255, 255, 255, 255), (0, 0, image.width, image.height), mask)
    if profile == 'median3-ruling-patterns':
        patterns = raster_ruling_patterns.observe_dashes(image, observation)
        extra_mask, erased = raster_ruling_patterns.recognition_mask(filtered, patterns)
        filtered.paste((255, 255, 255, 255), (0, 0, image.width, image.height), extra_mask)
        mask = ImageChops.lighter(mask, extra_mask)
        if details is not None:
            details.update(schema=1, kind='source_derived_ruling_pattern_erasure',
                           original=patterns, erasure=erased)
    return filtered, mask


def prepare(asset_directory, pdf, output, *, profile, core_size=768, halo=128, retain_window=False,
            periodic_partition=False, complete_ruling_bands=False, record_ink_support=False,
            orientation_report=None):
    if type(retain_window) is not bool:
        raise ValueError('Expected a boolean source-window retention option')
    if type(periodic_partition) is not bool:
        raise ValueError('Expected a boolean periodic-boundary partition option')
    if type(complete_ruling_bands) is not bool or (complete_ruling_bands and not periodic_partition):
        raise ValueError('Source band completion requires the explicit periodic partition')
    if type(record_ink_support) is not bool:
        raise ValueError('Expected a boolean recognition-copy ink recording option')
    asset_directory, output = Path(asset_directory), Path(output)
    binding_raw = (asset_directory / 'pdf-binding.json').read_bytes()
    binding = json.loads(binding_raw)
    if (binding.get('schema') != 1 or binding.get('kind') != 'unclassified_pdf_raster_ruling_faces'
            or binding.get('coverage_verified') is not False or binding.get('article_assignment') is not None
            or binding.get('native_glyph_binding') is not None):
        raise ValueError('Expected unclassified PDF raster ruling observations')
    source = binding['source']
    pdf_raw = Path(pdf).read_bytes()
    png_raw = (asset_directory / 'source.png').read_bytes()
    observation_raw = local(asset_directory, binding['observation']).read_bytes()
    receipt_raw = local(asset_directory, source['source_receipt']).read_bytes()
    placement_raw = local(asset_directory, source['source_placement']).read_bytes()
    if (sha(pdf_raw) != source['document_sha256'] or sha(png_raw) != source['source_PNG_sha256']
            or sha(observation_raw) != binding['observation_sha256']
            or sha(receipt_raw) != source['source_receipt_sha256']
            or sha(placement_raw) != source['source_placement_sha256']):
        raise ValueError('Original PDF/raster observation producer hashes differ')
    image = Image.open(BytesIO(png_raw)).convert('RGBA')
    observation, receipt, frame = (json.loads(raw) for raw in (observation_raw, receipt_raw, placement_raw))
    if (sha(image.tobytes()) != source['source_RGBA_sha256']
            or observation['source']['PNG_sha256'] != sha(png_raw)
            or observation['source']['RGBA_sha256'] != sha(image.tobytes())
            or receipt['document_sha256'] != sha(pdf_raw) or receipt['PNG_sha256'] != sha(png_raw)
            or receipt['placement_sha256'] != sha(placement_raw)
            or receipt['RGBA_pixels_sha256'] != sha(image.tobytes())
            or receipt['source_draw']['encoded_sha256'] != source['encoded_sha256']
            or (receipt['page'], receipt['draw']) != (source['page'], source['draw'])
            or (frame['pixel_width'], frame['pixel_height']) != image.size):
        raise ValueError('Original raster pixels or placement do not match the PDF receipt')
    verify_grid(image, observation)
    pattern_details = {}
    filtered, mask = recognition_copy(image, observation, profile, details=pattern_details)
    parent_buffer = BytesIO()
    filtered.save(parent_buffer, format='PNG')
    parent_raw = parent_buffer.getvalue()
    orientation_raw = None
    quarter_turns = 0
    if orientation_report is not None:
        import raster_orientation
        orientation_raw = Path(orientation_report).read_bytes()
        orientation = json.loads(orientation_raw)
        expected_source = {**source, 'binding_sha256': sha(binding_raw), 'observation_sha256': sha(observation_raw)}
        if (orientation.get('schema') != 1 or orientation.get('kind') != 'source_derived_raster_orientation'
                or orientation.get('source') != expected_source
                or orientation.get('recognition_parent_sha256') != sha(parent_raw)):
            raise ValueError('Orientation report belongs to another recognition copy or source')
        observed_orientation = raster_orientation.observe_orientation(filtered, orientation['vision']['page'])
        if orientation.get('observation') != observed_orientation:
            raise ValueError('Orientation does not reproduce from the source-bound character quadrilaterals')
        quarter_turns = observed_orientation['quarter_turns_ccw']
    partition_observation = None
    if periodic_partition:
        import raster_periodic_grid
        patterns = (pattern_details['original'] if pattern_details
                    else raster_ruling_patterns.observe_dashes(image, observation))
        partition_observation = raster_periodic_grid.periodic_faces(observation, patterns,
                                                                   image=image if complete_ruling_bands else None)
        partition_observation['source_observation_sha256'] = sha(observation_raw)
        partition_observation['periodic_observations'] = patterns
    cells = (partition_observation or observation)['rectangular_cells']
    parts = source_parts(image.width, image.height, cells)
    tiles = windows(parts, core_size=core_size, halo=halo)
    output.mkdir()
    # Capture inputs before writing any derived image.
    for name, raw in [('source.pdf', pdf_raw), ('source.png', png_raw),
                      ('source-binding.json', binding_raw), ('source-receipt.json', receipt_raw),
                      ('source-placement.json', placement_raw), ('source-observation.json', observation_raw)]:
        (output / name).write_bytes(raw)
        (output / name).chmod(0o400)
    implementation = output / 'implementation'
    implementation.mkdir()
    code = {name: path.read_bytes() for name, path in
            [('prepare_raster.py', Path(__file__)), ('raster_grid.py', Path(raster_grid.__file__)),
             ('source_gutters.py', Path(source_gutters.__file__)),
             ('raster_ruling_patterns.py', Path(raster_ruling_patterns.__file__))]}
    if periodic_partition:
        code['raster_periodic_grid.py'] = Path(raster_periodic_grid.__file__).read_bytes()
    if complete_ruling_bands:
        import raster_band_completion
        code['raster_band_completion.py'] = Path(raster_band_completion.__file__).read_bytes()
    if orientation_report is not None:
        code['raster_orientation.py'] = Path(raster_orientation.__file__).read_bytes()
    for name, raw in code.items():
        (implementation / name).write_bytes(raw)
        (implementation / name).chmod(0o400)
    (output / 'recognition-parent.png').write_bytes(parent_raw)
    mask.save(output / 'erasure-mask.png')
    for tile in tiles:
        crop = filtered.crop(tile['window'])
        bounds = ink_mask(crop).getbbox()
        if bounds is None:
            tile['status'] = 'recognition_copy_empty_unverified'
            continue
        # Trim only the declared recognition copy, retaining every original
        # source pixel in source.png and every core in the geometric plan.
        if retain_window:
            roi = tile['window'][:]
        else:
            x0, y0, x1, y1 = bounds
            bounds = [max(0, x0-1), max(0, y0-1), min(crop.width, x1+1), min(crop.height, y1+1)]
            wx, wy = tile['window'][:2]
            roi = [wx+bounds[0], wy+bounds[1], wx+bounds[2], wy+bounds[3]]
        crop = filtered.crop(roi)
        prefix = f"tile-{tile['id']:05}"
        png_name, frame_name = prefix + '.png', prefix + '.placement.json'
        placement = crop_frame(frame, roi)
        if orientation_report is not None:
            transform = raster_orientation.turn_matrix(crop.width, crop.height, quarter_turns)
            transform[4] += roi[0]
            transform[5] += roi[1]
            tile['recognition_to_source'] = transform
            placement = raster_orientation.turn_frame(placement, quarter_turns)
            crop = raster_orientation.turn_image(crop, quarter_turns)
        crop.save(output / png_name)
        frame_bytes = (json.dumps(placement, separators=(',', ':')) + '\n').encode()
        (output / frame_name).write_bytes(frame_bytes)
        tile.update(status='prepared_unclassified', recognition_roi=roi,
                    PNG=png_name, PNG_sha256=sha((output / png_name).read_bytes()),
                    RGBA_sha256=sha(crop.tobytes()), placement=frame_name, placement_sha256=sha(frame_bytes))
    result = {'schema': 1, 'kind': 'source_derived_ocr_preparation',
              'source': {**source, 'binding_sha256': sha(binding_raw), 'observation_sha256': sha(observation_raw)},
              'canvas': {'pixel_width': image.width, 'pixel_height': image.height},
              'profile': profile, 'settings': {'core_size': core_size, 'halo': halo,
                  'trim_padding': None if retain_window else 1,
                  'recognition_crop': 'source_window' if retain_window else 'nonwhite_bounds'},
              'operations': [] if profile == 'identity' else [
                  {'kind': 'pillow_channelwise_median', 'size': 3},
                  {'kind': 'erase_original_ruling_band_and_span', 'margin': 1}],
              'recognition_parent_sha256': sha((output / 'recognition-parent.png').read_bytes()),
              'erasure_mask_sha256': sha((output / 'erasure-mask.png').read_bytes()),
              'producer': {'code_sha256': {name: sha(raw) for name, raw in code.items()},
                           'pillow': Image.__version__},
              'parts': parts, 'tiles': tiles, 'pixel_partition_verified': True,
              'partition_ownership': 'innermost_containing_rectangle_with_explicit_partial_overlap',
              'ambiguous_parts': [p['id'] for p in parts if p['role'] == 'overlapping_ruling_faces'],
              'coverage_verified': False, 'visibility': 'unverified', 'article_assignment': None,
              'native_glyph_binding': None}
    if orientation_raw is not None:
        (output/'source-orientation.json').write_bytes(orientation_raw)
        (output/'source-orientation.json').chmod(0o400)
        result['orientation'] = {'file':'source-orientation.json','sha256':sha(orientation_raw)}
        result['settings']['recognition_quarter_turns'] = quarter_turns
    if partition_observation is not None:
        raw_partition = (json.dumps(partition_observation, separators=(',', ':')) + '\n').encode()
        (output / 'partition-observation.json').write_bytes(raw_partition)
        result['partition_observation'] = {'file': 'partition-observation.json', 'sha256': sha(raw_partition)}
        result['settings']['partition_geometry'] = 'periodic_ruling_candidates'
        if complete_ruling_bands:
            result['settings']['complete_ruling_bands'] = True
    if pattern_details:
        raw_details = (json.dumps(pattern_details, separators=(',', ':')) + '\n').encode()
        (output / 'ruling-patterns.json').write_bytes(raw_details)
        result['ruling_patterns'] = {'file': 'ruling-patterns.json', 'sha256': sha(raw_details)}
        result['operations'] += [
            {'kind': 'erase_isolated_one_pixel_components', 'connectivity': 8},
            {'kind': 'erase_bounded_periodic_ruling_components', 'connectivity': 8,
             'source': 'original_opaque_black_runs_between_perpendicular_rulings'}]
    if record_ink_support:
        ink = {'schema': 1, 'kind': 'unclassified_recognition_copy_ink_support',
               'source_RGBA_sha256': source['source_RGBA_sha256'],
               'recognition_parent_sha256': result['recognition_parent_sha256'],
               'recognition_RGBA_sha256': sha(filtered.tobytes()),
               'canvas': result['canvas'], 'connectivity': 8,
               'components': raster_ruling_patterns.components(ink_mask(filtered)),
               'coverage_verified': False}
        raw_ink = (json.dumps(ink, separators=(',', ':'))+'\n').encode()
        (output/'recognition-ink.json').write_bytes(raw_ink)
        result['recognition_ink'] = {'file': 'recognition-ink.json', 'sha256': sha(raw_ink)}
    (output / 'preparation.json').write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('asset_directory')
    parser.add_argument('--pdf', required=True)
    parser.add_argument('--output', required=True)
    parser.add_argument('--profile', required=True, choices=('identity', 'median3-ruling-bands', 'median3-ruling-patterns'))
    parser.add_argument('--core-size', type=int, default=768)
    parser.add_argument('--halo', type=int, default=128)
    parser.add_argument('--retain-window', action='store_true',
                        help='Keep the source-geometry window size instead of trimming to processed ink')
    parser.add_argument('--periodic-partition', action='store_true',
                        help='Partition using candidate faces with explicitly inferred periodic boundaries')
    parser.add_argument('--complete-ruling-bands', action='store_true',
                        help='Include short four-connected source support inside already observed ruling bands')
    parser.add_argument('--record-ink-support', action='store_true',
                        help='Record actual nonwhite copy pixels for independent recognition-region support diagnostics')
    parser.add_argument('--orientation-report',
                        help='Source-bound uncorrected character quadrilaterals for candidate glyph orientation normalization')
    args = parser.parse_args()
    result = prepare(args.asset_directory, args.pdf, args.output, profile=args.profile,
                     core_size=args.core_size, halo=args.halo, retain_window=args.retain_window,
                     periodic_partition=args.periodic_partition, complete_ruling_bands=args.complete_ruling_bands,
                     record_ink_support=args.record_ink_support, orientation_report=args.orientation_report)
    print(json.dumps({'parts': len(result['parts']), 'tiles': len(result['tiles']),
                      'prepared': sum(t['status'] == 'prepared_unclassified' for t in result['tiles']),
                      'coverage_verified': False}))


if __name__ == '__main__':
    main()

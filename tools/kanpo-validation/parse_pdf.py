#!/usr/bin/env python3
"""Run native parsing, opt-in paint readings and intrinsic image decoding.

Produces an image-aware document, source payloads, decoded PNG/placement receipts
and unresolved observations. This is not a whole-document accuracy certificate.
The PDF and executable bytes are captured once. No manuscript text is executed.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import sys

from PIL import Image
from read_painted_compounds import run as read_paint


def sha(data):
    return hashlib.sha256(data).hexdigest()


def local(directory, name):
    if not isinstance(name, str) or Path(name).name != name or name in ('', '.', '..'):
        raise ValueError('decoder file is not a local receipt member')
    return directory / name


def bind_decoded(analysis, decoded, directory, document_sha256):
    if (decoded.get('schema') != 1 or decoded.get('kind') != 'intrinsic_pdf_image_sources'
            or decoded.get('document_sha256') != document_sha256):
        raise ValueError('decoded image index belongs to another PDF')
    rows = {}
    for row in decoded['rows']:
        key = row['page'], row['draw']
        if key in rows:
            raise ValueError('duplicate decoded image draw')
        rows[key] = row
    keys = [(asset['page'], asset['draw']) for asset in analysis['assets']]
    if len(set(keys)) != len(keys) or set(keys) != set(rows):
        raise ValueError('native/parser/decoder draw inventories differ')
    result = []
    for asset in analysis['assets']:
        if (asset['document_sha256'] != document_sha256
                or sha(bytes(asset['encoded_bytes'])) != asset['encoded_sha256']):
            raise ValueError('parser image payload differs from the original source identity')
        row = rows[asset['page'], asset['draw']]
        if row['status'] != 'decoded':
            result.append({'asset': asset['id'], 'status': 'failed', 'error': row.get('error')})
            continue
        receipt_path = local(directory, row['receipt'])
        receipt_bytes = receipt_path.read_bytes()
        if sha(receipt_bytes) != row['receipt_sha256']:
            raise ValueError('decoded image receipt changed')
        receipt = json.loads(receipt_bytes)
        source = receipt['source_draw']
        if (receipt['document_sha256'] != document_sha256
                or (receipt['page'], receipt['draw']) != (asset['page'], asset['draw'])
                or source['encoded_sha256'] != asset['encoded_sha256']
                or source['encoded_bytes'] != len(asset['encoded_bytes'])):
            raise ValueError('decoded image receipt differs from the parser source')
        for key in ('object_ref', 'form_path', 'pixel_width', 'pixel_height', 'image_to_user',
                    'source_geometry_complete', 'declared_dictionary', 'clips', 'graphics_state_names_hex'):
            if asset['source'][key] != source[key]:
                raise ValueError('native image declaration differs: ' + key)
        png = local(directory, row['PNG'])
        if sha(png.read_bytes()) != receipt['PNG_sha256']:
            raise ValueError('decoded PNG changed')
        placement = local(directory, row['placement'])
        if sha(placement.read_bytes()) != receipt['placement_sha256']:
            raise ValueError('decoded placement changed')
        frame = json.loads(placement.read_bytes())
        if frame['crop_box'] != asset['crop_box'] or frame['rotate'] != asset['rotate']:
            raise ValueError('decoded page geometry differs from parser source')
        with Image.open(png) as image:
            rgba = image.convert('RGBA')
            if (list(image.size) != [frame['pixel_width'], frame['pixel_height']]
                    or sha(rgba.tobytes()) != receipt['RGBA_pixels_sha256']):
                raise ValueError('decoded pixel canvas differs from receipt')
        binding={'asset': asset['id'], 'status': 'decoded',
                       'PNG': png.name, 'placement': placement.name, 'receipt': receipt_path.name,
                       'PNG_sha256': receipt['PNG_sha256'], 'visibility': receipt['visibility']}
        raster=asset.get('raster_layout')
        if raster is not None:
            original=raster.get('source',{})
            if (raster.get('schema')!=1 or raster.get('kind')!='source_derived_raster_layout'
                    or raster.get('coverage_verified') is not False
                    or any(original.get(k)!=asset[k] for k in ('document_sha256','encoded_sha256','page','draw'))
                    or original.get('source_PNG_sha256')!=receipt['PNG_sha256']
                    or original.get('source_RGBA_sha256')!=receipt['RGBA_pixels_sha256']
                    or raster.get('source_frame')!=frame
                    or raster.get('canvas')!={'pixel_width':frame['pixel_width'],'pixel_height':frame['pixel_height']}):
                raise ValueError('editable raster layout differs from independently decoded original pixels or placement')
            binding['raster_layout_source']='original_decoded_pixels_and_placement_checked_text_unverified'
            characters=raster.get('character_layer')
            if characters is not None and (characters.get('kind')!='source_derived_raster_characters'
                    or characters.get('schema')!=1 or characters.get('coverage_verified') is not False
                    or characters.get('source')!=original or characters.get('canvas')!=raster['canvas']
                    or characters.get('recognition_parent_sha256')!=raster.get('provenance',{}).get('recognition_parent_sha256')):
                raise ValueError('character candidates differ from independently decoded raster source')
        result.append(binding)
    return result


def bind_raster_grids(analysis, decoded, directory, output, document_sha256, observer):
    """Retain raw raster observations and separately bind each crop to its PDF draw."""
    bindings = bind_decoded(analysis, decoded, directory, document_sha256)
    asset_by_id = {asset['id']: asset for asset in analysis['assets']}
    if len(asset_by_id) != len(analysis['assets']):
        raise ValueError('Duplicate parser image asset identity')
    output.mkdir()
    rows = []
    for binding in bindings:
        if binding['status'] != 'decoded':
            rows.append(binding)
            continue
        asset = asset_by_id[binding['asset']]
        prefix = f"asset-{asset['id']:04}"
        observation = observer.write_observation(local(directory, binding['PNG']), output / prefix)
        if observation['source']['PNG_sha256'] != binding['PNG_sha256']:
            raise ValueError('Raster observation source changed after PDF binding')
        # Revalidate the original receipt and payload after the observer read.
        if bindings != bind_decoded(analysis, decoded, directory, document_sha256):
            raise ValueError('Raster source inventory changed during observation')
        receipt_bytes = local(directory, binding['receipt']).read_bytes()
        receipt = json.loads(receipt_bytes)
        placement_bytes = local(directory, binding['placement']).read_bytes()
        frame = json.loads(placement_bytes)
        snapshot = output / prefix
        (snapshot / 'source-receipt.json').write_bytes(receipt_bytes)
        (snapshot / 'source-placement.json').write_bytes(placement_bytes)
        raw_observation = (snapshot / 'observation.json').read_bytes()
        cells = []
        for cell in observation['rectangular_cells']:
            crop = cell['crop']
            crop_bytes = local(snapshot, crop['PNG']).read_bytes()
            if sha(crop_bytes) != crop['PNG_sha256']:
                raise ValueError('Raster cell PNG changed')
            crop_placement = observer.crop_frame(frame, cell['interior_pixels'])
            name = f"cell-{cell['id']:04}.placement.json"
            raw_frame = (json.dumps(crop_placement, ensure_ascii=False, separators=(',', ':')) + '\n').encode()
            (snapshot / name).write_bytes(raw_frame)
            cells.append({'cell': cell['id'], 'source_interior_pixels': cell['interior_pixels'],
                          'PNG': crop['PNG'], 'PNG_sha256': crop['PNG_sha256'],
                          'placement': name, 'placement_sha256': sha(raw_frame)})
        source = {'document_sha256': document_sha256, 'asset': asset['id'],
                  'page': asset['page'], 'draw': asset['draw'],
                  'encoded_sha256': asset['encoded_sha256'],
                  'source_receipt': 'source-receipt.json', 'source_receipt_sha256': sha(receipt_bytes),
                  'source_placement': 'source-placement.json', 'source_placement_sha256': sha(placement_bytes),
                  'source_PNG_sha256': binding['PNG_sha256'],
                  'source_RGBA_sha256': receipt['RGBA_pixels_sha256']}
        if observation['source']['RGBA_sha256'] != source['source_RGBA_sha256']:
            raise ValueError('Raster source RGBA pixels differ from decoded receipt')
        bound = {'schema': 1, 'kind': 'unclassified_pdf_raster_ruling_faces', 'source': source,
                 'observation': 'observation.json', 'observation_sha256': sha(raw_observation),
                 'cells': cells, 'source_binding': 'producer_hashes_checked_visibility_unverified',
                 'visibility': 'unverified', 'article_assignment': None,
                 'native_glyph_binding': None, 'coverage_verified': False}
        (snapshot / 'pdf-binding.json').write_text(json.dumps(bound, ensure_ascii=False, indent=2) + '\n')
        rows.append({'asset': asset['id'], 'page': asset['page'], 'draw': asset['draw'],
                     'status': 'ruling_candidates_observed', 'directory': prefix,
                     'binding': prefix + '/pdf-binding.json',
                     'rectangular_cells': len(cells), 'unresolved': observation['unresolved']})
    result = {'schema': 1, 'kind': 'unclassified_pdf_raster_grids',
              'document_sha256': document_sha256, 'rows': rows,
              'coverage_verified': False, 'visibility': 'unverified'}
    (output / 'index.json').write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n')
    return result


def run(pdf, parser, native_extractor, image_extractor, output, native_glyph_engine='none', raster_grid=False,
        raster_layouts=()):
    raw = pdf.read_bytes()
    script_bytes = Path(__file__).read_bytes()
    decoder_path = Path(__file__).parents[1] / 'kanpo-ocr/decode_sources.py'
    decoder_bytes = decoder_path.read_bytes()
    glyph_reader_path=Path(__file__).with_name('read_native_glyphs.py')
    glyph_reader_bytes=glyph_reader_path.read_bytes()
    glyph_swift_bytes=Path(__file__).with_name('recognize_glyphs.swift').read_bytes()
    grid_path = Path(__file__).parents[1] / 'kanpo-ocr/raster_grid.py'
    grid_bytes = grid_path.read_bytes() if raster_grid else None
    if native_glyph_engine is None:
        native_glyph_engine='none'
    binaries = {name: path.read_bytes() for name, path in
                (('parser', parser), ('native-extractor', native_extractor), ('image-extractor', image_extractor))}
    raster_inputs=[Path(path).read_bytes() for path in raster_layouts]
    for data in raster_inputs:
        report=json.loads(data)
        if report.get('kind')!='source_derived_raster_layout' or report.get('source',{}).get('document_sha256')!=sha(raw):
            raise ValueError('raster layout report belongs to another original PDF')
    output.mkdir(parents=True, exist_ok=False)
    implementation = output / 'implementation'
    implementation.mkdir()
    (implementation / 'parse_pdf.py').write_bytes(script_bytes)
    decoder_snapshot = implementation / 'decode_sources.py'
    decoder_snapshot.write_bytes(decoder_bytes)
    decoder_snapshot.chmod(0o400)
    glyph_reader_snapshot=implementation/'read_native_glyphs.py'
    glyph_reader_snapshot.write_bytes(glyph_reader_bytes)
    glyph_reader_snapshot.chmod(0o400)
    (implementation/'recognize_glyphs.swift').write_bytes(glyph_swift_bytes)
    (implementation/'recognize_glyphs.swift').chmod(0o400)
    if grid_bytes is not None:
        (implementation / 'raster_grid.py').write_bytes(grid_bytes)
        (implementation / 'raster_grid.py').chmod(0o400)
    original = output / 'source.pdf'
    original.write_bytes(raw)
    original.chmod(0o400)
    raster_arguments=[]
    if raster_inputs:
        reports_directory=output/'raster-inputs';reports_directory.mkdir()
        for i,data in enumerate(raster_inputs):
            path=reports_directory/f'layout-{i:04}.json';path.write_bytes(data);path.chmod(0o400)
            raster_arguments+=['--raster-layout',str(path.resolve())]
    try:
        spec = importlib.util.spec_from_file_location('kanpo_intrinsic_decoder', decoder_snapshot)
        decoder = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(decoder)
        with tempfile.TemporaryDirectory(prefix='kanpo-parse-') as temporary:
            executables = {}
            for name, data in binaries.items():
                path = Path(temporary) / name
                path.write_bytes(data)
                path.chmod(0o500)
                executables[name] = path
            read_paint(original, executables['native-extractor'], output / 'paint')
            with (output / 'parser.stdout').open('xb') as stdout, (output / 'parser.stderr').open('xb') as stderr:
                subprocess.run([str(executables['parser']), *raster_arguments, str(original.resolve()),
                                str((output / 'paint/observations.json').resolve()),
                                str((output / 'parse').resolve())], check=True, stdout=stdout, stderr=stderr)
            analysis = json.loads((output / 'parse/analysis.json').read_bytes())
            if analysis.get('schema') != 1 or analysis.get('kind') != 'source_media_analysis':
                raise ValueError('unsupported image-aware parser output')
            glyph_report=None
            if native_glyph_engine!='none' and analysis.get('native_glyphs',{}).get('candidates'):
                spec=importlib.util.spec_from_file_location('kanpo_native_glyph_reader',glyph_reader_snapshot)
                reader=importlib.util.module_from_spec(spec)
                spec.loader.exec_module(reader)
                glyph_report=reader.run(original,output/'paint/native-pages.json',output/'parse/analysis.json',output/'native-glyphs',native_glyph_engine)
                if glyph_report['observations']:
                    combined=json.loads((output/'paint/observations.json').read_bytes())
                    combined['native_glyph_observations']=glyph_report['observations']
                    combined['native_glyph_report']=glyph_report
                    combined_path=output/'combined-observations.json'
                    combined_path.write_text(json.dumps(combined,ensure_ascii=False)+'\n')
                    (output/'parse').rename(output/'parse-native')
                    for suffix in ('stdout','stderr'):
                        (output/f'parser.{suffix}').rename(output/f'parser-native.{suffix}')
                    with (output/'parser.stdout').open('xb')as stdout,(output/'parser.stderr').open('xb')as stderr:
                        subprocess.run([str(executables['parser']),*raster_arguments,str(original.resolve()),str(combined_path.resolve()),str((output/'parse').resolve())],check=True,stdout=stdout,stderr=stderr)
                    analysis=json.loads((output/'parse/analysis.json').read_bytes())
                    if analysis.get('schema')!=1 or analysis.get('kind')!='source_media_analysis':
                        raise ValueError('unsupported image-aware glyph parser output')
            decoded = decoder.extract(original, executables['image-extractor'], output / 'images')
            bindings = bind_decoded(analysis, decoded, output / 'images', sha(raw))
            if raster_grid:
                spec = importlib.util.spec_from_file_location('kanpo_raster_grid', implementation / 'raster_grid.py')
                observer = importlib.util.module_from_spec(spec)
                spec.loader.exec_module(observer)
                analysis['raster_grids'] = bind_raster_grids(analysis, decoded, output / 'images',
                                                            output / 'raster-grids', sha(raw), observer)
        if original.read_bytes() != raw:
            raise ValueError('captured PDF changed during parsing')
        analysis['decoded_images'] = bindings
        analysis['runtime'] = {'document_sha256': sha(raw), 'document_bytes': len(raw),
                               'executables_sha256': {name: sha(data) for name, data in binaries.items()},
                               'script_sha256': sha(script_bytes),
                               'decoder_sha256': sha(decoder_bytes),
                               'native_glyph_reader_sha256':sha(glyph_reader_bytes),
                               'native_glyph_swift_sha256':sha(glyph_swift_bytes),
                               'native_glyph_engine':native_glyph_engine,
                               'decoded_image_directory': 'images', 'coverage_verified': False}
        if raster_grid:
            analysis['runtime']['raster_grid_sha256'] = sha(grid_bytes)
            analysis['runtime']['raster_grid_directory'] = 'raster-grids'
        if raster_inputs:
            analysis['runtime']['raster_layouts_sha256']=[sha(data) for data in raster_inputs]
        (output / 'document.json').write_text(json.dumps(analysis, ensure_ascii=False, separators=(',', ':')) + '\n')
        decoded_count = sum(row['status'] == 'decoded' for row in bindings)
        glyph_audit=analysis.get('native_glyphs',{})
        recognized_glyphs={(b['source']['page'],b['source']['glyph'])for b in glyph_audit.get('bindings',[])}
        pending_glyphs=sum((c['page'],c['glyph'])not in recognized_glyphs for c in glyph_audit.get('candidates',[]))
        unresolved_scalars=sum(issue['remaining_scalars']for issue in glyph_audit.get('source_issues',[]))
        raster_audits=[a['raster_layout'] for a in analysis['assets'] if a.get('raster_layout') is not None]
        raster_failed=sum(len(r['failed_tiles']) for r in raster_audits)
        raster_counts=({'raster_layouts':len(raster_audits),'raster_word_candidates':sum(len(r['words']) for r in raster_audits),
                        'unresolved_raster_layout':sum(len(r['unresolved']) for r in raster_audits),
                        'failed_raster_tiles':raster_failed} if raster_audits else {})
        character_layers=[r['character_layer'] for r in raster_audits if r.get('character_layer') is not None]
        if character_layers:
            raster_counts.update(raster_character_observations=sum(len(c['characters']) for c in character_layers),
                                 raster_character_fragments=sum(len(c['fragments']) for c in character_layers),
                                 unresolved_raster_characters=sum(len(c['unresolved']) for c in character_layers))
        (output / 'terminal.json').write_text(json.dumps({'status': 'finished_with_audit' if decoded_count == len(bindings) and not raster_failed else 'partial',
            'placed_images': len(analysis['placements']),
            'page_images': len(analysis.get('page_placements',[])), 'decoded_images': decoded_count,
            'recognized_native_glyphs':len(recognized_glyphs),'unresolved_native_glyphs':pending_glyphs,
            'unresolved_text_scalars':unresolved_scalars,
            'framed_enclosures':len(analysis.get('frames',{}).get('bindings',[])),
            'unresolved_frames':len(analysis.get('frames',{}).get('unresolved',[])),
            'baseline_shift_runs':len(analysis.get('baseline_shifts',{}).get('bindings',[])),
            'unresolved_baseline_shifts':len(analysis.get('baseline_shifts',{}).get('unresolved',[])),
            'unresolved': analysis['unresolved'], **raster_counts, 'coverage_verified': False}, ensure_ascii=False, indent=2) + '\n')
        return analysis
    except Exception as error:
        (output / 'terminal.json').write_text(json.dumps({'status': 'failed', 'error': str(error),
            'coverage_verified': False}, ensure_ascii=False, indent=2) + '\n')
        raise


def main():
    args = argparse.ArgumentParser(description=__doc__)
    args.add_argument('pdf', type=Path)
    args.add_argument('--parser', required=True, type=Path)
    args.add_argument('--native-extractor', required=True, type=Path)
    args.add_argument('--image-extractor', required=True, type=Path)
    args.add_argument('--output', required=True, type=Path)
    args.add_argument('--native-glyph-engine',choices=('vision','none'),default='none',
                      help='Isolated unknown-glyph recognition; disabled by default, Vision is opt-in on macOS')
    args.add_argument('--raster-grid', action='store_true',
                      help='Observe source raster rulings and bounded cell candidates with original PDF/crop placement receipts')
    args.add_argument('--raster-layout',action='append',default=[],type=Path,
                      help='Attach source-bound positioned OCR candidates; repeat for multiple image draws')
    options = args.parse_args()
    result = run(options.pdf, options.parser, options.native_extractor, options.image_extractor, options.output,options.native_glyph_engine,options.raster_grid,options.raster_layout)
    print(json.dumps({'assets': len(result['assets']), 'placed': len(result['placements']),
                      'decoded': sum(r['status'] == 'decoded' for r in result['decoded_images']),
                      'unresolved': result['unresolved']}, ensure_ascii=False))


if __name__ == '__main__':
    main()

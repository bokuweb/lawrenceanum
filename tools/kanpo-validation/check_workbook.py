#!/usr/bin/env python3
"""Run the complete attached PDF fixture set with model OCR disabled."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

from parse_pdf import run
from verify_documents import verify as verify_document
from verify_native_paint import verify as verify_paint


def check(fixtures, binaries, output):
    output.mkdir(parents=True, exist_ok=False)
    index = json.loads((fixtures / 'index.json').read_bytes())
    declared = {s['fixture']: s['sha256'] for c in index['cases'] for s in c['sources']
                if s['kind'] == 'pdf'}
    declared.update({s['fixture']: s['sha256'] for s in
                     json.loads((fixtures / 'supplement-sources.json').read_bytes())['sources']})
    assert set(declared) == {p.name for p in fixtures.glob('case-*.pdf')}

    def one(entry):
        filename, digest = entry
        pdf = fixtures / filename
        assert hashlib.sha256(pdf.read_bytes()).hexdigest() == digest
        target = output / pdf.stem
        model = run(pdf, binaries / 'analyze_media_pdf', binaries / 'native_pages',
                    binaries / 'image_sources', target, native_glyph_engine='none')
        document = verify_document(model)
        paint = verify_paint((target / 'source.pdf').read_bytes(), model, scale=2)
        assert paint['all_pixels_equal'], filename
        assert model['runtime']['document_sha256'] == digest
        (target / 'document-verification.json').write_text(json.dumps(document, indent=2) + '\n')
        (target / 'vector-paint-verification.json').write_text(json.dumps(paint, indent=2) + '\n')
        row = {'input': filename, 'source_sha256': digest, **document,
               'vector_paths': sum(p['paths'] for p in paint['pages']),
               'font_rectangles': sum(len(p['font_rectangles']) for p in model['page_graphics']),
               'all_vector_pixels_equal': True}
        print(json.dumps(row), flush=True)
        return row

    # PDFium rendering is kept sequential in this process.
    rows = [one(entry) for entry in sorted(declared.items())]
    result = {'scope': 'attachment_non_OCR_source_conservation', 'model_OCR_enabled': False,
              'inputs': len(rows), 'pages': sum(r['pages'] for r in rows), 'rows': rows,
              'all_checks_passed': True}
    assert result['inputs'] == 21 and result['pages'] == 185
    (output / 'verification.json').write_text(json.dumps(result, indent=2) + '\n')
    return result


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument('--fixtures', type=Path, default=Path('crates/kanpo-parser/tests/data/workbook'))
    cli.add_argument('--bin-dir', type=Path, default=Path('target/debug/examples'))
    cli.add_argument('--output', type=Path, required=True)
    args = cli.parse_args()
    result = check(args.fixtures.resolve(), args.bin_dir.resolve(), args.output.resolve())
    print(json.dumps({k: result[k] for k in ('inputs', 'pages', 'all_checks_passed', 'model_OCR_enabled')}))


if __name__ == '__main__':
    main()

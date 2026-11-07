#!/usr/bin/env python3
"""Build a genuine, offline comparison of original PDFs and kanpo-docx output."""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import unicodedata
from datetime import datetime
from zoneinfo import ZoneInfo
from collections import Counter
from zipfile import ZipFile
from xml.etree import ElementTree as ET

ROOT = Path(__file__).resolve().parents[2]
RUNTIME = Path('/Users/bokuweb/.cache/codex-runtimes/codex-primary-runtime/dependencies')
PYTHON = RUNTIME / 'python/bin/python3'
RENDER = Path('/Users/bokuweb/.codex/plugins/cache/openai-primary-runtime/documents/26.1026.10807/skills/documents/render_docx.py')
NDL = ROOT.parent / 'ndlocr-lite-rs'

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def encoded(path):
    return base64.b64encode(path.read_bytes()).decode('ascii')

def run(cmd, log):
    with log.open('w') as stream:
        subprocess.run([str(x) for x in cmd], stdout=stream, stderr=subprocess.STDOUT, check=True)

def docx_contents(path):
    with ZipFile(path) as z:
        xml = ET.fromstring(z.read('word/document.xml'))
    w = '{http://schemas.openxmlformats.org/wordprocessingml/2006/main}'
    text = ''.join(x.text or '' for x in xml.iter(w + 't'))
    underlined = double_underlined = 0
    for run in xml.iter(w + 'r'):
        mark = run.find(w + 'rPr/' + w + 'u')
        if mark is not None and mark.get(w + 'val') != 'none':
            count = sum(not c.isspace() for t in run.iter(w + 't') for c in (t.text or ''))
            underlined += count
            if mark.get(w + 'val') == 'double':
                double_underlined += count
    annotations = dict(ruby_annotations=sum(1 for _ in xml.iter(w + 'ruby')),
                       dashed_cell_borders=sum(1 for borders in xml.iter(w + 'tcBorders') for edge in borders if edge.get(w + 'val') == 'dashed'),
                       superscript_characters=0, subscript_characters=0, boxed_characters=0)
    for run in xml.iter(w + 'r'):
        count = sum(not c.isspace() for t in run.findall(w + 't') for c in (t.text or ''))
        align = run.find(w + 'rPr/' + w + 'vertAlign')
        if align is not None and align.get(w + 'val') in ('superscript', 'subscript'):
            annotations[align.get(w + 'val') + '_characters'] += count
        if run.find(w + 'rPr/' + w + 'bdr') is not None:
            annotations['boxed_characters'] += count
    return text, underlined, double_underlined, annotations

def main():
    from pypdf import PdfReader
    import openpyxl
    p = argparse.ArgumentParser()
    p.add_argument('--output', type=Path, required=True)
    args = p.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    audit = out.parent / 'report-build-audit'
    audit.mkdir(exist_ok=True)
    fontconfig = audit / 'fonts.conf'
    fontconfig.write_text('''<?xml version="1.0"?><!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd"><fontconfig><dir>/System/Library/Fonts</dir><dir>/System/Library/Fonts/Supplemental</dir><cachedir>/tmp/kanpo-report-fontcache</cachedir><alias><family>ＭＳ 明朝</family><prefer><family>Hiragino Mincho ProN</family></prefer></alias><alias><family>ＭＳ ゴシック</family><prefer><family>Hiragino Sans</family></prefer></alias><alias><family>sans-serif</family><prefer><family>Hiragino Sans</family></prefer></alias><alias><family>serif</family><prefer><family>Hiragino Mincho ProN</family></prefer></alias></fontconfig>''')
    os.environ['FONTCONFIG_FILE'] = str(fontconfig)
    for folder in ('assets', 'data', 'files', 'licenses'):
        (out / folder).mkdir(exist_ok=True)
    fixture = ROOT / 'crates/kanpo-parser/tests/data/workbook'
    index = json.loads((fixture / 'index.json').read_bytes())
    workbook = Path('/Users/bokuweb/Downloads/官報PDFの自動Word化に向けたテスト原稿.xlsx')
    assert sha(workbook) == index['workbook_sha256']
    sheet = openpyxl.load_workbook(workbook, read_only=True, data_only=True).active
    entries = []
    for case in index['cases']:
        n, m = case['number'], case['metadata']
        assert sheet.cell(case['row'], 7).value == m['G']
        source = next(s for s in case['sources'] if s['kind'] == 'pdf')
        entries.append(dict(id=f'case-{n:02}', number=n, title=m['G'], label=f'No.{n}　{m["G"]}',
            issue=f'{m.get("C", "")} {m.get("D", "")}第{m.get("E", "").removesuffix(".0")}号',
            notice=m.get('H', ''), note='', filename=source['fixture'], source_sha256=source['sha256']))
        if n == 10:
            entries[-1].update(title='医療施設調査に関する告示（Excel No.10のリンク先）',
                label='No.10 リンク先　医療施設調査に関する告示',
                issue='令和8年5月21日', notice='厚生労働省告示第224号',
                note='Excelに記載された法令名とリンク先PDFが異なるため、両方を収録しています。')
            supplements = json.loads((fixture / 'supplement-sources.json').read_bytes())['sources']
            for i, s in enumerate(supplements, 1):
                entries.append(dict(id=f'case-10-listed-{i}', number=10, title=m['G'],
                    label=f'No.10 記載法令 {i}/3　{m["G"]}', issue=s['issue'], notice=m.get('H', ''),
                    note=f'Excel No.10の記載法令。原本の印刷ページ {i}（全3ページのうち）。',
                    filename=s['fixture'], source_sha256=s['sha256']))
    binary = ROOT / 'target/release/kanpo-docx'
    engine = NDL / 'target/release/ndlocr-lite-rs'
    assert binary.is_file() and engine.is_file()
    created = datetime.now(ZoneInfo('Asia/Tokyo')).date()
    manifest = dict(created=created.isoformat(), converter_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        converter_sha256=sha(binary), ocr_engine='NDLOCR Lite Rust ONNX', ocr_binary_sha256=sha(engine),
        models={f.name: sha(f) for f in (NDL / 'models').iterdir() if f.suffix in ('.onnx', '.yaml')},
        workbook_sha256=sha(workbook), preview_font='MS Mincho and MS Gothic mapped by fontconfig to Hiragino Mincho ProN and Hiragino Sans for preview only',
        preview_font_sha256=sha(Path('/System/Library/Fonts/ヒラギノ明朝 ProN.ttc')), entries=entries)
    preview_fonts = [Path('/System/Library/Fonts/ヒラギノ明朝 ProN.ttc')]
    preview_fonts += sorted(p for p in Path('/System/Library/Fonts').iterdir() if unicodedata.normalize('NFC',p.name).startswith('ヒラギノ角ゴシック W'))
    manifest['preview_font_files_sha256'] = {p.name:sha(p) for p in preview_fonts}
    manifest['preview_fontconfig_sha256'] = sha(fontconfig)
    manifest['preview_font_sha256'] = hashlib.sha256(json.dumps([manifest['preview_font_files_sha256'],manifest['preview_fontconfig_sha256']],sort_keys=True).encode()).hexdigest()
    manifest['converter_source_dirty'] = bool(subprocess.check_output(['git','status','--porcelain','--','crates/kanpo-docx','crates/kanpo-parser','crates/kanpo-pdf'],cwd=ROOT,text=True).strip())
    source_files = [ROOT / 'Cargo.lock']
    for crate in ('kanpo-docx', 'kanpo-docx-wasm', 'kanpo-parser', 'kanpo-pdf'):
        folder = ROOT / 'crates' / crate
        source_files += [folder / 'Cargo.toml', *sorted((folder / 'src').rglob('*.rs'))]
    manifest['converter_source_files_sha256'] = {str(p.relative_to(ROOT)): sha(p) for p in source_files}
    manifest['underline_policy'] = 'Native CFF source decorations transferred only across complete unchanged field streams'
    manifest['annotation_policy'] = 'Source-certified ruby, baseline shifts and hollow text frames bound by complete field streams and original scalar positions; raw drawing glyphs remain auditable in the native parser'
    manifest['pagination_policy'] = 'Comparison labels and the last direct paragraph of each ordinary opening-row cell keep with the following row, without adding a repeating-header role or forbidding long body row splits; source-certified nested compound headers retain a separate outer table and required page break; physical image-table canvases remain unchanged'
    manifest['font_style_policy'] = 'Native Mincho/Gothic family and explicit Bold/Regular/Light/Medium names attached by complete field scalar positions (or the complete native trace field as an exact leading prefix when OCR appends articles) and uniform original glyph identity; mixed-family size scopes split only at certified original word positions, and mixed-family words only when complete raw/normalized glyph reconstruction agrees; Gothic and opaque W-numbers are not treated as bold evidence'
    manifest['cell_alignment_policy'] = 'Exact complete source-table text/merges and original glyph bounds; every physical line must share an unambiguous left/center/right anchor with spare space; filled lines, conflicting sources, mixed anchors and physical image-table canvases remain unspecified'
    manifest['font_policy'] = 'Effective native glyph sizes from original text/page transforms; original word occurrence and complete field scalar bindings; ruby base/reading/raise and physical image-table canvas scale preserved; predominant certified source size is the document fallback; OCR has no inferred glyph metrics'
    manifest['cell_inset_policy'] = 'Positive original leading glyph bearing mapped to a symmetric Word cell inset only for a uniquely matched source table; hanging end punctuation does not erase the leading inset'
    manifest['ruling_policy'] = 'Black opaque native dashed strokes cover complete existing cell edges; source field, full table text/merges and unique continuation submatrix must agree; no new grid or missing frame side is inferred'
    previous = json.loads((out / 'manifest.json').read_text()) if (out / 'manifest.json').exists() else {}
    converter_changed = any(previous.get(k) != manifest.get(k) for k in ('converter_sha256','ocr_binary_sha256','models'))
    frozen_binary = audit / ('kanpo-docx-' + manifest['converter_sha256'][:16])
    if not frozen_binary.exists():
        shutil.copy2(binary, frozen_binary)
    assert sha(frozen_binary) == manifest['converter_sha256']
    binary = frozen_binary
    for entry in entries:
        id = entry['id']
        folder = out / 'files' / id
        folder.mkdir(exist_ok=True)
        src = fixture / entry['filename']
        assert sha(src) == entry['source_sha256']
        original = folder / 'original.pdf'
        shutil.copyfile(src, original)
        entry['source_pages'] = len(PdfReader(original).pages)
        entry['variants'] = {}
        for mode in ('off', 'on'):
            docx = folder / f'ocr-{mode}.docx'
            log = audit / f'{id}-{mode}-convert.log'
            if not docx.exists() or converter_changed:
                cmd = [binary, original, '-o', docx]
                if mode == 'off':
                    cmd += ['--no-ocr']
                else:
                    cmd += ['--ndlocr', engine, '--models', NDL / 'models']
                print(f'CONVERT {id} OCR={mode}', flush=True)
                run(cmd, log)
            render_dir = audit / f'{id}-{mode}-render-{sha(docx)[:12]}-{manifest["preview_font_sha256"][:8]}'
            render_pdf = render_dir / f'ocr-{mode}.pdf'
            if not render_pdf.exists():
                print(f'RENDER {id} OCR={mode}', flush=True)
                run([PYTHON, RENDER, docx, '--output_dir', render_dir, '--emit_pdf', '--dpi', '70'],
                    audit / f'{id}-{mode}-render.log')
            pdf = folder / f'ocr-{mode}.pdf'
            shutil.copyfile(render_pdf, pdf)
            source_text, underlined_characters, double_underlined_characters, annotations = docx_contents(docx)
            rendered_text = ''.join(page.extract_text() for page in PdfReader(pdf).pages)
            def characters(text):
                # PDF font mappings may choose either Unicode wave-dash form.
                return Counter(c for c in unicodedata.normalize('NFKC', text).replace('〜','~') if not c.isspace())
            missing = characters(source_text) - characters(rendered_text)
            entry['variants'][mode] = dict(pages=len(PdfReader(pdf).pages), text_characters=len(source_text),
                underlined_characters=underlined_characters,
                double_underlined_characters=double_underlined_characters,
                source_annotations=annotations,
                docx_sha256=sha(docx), pdf_sha256=sha(pdf),
                preview_text_audit={'missing_character_count':missing.total(), 'kind':'normalized_character_multiset_only_not_reading_order'},
                ocr_pages=int(re.search(r'\((\d+) page\(s\) with OCR\)', log.read_text()).group(1)))
            if missing:
                entry['note'] = 'PDFプレビューの抽出文字とDOCXの文字に差があります。表の表示などは、同梱DOCXも併せてご確認ください。' + entry['note']
        entry['same_text_count'] = entry['variants']['off']['text_characters'] == entry['variants']['on']['text_characters']
        payload = {'original': encoded(original)}
        for mode in ('off', 'on'):
            payload[mode] = {ext: encoded(folder / f'ocr-{mode}.{ext}') for ext in ('pdf', 'docx')}
        (out / 'data' / f'{id}.js').write_text('window.KANPO_DATA[' + json.dumps(id) + ']=' + json.dumps(payload) + ';\n')
        print(f'DONE {id}: {entry["source_pages"]} original, {entry["variants"]}', flush=True)
        (audit / 'manifest-progress.json').write_text(json.dumps(manifest, ensure_ascii=False, indent=2))
    (out / 'manifest.json').write_text(json.dumps(manifest, ensure_ascii=False, indent=2))
    (out / 'assets/manifest.js').write_text('window.KANPO_MANIFEST=' + json.dumps(manifest, ensure_ascii=False) + ';\n')
    pdfjs = RUNTIME / 'node/node_modules/pdfjs-dist'
    for source, target in [('pdf.min.mjs', 'pdf.js'), ('pdf.worker.min.mjs', 'pdf.worker.js')]:
        code = (pdfjs / 'build' / source).read_text()
        code, n = re.subn(r'export\{[^}]+\};\s*$', '', code)
        assert n == 1
        code = code.replace('import.meta.url', 'globalThis.location.href')
        (out / 'assets' / target).write_text('(function(){\n' + code + '\n})();\n')
    resources = {}
    for kind, subdir in [('cMapUrl', 'cmaps'), ('standardFontDataUrl', 'standard_fonts'), ('wasmUrl', 'wasm')]:
        resources[kind] = {f.name: encoded(f) for f in (pdfjs / subdir).iterdir()
                           if f.is_file() and f.suffix in ('.bcmap', '.pfb', '.ttf', '.wasm')}
    (out / 'assets/pdf-resources.js').write_text('window.PDF_RESOURCES=' + json.dumps(resources) + ';\n')
    shutil.copyfile(pdfjs / 'LICENSE', out / 'licenses/PDFJS-LICENSE.txt')
    shutil.copyfile(ROOT / 'LICENSE', out / 'licenses/KANPO-LICENSE.txt')
    for name in ('index.html', 'report.css', 'report.js', 'README.txt'):
        content = (Path(__file__).parent / name).read_text()
        content = content.replace('{{CREATED_DOT}}', created.strftime('%Y.%m.%d'))
        content = content.replace('{{CREATED_JA}}', f'{created.year}年{created.month}月{created.day}日')
        (out / name).write_text(content)
    (out / 'licenses/NOTICE.txt').write_text('PDF.js 5.6.205 by Mozilla Foundation, Apache-2.0.\nThe self-contained ESM bundles are wrapped as classic scripts for file:// use; exports are removed and import.meta.url is replaced with location.href. No parsing or rendering logic is changed.\nPDF.js auxiliary CMaps, standard fonts and WASM are embedded as bytes.\nNDLOCR models and binaries are used only at build time and are not redistributed.\n')
    for path in (pdfjs / 'wasm').glob('LICENSE*'):
        shutil.copyfile(path, out / 'licenses' / path.name)
    for path in (pdfjs / 'standard_fonts').glob('LICENSE*'):
        shutil.copyfile(path, out / 'licenses' / path.name)
    print('COMPLETE ' + str(out), flush=True)

if __name__ == '__main__':
    main()

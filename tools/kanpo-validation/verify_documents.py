#!/usr/bin/env python3
"""Verify complete native-to-document streams by replaying source observations.

No reference manuscript or case-specific replacement is accepted. This checks
conservation and source/target correspondence; semantic layout decisions are
covered separately by source-reviewed parser regression tests.
"""
import argparse
from collections import defaultdict
import hashlib
import json
from pathlib import Path


def compact(text):
    return ''.join(c for c in text if not c.isspace())


def table(value):
    occupied = set()
    for merge in value.get('merges', []):
        assert merge['rowspan'] > 0 and merge['colspan'] > 0
        for row in range(merge['row'], merge['row'] + merge['rowspan']):
            for col in range(merge['col'], merge['col'] + merge['colspan']):
                assert 0 <= row < len(value['rows']) and 0 <= col < len(value['rows'][row])
                assert (row, col) not in occupied
                occupied.add((row, col))
    return ''.join(runs(cell) for row in value['rows'] for cell in row)


def runs(values):
    return ''.join(compact(v.get('text', '')) + (table(v['table']) if v.get('table') else '')
                   for v in values)


def fields(document):
    result = dict(paragraph='', after='', before='')
    for block in document['blocks']:
        if block['kind'] == 'paragraph':
            result['paragraph'] += runs(block['runs'])
        else:
            assert block['kind'] == 'shinkyu'
            for row in block['rows']:
                for field in ('after', 'before'):
                    result[field] += runs(row[field])
                    if row.get(field + '_table'):
                        result[field] += table(row[field + '_table'])
    return result


def image_refs(value):
    if isinstance(value, list):
        return [asset for child in value for asset in image_refs(child)]
    if isinstance(value, dict):
        if value.get('kind') == 'image':
            return [value['asset']]
        return [asset for child in value.values() for asset in image_refs(child)]
    return []


def verify(model):
    source_hash = model['runtime']['document_sha256']
    assert model['runtime']['native_glyph_engine'] == 'none'
    native = model['native']
    expected = [fields(a['document']) for a in native['articles']]
    actual = [fields(d) for d in model['documents']]
    assert len(actual) == len(expected)
    paint = model.get('painted_text')
    edits = defaultdict(list)
    if paint:
        grouped = defaultdict(list)
        for binding in paint['bindings']:
            grouped[binding['page'], binding['region'], binding['line']].append(binding)
        for (page, region, line), bindings in grouped.items():
            region = native['pages'][page]['regions'][region]
            field = region['role'] if region['role'] in ('after', 'before') else 'paragraph'
            line = region['lines'][line]
            original = line['text']
            data = original.encode()
            for binding in sorted(bindings, key=lambda b: b['byte_offset'], reverse=True):
                observation = paint['observation_report']['observations'][binding['observation']]
                assert observation['page'] == page
                assert binding['before_word'] in line['words'] and binding['after_word'] in line['words']
                offset = binding['byte_offset']
                data[:offset].decode()  # Must be a Unicode scalar boundary.
                data[offset:].decode()
                data = data[:offset] + observation['text'].encode() + data[offset:]
            source, target = compact(original), compact(data.decode())
            locations = [(i, text[field].find(source)) for i, text in enumerate(expected)
                         if text[field].count(source) == 1]
            assert len(locations) == 1, 'source paint line has no unique complete document correspondence'
            document, start = locations[0]
            edits[document, field].append((start, len(source), source, target))
        for (document, field), patches in edits.items():
            for start, length, source, target in sorted(patches, reverse=True):
                assert expected[document][field][start:start + length] == source
                expected[document][field] = (expected[document][field][:start] + target
                                             + expected[document][field][start + length:])
    removals = defaultdict(list)
    for binding in model['frames']['bindings']:
        removals[binding['document'], binding['field']].append(binding)
    for (document, field), bindings in removals.items():
        for b in sorted(bindings, key=lambda b: b['removed_offset'], reverse=True):
            offset, length = b['removed_offset'], b['removed_length']
            assert expected[document][field][offset:offset + length] == compact(b['removed_text'])
            expected[document][field] = expected[document][field][:offset] + expected[document][field][offset + length:]
    assert expected == actual, 'final document changes a complete native field without source evidence'
    words = 0
    for page in native['pages']:
        owned = []
        for region in page['regions']:
            for line in region['lines']:
                owned.extend(line['words'])
                if line.get('table'):
                    slots = set()
                    for cell in line['table']['cells']:
                        owned.extend(cell['words'])
                        for r in range(cell['row'], cell['row'] + cell['rowspan']):
                            for c in range(cell['col'], cell['col'] + cell['colspan']):
                                assert (r, c) not in slots
                                slots.add((r, c))
        assert len(owned) == len(set(owned))
        assert set(owned) == {w['id'] for w in page['words'] if w['status'] == 'used'}
        words += len(owned)
    for key, member in [('underlines', 'parallel_bindings'), ('ruby', 'bindings'),
                        ('frames', 'bindings'), ('baseline_shifts', 'bindings')]:
        audit = model[key]
        assert all(not v for k, v in audit.items() if k.startswith('unresolved') or k == 'source_issues')
        for binding in audit.get(member, []):
            assert binding['source_document_sha256'] == source_hash
            stream = actual[binding['document']][binding['field']]
            start, length = binding['offset'], binding['length']
            assert 0 <= start < start + length <= len(stream)
            if 'text' in binding:
                assert stream[start:start + length] == compact(binding['text'])
    refs = image_refs(model['documents'])
    assert sorted(refs) == sorted(p['asset'] for p in model['placements'])
    images = [p['asset'] for p in model['placements']] + [p['asset'] for p in model['page_placements']]
    assert len(images) == len(set(images)) and set(images) == {a['id'] for a in model['assets']}
    assert not model['unresolved']
    for asset in model['assets']:
        assert asset['document_sha256'] == source_hash
        assert hashlib.sha256(bytes(asset['encoded_bytes'])).hexdigest() == asset['encoded_sha256']
    assert not model['native_glyphs']['bindings'], 'OCR readings are outside this verification'
    unmapped = sum(b['remaining_scalars'] for b in model['native_glyphs']['source_issues'])
    assert unmapped == sum(text.count('\ufffd') for document in actual for text in document.values())
    return {'pages': len(native['pages']), 'used_words': words,
            'document_scalars': sum(len(v) for d in actual for v in d.values()),
            'body_images': len(model['placements']), 'page_images': len(model['page_placements']),
            'OCR_excluded_unmapped_scalars': unmapped, 'all_checks_passed': True}


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument('analysis', type=Path)
    cli.add_argument('output', type=Path)
    args = cli.parse_args()
    result = verify(json.loads(args.analysis.read_bytes()))
    args.output.write_text(json.dumps(result, indent=2) + '\n')
    print(result)


if __name__ == '__main__':
    main()

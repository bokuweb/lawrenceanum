"""Replay complete old ASTs and literal original field annotations; no parser calls."""
from pathlib import Path
import copy
import hashlib
import json


def canonical(value):
    return hashlib.sha256(json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


def nested(table):
    rows = [[[] for _ in range(table['cols'])] for _ in range(table['rows'])]
    merges = []
    for cell in table['cells']:
        if cell['text']:
            rows[cell['row']][cell['col']] = [{'text': cell['text']}]
        if cell['rowspan'] > 1 or cell['colspan'] > 1:
            merges.append({k: cell[k] for k in ['row', 'col', 'rowspan', 'colspan']})
    result = {'rows': rows}
    if merges:
        result['merges'] = merges
    return result


def append_source_rows(previous, current):
    assert len(previous['rows'][0]) == len(current['rows'][0]) == 2
    assert ''.join(r['text'] for r in current['rows'][0][0]) == '１'
    offset = len(previous['rows'])
    previous['rows'].extend(copy.deepcopy(current['rows']))
    for merge in current.get('merges', []):
        previous.setdefault('merges', []).append(merge | {'row': merge['row'] + offset})


def construct(baseline, case, reference=None):
    expected = copy.deepcopy(baseline)
    streams = []
    for annotation in case['annotations']:
        page = expected['pages'][annotation['page']]
        old = page['text']
        assert len(page['regions']) == 1 and page['regions'][0]['role'] == 'body'
        lines = page['regions'][0]['lines']
        after, before = annotation['after_lines'], annotation['before_lines']
        assert sorted(after + before) == list(range(len(lines)))
        page['regions'] = [
            {'role': role, 'unit': 0, 'part': 0, 'lines': [copy.deepcopy(lines[i]) for i in ids]}
            for role, ids in [('after', after), ('before', before)]
        ]
        fields = ['\n'.join(line['text'] for line in region['lines']).strip() for region in page['regions']]
        page['text'] = '\n'.join(['改正後', fields[0], '改正前', fields[1]])
        streams.append((*fields, page['text']))
        assert expected['text'].count(old) == 1
        expected['text'] = expected['text'].replace(old, page['text'], 1)
        article = expected['articles'][case['owner']]
        assert article['text'].count(old) == 1
        article['text'] = article['text'].replace(old, page['text'], 1)
    blocks = expected['articles'][case['owner']]['document']['blocks']
    assert len(blocks) == 3 and blocks[1]['kind'] == 'shinkyu' and blocks[2]['kind'] == 'paragraph'
    if case['mode'] == 'structural_notes':
        # Original bracketed form notes start separate lines. These add no rows
        # to the existing shared model's continuation of the same provision.
        for side, fragment in zip(['after', 'before'], streams[0][:2]):
            runs = blocks[1]['rows'][-1][side]
            assert len(runs) == 1 and set(runs[0]) == {'text'}
            runs[0]['text'] += '\n' + fragment
    elif case['mode'] == 'source_nested_tables':
        annotation = case['annotations'][0]
        lines = baseline['pages'][annotation['page']]['regions'][0]['lines']
        rows = blocks[1]['rows']
        for side, i in [('after', 0), ('before', 5)]:
            assert rows[-1][side][-1]['text'] == ''
            append_source_rows(rows[-1][side][-1]['table'], nested(lines[i]['table']))
        for ai, bi in [(1, 6), (3, 8)]:
            assert lines[ai]['text'] == lines[bi]['text']
            rows.append({
                side: [{'text': lines[i]['text']}, {'text': '', 'table': nested(lines[i + 1]['table'])}]
                for side, i in [('after', ai), ('before', bi)]
            })
    elif case['mode'] == 'frozen_canonical_tail':
        assert reference is not None and len(reference['blocks']) == 1
        rows = reference['blocks'][0]['rows']
        for side, k in [('after', 0), ('before', 1)]:
            actual = ''.join(run['text'] for row in rows for run in row[side])
            source = ''.join(stream[k] for stream in streams)
            assert ''.join(actual.split()) == ''.join(source.split())
            # Retain the frozen shared model's form-heading row granularity.
            # It appends this first plain form heading after the preceding table.
            assert 'table' in blocks[1]['rows'][-1][side][-1]
            blocks[1]['rows'][-1][side].extend(copy.deepcopy(rows[0][side]))
        blocks[1]['rows'].extend(copy.deepcopy(rows[1:]))
    else:
        raise AssertionError(case['mode'])
    del blocks[2]
    return expected, streams


if __name__ == '__main__':
    root = Path(__file__).resolve().parent
    crate = root.parents[2]
    index = json.loads((root / 'index.json').read_text())
    for source in index['inputs']:
        assert hashlib.sha256((crate / source['path']).read_bytes()).hexdigest() == source['sha256']
    for path, sha in index['reference'].items():
        assert hashlib.sha256((root / path).read_bytes()).hexdigest() == sha
    reference = json.loads((root / 'reference-output-213.json').read_text())
    for case in index['cases']:
        baseline = json.loads((root / case['baseline']).read_text())
        assert canonical(baseline) == case['baseline_Analysis_sha256']
        expected, streams = construct(baseline, case, reference)
        if case['mode'] == 'frozen_canonical_tail':
            literal = '\n'.join(stream[2] + '\n\ue002' for stream in streams)
            assert literal == (root / 'reference-input-213.txt').read_text()
        assert canonical(expected) == case['expected_Analysis_sha256']
        assert expected == json.loads((root / case['expected']).read_text())
    print('3 complete multi-page source golds replayed; all12 original PDFs and frozen reference hashes exact; no Rust/parser/detector calls')

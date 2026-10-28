from pathlib import Path
import json, hashlib
D = Path(__file__).resolve().parent
pair = json.loads((D.parent / 'native-comparison-scope/expected-pair181.json').read_text())
for kind, count in [('three', 1), ('four', 2)]:
    old = json.loads((D / f'baseline-{kind}183.json').read_text())
    assert old['pages'][count:] == pair['pages']
    assert old['articles'][1]['text'].endswith(pair['articles'][0]['text'])
    assert old['articles'][2]['text'] == '\n'.join(x['text'] for x in pair['articles'][1:])
    expected = json.loads(json.dumps(old))
    expected['articles'] = old['articles'][:2] + pair['articles'][1:]
    assert '\n'.join(x['text'] for x in expected['articles']) == '\n'.join(x['text'] for x in old['articles'])
    assert expected == json.loads((D / f'expected-{kind}187.json').read_text())
for name, sha in json.loads((D / 'index.json').read_text())['files'].items():
    assert hashlib.sha256((D / name).read_bytes()).hexdigest() == sha, name
print('Both complete187 golds use only independent181 article splits and frozen183 Analysis; all pages, text and first two articles remain exact.')

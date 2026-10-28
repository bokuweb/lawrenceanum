"""Replay exact source cuts without Rust/parser or the new detector."""
from pathlib import Path
import json,hashlib,sys
sys.dont_write_bytecode=True
from gold_logic import construct,H
P=Path(__file__).parent
index=json.loads((P/'index.json').read_text())
for c in index['cases']:
 assert hashlib.sha256((P.parents[2]/c['PDF']).read_bytes()).hexdigest()==c['PDF_sha256']
 b=json.loads((P/'baseline'/(c['key']+'.json')).read_text())
 assert H(b)==c['baseline_sha256']
 witness=''.join(c['source']['witness'].split())
 owners=[i for i,a in enumerate(b['articles'])if witness in ''.join(a['text'].split())]
 assert len(owners)==1
 expected=construct(b,owners[0],c['source']['title'])
 stored=json.loads((P/'expected'/(c['key']+'.json')).read_text())
 assert expected==stored and H(expected)==c['expected_sha256']
 print(c['key'],'complete independent source cut exact')
c=index['unchanged_control']
assert hashlib.sha256((P.parents[2]/c['PDF']).read_bytes()).hexdigest()==c['PDF_sha256']
assert H(json.loads((P/'control'/(c['key']+'.json')).read_text()))==c['Analysis_sha256']
print('original coauthored committee control exact')

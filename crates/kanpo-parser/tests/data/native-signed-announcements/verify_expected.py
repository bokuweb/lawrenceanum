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
 expected=b
 # Independent source cuts on one page are replayed in their original order.
 for s in [c['source'],*c.get('column_continued_sources',[])]:
  witness=''.join(s['witness'].split())
  owners=[i for i,a in enumerate(expected['articles'])if witness in ''.join(a['text'].split())]
  assert len(owners)==1
  expected=construct(expected,owners[0],s['title'])
 stored=json.loads((P/'expected'/(c['key']+'.json')).read_text())
 assert expected==stored and H(expected)==c['expected_sha256']
 print(c['key'],'complete independent source cut exact')
c=index['unchanged_control']
assert hashlib.sha256((P.parents[2]/c['PDF']).read_bytes()).hexdigest()==c['PDF_sha256']
assert H(json.loads((P/'control'/(c['key']+'.json')).read_text()))==c['Analysis_sha256']
print('original coauthored committee control exact')

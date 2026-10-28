"""Replay the katakana heading cuts from the previous frozen golds; no Rust/parser."""
from pathlib import Path
import json,sys
sys.dont_write_bytecode=True
P=Path(__file__).parent
sys.path.insert(0,str(P))
from split import split,H
index=json.loads((P/'index.json').read_text())
for c in index['cases']:
 source=json.loads((P.parents[1]/c['source'].removeprefix('tests/')).read_text())
 assert H(source)==c['source_sha256'],c['name']
 expected=split(source,c['anchor'])
 stored=json.loads((P/(c['name']+'.json')).read_text())
 assert expected==stored and H(stored)==c['expected_sha256'],c['name']
 print(c['name'],'exact cut at the original katakana heading')

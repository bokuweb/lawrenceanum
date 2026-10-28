"""Replay independent source golds with only Python standard library."""
from pathlib import Path
import hashlib,json,sys
sys.dont_write_bytecode=True
from gold_logic import construct,july,august,september,H
P=Path(__file__).parent
index=json.loads((P/'index.json').read_text())
for ref in index['frozen190_text_only_references']:
 text=(P/'references'/(ref['name']+'.input.txt')).read_bytes()
 output=json.loads((P/'references'/(ref['name']+'.output.json')).read_text())
 assert hashlib.sha256(text).hexdigest()==ref['input_sha256']
 assert H(output)==ref['output_sha256']
for case in index['cases']:
 name=case['name'];a=case['source_annotations']
 for pdf in case['pdfs']:
  path=P.parents[2]/pdf['path']
  assert hashlib.sha256(path.read_bytes()).hexdigest()==pdf['sha256']
 baseline=json.loads((P/'baseline'/(name+'.json')).read_text())
 assert H(baseline)==case['baseline_sha256']
 standalone=json.loads((P/'baseline'/('standalone-'+name+'.json')).read_text())
 assert H(standalone)==case['standalone_sha256']
 if name=='171-three':expected=july(baseline,2,a['owner'])
 elif name=='192-four':expected=construct(baseline,3,a['owner'])
 elif name=='192-three':expected=august(baseline,2,a['owner'])
 elif name=='215-four':expected=september(baseline,3,a['owner'],a['old_comparison_block'])
 else:assert name=='226-three';expected=baseline
 stored=json.loads((P/'expected'/(name+'.json')).read_text())
 assert expected==stored and H(expected)==case['expected_sha256'],name
 print(name,'complete independent gold exact')

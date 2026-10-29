"""Replay a source article cut at a katakana-initial `〇` heading without Rust.

The cut uses the same exact Article/Run split as the signed-announcement golds.
Each part's Document format is decided again from its own text by the
published rule: a standalone 改正後 line followed by a standalone 改正前 line
is a comparison table; otherwise an amendment or enactment phrase is prose;
otherwise it is unknown (for example a lone printed section label).
"""
from pathlib import Path
import copy,importlib.util,sys
sys.dont_write_bytecode=True
P=Path(__file__).parent
spec=importlib.util.spec_from_file_location('signed_gold',P.parent/'native-signed-announcements'/'gold_logic.py')
G=importlib.util.module_from_spec(spec);spec.loader.exec_module(G)
H=G.H
def split(doc,anchor):
 owners=[i for i,a in enumerate(doc['articles'])if ''.join(anchor.split())in''.join(a['text'].split())]
 assert len(owners)==1,owners
 i=owners[0];assert not doc['articles'][i]['text'].startswith(anchor)
 out=G.construct(copy.deepcopy(doc),i,anchor)
 for part in out['articles'][i:i+2]:part['document']['format']=fmt(part['text'])
 return out
def fmt(text):
 lines=[l.strip()for l in text.split('\n')]
 if '改正後'in lines and '改正前'in lines[lines.index('改正後')+1:]:return 'shinkyu'
 if any(k in text for k in['に改める','次のように改正する','を加える','を削る','廃止する','定める']):return 'prose'
 return 'unknown' 

"""Replay wrapped notice-title boundaries on a previous frozen gold; no Rust/parser.

A notice title can wrap at any character (「…の登録事項の変更に関」＋「する公示」).
`cut` starts a new article at the title's first line with the exact Article/Run
split of the signed-announcement golds. `move` handles an old boundary at the
title's last line (a standalone 「公示」): the earlier title lines leave the end
of the previous article and its last Run, and lead the next article and its first
Run. `join` replays an earlier adopted relation in a test (a printed masthead
article joined to the next one): its text and Blocks lead the next article.
Text and Document format labels are otherwise retained.
"""
from pathlib import Path
import copy,importlib.util,sys
sys.dont_write_bytecode=True
P=Path(__file__).parent
spec=importlib.util.spec_from_file_location('signed_gold',P.parent/'native-signed-announcements'/'gold_logic.py')
G=importlib.util.module_from_spec(spec);spec.loader.exec_module(G)
H=G.H
compact=lambda s:''.join(s.split())
def cut(doc,anchor):
 owners=[i for i,a in enumerate(doc['articles'])if compact(anchor)in compact(a['text'])]
 assert len(owners)==1,owners
 i=owners[0];assert not compact(doc['articles'][i]['text']).startswith(compact(anchor))
 return G.construct(copy.deepcopy(doc),i,anchor)
def move(doc,prefix):
 out=copy.deepcopy(doc);arts=out['articles']
 owners=[i for i,a in enumerate(arts[:-1])if a['text'].endswith('\n'+prefix)]
 assert len(owners)==1,owners
 i=owners[0];a,b=arts[i],arts[i+1]
 a['text']=a['text'][:-len(prefix)-1];b['text']=prefix+'\n'+b['text']
 last=a['document']['blocks'][-1];run=last['runs'][-1]
 assert last['kind']=='paragraph' and set(run)=={'text'} and run['text'].endswith(prefix)
 run['text']=run['text'][:-len(prefix)]
 if not run['text']:last['runs'].pop()
 if not last['runs']:a['document']['blocks'].pop()
 first=b['document']['blocks'][0];head=first['runs'][0]
 assert first['kind']=='paragraph' and set(head)=={'text'}
 head['text']=prefix+head['text']
 return out
def join(doc,index):
 out=copy.deepcopy(doc);arts=out['articles'];parent=arts.pop(index);child=arts[index]
 child['text']=parent['text']+'\n'+child['text']
 child['document']['blocks']=parent['document']['blocks']+child['document']['blocks']
 return out

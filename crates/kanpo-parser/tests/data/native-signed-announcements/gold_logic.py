"""Original signed publication cut in frozen source Word/Run material."""
from pathlib import Path
import copy,json,hashlib
H=lambda v:hashlib.sha256(json.dumps(v,ensure_ascii=False,sort_keys=True,separators=(',',':')).encode()).hexdigest()
def locate(text,anchor):
 positions=[i for i,c in enumerate(text)if not c.isspace()];material=''.join(text[i]for i in positions);wanted=''.join(anchor.split());assert wanted and material.count(wanted)==1
 return positions[material.index(wanted)]
def construct(baseline,owner,anchor):
 out=copy.deepcopy(baseline);old=out['articles'][owner];assert set(old)=={'text','document'};cut=locate(old['text'],anchor);a=copy.deepcopy(old);b=copy.deepcopy(old);a['text']=old['text'][:cut].rstrip();b['text']=old['text'][cut:].lstrip();assert a['text']and b['text']
 blocks=old['document']['blocks'];hits=[(i,j,locate(r['text'],anchor))for i,block in enumerate(blocks)if block['kind']=='paragraph'for j,r in enumerate(block['runs'])if ''.join(anchor.split())in ''.join(r['text'].split())];assert len(hits)==1
 i,j,k=hits[0];block=blocks[i];run=block['runs'][j];assert set(run)=={'text'}
 left=copy.deepcopy(block);right=copy.deepcopy(block);left['runs']=copy.deepcopy(block['runs'][:j]);right['runs']=copy.deepcopy(block['runs'][j+1:]);p=run['text'][:k];q=run['text'][k:]
 if p:left['runs'].append({'text':p})
 if q:right['runs'].insert(0,{'text':q})
 a['document']['blocks']=copy.deepcopy(blocks[:i])+([left]if left['runs']else[]);b['document']['blocks']=([right]if right['runs']else[])+copy.deepcopy(blocks[i+1:]);out['articles'][owner:owner+1]=[a,b]
 return out

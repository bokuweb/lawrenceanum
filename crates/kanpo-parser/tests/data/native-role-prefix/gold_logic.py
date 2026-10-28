"""Reconstruct source golds without invoking Rust, PDF extraction or191 detection."""
from pathlib import Path
import copy,hashlib,json
P=Path(__file__).parent
C=copy.deepcopy
H=lambda v:hashlib.sha256(json.dumps(v,ensure_ascii=False,sort_keys=True,separators=(',',':')).encode()).hexdigest()
def model(text,name):
    assert (P/'references'/(name+'.input.txt')).read_text()==text
    return json.loads((P/'references'/(name+'.output.json')).read_text())
def nested(t):
 rows=[[[]for _ in range(t['cols'])]for _ in range(t['rows'])];merges=[]
 for c in t['cells']:
  if c['text']:rows[c['row']][c['col']]=[{'text':c['text']}]
  if c['rowspan']>1 or c['colspan']>1:merges.append({k:c[k]for k in ['row','col','rowspan','colspan']})
 out={'rows':rows}
 if merges:out['merges']=merges
 return out
def construct(baseline,pi,owner):
 out=copy.deepcopy(baseline);p=out['pages'][pi];old=copy.deepcopy(p);r=old['regions'];assert[(x['role'],len(x['lines']))for x in r]==[('preamble',16),('after',6),('before',6)]
 lines=r[0]['lines'];after=lines[:3];before=lines[3:6];suffix=lines[6:]
 assert [i for i,l in enumerate(lines[:6])if 'table'in l]==[0,2,3,5]
 assert lines[1]['text']==lines[4]['text']
 p['regions']=[dict(role=side,unit=0,part=0,lines=copy.deepcopy(ls))for side,ls in [('after',after),('before',before)]]
 for region in copy.deepcopy(r):
  region['unit']=1
  if region['role']=='preamble':region['lines']=copy.deepcopy(suffix)
  p['regions'].append(region)
 join=lambda ls:'\n'.join(l['text']for l in ls).strip()
 prefix='\n'.join(['改正後',join(after),'改正前',join(before)])
 oldprefix=join(lines[:6]);assert old['text'].startswith(oldprefix+'\n')
 p['text']=prefix+old['text'][len(oldprefix):]
 assert out['text'].count(old['text'])==1;out['text']=out['text'].replace(old['text'],p['text'],1)
 art=out['articles'][owner];assert art['text'].count(oldprefix)==1;art['text']=art['text'].replace(oldprefix,prefix,1)
 blocks=art['document']['blocks'];assert len(blocks)==3 and blocks[1]['kind']=='shinkyu'and blocks[2]['kind']=='paragraph'
 rows=blocks[1]['rows'];assert len(rows)==6
 for side,i in [('after',0),('before',3)]:
  previous=rows[-1][side][-1]['table'];current=nested(lines[i]['table']);assert len(previous['rows'][0])==len(current['rows'][0])==2
  offset=len(previous['rows']);previous['rows'].extend(copy.deepcopy(current['rows']))
  for merge in current.get('merges',[]):previous.setdefault('merges',[]).append(merge|{'row':merge['row']+offset})
 rows.append({side:[{'text':lines[i]['text']},{'text':'','table':nested(lines[i+1]['table'])}]for side,i in [('after',1),('before',4)]})
 runs=blocks[2]['runs'];assert len(runs)==6
 assert runs[0]=={'text':'','table':nested(lines[0]['table'])}
 # Frozen old190 unlabelled paragraph merged the next opposite-side table into
 # the preceding two-column table. Verify that exact old composition first.
 oldjoined=nested(lines[2]['table']);tail=nested(lines[3]['table']);offset=len(oldjoined['rows']);oldjoined['rows'].extend(copy.deepcopy(tail['rows']))
 for merge in tail.get('merges',[]):oldjoined.setdefault('merges',[]).append(merge|{'row':merge['row']+offset})
 assert runs[2]=={'text':'','table':oldjoined}
 assert runs[4]=={'text':'','table':nested(lines[5]['table'])}
 assert runs[1]=={'text':lines[1]['text']}and runs[3]=={'text':lines[4]['text']}
 assert runs[-1]['text'].startswith('附則この省令は、')
 blocks[2]['runs']=[copy.deepcopy(runs[-1])]
 return out

class Direct:
 nested=staticmethod(nested)
 construct=staticmethod(construct)
direct=Direct()
def literal(a,b):return '改正後\n'+a+'\n改正前\n'+b+'\n\ue002'
def join(ls):return '\n'.join(l['text']for l in ls).strip()
def roles(baseline,pi,owner,na,nb):
 out=C(baseline);p=out['pages'][pi];old=C(p);r=old['regions'];ls=r[0]['lines'];assert r[0]['role']=='preamble'and len(ls)>na+nb
 after,before,suffix=ls[:na],ls[na:na+nb],ls[na+nb:];prefix=literal(join(after),join(before)).removesuffix('\n\ue002');oldprefix=join(ls[:na+nb]);assert old['text'].startswith(oldprefix+'\n')
 p['text']=prefix+old['text'][len(oldprefix):];p['regions']=[dict(role=side,unit=0,part=0,lines=C(lines))for side,lines in [('after',after),('before',before)]]
 for region in C(r):
  region['unit']=1
  if region['role']=='preamble':region['lines']=C(suffix)
  p['regions'].append(region)
 assert out['text'].count(old['text'])==1;out['text']=out['text'].replace(old['text'],p['text'],1)
 art=out['articles'][owner];assert art['text'].count(oldprefix)==1;art['text']=art['text'].replace(oldprefix,prefix,1)
 return out,after,before,suffix

def july(baseline,pi,owner):
 out,a,b,suffix=roles(baseline,pi,owner,13,13);blocks=out['articles'][owner]['document']['blocks'];old=baseline['articles'][owner]['document']['blocks'][2]
 control=model(join(a+b+suffix[:1]),'171-old-tail-control');assert control['blocks']==[old]
 ref=model(literal(join(a),join(b)),'171-paired-tail');assert len(ref['blocks'])==1 and ref['blocks'][0]['kind']=='shinkyu'and len(ref['blocks'][0]['rows'])==1
 row=ref['blocks'][0]['rows'][0]
 for side in ['after','before']:blocks[1]['rows'][-1][side].extend(C(row[side]))
 note=model(join(suffix[:1]),'171-outside-note');assert len(note['blocks'])==1
 blocks[2:]=C(note['blocks']);return out

def august(baseline,pi,owner):
 out,a,b,suffix=roles(baseline,pi,owner,17,18);blocks=out['articles'][owner]['document']['blocks'];old=baseline['articles'][owner]['document']['blocks'][1]['rows'][-1]
 # Paragraph wrapping depends on the complete article's physical-line context.
 # First reproduce the unchanged complete old article exactly, rather than
 # refeeding an already-normalized last row as though it were raw source lines.
 old_article=baseline['articles'][owner];source=baseline['pages'][pi]['regions'][0]['lines']
 oldprefix=join(source[:35]);assert old_article['text'].count(oldprefix)==1
 control=model(old_article['text'].replace(oldprefix,'\ue002\n'+oldprefix,1),'192-whole-old-control')
 assert control==old_article['document']
 note=suffix[0]['text'];text=out['articles'][owner]['text'];assert text.count(note)==1
 out['articles'][owner]['document']=model(text.replace(note,'\ue002\n'+note,1),'192-whole-new-reference')
 return out

def september(baseline,pi=343,owner=162,bi=7):
 out,a,b,suffix=roles(baseline,pi,owner,17,1);blocks=out['articles'][owner]['document']['blocks'];old=baseline['articles'][owner]['document']['blocks'][bi+1];assert old['kind']=='paragraph'and len(old['runs'])==3
 control=model(join(a),'215-old-after-tail-control');assert control['blocks']==[{'kind':'paragraph','runs':[old['runs'][0]]}]
 assert old['runs'][1]=={'text':'','table':direct.nested(b[0]['table'])}
 ref=model(literal(join(a),''),'215-after-tail');assert len(ref['blocks'])==1 and ref['blocks'][0]['kind']=='shinkyu'and len(ref['blocks'][0]['rows'])==1
 row=blocks[bi]['rows'][-1];row['after'].extend(C(ref['blocks'][0]['rows'][0]['after']))
 previous=row['before'][-1]['table'];current=direct.nested(b[0]['table']);assert len(previous['rows'][0])==2 and len(current['rows'][0])==3
 row['before'].append({'text':'','table':current})
 blocks[bi+1]['runs']=[C(old['runs'][2])];return out
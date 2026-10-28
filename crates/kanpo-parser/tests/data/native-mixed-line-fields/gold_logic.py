"""Literal source fields and exact original Word/text slices; no192 detector."""
from pathlib import Path
import copy,json,hashlib
H=lambda v:hashlib.sha256(json.dumps(v,ensure_ascii=False,sort_keys=True,separators=(',',':')).encode()).hexdigest()
def nested(t):
 rows=[[[]for _ in range(t['cols'])]for _ in range(t['rows'])];merges=[]
 for c in t['cells']:
  if c['text']:rows[c['row']][c['col']]=[{'text':c['text']}]
  if c['rowspan']>1 or c['colspan']>1:merges.append({k:c[k]for k in ['row','col','rowspan','colspan']})
 out={'rows':rows}
 if merges:out['merges']=merges
 return out
def join(lines):
 s='\n'.join(l['text']for l in lines);first=next((l for l in lines if l['text'].strip()),None)
 leading=0
 if first and 'table'in first:
  cells=[c for c in first['table']['cells']if c['text'].strip()]
  row=min(c['row']for c in cells);leading=min(c['col']for c in cells if c['row']==row)
 return '　'*leading+s.strip()
def append(a,b):
 assert len(a['rows'][0])==len(b['rows'][0])==3
 # Original19 ends a reactor row; original20 begins a distinct omitted row.
 # There is no repeated header and an existing source span reaches this edge.
 assert a['rows'][0]!=b['rows'][0]
 assert any(m['row']+m['rowspan']==len(a['rows'])for m in a['merges'])
 offset=len(a['rows']);a['rows'].extend(copy.deepcopy(b['rows']))
 for m in b.get('merges',[]):a.setdefault('merges',[]).append(m|{'row':m['row']+offset})
def construct(baseline,pi,owner):
 out=copy.deepcopy(baseline);p=out['pages'][pi];old=copy.deepcopy(p);regions=old['regions'];assert[(r['role'],len(r['lines']))for r in regions]==[('preamble',10),('after',3),('before',3)]
 ls=regions[0]['lines'];footer=ls[2];assert footer=={'text':'（備考）　（略）　（備考）　（略）','words':[124,97,56,36]}
 a={'text':'（備考）　（略）','words':[124,97]};b={'text':'（備考）　（略）','words':[56,36]}
 assert footer['text']==a['text']+'　'+b['text']and footer['words']==a['words']+b['words']
 for line in [a,b]:assert ''.join(line['text'].split())==''.join(''.join(p['words'][i]['text'].split())for i in line['words'])
 fields=[[copy.deepcopy(ls[0]),a],[copy.deepcopy(ls[1]),b]]
 p['regions']=[dict(role=role,unit=0,part=0,lines=lines)for role,lines in zip(['after','before'],fields)]
 for r in copy.deepcopy(regions):
  r['unit']=1
  if r['role']=='preamble':r['lines']=copy.deepcopy(ls[3:])
  p['regions'].append(r)
 oldprefix=join(ls[:3]);assert old['text'].startswith(oldprefix+'\n')
 prefix='\n'.join(['改正後',join(fields[0]),'改正前',join(fields[1])]);p['text']=prefix+old['text'][len(oldprefix):]
 assert out['text'].count(old['text'])==1;out['text']=out['text'].replace(old['text'],p['text'],1)
 art=out['articles'][owner];assert art['text'].count(oldprefix)==1;art['text']=art['text'].replace(oldprefix,prefix,1)
 blocks=art['document']['blocks'];assert len(blocks)==3 and blocks[1]['kind']=='shinkyu'and len(blocks[1]['rows'])==3
 runs=blocks[2]['runs'];assert len(runs)==2 and runs[1]=={'text':footer['text']}
 # Frozen old paragraph joined the two unlabelled tables and dropped the
 # repeated first row of the second. Verify this exact old composition first.
 oldjoined=nested(ls[0]['table']);other=nested(ls[1]['table']);assert oldjoined['rows'][0]==other['rows'][0];oldjoined['rows'].extend(copy.deepcopy(other['rows'][1:]));assert not other.get('merges')and not oldjoined.get('merges');assert runs[0]=={'text':'','table':oldjoined}
 row=blocks[1]['rows'][-1]
 for side,i,tail in [('after',0,a),('before',1,b)]:
  assert row[side][-1]['text']=='';append(row[side][-1]['table'],nested(ls[i]['table']));row[side].append({'text':tail['text']})
 del blocks[2]
 return out

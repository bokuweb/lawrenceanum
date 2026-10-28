from pathlib import Path
import copy,json,hashlib,collections
D=Path(__file__).parent.resolve();key='20260723g001630108';C=lambda a:hashlib.sha256(json.dumps(a,ensure_ascii=False,sort_keys=True,separators=(',',':')).encode()).hexdigest();definitions={'after':{1:[([31],[32]),([35,39],[36,40])],4:[([52],[53]),([56],[57]),([60],[61])]},'before':{1:[([33],[34]),([37,41],[38,42])],2:[([46],[47]),([50],[51])]}}
def nested(t):
 rows=[[[]for _ in range(t['cols'])]for _ in range(t['rows'])]
 for c in t['cells']:
  assert c['rowspan']==c['colspan']==1
  if c['text']:rows[c['row']][c['col']]=[{'text':c['text']}]
 return {'rows':rows}
def render(t):
 out=[]
 for r in range(t['rows']):
  fields=[''for _ in range(t['cols'])]
  for c in t['cells']:
   if c['row']==r:fields[c['col']]=c['text'].replace('\n','　')
  out.append('　'.join(fields).rstrip())
 return '\n'.join(out)
def piece(cell,words,parts,k):
 # The exact original cell text from this record's first Word to the next record's first Word.
 assert [i for ids in parts for i in ids]==cell['words']
 at=[i for i,ch in enumerate(cell['text'])if not ch.isspace()];starts=[];n=0
 for ids in parts:starts.append(n);n+=sum(len(''.join(words[i]['text'].split()))for i in ids)
 assert n==len(at)
 text=cell['text'][at[starts[k]]:at[starts[k+1]]if k+1<len(parts)else len(cell['text'])]
 assert ''.join(text.split())==''.join(''.join(words[i]['text'].split())for i in parts[k])
 return text.strip()
def construct(old,pi):
 out=copy.deepcopy(old);page=out['pages'][pi];words=page['words'];changes=[]
 for region in page['regions']:
  role=region['role']
  for line in region['lines']:
   if role not in definitions or 'table'not in line:continue
   required={i for pairs in definitions[role].values()for pair in pairs for ids in pair for i in ids}
   if not required.issubset({i for c in line['table']['cells']for i in c['words']}):continue
   original=copy.deepcopy(line['table']);assert line['text']==render(original);t=line['table'];cells=[];n=0
   for row in range(original['rows']):
    oldcells=sorted([copy.deepcopy(c)for c in original['cells']if c['row']==row],key=lambda c:c['col']);assert len(oldcells)==2
    if row not in definitions[role]:
     for c in oldcells:c['row']=n;cells.append(c)
     n+=1;continue
    source_pairs=definitions[role][row]
    for k,ids_pair in enumerate(source_pairs):
     for col,ids in enumerate(ids_pair):
      c=copy.deepcopy(oldcells[col]);assert all(i in c['words']for i in ids);c.update(row=n,words=ids,text=piece(oldcells[col],words,[p[col]for p in source_pairs],k),x0=min(words[i]['x0']for i in ids),x1=max(words[i]['x1']for i in ids));cells.append(c)
     n+=1
   t['cells']=cells;t['rows']=n;assert t['rows']==8
   assert collections.Counter(i for c in t['cells']for i in c['words'])==collections.Counter(i for c in original['cells']for i in c['words']);line['text']=render(t);changes.append((render(original),render(t),nested(original),nested(t)))
 assert len(changes)==2
 for before,after,oldtable,newtable in changes:
  assert page['text'].count(before)==1;page['text']=page['text'].replace(before,after);assert out['text'].count(before)==1;out['text']=out['text'].replace(before,after);changed_articles=0;changed_tables=0
  def walk(v):
   nonlocal changed_tables
   if isinstance(v,dict):
    for k,x in v.items():
     if k=='table'and x==oldtable:v[k]=copy.deepcopy(newtable);changed_tables+=1
     else:walk(x)
   elif isinstance(v,list):
    for x in v:walk(x)
  for a in out['articles']:
   if before in a['text']:assert a['text'].count(before)==1;a['text']=a['text'].replace(before,after);changed_articles+=1
   walk(a['document'])
  assert changed_articles==changed_tables==1,(changed_articles,changed_tables)
 return out

if __name__=='__main__':
 old=json.loads((D/'baseline.json').read_text());expected=construct(old,0)
 assert expected==json.loads((D/'expected.json').read_text())
 print('Independent original source five-cut gold matches all Analysis fields; full 9/9 remains unresolved')

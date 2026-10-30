# regold_title.py OLD TITLE OUT : cut each Article that carries TITLE as an inner text line at
# that line (native263u). The Article text is cut at the line; its Document blocks are cut
# at the one Run holding TITLE. Pages, whole text, formats and other Articles are copied.
import json,sys
old,title,out=sys.argv[1:4]
e=json.load(open(old))
arts=[]
for a in e['articles']:
    sep='\n'+title+'\n'
    if sep not in a['text']:
        arts.append(a);continue
    assert a['text'].count(sep)==1
    head,tail=a['text'].split(sep)
    blocks=a['document']['blocks']
    hits=[(bi,ri) for bi,b in enumerate(blocks) for ri,r in enumerate(b.get('runs',[])) if title in r['text']]
    assert len(hits)==1,hits
    bi,ri=hits[0];b=blocks[bi];run=b['runs'][ri]['text'];i=run.index(title)
    assert run.count(title)==1 and b['kind']=='paragraph' and set(b)=={'kind','runs'}
    pre=b['runs'][:ri]+([{**b['runs'][ri],'text':run[:i]}] if run[:i] else [])
    post=[{**b['runs'][ri],'text':run[i:]}]+b['runs'][ri+1:]
    f=a['document']['format']
    first=blocks[:bi]+([{'kind':'paragraph','runs':pre}] if pre else [])
    second=[{'kind':'paragraph','runs':post}]+blocks[bi+1:]
    arts.append({**a,'text':head,'document':{**a['document'],'blocks':first}})
    arts.append({**a,'text':title+'\n'+tail,'document':{'format':f,'blocks':second}})
e['articles']=arts
open(out,'w').write(json.dumps(e,ensure_ascii=False,indent=2)+'\n')

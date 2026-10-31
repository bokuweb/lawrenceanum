# split.py OLD OUT FORMAT : cut each Article that carries the spaced section heading 「資　料」 as an
# inner or last text line, so that the label starts its own Article (native263w).
# OLD is a full Analysis (with "articles") or an article partition list. The Article text
# is cut at the label line; its Document blocks are cut at the one Run holding the label
# (dropping the newline right before the label).
# The cut-off Article gets FORMAT (its own text decides it: 「資　料」 reports carry no
# 改正後/改正前 labels and no amendment phrase, so `unknown`). Everything else (pages,
# whole text, the first part's format, other Articles) is copied.
import json,sys
LABEL='資\u3000料'
def cut(a):
    lines=a['text'].split('\n')
    at=[i for i,l in enumerate(lines) if i>0 and l==LABEL]
    if not at:
        return [a]
    assert len(at)==1
    i=at[0];head='\n'.join(lines[:i]);tail='\n'.join(lines[i:])
    blocks=a['document']['blocks']
    hits=[(bi,ri) for bi,b in enumerate(blocks) for ri,r in enumerate(b.get('runs',[])) if LABEL in r.get('text','')]
    assert len(hits)==1,hits
    bi,ri=hits[0];b=blocks[bi];run=b['runs'][ri]['text'];k=run.index(LABEL)
    assert run.count(LABEL)==1 and b['kind']=='paragraph' and set(b)=={'kind','runs'}
    # The line break before the label ends the previous Article, which keeps no trailing newline.
    head_run=run[:k][:-1] if run[:k].endswith('\n') else run[:k]
    pre=b['runs'][:ri]+([{**b['runs'][ri],'text':head_run}] if head_run else [])
    post=[{**b['runs'][ri],'text':run[k:]}]+b['runs'][ri+1:]
    first=blocks[:bi]+([{'kind':'paragraph','runs':pre}] if pre else [])
    second=[{'kind':'paragraph','runs':post}]+blocks[bi+1:]
    return [{**a,'text':head,'document':{**a['document'],'blocks':first}},
            {**a,'text':tail,'document':{**a['document'],'format':FORMAT,'blocks':second}}]
old,out,FORMAT=sys.argv[1:4]
e=json.load(open(old))
arts=e['articles'] if isinstance(e,dict) else e
arts=[p for a in arts for p in cut(a)]
if isinstance(e,dict): e['articles']=arts
else: e=arts
open(out,'w').write(json.dumps(e,ensure_ascii=False,indent=2)+'\n')

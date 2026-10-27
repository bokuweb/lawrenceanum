# -*- coding: utf-8 -*-
"""Literal old-AST edits at source-curated rows; no detector or parser calls."""
import collections,copy,hashlib,json
def canonical(value):
    return hashlib.sha256(json.dumps(value,ensure_ascii=False,sort_keys=True,separators=(',',':')).encode()).hexdigest()
def compact(value):return ''.join(c for c in value if not c.isspace())
def run_refs(value):
    if isinstance(value,dict):
        if 'runs'in value:
            for run in value['runs']:
                if isinstance(run,dict)and isinstance(run.get('text'),str):yield run
        else:
            for child in value.values():yield from run_refs(child)
    elif isinstance(value,list):
        for child in value:yield from run_refs(child)
def apply(text,edits):
    for at,addition in sorted(edits,reverse=True):text=text[:at]+addition+text[at:]
    return text
def row_edits(before,slots):
    tokens=before.split('\u3000');edits=[];parts=[]
    counts=collections.Counter(slots)
    assert all(0<slot<=len(tokens)for slot in counts)
    for index in range(len(tokens)+1):
        n=counts[index]
        if n:
            at=sum(len(t)+1 for t in tokens[:index])if index<len(tokens)else len(before)
            addition='－\u3000'*n if index<len(tokens)else '\u3000－'*n
            edits.append((at,addition,index==len(tokens)))
            parts.extend(['－']*n)
        if index<len(tokens):parts.append(tokens[index])
    expected='\u3000'.join(parts)
    assert apply(before,[(at,text)for at,text,_ in edits])==expected
    return expected,edits
def construct(before,placements):
    expected=copy.deepcopy(before);runs=[r for article in expected['articles']for r in run_refs(article['document'])];run_edits={}
    for placement in placements:
        pi=placement['page'];old_page=before['pages'][pi];new_page=expected['pages'][pi]
        by_region=collections.defaultdict(list);rows=[]
        for annotation in placement['rows']:
            matches=[(ri,li,line)for ri,region in enumerate(old_page['regions'])for li,line in enumerate(region['lines'])
                if line['text']==annotation['old_text']and line['words']==annotation['old_words']]
            assert len(matches)==1,matches
            ri,li,old_line=matches[0];new_line=new_page['regions'][ri]['lines'][li]
            text,edits=row_edits(old_line['text'],annotation['before_token'])
            assert text==annotation['expected_text']
            assert len(annotation['insertions'])==len(annotation['before_token'])
            ids=old_line['words'].copy()
            for insertion in annotation['insertions']:
                word_id=insertion['word'];old_word=old_page['words'][word_id]
                assert old_word['id']==word_id and old_word['status']=='empty'and old_word['text']==''
                assert old_word['raw']==insertion['raw']and old_word['source_box']==insertion['source_box']
                anchor=insertion['before_word'];at=ids.index(anchor)if anchor is not None else len(ids)
                ids.insert(at,word_id)
                new_page['words'][word_id]['text']='－';new_page['words'][word_id]['status']='used'
            new_line['words']=ids;new_line['text']=text
            by_region[ri].append((li,edits));rows.append((old_line['text'],text))
        replacements={}
        for old,new in rows:
            if old in replacements:assert replacements[old]==new
            replacements[old]=new
        counts=collections.Counter(old for old,_ in rows)
        page_text=old_page['text']
        for old,new in replacements.items():
            assert page_text.count(old)==counts[old],(placement['key'],old)
            page_text=page_text.replace(old,new)
        new_page['text']=page_text
        assert expected['text'].count(old_page['text'])==1,placement['key']
        expected['text']=expected['text'].replace(old_page['text'],page_text,1)
        articles=[article for article in expected['articles']if old_page['text']in article['text']]
        assert len(articles)==1,placement['key']
        assert articles[0]['text'].count(old_page['text'])==1
        articles[0]['text']=articles[0]['text'].replace(old_page['text'],page_text,1)
        for ri,changes in by_region.items():
            lines=old_page['regions'][ri]['lines'];context='\n'.join(line['text']for line in lines);needle=compact(context)
            matches=[run for run in runs if compact(run['text']).count(needle)>0]
            assert len(matches)==1 and compact(matches[0]['text']).count(needle)==1,(placement['key'],ri)
            run=matches[0];indices=[i for i,c in enumerate(run['text'])if not c.isspace()];start=compact(run['text']).index(needle)
            chosen=run_edits.setdefault(id(run),(run,[]))[1]
            for li,edits in changes:
                prefix='\n'.join(line['text']for line in lines[:li])
                if li:prefix+='\n'
                for at,text,trailing in edits:
                    position=start+len(compact(prefix+lines[li]['text'][:at]))
                    index=indices[position-1]+1 if trailing else indices[position]
                    chosen.append((index,text))
    for run,edits in run_edits.values():run['text']=apply(run['text'],edits)
    return expected

if __name__ == "__main__":
    from pathlib import Path
    root=Path(__file__).resolve().parent;crate=root.parents[2]
    index=json.loads((root/'index.json').read_text())
    for case in index['cases']:
        assert hashlib.sha256((crate/case['pdf']).read_bytes()).hexdigest()==case['PDF_sha256']
        before=json.loads((root/case['baseline']).read_text());assert canonical(before)==case['baseline_Analysis_sha256']
        expected=construct(before,case['placements']);assert canonical(expected)==case['expected_Analysis_sha256']
        assert expected==json.loads((root/case['expected']).read_text()),case['key']
    print(str(len(index['cases']))+" complete original-source Analysis fixtures reconstructed independently")

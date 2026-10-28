"""Source page substitution and literal old AST restoration; no parser calls."""
import copy,hashlib,json
def canonical(v):return hashlib.sha256(json.dumps(v,ensure_ascii=False,sort_keys=True,separators=(',',':')).encode()).hexdigest()
def compact(t):return ''.join(t.split())
def text(runs):
    assert all(set(run)=={'text'}for run in runs)
    return ''.join(run['text']for run in runs)
def construct(before,source,page):
    expected=copy.deepcopy(before);old=before['pages'][page];desired=source['pages'][0]
    assert old['words']==desired['words']
    assert [r['role']for r in old['regions']]==['after','before','body']
    assert [r['role']for r in desired['regions']]==['body']
    assert expected['text'].count(old['text'])==1
    expected['pages'][page]=copy.deepcopy(desired)
    expected['text']=expected['text'].replace(old['text'],desired['text'],1)
    owners=[article for article in expected['articles']if old['text']in article['text']]
    assert len(owners)==1 and owners[0]['text'].count(old['text'])==1
    article=owners[0]
    assert len(before['pages'])==2 and page==1
    blocks=article['document']['blocks'];assert len(blocks)==5
    outside,false,body=blocks[2:5]
    assert outside['kind']=='paragraph'and false['kind']=='shinkyu'and body['kind']=='paragraph'
    assert len(outside['runs'])==len(body['runs'])==1 and len(false['rows'])==1
    for key,region in [('after',old['regions'][0]),('before',old['regions'][1])]:
        assert compact(text(false['rows'][0][key]))==compact('\n'.join(l['text']for l in region['lines']))
    assert compact(text(body['runs']))==compact('\n'.join(l['text']for l in old['regions'][2]['lines']))
    # Source review: page298 ends with its outside enactment provision;
    # page299 begins a separate category/title, complete prose, date and issuer.
    # Preserve the complete original legal article through that outside block.
    # The following report is the independently reviewed standalone article.
    assert article['text'].endswith('\n'+old['text'])
    article['text']=article['text'][:-len('\n'+old['text'])]
    article['document']['blocks']=blocks[:3]
    assert len(source['articles'])==1
    assert owners[0]is expected['articles'][-1]
    expected['articles'].append(copy.deepcopy(source['articles'][0]))
    return expected

if __name__ == "__main__":
    from pathlib import Path
    root=Path(__file__).resolve().parent;crate=root.parents[2]
    index=json.loads((root/'index.json').read_text())
    for source in index['inputs']:assert hashlib.sha256((crate/source['path']).read_bytes()).hexdigest()==source['sha256']
    before=json.loads((root/index['baseline']).read_text());source=json.loads((crate/index['source_analysis']).read_text())
    assert canonical(before)==index['baseline_Analysis_sha256']and canonical(source)==index['source_analysis_sha256']
    expected=construct(before,source,index['changed_page']);assert canonical(expected)==index['expected_Analysis_sha256']
    assert expected==json.loads((root/index['expected']).read_text())
    assert canonical(json.loads((root/index['control']).read_text()))==index['control_Analysis_sha256']
    print("Complete original pair independently reconstructed; explicit-current control and4 original PDF hashes verified")

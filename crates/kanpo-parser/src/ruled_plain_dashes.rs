//! Preserve existing Plain strips while returning original dash-only amount slots.
use crate::{grid::Grid, trace::{Line, Unit, Word, WordStatus}};
use std::collections::{BTreeMap, BTreeSet};

fn cy(w: &Word) -> f32 { (w.y0+w.y1)*0.5 }
fn cjk(s:&str)->bool {s.chars().any(|c| matches!(c,'ぁ'..='ヿ'|'㐀'..='鿿'))}
fn numeric(s:&str)->bool {!s.is_empty() && s.chars().all(|c|c.is_ascii_digit()||",.−△▲－-()（）".contains(c))}
fn column(w:&Word,g:&Grid)->Option<usize>{
    let cs:Vec<_>=(0..g.xs.len()-1).filter(|&c|g.xs[c]<=w.x0 && w.x1<=g.xs[c+1]).collect();
    (cs.len()==1).then(||cs[0])
}
fn inside(w:&Word,g:&Grid)->bool {g.left()<=w.x0 && w.x1<=g.right() && w.y0>=g.top()-1.25 && w.y1<=g.bottom()+1.25}
fn root(p:&[usize],mut i:usize)->usize{while p[i]!=i{i=p[i];}i}
struct Change { line:usize, text:String, ids:Vec<usize>, added:Vec<usize> }
struct Added { at:usize, y:f32, line:Line }

fn proposal(lines:&[Line],ws:&[Word],g:&Grid)->Option<(Vec<Change>,Vec<Added>)>{
    let n=g.xs.len().checked_sub(1)?;
    if !(4..=16).contains(&n) || g.ys.len()!=3 || g.cells.len()!=n*2
        || !(0..n).all(|c|(0..2).all(|r|g.cells.iter().filter(|p|p.x0==c&&p.x1==c+1&&p.y0==r&&p.y1==r+1).count()==1)) {return None;}
    let ds:Vec<_>=ws.iter().filter(|w|inside(w,g)&&w.raw=="\u{e61c}"&&w.text.is_empty()&&w.status==WordStatus::Empty&&w.y0>g.ys[1]).collect();
    let em=ds.first()?.y1-ds.first()?.y0;
    if !(3.0..=20.0).contains(&em) || g.ys[1]-g.top()<em || g.ys[1]-g.top()>em*6.
        || g.bottom()-g.ys[1]<em*8. || ds.iter().any(|w|(w.y1-w.y0-em).abs()>em*0.1||!(em*0.7..=em*1.3).contains(&(w.x1-w.x0))) {return None;}
    let body:Vec<_>=ws.iter().filter(|w|inside(w,g)&&w.y0>g.ys[1]).collect();
    let header:Vec<_>=ws.iter().filter(|w|inside(w,g)&&w.y1<=g.ys[1]).collect();
    if !(0..n).all(|c|header.iter().any(|w|column(w,g)==Some(c)&&w.status==WordStatus::Used&&cjk(&w.text))) {return None;}
    let mut owners=vec![0usize;ws.len()];
    let mut in_lines=vec![];
    for (i,l) in lines.iter().enumerate(){
        let ids:Option<Vec<_>>=l.words.iter().map(|&id|ws.get(id)).collect();let ids=ids?;
        let ins=ids.iter().filter(|w|inside(w,g)).count();
        if ins==0 {continue;}
        if l.table.is_some()||ins!=ids.len(){return None;}
        for w in ids {owners[w.id]+=1;}
        in_lines.push(i);
    }
    if in_lines.is_empty() || body.iter().any(|w|{
        column(w,g).is_none() || if w.status==WordStatus::Empty {w.raw!="\u{e61c}"||!w.text.is_empty()||owners[w.id]!=0}
        else {w.status!=WordStatus::Used||owners[w.id]!=1||w.text.trim().is_empty()}
    }){return None;}
    if header.iter().any(|w|w.status!=WordStatus::Used||owners[w.id]!=1||column(w,g).is_none()){return None;}
    let mut values:Vec<_>=body.iter().copied().filter(|w|column(w,g)!=Some(0)).collect();
    if values.iter().any(|w|w.status==WordStatus::Used&&!numeric(&w.text)){return None;}
    values.sort_by(|a,b|cy(a).total_cmp(&cy(b)).then(a.x0.total_cmp(&b.x0)));
    let mut rows:Vec<Vec<&Word>>=vec![];
    for w in values {
        if rows.last().is_none_or(|r|(cy(r[0])-cy(w)).abs()>em*0.25){rows.push(vec![]);}
        rows.last_mut()?.push(w);
    }
    if rows.len()<5 || rows.windows(2).any(|p|cy(p[1][0])-cy(p[0][0])<em*1.1){return None;}
    let mut edges:Vec<Vec<f32>>=vec![vec![];n];
    let mut rowids=BTreeMap::new();
    for (ri,row) in rows.iter().enumerate(){
        let y=cy(row[0]);
        let labs:Vec<_>=body.iter().filter(|w|column(w,g)==Some(0)&&(cy(w)-y).abs()<=em*0.25&&cjk(&w.text)).collect();
        if labs.is_empty(){return None;}
        for c in 1..n{
            let mut slot:Vec<_>=row.iter().copied().filter(|w|column(w,g)==Some(c)).collect();
            slot.sort_by(|a,b|a.x0.total_cmp(&b.x0));
            if slot.is_empty(){return None;}
            let d=slot.iter().filter(|w|w.status==WordStatus::Empty).count();
            if d>0 {if slot.len()!=1{return None;}}
            else {
                let t:String=slot.iter().map(|w|w.text.as_str()).collect();
                if !numeric(&t)||t.chars().filter(|c|c.is_ascii_digit()).count()<2
                    ||slot.windows(2).any(|p|p[1].x0-p[0].x1>em*0.75){return None;}
                edges[c].push(slot.iter().map(|w|w.x1).fold(f32::NEG_INFINITY,f32::max));
            }
            for w in slot {rowids.insert(w.id,ri);}
        }
    }
    for c in 1..n{
        if edges[c].len()<3{return None;}
        let lo=edges[c].iter().copied().fold(f32::INFINITY,f32::min);
        let hi=edges[c].iter().copied().fold(f32::NEG_INFINITY,f32::max);
        if hi-lo>em*0.15 || !(0.0..=em*1.5).contains(&(g.xs[c+1]-hi)){return None;}
        if ds.iter().filter(|w|column(w,g)==Some(c)).any(|w|(w.x1-hi).abs()>em*0.15){return None;}
    }
    // Existing lines certify which physical columns are already read together.
    let mut parents:Vec<_>=(0..n).collect();
    for &i in &in_lines{
        let cs:Option<Vec<_>>=lines[i].words.iter().map(|&id|column(&ws[id],g)).collect();let cs=cs?;
        for c in cs {let a=root(&parents,c);let b=root(&parents,column(&ws[lines[i].words[0]],g)?);parents[a]=b;}
    }
    let group:Vec<_>=(0..n).map(|c|root(&parents,c)).collect();
    let mut ordered:Vec<usize>=vec![];
    for &v in &group {if ordered.last()!=Some(&v){if ordered.contains(&v){return None;}ordered.push(v);}}
    let rank=|c:usize|ordered.iter().position(|v|*v==group[c]).unwrap();
    let mut group_lines:Vec<Vec<usize>>=vec![vec![];ordered.len()];
    let mut previous=0;
    for &i in &in_lines{
        let k=rank(column(&ws[lines[i].words[0]],g)?);
        if k<previous{return None;}previous=k;group_lines[k].push(i);
    }
    let mut new_by_line:BTreeMap<usize,Vec<usize>>=BTreeMap::new();
    let mut new_by_row:BTreeMap<(usize,usize),Vec<usize>>=BTreeMap::new();
    for d in &ds{
        let c=column(d,g)?;if c==0{return None;}
        let k=rank(c);let ri=*rowids.get(&d.id)?;let y=cy(d);
        let targets:Vec<_>=group_lines[k].iter().copied().filter(|&i|lines[i].words.iter().any(|&id|{
            let w=&ws[id];w.y0>g.ys[1]&&(cy(w)-y).abs()<=em*0.25
        })).collect();
        match targets.as_slice(){
            [i]=>{new_by_line.entry(*i).or_default().push(d.id);}
            []=>{if rank(0)==k{return None;}new_by_row.entry((k,ri)).or_default().push(d.id);}
            _=>return None,
        }
    }
    let mut changes=vec![];
    for (i,mut added) in new_by_line{
        let l=&lines[i];let mut cursor=0;let mut offsets=vec![];
        for &id in &l.words{
            let t=&ws[id].text;let rest=l.text.get(cursor..)?;let start=cursor+rest.find(t)?;
            if !l.text[cursor..start].chars().all(char::is_whitespace){return None;}
            offsets.push((id,start));cursor=start+t.len();
        }
        if !l.text[cursor..].chars().all(char::is_whitespace){return None;}
        added.sort_by(|&a,&b|ws[a].x0.total_cmp(&ws[b].x0));
        let mut inserts:BTreeMap<usize,Vec<usize>>=BTreeMap::new();
        for &id in &added{
            let at=offsets.iter().find(|(w,_)|ws[*w].x0>ws[id].x0).map(|v|v.1).unwrap_or(l.text.len());
            inserts.entry(at).or_default().push(id);
        }
        let mut text=l.text.clone();
        for (&at,ids) in inserts.iter().rev(){
            let value=vec!["－";ids.len()].join("　");
            text.insert_str(at,&if at==l.text.len(){format!("　{value}")}else{format!("{value}　")});
        }
        let mut ids=vec![];
        let mut ds=added.iter().peekable();
        for &id in &l.words{while ds.peek().is_some_and(|&&d|ws[d].x0<ws[id].x0){ids.push(*ds.next()?);}ids.push(id);}
        ids.extend(ds.copied());
        changes.push(Change{line:i,text,ids,added});
    }
    let mut added_lines=vec![];
    for ((k,ri),mut ids) in new_by_row{
        let y=cy(rows[ri][0]);ids.sort_by(|&a,&b|ws[a].x0.total_cmp(&ws[b].x0));
        let cols:BTreeSet<_>=ids.iter().filter_map(|&id|column(&ws[id],g)).collect();
        let expected:BTreeSet<_>=(1..n).filter(|&c|rank(c)==k).collect();
        if cols!=expected||cols.len()!=ids.len(){return None;}
        let ls=&group_lines[k];
        let at=ls.iter().copied().find(|&i|lines[i].words.iter().map(|&id|cy(&ws[id])).fold(f32::INFINITY,f32::min)>y).unwrap_or(ls.last()?+1);
        added_lines.push(Added{at,y,line:Line::text(vec!["－";ids.len()].join("　"),ids)});
    }
    if changes.iter().flat_map(|v|&v.added).chain(added_lines.iter().flat_map(|v|&v.line.words)).copied().collect::<BTreeSet<_>>() !=ds.iter().map(|w|w.id).collect(){return None;}
    Some((changes,added_lines))
}

pub(crate) fn restore(units:&mut[Unit],ws:&mut[Word],grids:&[Grid]){
    // A single existing Plain unit prevents crossing an article/new-old boundary.
    let [Unit::Plain(lines)]=units else{return;};
    for g in grids{
        let Some((changes,mut added))=proposal(lines,ws,g)else{continue;};
        for p in changes{
            lines[p.line].text=p.text;lines[p.line].words=p.ids;
            for id in p.added{ws[id].text="－".into();ws[id].status=WordStatus::Used;}
        }
        added.sort_by(|a,b|b.at.cmp(&a.at).then(b.y.total_cmp(&a.y)));
        for p in added{
            for &id in &p.line.words{ws[id].text="－".into();ws[id].status=WordStatus::Used;}
            lines.insert(p.at,p.line);
        }
    }
}

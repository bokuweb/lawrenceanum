//! Attach only an original unowned hyphen touching a terminal stereochemical L/D prefix.
use crate::trace::{Line,Unit,Word,WordStatus};
fn groups(units: &[Unit]) -> Vec<&Vec<Line>> {
    let mut out=vec![];
    for u in units {match u {
        Unit::Plain(l)=>out.push(l),
        Unit::Shinkyu{preamble,after,before,outside}=>{out.push(preamble);out.extend(after);out.extend(before);out.extend(outside);}
    }} out
}
fn groups_mut(units: &mut[Unit]) -> Vec<&mut Vec<Line>> {
    let mut out=vec![];
    for u in units {match u {
        Unit::Plain(l)=>out.push(l),
        Unit::Shinkyu{preamble,after,before,outside}=>{out.push(preamble);out.extend(after);out.extend(before);out.extend(outside);}
    }} out
}
fn katakana_pair(text: &str) -> bool {
    let mut chars=text.chars();
    (0..2).all(|_| chars.next().is_some_and(|c|matches!(c,'ァ'..='ヺ')))
}
fn bounds(line:&Line,words:&[Word])->Option<(f32,f32,f32,f32)> {
    let first=words.get(*line.words.first()?)?;
    let mut b=(first.x0,first.y0,first.x1,first.y1);
    for &id in &line.words {let w=words.get(id)?;b=(b.0.min(w.x0),b.1.min(w.y0),b.2.max(w.x1),b.3.max(w.y1));}Some(b)
}
pub(crate) fn restore(units:&mut[Unit],words:&mut[Word]) {
    let gs=groups(units);
    let mut owners=vec![0usize;words.len()];
    for g in &gs {for l in g.iter() {
        for &id in &l.words {if let Some(n)=owners.get_mut(id){*n+=1}}
        if let Some(t)=&l.table {for c in &t.cells {for &id in &c.words {if let Some(n)=owners.get_mut(id){*n+=1}}}}
    }}
    let mut edits=vec![];
    for (gi,g) in gs.iter().enumerate() {for (li,pair) in g.windows(2).enumerate() {
        let (l,next)=(&pair[0],&pair[1]);
        if l.table.is_some() || next.table.is_some() || !katakana_pair(&next.text) {continue}
        let Some(&pid)=l.words.last() else {continue};
        let Some(p)=words.get(pid) else {continue};
        if owners[pid]!=1 || p.status!=WordStatus::Used || !matches!(p.raw.as_str(),"Ｌ"|"Ｄ"|"L"|"D") || p.raw!=p.text || !l.text.ends_with(&p.text) {continue}
        let (Some(lb),Some(nb))=(bounds(l,words),bounds(next,words)) else {continue};
        let ph=p.y1-p.y0;
        if ph<=0. || lb.2>p.x1+0.05 {continue}
        let candidates:Vec<_>=words.iter().filter(|d| {
            let h=d.y1-d.y0;let width=d.x1-d.x0;let scale=h.max(ph);
            d.raw=="\u{E61C}" && d.status==WordStatus::Empty && d.text.is_empty()
                && owners.get(d.id)==Some(&0) && h>0.
                && (0.7*h..=1.3*h).contains(&width)
                && (0.0..=0.15*scale).contains(&(d.x0-p.x1))
                && ((d.y0+d.y1-p.y0-p.y1)*0.5).abs()<=0.25*scale
                && (d.y1.min(p.y1)-d.y0.max(p.y0))>=0.8*ph.min(h)
                && (nb.0-lb.0).abs()<=1.5*scale
                && (1.0*scale..=2.2*scale).contains(&(nb.1-lb.1))
                && !words.iter().any(|w|w.id!=d.id && w.id!=p.id && !w.raw.is_empty()
                    && w.x1.min(d.x1)-w.x0.max(d.x0)>0.02
                    && w.y1.min(d.y1)-w.y0.max(d.y0)>=0.75*(w.y1-w.y0).min(h)
                    && ((w.y0+w.y1-d.y0-d.y1)*0.5).abs()<=0.3*(w.y1-w.y0).max(h))
        }).map(|d|d.id).collect();
        if candidates.len()==1 {edits.push((gi,li,candidates[0]));}
    }}
    drop(gs);
    // A raw hyphen must not be assigned to two output lines even with malformed geometry.
    let mut uses=std::collections::BTreeMap::new();
    for &(_,_,id) in &edits {*uses.entry(id).or_insert(0)+=1;}
    let mut gs=groups_mut(units);
    for (gi,li,id) in edits {if uses[&id]!=1 {continue}let l=&mut gs[gi][li];l.text.push('－');l.words.push(id);words[id].text="－".into();words[id].status=WordStatus::Used;}
}
#[cfg(test)]
#[path="chemical_line_end_tests.rs"] mod tests;

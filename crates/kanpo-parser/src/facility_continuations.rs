//! A two-record continuation beneath an original five-field vertical header.
//! Only regroup final Plain lines whose source ownership and ruled page band agree.
use std::collections::{HashMap, HashSet};
use crate::lines::PageRules;
use crate::trace::{Line, Unit, Word, WordStatus};

fn compact(s: &str) -> String { s.chars().filter(|c| !c.is_whitespace()).collect() }
fn cx(w: &Word) -> f32 { (w.x0 + w.x1) * 0.5 }
fn em(w: &Word) -> f32 { w.x1 - w.x0 }
fn code(s: &str) -> bool {
    (3..=8).contains(&s.chars().count()) && s.chars().all(|c| "〇一二三四五六七八九０１２３４５６７８９0123456789".contains(c))
}
fn walk(u: &Unit, f: &mut impl FnMut(&Line)) {
    match u {
        Unit::Plain(ls) => ls.iter().for_each(f),
        Unit::Shinkyu { preamble, after, before, outside } => {
            for l in preamble { f(l); }
            for ls in after.iter().chain(before).chain(outside) { for l in ls { f(l); } }
        }
    }
}
fn ownership(units: &[Unit]) -> HashMap<usize, usize> {
    let mut out = HashMap::new();
    for u in units { walk(u, &mut |l| {
        for id in &l.words { *out.entry(*id).or_insert(0) += 1; }
        if let Some(t) = &l.table { for c in &t.cells { for id in &c.words { *out.entry(*id).or_insert(0) += 1; } } }
    }); }
    out
}
fn source<'a>(line: &Line, by: &HashMap<usize, &'a Word>, owners: &HashMap<usize, usize>) -> Option<Vec<&'a Word>> {
    if line.table.is_some() || line.words.is_empty() { return None; }
    let mut ws = line.words.iter().map(|id| by.get(id).copied()).collect::<Option<Vec<_>>>()?;
    if ws.iter().any(|w| w.status != WordStatus::Used || w.text.trim().is_empty() || owners.get(&w.id) != Some(&1)) { return None; }
    ws.sort_by(|a,b| a.y0.total_cmp(&b.y0).then(b.x0.total_cmp(&a.x0)));
    (compact(&ws.iter().map(|w| w.text.as_str()).collect::<String>()) == compact(&line.text)).then_some(ws)
}
#[derive(Clone)]
struct Header { starts: [f32;5], ends: [f32;5], em: f32 }
fn header(line: &Line, by: &HashMap<usize, &Word>, owners: &HashMap<usize, usize>) -> Option<Header> {
    let ws = source(line,by,owners)?;
    if compact(&line.text) != "施設番号施設名所在地名所有関係摘要" { return None; }
    let e = em(ws[0]);
    if !(5.0..=14.0).contains(&e) || ws.iter().any(|w| (cx(w)-cx(ws[0])).abs()>e*0.4 || (em(w)-e).abs()>e*0.25) { return None; }
    let mut starts=[0.0;5]; let mut ends=[0.0;5]; let mut at=0;
    for (i,label) in ["施設番号","施設名","所在地名","所有関係","摘要"].iter().enumerate() {
        starts[i]=ws.get(at)?.y0;
        let mut text=String::new();
        while text.len()<label.len() { let w=ws.get(at)?;text.push_str(&compact(&w.text));ends[i]=w.y1;at+=1; }
        if text!=*label || (i>0 && starts[i]<=ends[i-1]) {return None;}
    }
    (at==ws.len()).then_some(Header{starts,ends,em:e})
}
fn original_header(previous: &Unit, by: &HashMap<usize,&Word>, owners: &HashMap<usize,usize>) -> Option<Header> {
    let Unit::Plain(ls)=previous else {return None;};
    let found:Vec<_>=ls.iter().enumerate().filter_map(|(i,l)|header(l,by,owners).map(|h|(i,h))).collect();
    let [(i,h)]=found.as_slice() else {return None;};
    if ls[i+1..].iter().any(|l|l.table.is_some() || l.text.trim_start().starts_with(['〇','○','◎'])) {return None;}
    // The header must have a preceding printed record, not be an isolated legend.
    if !ls[i+1..].iter().any(|l| l.words.first().and_then(|id|by.get(id)).is_some_and(|w| code(&w.text))) {return None;}
    Some(h.clone())
}

/// Upright vertical pages only; call after existing restorers and before Unit::text.
/// No Word fields are modified. Existing unrelated Units and all source IDs survive.
pub(crate) fn restore(units: &mut Vec<Unit>, words: &[Word], rules: Option<&PageRules>, width: f32, height: f32) {
    let Some(rules)=rules else {return;};
    if !(width.is_finite() && height.is_finite() && height>width) {return;}
    let by:HashMap<_,_>=words.iter().map(|w|(w.id,w)).collect();
    if by.len()!=words.len() {return;}
    let mut i=1;
    while i+3<units.len() {
        let owners=ownership(units);
        if let Some(lines)=candidate(&units[i-1],&units[i..i+4],words,&by,&owners,rules,width,height) {
            units.splice(i..i+4,[Unit::Plain(lines)]);
        }
        i+=1;
    }
}
fn candidate(previous: &Unit, group: &[Unit], all: &[Word], by: &HashMap<usize,&Word>, owners: &HashMap<usize,usize>, rules:&PageRules,width:f32,height:f32) -> Option<Vec<Line>> {
    let h=original_header(previous,by,owners)?;
    let bands=group.iter().map(|u|match u {Unit::Plain(ls)=>Some(ls.as_slice()),_=>None}).collect::<Option<Vec<_>>>()?;
    if bands.len()!=4 || bands[0].len()!=2 || bands[1].len()!=2 || bands[2].len()!=2 || bands[3].len()<2 {return None;}
    let mut fields:Vec<Vec<Vec<&Word>>>=vec![vec![Vec::new();5];2];
    let mut anchors=[0.0;2]; let mut starts=[[0.0;5];2]; let mut e=0.0;
    for r in 0..2 {
        let ws=source(&bands[0][r],by,owners)?;
        let [number,name]=ws.as_slice() else {return None;};
        if !code(&number.text) || code(&name.text) || !(2..=30).contains(&name.text.chars().count()) || name.text.contains(['。','：','〇']) {return None;}
        e+=em(number)*0.5;
        anchors[r]=cx(number);starts[r][0]=number.y0;starts[r][1]=name.y0;
        fields[r][0].push(number);fields[r][1].push(name);
    }
    if !(5.0..=14.0).contains(&e) || (e-h.em).abs()>e*0.25 || !(2.5*e..=20.0*e).contains(&(anchors[0]-anchors[1]))
        || fields[0][0][0].text==fields[1][0][0].text || fields[0][1][0].text==fields[1][1][0].text {return None;}
    for (band,f) in [(1,2),(2,3)] { for r in 0..2 {
        let ws=source(&bands[band][r],by,owners)?; let [w]=ws.as_slice() else {return None;};
        if w.text.contains(['。','：','〇']) || !(2..=24).contains(&w.text.chars().count()) {return None;}
        starts[r][f]=w.y0;fields[r][f].push(w);
    } }
    let mut notes=[Vec::<String>::new(),Vec::<String>::new()];
    let mut last_row=0;let mut last_x=f32::INFINITY;
    for line in bands[3] {
        let ws=source(line,by,owners)?;
        if ws.iter().any(|w|w.text.trim_start().starts_with(['〇','○','◎'])) {return None;}
        let x=cx(ws[0]);
        let r=anchors.iter().rposition(|a|x<=a+e*0.35)?;
        if r<last_row || (r==last_row && x>last_x+e*0.35) || ws.iter().any(|w|(cx(w)-x).abs()>e*0.35) {return None;}
        if fields[r][4].is_empty() {if (x-anchors[r]).abs()>e*0.35 {return None;} starts[r][4]=ws[0].y0;}
        notes[r].push(line.text.clone()); fields[r][4].extend(ws);last_row=r;last_x=x;
    }
    if fields.iter().any(|fs|fs.iter().any(Vec::is_empty)) {return None;}
    for r in 0..2 { for f in 0..4 {
        if (cx(fields[r][f][0])-anchors[r]).abs()>e*0.35 {return None;}
    } }
    if (0..5).any(|f|(starts[0][f]-starts[1][f]).abs()>e*0.35) {return None;}
    let mut offsets:Vec<_>=(0..5).map(|f|starts[0][f]-h.starts[f]).collect();offsets.sort_by(f32::total_cmp);let shift=offsets[2];
    if offsets.iter().any(|d|(*d-shift).abs()>e*1.25) {return None;}
    let boundaries:Vec<_>=(0..4).map(|f|(h.ends[f]+h.starts[f+1])*0.5+shift).collect();
    for fs in &fields { for (f,ws) in fs.iter().enumerate() { for w in ws {
        if (em(w)-e).abs()>e*0.25 || (f>0 && w.y0<boundaries[f-1]-e*0.5) || (f<4 && w.y1>boundaries[f]+e*0.5) {return None;}
    } } }
    let selected:Vec<_>=fields.iter().flatten().flatten().copied().collect();
    let ids:HashSet<_>=selected.iter().map(|w|w.id).collect();if ids.len()!=selected.len(){return None;}
    let x0=selected.iter().map(|w|w.x0).fold(f32::INFINITY,f32::min);let x1=selected.iter().map(|w|w.x1).fold(f32::NEG_INFINITY,f32::max);
    let y0=selected.iter().map(|w|w.y0).fold(f32::INFINITY,f32::min);let y1=selected.iter().map(|w|w.y1).fold(f32::NEG_INFINITY,f32::max);
    let cover=|r:&&crate::lines::HRule|r.x0<=x0+e*0.5 && r.x1>=x1-e*0.5 && r.x1-r.x0>=width*0.6;
    let top:Vec<_>=rules.horizontal.iter().filter(cover).filter(|r| r.y<=y0 && y0-r.y<e*2.0 && (r.y-height*0.5).abs()<e*2.0).collect();
    let bottom:Vec<_>=rules.horizontal.iter().filter(cover).filter(|r|r.y>=y1 && r.y-y1<e*2.0).collect();
    if top.len()!=1 || bottom.len()!=1 || rules.horizontal.iter().any(|r|r.y>top[0].y+e*0.5 && r.y<bottom[0].y-e*0.5 && r.x0<x1 && r.x1>x0)
        || rules.vertical.iter().any(|r|r.x>x0+e*0.5 && r.x<x1-e*0.5 && r.y0<y1 && r.y1>y0) {return None;}
    if all.iter().any(|w|w.status==WordStatus::Used && !ids.contains(&w.id) && cx(w)>x0 && cx(w)<x1 && (w.y0+w.y1)*0.5>y0 && (w.y0+w.y1)*0.5<y1) {return None;}
    let mut result=Vec::new();
    for r in 0..2 {
        let mut values:Vec<String>=fields[r][..4].iter().map(|ws|ws.iter().map(|w|w.text.as_str()).collect()).collect();
        values.push(notes[r].concat());
        result.push(Line::text(values.join("　"),fields[r].iter().flatten().map(|w|w.id).collect()));
    }
    let mut old:Vec<_>=bands.iter().flat_map(|ls|ls.iter()).flat_map(|l|l.text.chars()).filter(|c|!c.is_whitespace()).collect();
    let mut new:Vec<_>=result.iter().flat_map(|l|l.text.chars()).filter(|c|!c.is_whitespace()).collect();old.sort_unstable();new.sort_unstable();
    let original_ids:Vec<_>=bands.iter().flat_map(|ls|ls.iter()).flat_map(|l|l.words.iter()).copied().collect();
    if old!=new || original_ids.len()!=ids.len() || original_ids.iter().any(|id|!ids.contains(id)) {return None;}
    Some(result)
}

#[cfg(test)]
#[path="facility_continuation_shape_tests.rs"]
mod tests;

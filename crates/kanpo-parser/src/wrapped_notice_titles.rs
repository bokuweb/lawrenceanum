//! Restore only the article boundary inside an original multi-line notice title.
use crate::trace::{PageTrace, Region, Role, Word, WordStatus};

/// The page text, its line/word ownership and every source Word remain untouched.
/// A standalone notice label can be the last line of a larger printed title.
pub(crate) fn repair_wrapped_notice_titles(articles: &mut [String], pages: &[PageTrace]) -> usize {
    let mut changed = 0;
    for i in 1..articles.len() {
        let Some(label) = articles[i].lines().next() else { continue };
        if !matches!(label, "公告" | "公示") { continue; }
        let mut candidates = Vec::new();
        for page in pages {
            for prefix in prefixes(&page.words, &page.regions, label) {
                let suffix = format!("\n{prefix}");
                if articles[i-1].ends_with(&suffix) { candidates.push(prefix); }
            }
        }
        // Repeated labels or titles at different physical positions are ambiguous.
        if candidates.len() != 1 { continue; }
        let prefix = &candidates[0];
        let end = articles[i-1].len() - prefix.len() - 1;
        if articles[i-1][..end].trim().is_empty() { continue; }
        articles[i-1].truncate(end);
        articles[i] = format!("{prefix}\n{}", articles[i]);
        changed += 1;
    }
    changed
}

#[derive(Clone, Copy)]
struct Box2 { x0:f32, y0:f32, x1:f32, y1:f32, em:f32 }
fn compact(s:&str)->String { s.chars().filter(|c|!c.is_whitespace()).collect() }
fn line_box(words:&[Word], line:&crate::trace::Line)->Option<Box2> {
    if line.table.is_some() || line.words.is_empty() { return None; }
    let mut ws=Vec::new();
    for id in &line.words {
        if line.words.iter().filter(|x|*x==id).count()!=1 { return None; }
        let mut found=words.iter().filter(|w|w.id==*id);
        let w=found.next()?;
        if found.next().is_some() || w.status!=WordStatus::Used || w.text.is_empty() { return None; }
        let h=w.y1-w.y0; let width=w.x1-w.x0; let chars=compact(&w.text).chars().count() as f32;
        if chars<1. || ![w.x0,w.y0,w.x1,w.y1].iter().all(|v|v.is_finite()) || h<=0. || width<0.5*h*chars {return None;}
        ws.push(w);
    }
    if compact(&ws.iter().map(|w|w.text.as_str()).collect::<String>())!=compact(&line.text) {return None;}
    let em=ws.iter().map(|w|w.y1-w.y0).fold(0.,f32::max);
    let b=Box2{x0:ws.iter().map(|w|w.x0).fold(f32::INFINITY,f32::min),y0:ws.iter().map(|w|w.y0).fold(f32::INFINITY,f32::min),x1:ws.iter().map(|w|w.x1).fold(f32::NEG_INFINITY,f32::max),y1:ws.iter().map(|w|w.y1).fold(f32::NEG_INFINITY,f32::max),em};
    if ws.iter().any(|w| (w.y1-w.y0-em).abs()>0.1*em || (w.y0-b.y0).abs()>0.15*em) {return None;}
    Some(b)
}
fn prefixes(words:&[Word], regions:&[Region], label:&str)->Vec<String> {
    let mut found=Vec::new();
    for r in regions {
        if r.role!=Role::Body {continue;}
        for end in 1..r.lines.len().saturating_sub(1) {
            if r.lines[end].text!=label {continue;}
            for count in 1..=3usize.min(end) {
                let lines=&r.lines[end-count..=end];
                let prefix=lines[..count].iter().map(|l|l.text.as_str()).collect::<Vec<_>>().join("\n");
                let title=compact(&prefix);
                if !title.ends_with("に関する") || !(12..=120).contains(&title.chars().count()) || title.contains(['。','、','「','」','：',':']) {continue;}
                let Some(boxes)=lines.iter().map(|l|line_box(words,l)).collect::<Option<Vec<_>>>() else {continue;};
                let em=boxes[0].em;
                if boxes.iter().any(|b|(b.em-em).abs()>0.1*em || (b.x0-boxes[0].x0).abs()>0.35*em) {continue;}
                if boxes.windows(2).any(|b|{let dy=b[1].y0-b[0].y0; !(0.75*em..=1.6*em).contains(&dy)}) {continue;}
                // Do not take only the tail of a longer equally aligned title.
                // If more than three prefix lines remain, leave the boundary alone.
                if end>count {
                    if let Some(prev)=line_box(words,&r.lines[end-count-1]) {
                        let dy=boxes[0].y0-prev.y0;
                        if (prev.em-em).abs()<=0.1*em && (prev.x0-boxes[0].x0).abs()<=0.35*em && (0.75*em..=1.6*em).contains(&dy) {continue;}
                    }
                }
                let Some(body)=line_box(words,&r.lines[end+1]) else {continue;};
                let x0=boxes.iter().map(|b|b.x0).fold(f32::INFINITY,f32::min);
                let x1=boxes.iter().map(|b|b.x1).fold(f32::NEG_INFINITY,f32::max);
                let y0=boxes[0].y0; let y1=boxes.last().unwrap().y1;
                // A larger, tightly aligned title sits above the wider body column.
                if em<1.06*body.em || em>1.6*body.em || body.y0<=boxes.last().unwrap().y0 || body.y0-y1>2.*em || body.x0>x0 || body.x1<x1 || compact(&r.lines[end+1].text).chars().count()<20 {continue;}
                let ids:Vec<_>=lines.iter().flat_map(|l|l.words.iter().copied()).collect();
                // Every title Word has exactly one existing logical owner.
                if ids.iter().any(|id| regions.iter().flat_map(|r|&r.lines).map(|l|l.words.iter().filter(|x|*x==id).count()+l.table.iter().flat_map(|t|&t.cells).flat_map(|c|&c.words).filter(|x|*x==id).count()).sum::<usize>()!=1) {continue;}
                // A neighbouring article, table fragment or unrelated used word may not
                // occupy the same printed title rectangle.
                if words.iter().any(|w|w.status==WordStatus::Used && !ids.contains(&w.id) && w.x0<x1 && w.x1>x0 && w.y0<y1 && w.y1>y0) {continue;}
                found.push(prefix);
            }
        }
    }
    found
}

#[cfg(test)]
#[path = "wrapped_notice_title_tests.rs"]
mod tests;

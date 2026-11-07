//! Paragraph boundaries and signature order certified by original formal lines.
use kanpo_parser::{
    media::{Block, Document, Inline},
    trace::WordStatus,
    Analysis,
};
use std::collections::{BTreeMap, BTreeSet};

type Runs = Vec<(String, bool)>;
#[derive(Default)]
struct Plan {
    cuts: BTreeSet<usize>,
    spaces: BTreeMap<usize, usize>,
    move_signature: Option<(usize, usize, usize)>,
    signature_leading: BTreeMap<usize, usize>,
}
struct Line {
    text: String,
    start: usize,
    end: usize,
}
struct Source {
    page: usize,
    bounds: [f32; 4],
    role_boundary: bool,
    spaces: Vec<(usize, usize)>,
    regular: bool,
}

fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
fn number(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| "〇零一二三四五六七八九十百千0123456789０１２３４５６７８９".contains(c))
}
fn date(s: &str) -> bool {
    let Some((year, rest)) = s.split_once('年') else {
        return false;
    };
    let Some((month, day)) = rest.split_once('月') else {
        return false;
    };
    let Some(day) = day.strip_suffix('日') else {
        return false;
    };
    let era_end = year
        .find(|c: char| c == '元' || number(&c.to_string()))
        .unwrap_or(year.len());
    let era = &year[..era_end];
    let year = &year[era_end..];
    (era.is_empty()
        || (era.chars().count() <= 4
            && era.chars().all(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c))))
        && (year == "元" || number(year))
        && number(month)
        && number(day)
}
fn office_end(s: &str) -> Option<usize> {
    let end = ["大臣", "長官", "委員長"]
        .iter()
        .filter_map(|role| s.find(role).map(|i| i + role.len()))
        .min()?;
    let office = &s[..end];
    let name = &s[end..];
    (office.chars().count() <= 18
        && office
            .chars()
            .all(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c))
        && !office.contains('第')
        && (2..=20).contains(&name.chars().count())
        && name.chars().all(|c| {
            ('\u{3041}'..='\u{30FF}').contains(&c)
                || ('\u{4E00}'..='\u{9FFF}').contains(&c)
                || c.is_ascii_alphabetic()
        }))
    .then_some(end)
}
fn dated_signature(s: &str) -> bool {
    s.find('日').is_some_and(|i| {
        date(&s[..i + '日'.len_utf8()]) && office_end(&s[i + '日'.len_utf8()..]).is_some()
    })
}
fn outside_quotes(text: &str) -> bool {
    let mut depth = 0i32;
    for c in text.chars() {
        match c {
            '「' | '『' => depth += 1,
            '」' | '』' => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            return false;
        }
    }
    depth == 0
}
fn identifier(s: &str) -> bool {
    let s = s.trim_start_matches(['〇', '○']);
    let Some((kind, n)) = s.split_once('第') else {
        return false;
    };
    let Some(n) = n.strip_suffix('号') else {
        return false;
    };
    !kind.is_empty()
        && !kind.contains('年')
        && kind.chars().count() <= 20
        && kind
            .chars()
            .all(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c) || c == '・')
        && number(n)
}

fn sources(analysis: &Analysis, text: &str) -> Vec<Source> {
    let mut found = Vec::new();
    for (page, p) in analysis.pages.iter().enumerate() {
        for line in p
            .regions
            .iter()
            .flat_map(|r| &r.lines)
            .filter(|l| l.table.is_none() && compact(&l.text) == text)
        {
            let Some(words) = line
                .words
                .iter()
                .map(|&id| p.words.get(id))
                .collect::<Option<Vec<_>>>()
            else {
                return Vec::new();
            };
            if words.is_empty()
                || words.iter().any(|w| w.status != WordStatus::Used)
                || words.iter().map(|w| compact(&w.text)).collect::<String>() != text
            {
                continue;
            }
            let mut bounds = [
                f32::INFINITY,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
            ];
            let mut ends = BTreeSet::new();
            let mut offset = 0;
            let mut spaces = Vec::new();
            for (i, w) in words.iter().enumerate() {
                let b = w.source_box;
                if !b.iter().all(|v| v.is_finite()) || b[0] >= b[2] || b[1] >= b[3] {
                    return Vec::new();
                }
                bounds[0] = bounds[0].min(b[0]);
                bounds[1] = bounds[1].min(b[1]);
                bounds[2] = bounds[2].max(b[2]);
                bounds[3] = bounds[3].max(b[3]);
                offset += compact(&w.text).len();
                ends.insert(offset);
                if let Some(next) = words.get(i + 1) {
                    if let Some(n) = crate::spacing::measured(w, next).filter(|&n| n > 0) {
                        spaces.push((offset, n));
                    }
                }
            }
            found.push(Source {
                page,
                bounds,
                role_boundary: office_end(text).is_some_and(|n| ends.contains(&n)),
                spaces,
                regular: {
                    let cross = if bounds[3]-bounds[1] > bounds[2]-bounds[0] { 0 } else { 1 };
                    let width = bounds[cross+2]-bounds[cross];
                    words.iter().all(|w| w.source_box[cross+2]-w.source_box[cross] >= width / 1.2)
                },
            });
        }
    }
    found
}

#[cfg(test)]
fn adjacent_signatures(a: &Source, b: &Source) -> bool {
    signature_pitch(a,b).is_some()
}
fn signature_pitch(a: &Source, b: &Source) -> Option<f32> {
    source_pitch(a,b,true)
}
fn source_pitch(a: &Source, b: &Source, require_anchor: bool) -> Option<f32> {
    if a.page != b.page || !a.regular || !b.regular { return None; }
    for axis in [0, 1] {
        let cross = 1 - axis;
        let extent = |s: &Source, i: usize| s.bounds[i + 2] - s.bounds[i];
        let em = extent(a, cross).min(extent(b, cross));
        if em <= 0. || extent(a, axis) < 1.5 * extent(a, cross)
            || extent(b, axis) < 1.5 * extent(b, cross)
            || extent(a, cross).max(extent(b, cross)) > 1.2 * em { continue; }
        let center = |s: &Source| (s.bounds[cross] + s.bounds[cross + 2]) / 2.;
        let pitch = if axis == 1 { center(a) - center(b) } else { center(b) - center(a) };
        let aligned = (a.bounds[axis] - b.bounds[axis]).abs() <= 0.5 * em
            || (a.bounds[axis + 2] - b.bounds[axis + 2]).abs() <= 0.5 * em;
        if (!require_anchor || aligned) && (0.8 * em..=2.5 * em).contains(&pitch) { return Some(pitch); }
    }
    None
}

// The paragraph immediately before a signature group may share its original
// leading too, but only when every complete source line has the same measured
// pitch through the first signer. Mixed-size/ruby lines cannot authorize exact
// leading. This also keeps a lone signer from overflowing because Word's font
// fallback has a taller default line box than the source PDF.
fn preceding_leading(article: &str, analysis: &Analysis, raw: &str, offset: usize, signatures: &BTreeMap<usize,usize>) -> Option<(usize,usize)> {
    let (&first,&twips) = signatures.range(offset..).next()?;
    let end = first.checked_sub(offset)?;
    let mut starts = vec![0];let mut cursor = 0;
    for c in raw.chars() {
        if c=='\n' { starts.push(cursor); }
        else if !c.is_whitespace() { cursor += c.len_utf8(); }
    }
    let start = starts.into_iter().filter(|&s| s<end).max()?;
    let mut at = 0;let mut lines = vec![];let mut first_text = None;
    for text in article.lines().map(compact).filter(|s|!s.is_empty()) {
        let begin = at;at += text.len();
        if begin==first { first_text=Some(text.clone()); }
        if begin>=offset+start && at<=first { lines.push((begin,at,text)); }
    }
    if lines.first()?.0!=offset+start || lines.last()?.1!=first { return None; }
    let mut physical = vec![];
    for (_,_,text) in lines {
        let mut found=sources(analysis,&text);
        if found.len()!=1 { return None; }
        let own=found.remove(0);
        if analysis.pages[own.page].words.iter().any(|w|w.status==WordStatus::Ruby) { return None; }
        physical.push(own);
    }
    let rounded = |p:f32| (p*20.).round() as usize;
    if physical.windows(2).any(|p|source_pitch(&p[0],&p[1],false).map(rounded)!=Some(twips)) { return None; }
    let tail=physical.last()?;
    let next=sources(analysis,&first_text?);
    if next.iter().filter(|b|source_pitch(tail,b,false).map(rounded)==Some(twips)).count()!=1 { return None; }
    Some((start,twips))
}

impl Plan {
    fn line(&mut self, l: &Line, analysis: &Analysis) {
        self.cuts.extend([l.start, l.end]);
        if l.text == "御名御璽" {
            self.spaces.insert(l.start + "御名".len(), 1);
        }
        if office_end(&l.text).is_some() || dated_signature(&l.text) {
            let candidates = sources(analysis, &l.text);
            if let Some(first) = candidates
                .first()
                .filter(|first| candidates.iter().all(|c| c.spaces == first.spaces))
            {
                self.spaces
                    .extend(first.spaces.iter().map(|&(i, n)| (l.start + i, n)));
            }
        }
    }
    fn new(article: &str, analysis: &Analysis, available: usize) -> Self {
        let mut at = 0;
        let lines = article
            .lines()
            .map(compact)
            .filter(|s| !s.is_empty())
            .map(|text| {
                let start = at;
                at += text.len();
                Line {
                    text,
                    start,
                    end: at,
                }
            })
            .collect::<Vec<_>>();
        let mut plan = Self::default();
        if let Some(i) = lines.iter().take(12).position(|l| l.text == "御名御璽") {
            let prefix = lines[..i]
                .iter()
                .map(|l| l.text.as_str())
                .collect::<String>();
            if let Some(title) = prefix.strip_suffix("をここに公布する。") {
                if lines.get(i + 1).is_some_and(|l| date(&l.text)) {
                    plan.line(&lines[i], analysis);
                    plan.line(&lines[i + 1], analysis);
                    let range = i + 2..lines.len().min(i + 12);
                    let signer = range
                        .clone()
                        .find(|&n| office_end(&lines[n].text).is_some());
                    if let Some(s) = signer {
                        plan.line(&lines[s], analysis);
                    }
                    if let Some(n) = range.clone().find(|&n| identifier(&lines[n].text)) {
                        plan.line(&lines[n], analysis);
                        // The title is independently repeated in the declaration.
                        // Join its original wrapped lines only on exact equality.
                        let mut repeated = String::new();
                        for end in n + 1..lines.len().min(n + 6) {
                            repeated.push_str(&lines[end].text);
                            if repeated == title {
                                plan.cuts.extend([lines[n + 1].start, lines[end].end]);
                                break;
                            }
                            if !title.starts_with(&repeated) {
                                break;
                            }
                        }
                        if let Some(s) = signer.filter(|&s| s > n) {
                            let dates = sources(analysis, &lines[i + 1].text);
                            let signatures = sources(analysis, &lines[s].text);
                            let numbers = sources(analysis, &lines[n].text);
                            let vertical = |b: [f32; 4]| b[3] - b[1] > (b[2] - b[0]) * 1.5;
                            let mut proofs = 0;
                            for d in &dates {
                                for sig in &signatures {
                                    for num in &numbers {
                                        if d.page == sig.page
                                            && sig.page == num.page
                                            && sig.role_boundary
                                            && [d.bounds, sig.bounds, num.bounds]
                                                .into_iter()
                                                .all(vertical)
                                            && sig.bounds[2] <= d.bounds[0]
                                            && num.bounds[2] <= sig.bounds[0]
                                        {
                                            proofs += 1;
                                        }
                                    }
                                }
                            }
                            // Bind the complete physical triplet, not a repeated
                            // signer's text elsewhere in the same PDF.
                            if proofs == 1 {
                                plan.move_signature =
                                    Some((lines[s].start, lines[s].end, lines[n].start));
                            }
                        }
                    }
                }
            }
        } else if lines
            .first()
            .is_some_and(|l| identifier(&l.text) && l.text.starts_with(['〇', '○']))
        {
            if let Some(i) = lines
                .iter()
                .take(16)
                .position(|l| dated_signature(&l.text) || date(&l.text))
            {
                let prefix = lines[..i]
                    .iter()
                    .map(|l| l.text.as_str())
                    .collect::<String>();
                let mut end = i;
                if prefix.ends_with('。')
                    && outside_quotes(&prefix)
                    && (dated_signature(&lines[i].text)
                        || lines
                            .get(i + 1)
                            .is_some_and(|l| office_end(&l.text).is_some()))
                {
                    plan.line(&lines[i], analysis);
                    while lines
                        .get(end + 1)
                        .is_some_and(|l| office_end(&l.text).is_some())
                    {
                        end += 1;
                        plan.line(&lines[end], analysis);
                    }
                    let mut title = String::new();
                    for last in end + 1..lines.len().min(end + 6) {
                        title.push_str(&lines[last].text);
                        if title.contains(['。', '「', '」']) {
                            break;
                        }
                        if ["を次のように定める。", "を次のように制定する。"]
                            .iter()
                            .any(|suffix| prefix.ends_with(&format!("{title}{suffix}")))
                        {
                            plan.cuts.extend([lines[end + 1].start, lines[last].end]);
                            break;
                        }
                    }
                }
            }
        }
        // Nominal source signatures after a sentence are also separate in the
        // closing area. A role must end at an original Word boundary. Open
        // amendment quotations cannot provide this evidence.
        let mut quotes = 0i32;
        let mut previous_signatures: Vec<Source> = vec![];
        for (i, l) in lines.iter().enumerate() {
            let signatures = if office_end(&l.text).is_some() {
                sources(analysis, &l.text)
            } else { vec![] };
            let pitches = previous_signatures.iter().flat_map(|a| signatures.iter().filter_map(move |b| signature_pitch(a,b))).collect::<Vec<_>>();
            let follows = pitches.len() == 1;
            if quotes == 0
                && i > 0
                && (lines[i - 1].text.ends_with('。') || follows)
                && !signatures.is_empty() && signatures.iter().all(|s| s.role_boundary)
            {
                plan.line(l, analysis);
                if follows {
                    let twips = (pitches[0] * 20.).round() as usize;
                    plan.signature_leading.insert(lines[i-1].start, twips);
                    plan.signature_leading.insert(l.start, twips);
                }
                previous_signatures = signatures;
            } else {
                previous_signatures.clear();
            }
            for c in l.text.chars() {
                match c {
                    '「' | '『' => quotes += 1,
                    '」' | '』' => quotes -= 1,
                    _ => {}
                }
            }
            if quotes < 0 {
                break;
            }
        }
        plan.cuts.retain(|&n| n <= available);
        plan.spaces.retain(|&n, _| n < available);
        if plan
            .move_signature
            .is_some_and(|(_, end, target)| end > available || target >= available)
        {
            plan.move_signature = None;
        }
        plan
    }
    fn apply(&self, runs: Runs) -> Runs {
        let available = runs.iter().map(|(s, _)| compact(s).len()).sum::<usize>();
        let mut boundaries = self.cuts.clone();
        boundaries.extend([0, available]);
        let ranges = boundaries
            .into_iter()
            .collect::<Vec<_>>()
            .windows(2)
            .map(|w| (w[0], w[1]))
            .collect::<Vec<_>>();
        let mut chunks = vec![Runs::new(); ranges.len()];
        let (mut at, mut piece, mut whitespace) = (0, 0, true);
        let push = |out: &mut Runs, c: char, style: bool| {
            if let Some((s, _)) = out.last_mut().filter(|(_, previous)| *previous == style) {
                s.push(c);
            } else {
                out.push((c.to_string(), style));
            }
        };
        for (s, style) in runs {
            for c in s.chars() {
                if !c.is_whitespace() {
                    while piece + 1 < ranges.len() && at >= ranges[piece].1 {
                        piece += 1;
                    }
                    if !whitespace {
                        if let Some(&count) = self.spaces.get(&at) {
                            for _ in 0..count {
                                push(&mut chunks[piece], '　', false);
                            }
                        }
                    }
                    at += c.len_utf8();
                }
                push(&mut chunks[piece], c, style);
                whitespace = c.is_whitespace();
            }
        }
        let mut chunks = ranges.into_iter().zip(chunks).collect::<Vec<_>>();
        if let Some((start, end, target)) = self.move_signature {
            if let Some(i) = chunks.iter().position(|(range, _)| *range == (start, end)) {
                let chunk = chunks.remove(i);
                if let Some(i) = chunks.iter().position(|((start, _), _)| *start == target) {
                    chunks.insert(i, chunk);
                } else {
                    chunks.insert(i, chunk);
                }
            }
        }
        let mut out = Runs::new();
        for (i, (_, chunk)) in chunks.into_iter().enumerate() {
            if i > 0 {
                out.push(("\n".into(), false));
            }
            out.extend(chunk);
        }
        out
    }
}

// A table or image ends the preamble-only projection below. A closing text
// suffix may nevertheless contain independently certified source signatures.
// Bind the entire suffix at its original terminal position, never search for a
// person's name in the rendered text, and never traverse or rewrite rich items.
fn closing_suffix(article: &str, analysis: &Analysis, doc: &mut Document) -> BTreeMap<usize,usize> {
    let mut start = None;
    for (bi, block) in doc.blocks.iter().enumerate().rev() {
        match block {
            Block::Paragraph { runs } => {
                if let Some(ri) = runs.iter().rposition(|r| !matches!(r, Inline::Text { .. })) {
                    start = Some((bi, ri + 1)); break;
                }
            }
            _ => { start = Some((bi + 1, 0)); break; }
        }
    }
    let Some((bi, ri)) = start.filter(|(bi,_)| *bi < doc.blocks.len()) else { return Default::default(); };
    let mut runs = Runs::new();
    for (i, block) in doc.blocks[bi..].iter().enumerate() {
        let Block::Paragraph { runs: items } = block else { return Default::default(); };
        if i > 0 { runs.push(("\n".into(), false)); }
        for item in &items[if i == 0 { ri } else { 0 }..] {
            let Inline::Text { text, underline } = item else { return Default::default(); };
            runs.push((text.clone(), *underline));
        }
    }
    let actual = runs.iter().map(|(s,_)| compact(s)).collect::<String>();
    let original = compact(article);
    if actual.is_empty() || !original.ends_with(&actual) { return Default::default(); }
    let offset = original.len() - actual.len();
    let all = Plan::new(article, analysis, original.len());
    let field_offset = doc.field_text(kanpo_parser::media::Field::Paragraph).chars().count() - actual.chars().count();
    let mut leading: BTreeMap<usize,usize> = all.signature_leading.iter().filter_map(|(&n,&v)| {
        let local = n.checked_sub(offset)?;
        Some((field_offset + actual[..local].chars().count(),v))
    }).collect();
    let raw=runs.iter().map(|(s,_)|s.as_str()).collect::<String>();
    if let Some((n,v))=preceding_leading(article,analysis,&raw,offset,&all.signature_leading) {
        leading.insert(field_offset+actual[..n].chars().count(),v);
    }
    let plan = Plan {
        cuts: all.cuts.into_iter().filter_map(|n| n.checked_sub(offset)).collect(),
        spaces: all.spaces.into_iter().filter_map(|(n,v)| n.checked_sub(offset).map(|n|(n,v))).collect(),
        move_signature: None,
        signature_leading: Default::default(),
    };
    if plan.cuts.is_empty() && plan.spaces.is_empty() { return Default::default(); }
    let Block::Paragraph { runs: items } = &mut doc.blocks[bi] else { return Default::default(); };
    items.truncate(ri);
    items.extend(plan.apply(runs).into_iter().map(|(text,underline)|Inline::Text{text,underline}));
    doc.blocks.truncate(bi + 1);
    leading
}

pub(crate) fn apply(analysis: &Analysis, documents: &mut [Document]) -> Vec<BTreeMap<usize,usize>> {
    let mut leading = vec![];
    for (article, doc) in analysis.articles.iter().zip(documents) {
        leading.push(closing_suffix(&article.text, analysis, doc));
        let mut runs = Runs::new();
        let mut count = 0;
        let mut tail = Vec::new();
        'prefix: for block in &doc.blocks {
            let Block::Paragraph { runs: items } = block else {
                break;
            };
            if count > 0 {
                runs.push(("\n".into(), false));
            }
            count += 1;
            for (i, item) in items.iter().enumerate() {
                if let Inline::Text { text, underline } = item {
                    runs.push((text.clone(), *underline));
                } else {
                    tail = items[i..].to_vec();
                    break 'prefix;
                }
            }
        }
        let actual = runs.iter().map(|(s, _)| compact(s)).collect::<String>();
        if actual.is_empty() {
            continue;
        }
        // A later table or body repair cannot certify or invalidate an earlier
        // preamble. Bind only the complete common source prefix, and leave all
        // later rich content untouched.
        let original = compact(&article.text);
        let common = actual
            .chars()
            .zip(original.chars())
            .take_while(|(a, b)| a == b)
            .map(|(c, _)| c.len_utf8())
            .sum::<usize>();
        let plan = Plan::new(&article.text, analysis, common);
        if plan.cuts.is_empty() && plan.spaces.is_empty() {
            continue;
        }
        let runs = plan
            .apply(runs)
            .into_iter()
            .map(|(text, underline)| Inline::Text { text, underline })
            .collect();
        let mut blocks = vec![Block::Paragraph { runs }];
        if !tail.is_empty() {
            blocks.push(Block::Paragraph { runs: tail });
        }
        doc.blocks.splice(..count, blocks);
    }
    leading
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn consecutive_signatures_need_adjacent_original_lines_at_any_scale_or_origin() {
        for axis in [0, 1] {
            for (scale,dx,dy) in [(0.7,42.,73.),(1.,0.,0.),(1.8,93.,23.)] {
                let source = |start:f32,cross:f32,page:usize| {
                    let b = if axis==0 {[start,cross,120.,cross+8.]}else{[cross,start,cross+8.,120.]};
                    Source{page,bounds:[b[0]*scale+dx,b[1]*scale+dy,b[2]*scale+dx,b[3]*scale+dy],role_boundary:true,spaces:vec![],regular:true}
                };
                let a=source(16.,40.,0);let b=source(32.,if axis==0 {52.} else {28.},0);
                assert!(adjacent_signatures(&a,&b));
                assert!(!adjacent_signatures(&a,&source(32.,if axis==0 {52.} else {28.},1)));
                assert!(!adjacent_signatures(&a,&source(32.,if axis==0 {92.} else {-12.},0)));
                assert!(!adjacent_signatures(&b,&a));
            }
        }
    }

    #[test]
    fn closing_projection_requires_a_complete_suffix_and_closed_source_quotes() {
        let a=crate::analyze(include_bytes!("../../kanpo-parser/tests/data/workbook/case-12.pdf"),&[]).unwrap();
        let article=&a.articles[0];let doc=Document::from(&article.document);
        let mut changed=doc.clone();
        let leading = closing_suffix(&article.text,&a,&mut changed);
        assert_eq!(leading.len(),3);
        assert!(leading.values().all(|&n|n==231),"source column pitch in twips");
        assert_ne!(serde_json::to_value(&changed).unwrap(),serde_json::to_value(&doc).unwrap());
        let before=serde_json::to_value(&changed).unwrap();
        closing_suffix(&format!("{}別の末尾",article.text),&a,&mut changed);
        assert_eq!(serde_json::to_value(&changed).unwrap(),before);
        let mut quoted=doc.clone();
        if let Block::Paragraph{runs}=&mut quoted.blocks[0] {
            let i=runs.iter().rposition(|r|!matches!(r,Inline::Text{..})).unwrap();
            if let Inline::Text{text,..}=&mut runs[i+1] {text.insert(0,'「');}
        }
        let source=article.text.replace("（国家公務員倫理規程の一部改正）","「（国家公務員倫理規程の一部改正）");
        let before=serde_json::to_value(&quoted).unwrap();
        closing_suffix(&source,&a,&mut quoted);
        assert_eq!(serde_json::to_value(&quoted).unwrap(),before,"quoted officer references are not closing signers");
    }
    #[test]
    fn dates_are_structural_and_not_tied_to_a_publication_date() {
        for s in [
            "令和八年三月三十一日",
            "平成元年一月八日",
            "昭和６３年１２月１日",
            "2031年4月7日",
        ] {
            assert!(date(s));
        }
        for s in [
            "令和年三月一日",
            "令和八年月一日",
            "令和八年三月一日を定める。",
        ] {
            assert!(!date(s));
        }
    }
    #[test]
    fn independent_identifiers_and_nominal_signatures_are_not_body_references() {
        assert!(identifier("政令第１２号"));
        assert!(identifier("〇任意省令第十二号"));
        for s in [
            "政令第十二号の規定により",
            "令和八年政令第十二号",
            "（政令第十二号）",
        ] {
            assert!(!identifier(s));
        }
        assert!(office_end("任意大臣任意姓名").is_some());
        for s in [
            "第八条任意大臣任意姓名",
            "任意大臣",
            "任意大臣任意姓名が定める。",
        ] {
            assert!(office_end(s).is_none());
        }
    }
    #[test]
    fn source_preamble_repeated_title_and_run_styles_are_preserved() {
        let analysis = crate::analyze(
            include_bytes!("../../kanpo-parser/tests/data/workbook/case-08.pdf"),
            &[],
        )
        .unwrap();
        let source="任意の法律をここに公布する。\n御名御璽\n平成元年一月八日\n内閣総理大臣任意姓名\n法律第一号\n任意の法律\n本文を定める。";
        let plain = compact(source);
        let p = Plan::new(source, &analysis, plain.len());
        let split = "任意の法律をここに公布する。御名御";
        let runs = p.apply(vec![
            (split.into(), true),
            (plain[split.len()..].into(), false),
        ]);
        let text = runs.iter().map(|(s, _)| s.as_str()).collect::<String>();
        assert!(text.contains("御名　御璽\n平成元年一月八日\n内閣総理大臣任意姓名\n法律第一号\n任意の法律\n本文を定める。"));
        assert!(runs.iter().any(|(s, style)| s == "御" && *style));
        assert_eq!(compact(&text), plain);
        let unrelated = source.replace("をここに公布する。", "について説明する。");
        assert!(Plan::new(&unrelated, &analysis, compact(&unrelated).len())
            .cuts
            .is_empty());
    }

    #[test]
    fn signature_reordering_needs_one_complete_original_physical_triplet() {
        for pdf in [
            include_bytes!("../../kanpo-parser/tests/data/workbook/case-13.pdf").as_slice(),
            include_bytes!("../../kanpo-parser/tests/data/workbook/case-18.pdf").as_slice(),
        ] {
            let mut a = crate::analyze(pdf, &[]).unwrap();
            // Exercise the converter's defensive projection even when the
            // native parser has already restored the original header order.
            let mut lines = a.articles[0]
                .text
                .lines()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            let sig = lines
                .iter()
                .position(|s| s.starts_with("内閣総理大臣"))
                .unwrap();
            let signature = lines.remove(sig);
            let number = lines.iter().position(|s| identifier(s)).unwrap();
            lines.insert(number + 2, signature);
            let text = lines.join("\n");
            let make = |a: &Analysis| Plan::new(&text, a, compact(&text).len());
            assert!(make(&a).move_signature.is_some());
            for p in &mut a.pages {
                for w in &mut p.words {
                    let b = &mut w.source_box;
                    b[0] = b[0] * 1.7 + 23.;
                    b[2] = b[2] * 1.7 + 23.;
                    b[1] = b[1] * 1.7 + 41.;
                    b[3] = b[3] * 1.7 + 41.;
                }
            }
            assert!(
                make(&a).move_signature.is_some(),
                "source scale and origin cannot change reading order"
            );
            for p in &mut a.pages {
                for w in &mut p.words {
                    if w.text == "内閣総理大臣" {
                        w.source_box[0] += 1000.;
                        w.source_box[2] += 1000.;
                    }
                }
            }
            assert!(
                make(&a).move_signature.is_none(),
                "a nominal name does not override physical source order"
            );
        }
    }

    #[test]
    fn an_open_body_quotation_cannot_authorize_dated_header_paragraphs() {
        let a = crate::analyze(
            include_bytes!("../../kanpo-parser/tests/data/workbook/case-08.pdf"),
            &[],
        )
        .unwrap();
        let source="〇任意省告示第一号\n次の文章を引用する。「任意を定める。\n平成元年一月八日内閣総理大臣任意姓名\n任意の標題\n引用の終わり。」";
        assert!(Plan::new(source, &a, compact(source).len()).cuts.is_empty());
    }
}

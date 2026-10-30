//! Restore only the article boundary inside an original multi-line notice title.
use crate::trace::{ContentStyles, NativeBounds, PageTrace, Region, Role, Word, WordStatus};

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

/// A notice title typeset in its own font can wrap at any character
/// (「…の登録事項の変更に関」＋「する公示」) and end in any word
/// (「基本測量関係事項公告」「…国民保護計画の変更について」「…国家試験の施行」). Its extent
/// is the maximal run of aligned lines whose original fonts share nothing with the
/// line above and the body below. The title starts at the column's line start and
/// its body's first line is indented from it; a heading that stands further in
/// (附則, a centered label or an annexed document's title) is not a notice title.
/// No line count of the previous notice or wording of the title is used.
/// The article starts at the first title line, and no boundary remains inside it.
/// A title the native notice boundaries already cut at is left to them.
pub(crate) fn split_typeset_notice_titles(
    articles: &mut Vec<String>,
    pages: &[PageTrace],
    rules: &[Option<crate::lines::PageRules>],
) -> usize {
    let mut titles: Vec<Vec<String>> = Vec::new();
    let mut instruments: Vec<Vec<String>> = Vec::new();
    // The last body line of the previous page is the line above a title at the top
    // of the next page.
    let mut carry = None;
    for (n, page) in pages.iter().enumerate() {
        // Without the page's original rules the native boundaries cannot be checked.
        let Some(rules) = rules.get(n).and_then(|r| r.as_ref()) else {
            carry = None;
            continue;
        };
        let found;
        (found, carry) = typeset_titles(page, carry);
        instruments.extend(comparison_heads(page));
        if found.is_empty() {
            continue;
        }
        let native = crate::notice_boundaries::cuts(page, rules);
        titles.extend(found.into_iter().filter(|(line, _)| !native.contains(line)).map(|(_, title)| title));
    }
    titles.extend(instruments.iter().cloned());
    let mut lines: Vec<&str> = Vec::new();
    let mut starts = std::collections::BTreeSet::new();
    let mut spans = std::collections::HashMap::new();
    for (n, article) in articles.iter().enumerate() {
        starts.insert(lines.len());
        let first = lines.len();
        lines.extend(article.lines());
        spans.insert((first, lines.len()), n);
    }
    let mut found = Vec::new();
    for title in &titles {
        let at: Vec<usize> = (0..lines.len().saturating_sub(title.len() - 1))
            .filter(|&i| title.iter().zip(&lines[i..]).all(|(t, l)| l.trim() == t))
            .collect();
        // Repeated titles at different source positions are ambiguous.
        if at.len() == 1 && titles.iter().filter(|t| *t == title).count() == 1 {
            found.push((at[0], at[0] + title.len()));
        }
    }
    let inside = |j: usize| found.iter().any(|&(a, b)| a <= j && j < b);
    let before = starts.clone();
    for &(first, end) in &found {
        // A boundary already inside the title moves to its first line.
        let inner = before.range(first + 1..end).next().is_some();
        // The table of contents lists each notice under 官庁報告 as its own item,
        // but groups several notices of the 公告 section into one item. The 公告
        // section begins at its own label or at its first field 「諸事項」. The
        // field 地方自治事項 is one item too, whatever headings (選挙) it holds.
        // A field of 官庁報告 (労働, 国家試験) shows the section when its label
        // is on an earlier page than the item's own. The table of contents lists
        // the same fields under the bracketed 〔官庁報告〕; those are not the body's.
        let label = |end: usize, wanted: &dyn Fn(&str) -> bool| {
            (0..end).rev().filter(|&j| !inside(j)).map(|j| (j, compact(lines[j]))).find(|(_, t)| wanted(t))
        };
        let report = match label(first, &|t| {
            matches!(t, "官庁報告" | "公告" | "諸事項" | "地方自治事項") || crate::segment::is_report_field(t)
        }) {
            Some((_, t)) if t == "官庁報告" => true,
            Some((j, t)) if crate::segment::is_report_field(&t) => {
                label(j, &|t| matches!(t, "官庁報告" | "〔官庁報告〕")).is_none_or(|(_, t)| t == "官庁報告")
            }
            _ => false,
        };
        // An instrument heading its own comparison table starts its own item in any section,
        // right after the previous instrument's last sentence (「…施行する。」). Above any
        // other line (its 〇 heading, or a line its preamble was read in after) it does not.
        let instrument = instruments.iter().any(|t| t.len() == end - first && t.iter().zip(&lines[first..end]).all(|(t, l)| l.trim() == t));
        if instrument && (first == 0 || !lines[first - 1].trim_end().ends_with('。')) {
            continue;
        }
        if !inner && !report && !instrument {
            continue;
        }
        starts.retain(|&s| s <= first || s >= end);
        // A title right under a section or field label stays in that label's article,
        // as does one right under a line that already starts an article (「資料」).
        if first > 0 && !crate::segment::is_section_label(lines[first - 1]) && !starts.contains(&(first - 1)) {
            starts.insert(first);
        }
    }
    if starts == before {
        return 0;
    }
    let bounds: Vec<usize> = starts.iter().copied().chain([lines.len()]).collect();
    // Articles whose own line range is unchanged keep their exact text. The
    // joined stream stays byte-identical, so native source ownership still applies.
    let rebuilt: Vec<String> = bounds
        .windows(2)
        .map(|w| match spans.get(&(w[0], w[1])) {
            Some(&n) => articles[n].clone(),
            None => lines[w[0]..w[1]].join("\n"),
        })
        .collect();
    if rebuilt.join("\n") != articles.join("\n") || rebuilt.iter().any(|a| a.trim().is_empty()) {
        return 0;
    }
    *articles = rebuilt;
    starts.difference(&before).count() + before.difference(&starts).count()
}

/// The lines of each comparison table's preamble that heads an instrument of its own:
/// the first preamble region of a table unit that carries the instrument's date line
/// (「令和八年十月一日　人事院総裁川本裕子」). The instrument starts at the sentence
/// ending right above that line (its enactment), since the region can also hold the end of
/// the previous instrument (「第二条…」 of its 附則). A preamble without a date only
/// introduces another table of the same instrument.
fn comparison_heads(page: &PageTrace) -> Vec<Vec<String>> {
    let dated = |l: &crate::trace::Line| {
        l.table.is_none() && l.text.trim().split(char::is_whitespace).next().is_some_and(crate::notice_boundaries::date)
    };
    let mut out = Vec::new();
    for (i, r) in page.regions.iter().enumerate() {
        if r.role != Role::Preamble
            || (i > 0 && page.regions[i - 1].role == Role::Preamble && page.regions[i - 1].unit == r.unit)
            || r.lines.iter().any(|l| crate::segment::is_heading_mark(l.text.trim()) || crate::segment::is_section_label(&l.text))
        {
            continue;
        }
        // The enactment sentence ends right above the date line; signatures of a joint
        // instrument (「農林水産省」) in between are not an enactment of their own.
        let Some(at) = r.lines.iter().position(dated) else { continue };
        if at == 0 || !r.lines[at - 1].text.trim_end().ends_with('。') {
            continue;
        }
        let start = (0..at - 1).rev().find(|&j| r.lines[j].text.trim_end().ends_with('。')).map_or(0, |j| j + 1);
        out.push(r.lines[start..].iter().map(|l| l.text.trim().to_string()).collect());
    }
    out
}

/// Each title with its first line's index across all of the page's regions,
/// the index the native notice boundaries use.
/// `carry` is the fonts and writing direction of the line above the page's first line,
/// and the page's own last body line is returned the same way.
fn typeset_titles(page: &PageTrace, carry: Option<Above>) -> (Vec<(usize, Vec<String>)>, Option<Above>) {
    let fonts = |line: &crate::trace::Line| -> Option<crate::trace::NativeStyles> {
        if line.table.is_some() || line.words.is_empty() {
            return None;
        }
        let mut out = crate::trace::NativeStyles::new();
        for &id in &line.words {
            out.extend(page.native_styles.get(id)?.as_ref()?.iter().copied());
        }
        (!out.is_empty()).then_some(out)
    };
    // A title line's fonts are those of its content glyphs, so punctuation set in
    // the body font (「、」) does not join the title to the body.
    let content = |line: &crate::trace::Line| -> Option<crate::trace::NativeStyles> {
        if line.table.is_some() || line.words.is_empty() {
            return None;
        }
        let mut out = crate::trace::NativeStyles::new();
        for &id in &line.words {
            let ContentStyles::Glyphs(s) = page.native_content_styles.get(id)? else {
                return None;
            };
            out.extend(s.iter().copied());
        }
        (!out.is_empty()).then_some(out)
    };
    // Writing direction, line-start edge and glyph size of a line's own used
    // source Words: the left edge and height for horizontal text, the top edge
    // and width for vertical text. Every Word needs complete native evidence;
    // compatibility boxes, unknown axes or unknown content fonts never stand in
    // for the original typesetting.
    let left = |line: &crate::trace::Line| -> Option<(bool, f32, f32)> {
        use crate::native_word_axes::NativeAxis;
        let mut out: Option<(bool, f32, f32)> = None;
        for &id in &line.words {
            page.words.get(id).filter(|w| w.id == id && w.status == WordStatus::Used)?;
            let (Some(axis), Some(NativeBounds::Glyphs(b)), Some(ContentStyles::Glyphs(_))) =
                (page.native_axes.get(id), page.native_bounds.get(id), page.native_content_styles.get(id))
            else {
                return None;
            };
            if !b.iter().all(|v| v.is_finite()) || b[0] >= b[2] || b[1] >= b[3] {
                return None;
            }
            let (down, start, size) = match axis {
                NativeAxis::Right => (false, b[0], b[3] - b[1]),
                NativeAxis::Down => (true, b[1], b[2] - b[0]),
                _ => return None,
            };
            out = match out {
                None => Some((down, start, size)),
                Some((d, s, z)) if d == down => Some((d, s.min(start), z.max(size))),
                Some(_) => return None,
            };
        }
        out
    };
    // A neighbor only has to show that its fonts differ from the title's: its
    // Words need a known native writing direction (the title's) and fonts.
    let direction = |line: &crate::trace::Line| -> Option<bool> {
        use crate::native_word_axes::NativeAxis;
        let mut out = None;
        for &id in &line.words {
            page.words.get(id).filter(|w| w.id == id && w.status == WordStatus::Used)?;
            let down = match page.native_axes.get(id)? {
                NativeAxis::Right => false,
                NativeAxis::Down => true,
                _ => return None,
            };
            if out.is_some_and(|d| d != down) {
                return None;
            }
            out = Some(down);
        }
        out
    };
    // A table row above shares no font with a title and no title wraps out of it;
    // it has no writing direction of its own to compare.
    let above = |line: &crate::trace::Line| {
        if line.table.is_some() {
            return Some((crate::trace::NativeStyles::new(), None));
        }
        Some((fonts(line)?, Some(direction(line)?)))
    };
    // The previous page may be set in the other direction (a vertical page before a
    // horizontal one); across pages only its fonts are compared.
    let mut last = carry.map(|(f, _)| (f, None));
    let mut found = Vec::new();
    // At the top of a column the line above in reading order ends the previous column.
    let mut offset = 0;
    for r in &page.regions {
        let base = offset;
        offset += r.lines.len();
        if r.role != Role::Body {
            last = None;
            continue;
        }
        let mut i = 0;
        while i < r.lines.len() {
            let prior = if i == 0 { last.clone() } else { above(&r.lines[i - 1]) };
            let (Some((above, down)), Some(style), Some(first)) =
                (prior, content(&r.lines[i]), left(&r.lines[i]))
            else {
                i += 1;
                continue;
            };
            if !above.is_disjoint(&style) || down.is_some_and(|d| d != first.0) {
                i += 1;
                continue;
            }
            let mut end = i + 1;
            while end < r.lines.len()
                && content(&r.lines[end]).as_ref() == Some(&style)
                && left(&r.lines[end]).is_some_and(|(d, x, _)| d == first.0 && (x - first.1).abs() <= 0.35 * first.2)
            {
                end += 1;
            }
            let run = &r.lines[i..end];
            let text: Vec<String> = run.iter().map(|l| l.text.trim().to_string()).collect();
            let title = text.concat();
            let body = r.lines.get(end);
            if run.len() <= 3
                && !title.contains(|c: char| c.is_whitespace() || "。「」『』".contains(c))
                && body.and_then(fonts).is_some_and(|b| b.is_disjoint(&style))
                && body.and_then(left).is_some_and(|(d, x, _)| d == first.0 && x - first.1 >= 0.5 * first.2)
                && body.is_some_and(|b| compact(&b.text).chars().count() >= 20)
            {
                found.push((base + i, text));
            }
            i = end;
        }
        last = r.lines.last().and_then(above);
    }
    (found, last.and_then(|(f, d)| Some((f, d?))))
}

type Above = (crate::trace::NativeStyles, bool);

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

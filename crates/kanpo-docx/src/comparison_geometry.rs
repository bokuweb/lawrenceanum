//! Comparison-table paragraphs and rows from the original column geometry.
//!
//! The parser decides paragraph breaks inside a comparison field from the
//! text alone, and pairs the two fields by provision. Two consequences are
//! visible in Word:
//!
//! * a wrapped line that happens to begin with a provision-like word
//!   (「第九条の三…」「イ、第四号イ…」「（１）」とあるのは…」) starts a new
//!   paragraph, although the original sets it at the continuation indent;
//! * inside one row, items that the original prints side by side (「一の二」
//!   opposite 「（新設）」) drift apart, because each Word cell packs its
//!   paragraphs from the top.
//!
//! Both are corrected here from the original lines, bound to the document by
//! complete field positions (the same contract as `paragraph_gaps`). Nothing
//! is decided by spelling a particular law or provision.
use crate::paragraph_gaps::{compact, geometry, Geometry};
use kanpo_parser::{
    media::{Block, Document, Field, Inline, Row},
    trace::{Role, WordStatus},
    Analysis,
};
use std::collections::{BTreeMap, BTreeSet};

/// One original line of a comparison field.
#[derive(Clone)]
struct Source {
    page: usize,
    region: usize,
    /// First non-whitespace scalar of the line in the field stream.
    pos: usize,
    text: String,
    geometry: Option<Geometry>,
}

fn cross_center(g: &Geometry) -> f32 {
    let cross = 1 - g.axis;
    (g.bounds[cross] + g.bounds[cross + 2]) / 2.
}

fn similar(a: &Geometry, b: &Geometry) -> bool {
    a.axis == b.axis && a.em <= 1.2 * b.em && b.em <= 1.2 * a.em
}

/// Next line of the same flow: one line pitch further along the cross axis.
fn adjacent(a: &Geometry, b: &Geometry) -> bool {
    let pitch = if a.axis == 1 {
        cross_center(a) - cross_center(b)
    } else {
        cross_center(b) - cross_center(a)
    };
    similar(a, b) && (0.8 * a.em.min(b.em)..=2. * a.em.max(b.em)).contains(&pitch)
}

/// The indentation level a provision marker is set at in a comparison cell:
/// articles, paragraphs (項) and supplementary headings at the cell top, items
/// (号) one em in, subdivisions (イ・ロ) two, and `（１）` three. Headings in
/// parentheses, omission marks and unmarked text give no level.
fn marker_level(text: &str) -> Option<usize> {
    const KANJI: &str = "〇一二三四五六七八九十百千";
    const IROHA: &str = "イロハニホヘトチリヌルヲワカヨタレソツネナラムウヰノオクヤマケフコエテアサキユメミシヱヒモセス";
    let t = text.trim_start();
    let mut chars = t.chars();
    let first = chars.next()?;
    let digit = |c: char| c.is_ascii_digit() || ('０'..='９').contains(&c);
    if let Some(rest) = t.strip_prefix('第') {
        let n = rest
            .chars()
            .take_while(|c| KANJI.contains(*c) || digit(*c))
            .count();
        // An article starts at the cell top. 「第三号中」「第二項」 never start
        // a provision, so they are measured against the same top level.
        return (n > 0 && matches!(rest.chars().nth(n), Some('条' | '号' | '項'))).then_some(0);
    }
    if t.starts_with("附則") || t.starts_with("別表") || digit(first) {
        return Some(0);
    }
    if KANJI.contains(first) {
        return Some(1);
    }
    if IROHA.contains(first) {
        return Some(2);
    }
    if let Some(rest) = t.strip_prefix('（') {
        let n = rest.chars().take_while(|c| digit(*c)).count();
        if n > 0 && rest.chars().nth(n) == Some('）') {
            return Some(3);
        }
    }
    None
}

/// Indentation of `g` in ems from `top`, when it lies on the em grid.
fn level(g: &Geometry, top: f32) -> Option<usize> {
    let d = (g.bounds[g.axis] - top) / g.em;
    (d > -0.25 && (d - d.round()).abs() <= 0.25).then(|| d.round().max(0.) as usize)
}

/// Whether original line `b` continues the wrapped line `a` although the
/// parser started a paragraph at it. `a` must run to the end of the column;
/// `b` must sit at the continuation indent of `a`'s paragraph (`a`'s own
/// indent, or one em deeper after a first line) where its marker would not
/// be set if it began a provision.
fn continues(a: &Geometry, a_text: &str, b: &Geometry, b_text: &str, region: &[&Geometry]) -> bool {
    // After a full stop, an item or subdivision marker begins a new provision
    // even where a nested frame shifts the indentation grid.
    if a_text.trim_end().ends_with('。') && matches!(marker_level(b_text), Some(1..)) {
        return false;
    }
    if !adjacent(a, b) {
        return false;
    }
    let axis = a.axis;
    let em = a.em.min(b.em);
    let peers = region.iter().filter(|g| similar(g, a)).collect::<Vec<_>>();
    let Some(top) = peers.iter().map(|g| g.bounds[axis]).reduce(f32::min) else {
        return false;
    };
    let Some(end) = peers.iter().map(|g| g.bounds[axis + 2]).reduce(f32::max) else {
        return false;
    };
    if end - a.bounds[axis + 2] > 0.5 * em {
        return false;
    }
    let (Some(la), Some(lb)) = (level(a, top), level(b, top)) else {
        return false;
    };
    if lb == 0 || !(lb == la || lb == la + 1) {
        return false;
    }
    let text = b_text.trim_start();
    if text.starts_with(['（', '(', '〔', '［']) && marker_level(text) != Some(3) {
        return false;
    }
    match marker_level(text) {
        Some(expected) => lb != expected,
        None => lb == la + 1,
    }
}

fn sources(analysis: &Analysis, field: Field) -> Option<(Vec<Source>, String)> {
    let role = if field == Field::After {
        Role::After
    } else {
        Role::Before
    };
    let mut out = Vec::new();
    let mut stream = String::new();
    let mut pos = 0;
    for (pi, page) in analysis.pages.iter().enumerate() {
        let mut counts = BTreeMap::new();
        for region in &page.regions {
            for line in &region.lines {
                for &id in &line.words {
                    *counts.entry(id).or_insert(0) += 1;
                }
            }
        }
        for (ri, region) in page.regions.iter().enumerate() {
            if region.role != role {
                continue;
            }
            for line in &region.lines {
                let text = compact(&line.text);
                if line.table.is_none()
                    && !line.words.is_empty()
                    && line.words.iter().all(|&id| {
                        page.words
                            .get(id)
                            .is_some_and(|w| w.status == WordStatus::Used)
                    })
                {
                    out.push(Source {
                        page: pi,
                        region: ri,
                        pos,
                        text: line.text.clone(),
                        geometry: geometry(page, line, &counts),
                    });
                } else {
                    // A table or an unbound line still occupies the stream;
                    // it only ends the flow of the lines around it.
                    out.push(Source {
                        page: pi,
                        region: ri,
                        pos,
                        text: line.text.clone(),
                        geometry: None,
                    });
                }
                pos += text.chars().count();
                stream.push_str(&text);
            }
        }
    }
    (stream == kanpo_parser::source_underlines::field_text(analysis, field))
        .then_some((out, stream))
}

fn side(row: &mut Row, field: Field) -> &mut Vec<Inline> {
    if field == Field::After {
        &mut row.after
    } else {
        &mut row.before
    }
}

fn scalars(runs: &[Inline]) -> usize {
    runs.iter()
        .map(|r| match r {
            Inline::Text { text, .. } => text.chars().filter(|c| !c.is_whitespace()).count(),
            Inline::Table { .. } | Inline::Image { .. } => usize::MAX / 4,
        })
        .sum()
}

/// Field scalars of an embedded table, as `Document::field_text` counts them.
fn table_len(table: &kanpo_parser::media::Table) -> usize {
    Document {
        format: String::new(),
        blocks: vec![Block::Paragraph {
            runs: vec![Inline::Table {
                table: table.clone(),
            }],
        }],
    }
    .field_text(Field::Paragraph)
    .chars()
    .count()
}

fn plain(runs: &[Inline]) -> bool {
    runs.iter().all(|r| matches!(r, Inline::Text { .. }))
}

/// Remove the paragraph break right before each scalar position in `joins`.
fn join(runs: &mut [Inline], joins: &BTreeSet<usize>, cursor: &mut usize) {
    // (inline, byte) of every newline since the last non-whitespace scalar.
    let mut pending: Vec<(usize, usize)> = Vec::new();
    let mut seen = false;
    let mut remove: Vec<(usize, usize)> = Vec::new();
    for (k, run) in runs.iter().enumerate() {
        match run {
            Inline::Text { text, .. } => {
                for (b, c) in text.char_indices() {
                    if c == '\n' {
                        pending.push((k, b));
                    } else if !c.is_whitespace() {
                        if seen && joins.contains(cursor) {
                            remove.append(&mut pending);
                        }
                        pending.clear();
                        seen = true;
                        *cursor += 1;
                    }
                }
            }
            Inline::Table { table } => {
                pending.clear();
                seen = false;
                *cursor += table_len(table);
            }
            Inline::Image { .. } => {
                pending.clear();
                seen = false;
            }
        }
    }
    for (k, b) in remove.into_iter().rev() {
        if let Inline::Text { text, .. } = &mut runs[k] {
            text.remove(b);
        }
    }
}

/// Split plain runs before the `at`-th non-whitespace scalar. The break and
/// surrounding whitespace at the cut belong to neither part.
fn split(runs: &[Inline], at: usize) -> (Vec<Inline>, Vec<Inline>) {
    let (mut left, mut right) = (Vec::new(), Vec::new());
    let mut n = 0;
    for run in runs {
        let Inline::Text { text, underline } = run else {
            unreachable!("plain runs")
        };
        let (mut l, mut r) = (String::new(), String::new());
        for c in text.chars() {
            if !c.is_whitespace() {
                n += 1;
            }
            if n <= at {
                l.push(c);
            } else {
                r.push(c);
            }
        }
        if !l.is_empty() {
            left.push(Inline::Text {
                text: l,
                underline: *underline,
            });
        }
        if !r.is_empty() {
            right.push(Inline::Text {
                text: r,
                underline: *underline,
            });
        }
    }
    let trim = |runs: &mut Vec<Inline>, end: bool| {
        while let Some(Inline::Text { text, .. }) = if end {
            runs.last_mut()
        } else {
            runs.first_mut()
        } {
            let t = if end {
                text.trim_end()
            } else {
                text.trim_start()
            }
            .to_string();
            if t.is_empty() {
                if end {
                    runs.pop();
                } else {
                    runs.remove(0);
                }
                continue;
            }
            *text = t;
            break;
        }
    };
    trim(&mut left, true);
    trim(&mut right, false);
    (left, right)
}

/// A row of the documents with its field stream ranges.
struct Span {
    doc: usize,
    block: usize,
    row: usize,
    start: [usize; 2],
    end: [usize; 2],
}

fn spans(documents: &[Document]) -> (Vec<Span>, [BTreeSet<usize>; 2]) {
    let mut out = Vec::new();
    let mut starts: [BTreeSet<usize>; 2] = Default::default();
    let mut cursor = [0usize; 2];
    for (d, doc) in documents.iter().enumerate() {
        for (b, block) in doc.blocks.iter().enumerate() {
            let Block::Shinkyu { rows, .. } = block else {
                continue;
            };
            for (r, row) in rows.iter().enumerate() {
                let start = cursor;
                for (fi, field) in [Field::After, Field::Before].into_iter().enumerate() {
                    let runs = if field == Field::After {
                        &row.after
                    } else {
                        &row.before
                    };
                    let table = if field == Field::After {
                        &row.after_table
                    } else {
                        &row.before_table
                    };
                    starts[fi].insert(cursor[fi]);
                    let mut fresh = true;
                    for run in runs {
                        match run {
                            Inline::Text { text, .. } => {
                                for c in text.chars() {
                                    if c == '\n' {
                                        fresh = true;
                                    } else if !c.is_whitespace() {
                                        if fresh {
                                            starts[fi].insert(cursor[fi]);
                                        }
                                        fresh = false;
                                        cursor[fi] += 1;
                                    }
                                }
                            }
                            Inline::Table { table } => {
                                cursor[fi] += table_len(table);
                                fresh = true;
                            }
                            Inline::Image { .. } => fresh = true,
                        }
                    }
                    if let Some(table) = table {
                        cursor[fi] += table_len(table);
                    }
                }
                out.push(Span {
                    doc: d,
                    block: b,
                    row: r,
                    start,
                    end: cursor,
                });
            }
        }
    }
    (out, starts)
}

pub(crate) fn apply(analysis: &Analysis, documents: &mut [Document]) {
    let fields = [Field::After, Field::Before];
    let mut lines: [Option<Vec<Source>>; 2] = [None, None];
    for (fi, field) in fields.into_iter().enumerate() {
        let Some((src, stream)) = sources(analysis, field) else {
            continue;
        };
        if stream
            != documents
                .iter()
                .map(|d| d.field_text(field))
                .collect::<String>()
        {
            continue;
        }
        lines[fi] = Some(src);
    }
    // 1. Wrapped lines that the parser broke at a provision-like word.
    for (fi, field) in fields.into_iter().enumerate() {
        let Some(src) = &lines[fi] else { continue };
        let mut joins = BTreeSet::new();
        let mut by_region: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
        for (i, s) in src.iter().enumerate() {
            by_region.entry((s.page, s.region)).or_default().push(i);
        }
        for ids in by_region.values() {
            let region = ids
                .iter()
                .filter_map(|&i| src[i].geometry.as_ref())
                .collect::<Vec<_>>();
            for w in ids.windows(2) {
                let (a, b) = (&src[w[0]], &src[w[1]]);
                if let (Some(ga), Some(gb)) = (&a.geometry, &b.geometry) {
                    if continues(ga, &a.text, gb, &b.text, &region) {
                        joins.insert(b.pos);
                    }
                }
            }
        }
        if joins.is_empty() {
            continue;
        }
        let mut cursor = 0;
        for doc in documents.iter_mut() {
            for block in &mut doc.blocks {
                match block {
                    Block::Shinkyu { rows, .. } => {
                        for row in rows {
                            join(side(row, field), &joins, &mut cursor);
                            let table = if field == Field::After {
                                &row.after_table
                            } else {
                                &row.before_table
                            };
                            if let Some(table) = table {
                                cursor += table_len(table);
                            }
                        }
                    }
                    Block::Paragraph { .. } => {}
                }
            }
        }
    }
    // 2. Rows realigned where both fields start a paragraph on one column.
    let (Some(after), Some(before)) = (&lines[0], &lines[1]) else {
        return;
    };
    let (spans, starts) = spans(documents);
    let row_of = |fi: usize, pos: usize| {
        spans
            .iter()
            .position(|s| s.start[fi] <= pos && pos < s.end[fi])
    };
    let count = |src: &[Source], from: usize, to: usize| {
        src.iter().filter(|s| from <= s.pos && s.pos < to).count()
    };
    // A heading just before a synchronised line moves with it when the other
    // field has no line at the heading's column.
    let lead = |src: &[Source], other: &[Source], i: usize, fi: usize| -> usize {
        let s = &src[i];
        let Some(prev) = i.checked_sub(1).map(|j| &src[j]) else {
            return s.pos;
        };
        let (Some(g), Some(pg)) = (&s.geometry, &prev.geometry) else {
            return s.pos;
        };
        let heading =
            prev.text.trim_start().starts_with('（') && marker_level(&prev.text).is_none();
        let empty = !other.iter().any(|o| {
            o.page == prev.page
                && o.geometry.as_ref().is_some_and(|og| {
                    similar(og, pg) && (cross_center(og) - cross_center(pg)).abs() <= 0.5 * pg.em
                })
        });
        if prev.page == s.page
            && prev.region == s.region
            && heading
            && empty
            && adjacent(pg, g)
            && starts[fi].contains(&prev.pos)
        {
            prev.pos
        } else {
            s.pos
        }
    };
    let mut cuts: BTreeMap<usize, Vec<[usize; 2]>> = BTreeMap::new();
    for (ai, a) in after.iter().enumerate() {
        let Some(ga) = &a.geometry else { continue };
        if !starts[0].contains(&a.pos) {
            continue;
        }
        let matches = before
            .iter()
            .enumerate()
            .filter(|(_, b)| {
                b.page == a.page
                    && starts[1].contains(&b.pos)
                    && b.geometry.as_ref().is_some_and(|gb| {
                        similar(ga, gb)
                            && (cross_center(ga) - cross_center(gb)).abs()
                                <= 0.25 * ga.em.min(gb.em)
                    })
            })
            .collect::<Vec<_>>();
        let [(bi, b)] = matches[..] else { continue };
        let pa = lead(after, before, ai, 0);
        let pb = lead(before, after, bi, 1);
        let (Some(ra), Some(rb)) = (row_of(0, a.pos), row_of(1, b.pos)) else {
            continue;
        };
        if ra != rb || row_of(0, pa) != Some(ra) || row_of(1, pb) != Some(ra) {
            continue;
        }
        cuts.entry(ra).or_default().push([pa, pb]);
    }
    // Rows are rebuilt back to front so earlier indices stay valid.
    for (r, mut pairs) in cuts.into_iter().rev() {
        let span = &spans[r];
        let Block::Shinkyu { rows, .. } = &mut documents[span.doc].blocks[span.block] else {
            continue;
        };
        let row = &rows[span.row];
        if row.after_table.is_some()
            || row.before_table.is_some()
            || !plain(&row.after)
            || !plain(&row.before)
        {
            continue;
        }
        pairs.sort();
        pairs.dedup();
        let mut last = span.start;
        let mut chosen = Vec::new();
        for p in pairs {
            // One field may still be at the previous cut (its counterpart
            // is blank there in the original); both may not.
            if p[0] < last[0] || p[1] < last[1] || p == last {
                continue;
            }
            if count(after, last[0], p[0]) != count(before, last[1], p[1]) {
                chosen.push(p);
                last = p;
            }
        }
        if chosen.is_empty() {
            continue;
        }
        let mut pieces = Vec::new();
        let (mut a, mut b) = (row.after.clone(), row.before.clone());
        let mut origin = span.start;
        for p in &chosen {
            let (la, ra) = split(&a, p[0] - origin[0]);
            let (lb, rb) = split(&b, p[1] - origin[1]);
            pieces.push(Row {
                after: la,
                before: lb,
                after_table: None,
                before_table: None,
            });
            a = ra;
            b = rb;
            origin = *p;
        }
        pieces.push(Row {
            after: a,
            before: b,
            after_table: None,
            before_table: None,
        });
        // A leading piece with one empty field continues the previous row
        // (a provision carried over from the previous page).
        let mut at = span.row;
        if span.row > 0 && (scalars(&pieces[0].after) == 0) != (scalars(&pieces[0].before) == 0) {
            let prev = &mut rows[span.row - 1];
            if prev.after_table.is_none()
                && prev.before_table.is_none()
                && plain(&prev.after)
                && plain(&prev.before)
            {
                let first = pieces.remove(0);
                for (dst, src) in [
                    (&mut prev.after, first.after),
                    (&mut prev.before, first.before),
                ] {
                    if src.is_empty() {
                        continue;
                    }
                    if let Some(Inline::Text { text, .. }) = dst.last_mut() {
                        if !text.ends_with('\n') {
                            text.push('\n');
                        }
                    }
                    dst.extend(src);
                }
            }
        }
        rows.remove(at);
        for piece in pieces {
            rows.insert(at, piece);
            at += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(top: f32, end: f32, x: f32) -> Geometry {
        Geometry {
            bounds: [x, top, x + 8., end],
            em: 8.,
            axis: 1,
        }
    }

    #[test]
    fn provision_words_at_the_continuation_indent_continue_a_full_column() {
        // Cell top 0, end 336. Article first line at 0, continuation at 8.
        let top = g(0., 336., 100.);
        let full = g(8., 336., 88.);
        let region = vec![&top, &full];
        for text in [
            "第九条の三第一項に規定する",
            "第三号中「、第三号ハ」とあるのは",
            "イ、第四号イ及び第五号イ」とあるのは",
            "（１）」とあるのは",
            "ニ並びに第四号ニ",
        ] {
            let next = g(8., 200., 76.);
            assert!(continues(&full, "前の列", &next, text, &region), "{text}");
        }
        // The same words where a provision is set start one.
        assert!(
            !continues(&full, "前の列", &g(0., 200., 76.), "第九条　本文", &region),
            "article at the cell top"
        );
        assert!(
            !continues(&full, "前の列", &g(8., 200., 76.), "一　本文", &region),
            "item one em in"
        );
        assert!(
            !continues(&full, "前の列", &g(16., 200., 76.), "イ　本文", &region),
            "subdivision two em in"
        );
        // A line that ends before the column end is not wrapped.
        let short = g(8., 200., 88.);
        assert!(!continues(
            &short,
            "前の列",
            &g(8., 200., 76.),
            "第九条の三",
            &region
        ));
        // Headings and omission marks are never joined.
        assert!(!continues(
            &full,
            "前の列",
            &g(8., 200., 76.),
            "（新設）",
            &region
        ));
        assert!(!continues(
            &full,
            "前の列",
            &g(0., 200., 76.),
            "（削る）",
            &region
        ));
        // Unmarked text continues a first line only one em deeper.
        let first = g(8., 336., 88.);
        assert!(continues(
            &first,
            "前の列",
            &g(16., 200., 76.),
            "以下同じ。）",
            &region
        ));
        assert!(!continues(
            &first,
            "前の列",
            &g(8., 200., 76.),
            "この省令は",
            &region
        ));
        // A full stop before an item marker ends the provision.
        assert!(!continues(
            &full,
            "ものとする。",
            &g(8., 200., 76.),
            "イ総合確保法第十一条",
            &region
        ));
        assert!(continues(
            &full,
            "ものを含む。",
            &g(8., 200., 76.),
            "第十五条において同じ。）",
            &region
        ));
        // Not adjacent lines.
        assert!(!continues(
            &full,
            "前の列",
            &g(8., 200., 60.),
            "第九条の三",
            &region
        ));
    }

    fn text(t: &str) -> Inline {
        Inline::Text {
            text: t.into(),
            underline: false,
        }
    }

    #[test]
    fn joins_remove_only_the_break_before_the_proved_line() {
        let mut runs = vec![text("前の行「法\n第九条の三\n二　次")];
        let mut cursor = 0;
        join(&mut runs, &BTreeSet::from([5]), &mut cursor);
        let Inline::Text { text, .. } = &runs[0] else {
            panic!()
        };
        assert_eq!(text, "前の行「法第九条の三\n二　次");
    }

    #[test]
    fn split_keeps_styles_and_drops_the_break_at_the_cut() {
        let runs = vec![
            text("一　前\n"),
            Inline::Text {
                text: "一の二　後".into(),
                underline: true,
            },
        ];
        let (l, r) = split(&runs, 2);
        assert_eq!(
            serde_json::to_string(&l).unwrap(),
            serde_json::to_string(&vec![text("一　前")]).unwrap()
        );
        assert!(matches!(&r[..], [Inline::Text { text, underline: true }] if text == "一の二　後"));
    }
}

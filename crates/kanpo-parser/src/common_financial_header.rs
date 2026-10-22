//! A shared financial header precedes its parallel statement blocks.
//! Only whole source lines move; no word, table, geometry or text is synthesized.
use crate::trace::{ContentStyles, Line, NativeBounds, NativeStyles, Unit, Word, WordStatus};
use std::collections::{BTreeSet, HashMap, HashSet};
type Rect = [f32; 4];
fn valid(b: Rect) -> bool {
    b.iter().all(|v| v.is_finite()) && b[0] < b[2] && b[1] < b[3]
}
fn contains(a: Rect, b: Rect) -> bool {
    a[0] <= b[0] && a[1] <= b[1] && b[2] <= a[2] && b[3] <= a[3]
}
fn intersects(a: Rect, b: Rect) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}
fn cx(b: Rect) -> f32 {
    ((b[0] as f64 + b[2] as f64) / 2.) as f32
}
fn cy(b: Rect) -> f32 {
    ((b[1] as f64 + b[3] as f64) / 2.) as f32
}
fn compact(t: &str) -> String {
    t.chars().filter(|c| !c.is_whitespace()).collect()
}
fn financial_title(t: &str) -> bool {
    let t = compact(t);
    t.ends_with("決算公告") && !t.contains(['。', '、', '「', '」', '『', '』'])
}
struct SourceLine {
    ids: Vec<usize>,
    rect: Rect,
    table: bool,
}
fn source_line(
    l: &Line,
    words: &HashMap<usize, &Word>,
    bounds: &[NativeBounds],
) -> Option<SourceLine> {
    let ids: Vec<_> = if let Some(t) = &l.table {
        t.cells
            .iter()
            .flat_map(|c| c.words.iter().copied())
            .collect()
    } else {
        l.words.clone()
    };
    if ids.is_empty() {
        return None;
    }
    let mut unique = HashSet::new();
    let mut boxes = Vec::new();
    for &id in &ids {
        let w = *words.get(&id)?;
        if !matches!(w.status, WordStatus::Used) || !unique.insert(id) {
            return None;
        }
        let NativeBounds::Glyphs(b) = bounds.get(id)? else {
            return None;
        };
        if !valid(*b) {
            return None;
        }
        boxes.push(*b);
    }
    if let Some(t) = &l.table {
        let b = [t.x0, t.y0, t.x1, t.y1];
        if !valid(b) {
            return None;
        }
        boxes.push(b);
    } else if compact(
        &ids.iter()
            .map(|id| words[id].text.as_str())
            .collect::<String>(),
    ) != compact(&l.text)
    {
        return None;
    }
    let rect = boxes.into_iter().reduce(|a, b| {
        [
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[2].max(b[2]),
            a[3].max(b[3]),
        ]
    })?;
    Some(SourceLine {
        ids,
        rect,
        table: l.table.is_some(),
    })
}
fn fonts(
    line: &SourceLine,
    all: &[Option<NativeStyles>],
    content: &[ContentStyles],
) -> Option<BTreeSet<usize>> {
    let mut out = BTreeSet::new();
    for &id in &line.ids {
        let styles = all.get(id)?.as_ref()?;
        let ContentStyles::Glyphs(cs) = content.get(id)? else {
            return None;
        };
        if !cs.is_subset(styles) {
            return None;
        }
        out.extend(cs.iter().map(|&(font, _)| font));
    }
    (!out.is_empty()).then_some(out)
}
fn permutation(
    lines: &[Line],
    src: &[SourceLine],
    words: &HashMap<usize, &Word>,
    bounds: &[NativeBounds],
    all: &[Option<NativeStyles>],
    content: &[ContentStyles],
    frame: Rect,
) -> Option<(usize, Vec<usize>)> {
    if !valid(frame) {
        return None;
    }
    // A complete frame cannot borrow fragments from a line crossing its edge.
    if src
        .iter()
        .any(|l| intersects(frame, l.rect) && !contains(frame, l.rect))
    {
        return None;
    }
    let selected: Vec<_> = src
        .iter()
        .enumerate()
        .filter_map(|(i, l)| contains(frame, l.rect).then_some(i))
        .collect();
    let &start = selected.first()?;
    let end = selected.last()? + 1;
    if selected != (start..end).collect::<Vec<_>>() {
        return None;
    }
    let mut seen = HashSet::new();
    for i in start..end {
        for &id in &src[i].ids {
            if !seen.insert(id) {
                return None;
            }
        }
    }
    for (&id, w) in words {
        if !matches!(w.status, WordStatus::Used) {
            continue;
        }
        let NativeBounds::Glyphs(b) = bounds.get(id)? else {
            return None;
        };
        if contains(frame, *b) && !seen.contains(&id) {
            return None;
        }
    }
    let root = &src[start];
    if root.table
        || root.rect[2] - root.rect[0] <= root.rect[3] - root.rect[1]
        || !financial_title(&lines[start].text)
    {
        return None;
    }
    let root_fonts = fonts(root, all, content)?;
    let children: Vec<_> = (start + 1..end)
        .filter(|&i| !src[i].table && financial_title(&lines[i].text))
        .collect();
    // Parallel blocks require distinct columns and a shared vertical row.
    if children.len() < 2 {
        return None;
    }
    for (n, &i) in children.iter().enumerate() {
        let b = src[i].rect;
        if fonts(&src[i], all, content)? != root_fonts || cy(b) <= cy(root.rect) {
            return None;
        }
        for &j in &children[n + 1..] {
            let c = src[j].rect;
            if (b[0] < c[2] && c[0] < b[2]) || b[1] >= c[3] || c[1] >= b[3] {
                return None;
            }
        }
    }
    let mut support = vec![false; children.len()];
    for i in start..end {
        if !src[i].table {
            continue;
        }
        let owners: Vec<_> = children
            .iter()
            .enumerate()
            .filter(|(_, j)| {
                src[i].rect[0] <= cx(src[**j].rect)
                    && cx(src[**j].rect) <= src[i].rect[2]
                    && cy(src[**j].rect) < src[i].rect[1]
            })
            .map(|(n, _)| n)
            .collect();
        if owners.len() != 1 {
            return None;
        }
        support[owners[0]] = true;
    }
    if support.iter().any(|v| !*v) {
        return None;
    }
    let top = children.iter().map(|&i| cy(src[i].rect)).reduce(f32::min)?;
    let header: Vec<_> = (start..end)
        .filter(|&i| {
            !src[i].table
                && src[i]
                    .ids
                    .iter()
                    .all(|id| matches!(&bounds[*id],NativeBounds::Glyphs(b) if cy(*b)<top))
        })
        .collect();
    if header.first() != Some(&start) {
        return None;
    }
    let span = header.iter().map(|&i| src[i].rect).reduce(|a, b| {
        [
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[2].max(b[2]),
            a[3].max(b[3]),
        ]
    })?;
    if children
        .iter()
        .any(|&i| !(span[0] < cx(src[i].rect) && cx(src[i].rect) < span[2]))
    {
        return None;
    }
    if header.iter().any(|&i| cy(src[i].rect) < cy(root.rect)) {
        return None;
    }
    let mut order = header.clone();
    order.extend((start..end).filter(|i| !header.contains(i)));
    (order != (start..end).collect::<Vec<_>>()).then_some((start, order))
}
pub(crate) fn restore(
    units: &mut [Unit],
    word_values: &[Word],
    all: &[Option<NativeStyles>],
    content: &[ContentStyles],
    bounds: &[NativeBounds],
    frames: &[Rect],
) {
    if frames.iter().any(|&b| !valid(b)) {
        return;
    }
    let words: HashMap<_, _> = word_values.iter().map(|w| (w.id, w)).collect();
    if words.len() != word_values.len() {
        return;
    }
    let mut declarations = HashMap::new();
    for w in word_values {
        if !matches!(w.status, WordStatus::Used) {
            continue;
        }
        let key = (
            [
                w.x0.to_bits(),
                w.y0.to_bits(),
                w.x1.to_bits(),
                w.y1.to_bits(),
            ],
            w.raw.as_str(),
        );
        let value = (bounds.get(w.id), all.get(w.id), content.get(w.id));
        if declarations
            .insert(key, value)
            .is_some_and(|old| old != value)
        {
            return;
        }
    }

    for unit in units {
        let Unit::Plain(lines) = unit else {
            continue;
        };
        let Some(src) = lines
            .iter()
            .map(|l| source_line(l, &words, bounds))
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        let mut proposals = Vec::new();
        for &frame in frames {
            if frames.iter().any(|&other| {
                intersects(frame, other) && !contains(frame, other) && !contains(other, frame)
            }) {
                continue;
            }
            if let Some(p) = permutation(lines, &src, &words, bounds, all, content, frame) {
                if !proposals.contains(&p) {
                    proposals.push(p);
                }
            }
        }
        let accepted: Vec<_> = proposals
            .iter()
            .enumerate()
            .filter(|(i, (start, order))| {
                !proposals
                    .iter()
                    .enumerate()
                    .any(|(j, (s, o))| i != &j && *start < s + o.len() && *s < start + order.len())
            })
            .map(|(_, p)| p)
            .collect();
        for &(start, ref order) in accepted {
            let original = lines.clone();
            for (offset, &index) in order.iter().enumerate() {
                lines[start + offset] = original[index].clone();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture {
        units: Vec<Unit>,
        words: Vec<Word>,
        all: Vec<Option<NativeStyles>>,
        content: Vec<ContentStyles>,
        bounds: Vec<NativeBounds>,
        frames: Vec<Rect>,
    }
    impl Fixture {
        fn push(&mut self, text: &str, b: Rect, font: usize, table: bool) {
            let id = self.words.len();
            self.words.push(Word {
                id,
                x0: b[0],
                y0: b[1],
                x1: b[2],
                y1: b[3],
                raw: text.into(),
                text: text.into(),
                status: WordStatus::Used,
            });
            let styles = NativeStyles::from([(font, 8f64.to_bits())]);
            self.all.push(Some(styles.clone()));
            self.content.push(ContentStyles::Glyphs(styles));
            self.bounds.push(NativeBounds::Glyphs(b));
            let mut line = Line::text(text.into(), vec![id]);
            if table {
                line.words.clear();
                line.table = Some(crate::trace::Table {
                    x0: b[0],
                    y0: b[1],
                    x1: b[2],
                    y1: b[3],
                    rows: 1,
                    cols: 1,
                    cells: vec![crate::trace::TableCell {
                        row: 0,
                        col: 0,
                        rowspan: 1,
                        colspan: 1,
                        x0: b[0],
                        y0: b[1],
                        x1: b[2],
                        y1: b[3],
                        text: text.into(),
                        words: vec![id],
                    }],
                });
            }
            let Unit::Plain(lines) = &mut self.units[0] else {
                unreachable!()
            };
            lines.push(line);
        }
        fn restore(&mut self) {
            restore(
                &mut self.units,
                &self.words,
                &self.all,
                &self.content,
                &self.bounds,
                &self.frames,
            );
        }
        fn lines(&self) -> &[Line] {
            let Unit::Plain(lines) = &self.units[0] else {
                unreachable!()
            };
            lines
        }
        fn order(&self) -> Vec<usize> {
            self.lines()
                .iter()
                .flat_map(|l| {
                    l.words.iter().chain(
                        l.table
                            .iter()
                            .flat_map(|t| t.cells.iter().flat_map(|c| &c.words)),
                    )
                })
                .copied()
                .collect()
        }
        fn snapshot(&self) -> serde_json::Value {
            serde_json::to_value(self.lines()).unwrap()
        }
    }
    fn fixture() -> Fixture {
        let mut f = Fixture {
            units: vec![Unit::Plain(vec![])],
            words: vec![],
            all: vec![],
            content: vec![],
            bounds: vec![],
            frames: vec![[90., 40., 510., 300.]],
        };
        f.push("決算公告", [250., 50., 350., 60.], 1, false);
        f.push("2026年7月1日", [100., 70., 240., 80.], 2, false);
        f.push("任意決算公告", [120., 100., 240., 110.], 1, false);
        f.push("左表", [100., 120., 250., 280.], 2, true);
        f.push("発行主体", [370., 72., 500., 82.], 1, false);
        f.push("清算人任意", [370., 92., 500., 102.], 2, false);
        f.push("別の決算公告", [360., 100., 480., 110.], 1, false);
        f.push("右表", [350., 120., 500., 280.], 2, true);
        f
    }
    #[test]
    fn common_header_precedes_parallel_statements_without_fixed_gaps() {
        let mut f = fixture();
        f.restore();
        assert_eq!(f.order(), vec![0, 1, 4, 5, 2, 3, 6, 7]);
    }
    #[test]
    fn original_lines_tables_words_and_coordinates_are_preserved() {
        let mut f = fixture();
        let mut old: Vec<_> = f
            .lines()
            .iter()
            .map(|l| serde_json::to_string(l).unwrap())
            .collect();
        let words = serde_json::to_value(&f.words).unwrap();
        f.restore();
        let mut new: Vec<_> = f
            .lines()
            .iter()
            .map(|l| serde_json::to_string(l).unwrap())
            .collect();
        old.sort();
        new.sort();
        assert_eq!(old, new);
        assert_eq!(words, serde_json::to_value(&f.words).unwrap());
    }
    #[test]
    fn already_ordered_headers_are_idempotent() {
        let mut f = fixture();
        f.restore();
        let old = f.snapshot();
        f.restore();
        assert_eq!(f.snapshot(), old);
    }
    #[test]
    fn scaled_and_translated_geometry_preserves_the_source_order_constraint() {
        for k in [0.125, 1., 8.] {
            let mut f = fixture();
            let transform = |b: Rect| {
                [
                    b[0] * k - 100.,
                    b[1] * k - 200.,
                    b[2] * k - 100.,
                    b[3] * k - 200.,
                ]
            };
            for w in &mut f.words {
                let b = transform([w.x0, w.y0, w.x1, w.y1]);
                (w.x0, w.y0, w.x1, w.y1) = (b[0], b[1], b[2], b[3]);
            }
            for b in &mut f.bounds {
                if let NativeBounds::Glyphs(b) = b {
                    *b = transform(*b);
                }
            }
            for b in &mut f.frames {
                *b = transform(*b);
            }
            let Unit::Plain(lines) = &mut f.units[0] else {
                unreachable!()
            };
            for l in lines {
                if let Some(t) = &mut l.table {
                    let b = transform([t.x0, t.y0, t.x1, t.y1]);
                    (t.x0, t.y0, t.x1, t.y1) = (b[0], b[1], b[2], b[3]);
                    for c in &mut t.cells {
                        let b = transform([c.x0, c.y0, c.x1, c.y1]);
                        (c.x0, c.y0, c.x1, c.y1) = (b[0], b[1], b[2], b[3]);
                    }
                }
            }
            f.restore();
            assert_eq!(f.order(), vec![0, 1, 4, 5, 2, 3, 6, 7]);
        }
    }
    #[test]
    fn unknown_native_geometry_cannot_authorize_a_header_move() {
        let mut f = fixture();
        f.bounds[4] = NativeBounds::Unknown;
        let old = f.snapshot();
        f.restore();
        assert_eq!(f.snapshot(), old);
    }
    #[test]
    fn incomplete_or_foreign_content_typography_abstains() {
        for bad in [
            ContentStyles::Unknown,
            ContentStyles::Compatibility,
            ContentStyles::Glyphs(NativeStyles::from([(99, 8f64.to_bits())])),
        ] {
            let mut f = fixture();
            f.content[0] = bad;
            let old = f.snapshot();
            f.restore();
            assert_eq!(f.snapshot(), old);
        }
    }
    #[test]
    fn a_missing_child_table_is_not_a_parallel_statement_group() {
        let mut f = fixture();
        let Unit::Plain(lines) = &mut f.units[0] else {
            unreachable!()
        };
        lines[7].table = None;
        lines[7].words = vec![7];
        let old = f.snapshot();
        f.restore();
        assert_eq!(f.snapshot(), old);
    }
    #[test]
    fn stacked_notices_are_not_parallel_children() {
        let mut f = fixture();
        f.bounds[6] = NativeBounds::Glyphs([360., 190., 480., 200.]);
        let old = f.snapshot();
        f.restore();
        assert_eq!(f.snapshot(), old);
    }
    #[test]
    fn a_line_crossing_the_source_enclosure_is_not_split() {
        let mut f = fixture();
        f.bounds[4] = NativeBounds::Glyphs([80., 72., 500., 82.]);
        let old = f.snapshot();
        f.restore();
        assert_eq!(f.snapshot(), old);
    }
    #[test]
    fn an_unclaimed_source_word_inside_the_frame_prevents_reordering() {
        let mut f = fixture();
        let mut w = f.words[4].clone();
        w.id = f.words.len();
        f.words.push(w);
        f.bounds.push(f.bounds[4].clone());
        f.all.push(f.all[4].clone());
        f.content.push(f.content[4].clone());
        let old = f.snapshot();
        f.restore();
        assert_eq!(f.snapshot(), old);
    }
    #[test]
    fn nonnested_intersecting_enclosures_are_ambiguous() {
        let mut f = fixture();
        f.frames.push([100., 40., 520., 300.]);
        let old = f.snapshot();
        f.restore();
        assert_eq!(f.snapshot(), old);
    }
    #[test]
    fn different_child_fonts_do_not_prove_a_common_header() {
        let mut f = fixture();
        let s = NativeStyles::from([(7, 8f64.to_bits())]);
        f.all[6] = Some(s.clone());
        f.content[6] = ContentStyles::Glyphs(s);
        let old = f.snapshot();
        f.restore();
        assert_eq!(f.snapshot(), old);
    }
    #[test]
    fn independent_child_metadata_below_their_headings_is_not_promoted() {
        let mut f = fixture();
        f.bounds[4] = NativeBounds::Glyphs([370., 111., 500., 118.]);
        f.bounds[5] = NativeBounds::Glyphs([370., 112., 500., 119.]);
        let old = f.snapshot();
        f.restore();
        assert_eq!(f.snapshot(), old);
    }
    #[test]
    fn three_parallel_children_and_additional_header_lines_have_no_fixed_count_limit() {
        let mut f = fixture();
        f.bounds[2] = NativeBounds::Glyphs([120., 100., 220., 110.]);
        f.bounds[6] = NativeBounds::Glyphs([270., 100., 330., 110.]);
        f.bounds[3] = NativeBounds::Glyphs([100., 120., 230., 280.]);
        f.bounds[7] = NativeBounds::Glyphs([250., 120., 350., 280.]);
        let Unit::Plain(lines) = &mut f.units[0] else {
            unreachable!()
        };
        lines[3].table.as_mut().unwrap().x1 = 230.;
        lines[7].table.as_mut().unwrap().x0 = 250.;
        lines[7].table.as_mut().unwrap().x1 = 350.;
        for i in [3, 7] {
            let t = lines[i].table.as_mut().unwrap();
            for c in &mut t.cells {
                c.x0 = t.x0;
                c.x1 = t.x1;
            }
        }
        f.push("もう一つの決算公告", [400., 100., 480., 110.], 1, false);
        f.push("第三表", [380., 120., 500., 280.], 2, true);
        f.push("任意の追加情報", [370., 84., 500., 90.], 2, false);
        f.restore();
        assert_eq!(f.order(), vec![0, 1, 4, 5, 10, 2, 3, 6, 7, 8, 9]);
    }
    #[test]
    fn arbitrary_header_words_and_title_prefixes_do_not_select_the_repair() {
        let mut f = fixture();
        for (id, text) in [
            (0, "任意の総括決算公告"),
            (1, "追加の共通情報"),
            (2, "別会計の決算公告"),
            (4, "別の発行者"),
            (5, "連絡情報"),
            (6, "当該部門の決算公告"),
        ] {
            f.words[id].text = text.into();
            f.words[id].raw = text.into();
            let Unit::Plain(lines) = &mut f.units[0] else {
                unreachable!()
            };
            lines[id].text = text.into();
        }
        f.restore();
        assert_eq!(f.order(), vec![0, 1, 4, 5, 2, 3, 6, 7]);
    }
    #[test]
    fn missing_enclosures_do_not_authorize_a_header_move() {
        let mut f = fixture();
        f.frames.clear();
        let old = f.snapshot();
        f.restore();
        assert_eq!(f.snapshot(), old);
    }
    #[test]
    fn a_table_shared_by_both_child_columns_is_ambiguous() {
        let mut f = fixture();
        let Unit::Plain(lines) = &mut f.units[0] else {
            unreachable!()
        };
        lines[3].table.as_mut().unwrap().x1 = 500.;
        let old = f.snapshot();
        f.restore();
        assert_eq!(f.snapshot(), old);
    }
}

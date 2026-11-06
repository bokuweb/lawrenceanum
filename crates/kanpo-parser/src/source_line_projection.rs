//! Forward provenance for the renderer's parallel-column composition.
//! Real text drives the existing grammar; tokens only observe its operations.
use crate::{
    source_underlines::Basis,
    trace::{Line, PageTrace, Word, WordStatus},
    vertical::Col,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Origin {
    Word {
        word: usize,
        scalar: usize,
    },
    ParallelJoin {
        before: Vec<usize>,
        after: Vec<usize>,
    },
    RelocatedWord {
        word: usize,
        scalar: usize,
        target_word: usize,
    },
    Whitespace,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Token {
    pub character: char,
    pub origin: Origin,
}
pub(crate) type Text = Vec<Token>;
#[derive(Default)]
pub(crate) struct Capture {
    pub entries: BTreeMap<usize, Text>,
    pub prefixes: Vec<Prefix>,
}
#[derive(Clone)]
pub(crate) struct Prefix {
    pub word: usize,
    pub target: usize,
    pub text: String,
}
pub(crate) fn prefix_len(c: &Col, until: f32) -> usize {
    let chars = c.text.chars().count();
    let pitch = (c.y1 - c.y) / chars.max(1) as f32;
    (0..chars)
        .take_while(|k| c.y + pitch * (*k as f32 + 0.5) < until)
        .count()
}
pub(crate) fn value(tokens: &[Token]) -> String {
    tokens.iter().map(|t| t.character).collect()
}
fn word_tokens(word: &Word) -> Text {
    let mut scalar = 0;
    word.text
        .chars()
        .map(|character| {
            let origin = if character.is_whitespace() {
                Origin::Whitespace
            } else {
                let at = scalar;
                scalar += 1;
                Origin::Word {
                    word: word.id,
                    scalar: at,
                }
            };
            Token { character, origin }
        })
        .collect()
}
fn owners(tokens: &[Token]) -> Vec<usize> {
    let mut out = Vec::new();
    for t in tokens {
        if let Origin::Word { word, .. } = t.origin {
            if !out.contains(&word) {
                out.push(word);
            }
        }
    }
    out
}
fn append(a: &mut Text, b: Text, separator: bool) {
    if separator {
        a.push(Token {
            character: '・',
            origin: Origin::ParallelJoin {
                before: owners(a),
                after: owners(&b),
            },
        });
    }
    a.extend(b);
}
impl Capture {
    pub fn seed(&mut self, cols: &[Col], words: &[Word]) {
        self.entries.clear();
        let mut counts = BTreeMap::<usize, usize>::new();
        for c in cols {
            *counts.entry(c.id).or_default() += 1;
        }
        for c in cols {
            if counts[&c.id] != 1 {
                continue;
            }
            let Some(w) = words.get(c.id).filter(|w| w.id == c.id && w.text == c.text) else {
                continue;
            };
            self.entries.insert(c.id, word_tokens(w));
        }
    }
    fn get(&self, c: &Col) -> Option<Text> {
        self.entries
            .get(&c.id)
            .filter(|t| value(t) == c.text)
            .cloned()
    }
    fn ordered(&self, cols: &[Col], ids: &[usize], compact: bool, sort: bool) -> Option<Text> {
        let mut own = ids
            .iter()
            .map(|&id| cols.get(id))
            .collect::<Option<Vec<_>>>()?;
        if sort {
            own.sort_by(|a, b| a.y.total_cmp(&b.y));
        }
        let mut out = Vec::new();
        for c in own {
            out.extend(self.get(c)?);
        }
        if compact {
            out.retain(|t| !t.character.is_whitespace());
        }
        Some(out)
    }
    pub fn three(
        &mut self,
        cols: &[Col],
        spaced: &[usize],
        left: &[usize],
        third: &[usize],
        owner: usize,
        actual_parts: &[String; 3],
        joins: [bool; 3],
        actual: &str,
    ) {
        let observed = (|| {
            let parts = [
                self.ordered(cols, spaced, false, false)?,
                self.ordered(cols, left, true, true)?,
                self.ordered(cols, third, true, true)?,
            ];
            if parts
                .iter()
                .zip(actual_parts)
                .any(|(p, text)| value(p) != *text)
            {
                return None;
            }
            let mut out = Vec::new();
            for (part, join) in parts.into_iter().zip(joins) {
                append(&mut out, part, join);
            }
            (value(&out) == actual).then_some(out)
        })();
        for &id in spaced.iter().chain(left).chain(third) {
            self.entries.remove(&cols[id].id);
        }
        if let Some(text) = observed {
            self.entries.insert(owner, text);
        }
    }
    pub fn two(
        &mut self,
        cols: &[Col],
        spaced: &[usize],
        left: &[usize],
        right: &Col,
        actual_left: &str,
        join: bool,
        actual: &str,
    ) {
        let observed = (|| {
            let mut out = self.ordered(cols, spaced, false, false)?;
            if value(&out) != right.text {
                return None;
            }
            let mut tail = Vec::new();
            let mut rest = BTreeMap::new();
            for &id in left {
                let c = &cols[id];
                let tokens = self.get(c)?;
                let k = prefix_len(c, right.y1);
                let pitch = (c.y1 - c.y) / c.text.chars().count().max(1) as f32;
                tail.extend(
                    tokens[..k]
                        .iter()
                        .cloned()
                        .enumerate()
                        .map(|(i, t)| (c.y + pitch * i as f32, t)),
                );
                if k < tokens.len() {
                    rest.insert(c.id, tokens[k..].to_vec());
                }
            }
            tail.sort_by(|a, b| a.0.total_cmp(&b.0));
            let tail = tail
                .into_iter()
                .map(|(_, t)| t)
                .filter(|t| !t.character.is_whitespace())
                .collect::<Vec<_>>();
            if value(&tail) != actual_left {
                return None;
            }
            append(&mut out, tail, join);
            (value(&out) == actual).then_some((out, rest))
        })();
        for &id in spaced.iter().chain(left) {
            self.entries.remove(&cols[id].id);
        }
        if let Some((text, rest)) = observed {
            self.entries.insert(right.id, text);
            self.entries.extend(rest);
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Record {
    pub page: usize,
    pub region: usize,
    pub line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cell: Option<usize>,
    pub field: crate::source_underlines::Field,
    pub offset: usize,
    pub text: String,
    pub tokens: Vec<Token>,
}

pub(crate) struct Page {
    cols: Vec<Col>,
    words: Vec<Word>,
    capture: Capture,
    geometry: BTreeMap<usize, (Basis, f64, f64, f64)>,
}
impl Page {
    pub fn new(
        node: scraper::ElementRef,
        width: f32,
        height: f32,
        input: &crate::PageInput,
        source: &kanpo_pdf::Aj1PageSources,
    ) -> Self {
        let (cols, _, capture) =
            crate::vertical::source_line_projection(node, width, height, input.rules.as_ref());
        // Late source normalizations can restore a Word omitted by the first
        // column pass. Re-run them from the original input, independently of
        // the observed/final document. Never borrow a result-only spelling.
        let original = crate::analyze_item(std::slice::from_ref(input));
        let words = original
            .pages
            .first()
            .map(|p| p.words.clone())
            .unwrap_or_default();
        let mut geometry = BTreeMap::new();
        for (word, ids) in original
            .pages
            .first()
            .map(|p| crate::source_baseline_shifts::occurrences(p, source.page()))
            .unwrap_or_default()
        {
            let observed = (|| {
                let mut points = Vec::new();
                for id in ids {
                    let g = source.page().glyphs.get(id)?;
                    if g.invisible {
                        return None;
                    }
                    let basis = Basis::glyph(g, source.glyph_text_axes(id)?)?;
                    let a = basis.point([g.x, g.y]);
                    let b = basis.point([g.end_x, g.end_y]);
                    if !a.iter().chain(&b).all(|v| v.is_finite()) || a[1] >= b[1] {
                        return None;
                    }
                    points.push((basis, a[0], a[1], b[1]));
                }
                let first = *points.first()?;
                if points.iter().any(|p| p.0 != first.0 || p.1 != first.1) {
                    return None;
                }
                Some((
                    first.0,
                    first.1,
                    points.iter().map(|p| p.2).reduce(f64::min)?,
                    points.iter().map(|p| p.3).reduce(f64::max)?,
                ))
            })();
            if let Some(entry) = observed {
                geometry.insert(word, entry);
            }
        }
        Self {
            cols,
            words,
            capture,
            geometry,
        }
    }
    fn validate(
        &self,
        mut tokens: Text,
        line: &Line,
        trace: &PageTrace,
        relocated: &BTreeMap<usize, usize>,
    ) -> Option<Text> {
        tokens.retain(|t| !t.character.is_whitespace());
        if value(&tokens)
            != line
                .text
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>()
        {
            return None;
        }
        let ids = line.words.iter().copied().collect::<BTreeSet<_>>();
        if ids.len() != line.words.len() {
            return None;
        }
        for &id in &ids {
            let word = trace.words.get(id).filter(|w| {
                w.id == id && (w.status == WordStatus::Used || relocated.contains_key(&id))
            })?;
            let original = self.words.get(id)?;
            if original.raw != word.raw
                || original.source_box != word.source_box
                || original.text != word.text
            {
                return None;
            }
            let expected = word
                .text
                .chars()
                .filter(|c| !c.is_whitespace())
                .enumerate()
                .collect::<Vec<_>>();
            let own = tokens
                .iter()
                .enumerate()
                .filter_map(|(pos, t)| match t.origin {
                    Origin::Word { word, scalar } | Origin::RelocatedWord { word, scalar, .. }
                        if word == id =>
                    {
                        Some((pos, scalar, t.character))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            if own.len() != expected.len()
                || own
                    .iter()
                    .zip(expected)
                    .any(|((_, i, c), (j, d))| *i != j || *c != d)
                || own.windows(2).any(|w| w[1].0 != w[0].0 + 1)
            {
                return None;
            }
        }
        for t in &tokens {
            match &t.origin {
                Origin::Word { word, .. } if ids.contains(word) => {}
                Origin::RelocatedWord {
                    word, target_word, ..
                } if relocated.get(word) == Some(target_word) && ids.contains(target_word) => {}
                Origin::ParallelJoin { before, after }
                    if !before.is_empty()
                        && !after.is_empty()
                        && before.iter().chain(after).all(|id| ids.contains(id)) => {}
                _ => return None,
            }
        }
        Some(tokens)
    }
    pub fn line(&self, line: &Line, trace: &PageTrace) -> Option<Text> {
        let ids = line.words.iter().copied().collect::<BTreeSet<_>>();
        let mut active = Vec::new();
        for c in &self.cols {
            let Some(tokens) = self
                .capture
                .entries
                .get(&c.id)
                .filter(|t| value(t) == c.text)
            else {
                continue;
            };
            let own = owners(tokens);
            if !own.is_empty() && own.iter().all(|id| ids.contains(id)) {
                active.push((c.id, tokens));
            }
        }
        let declared = line
            .words
            .iter()
            .flat_map(|id| {
                active
                    .iter()
                    .filter(move |(owner, _)| owner == id)
                    .flat_map(|(_, t)| t.iter().cloned())
            })
            .collect::<Vec<_>>();
        if let Some(tokens) = self.validate(declared, line, trace, &BTreeMap::new()) {
            return Some(tokens);
        }
        // Complete canonical source Words may use physical row order, including
        // a Word restored by a later source normalization than the column pass.
        // Parallel columns overlap primary spans and cannot enter this path.
        self.physical(line, trace)
    }
    fn physical(&self, line: &Line, trace: &PageTrace) -> Option<Text> {
        let ids = line.words.iter().copied().collect::<BTreeSet<_>>();
        let mut ordered = Vec::new();
        for id in ids {
            let word = self.words.get(id).filter(|w| w.id == id)?;
            ordered.push((self.geometry.get(&id)?, word_tokens(word)));
        }
        let first = ordered.first()?.0;
        if ordered
            .iter()
            .any(|(g, _)| g.0 != first.0 || g.1 != first.1)
        {
            return None;
        }
        ordered.sort_by(|a, b| a.0 .2.total_cmp(&b.0 .2));
        if ordered.windows(2).any(|w| w[0].0 .3 > w[1].0 .2) {
            return None;
        }
        self.validate(
            ordered.into_iter().flat_map(|(_, t)| t).collect(),
            line,
            trace,
            &BTreeMap::new(),
        )
    }
    fn unowned(trace: &PageTrace, id: usize) -> bool {
        trace
            .words
            .get(id)
            .is_some_and(|w| w.id == id && w.status != WordStatus::Used)
            && trace.regions.iter().flat_map(|r| &r.lines).all(|l| {
                !l.words.contains(&id)
                    && l.table
                        .as_ref()
                        .is_none_or(|t| t.cells.iter().all(|c| !c.words.contains(&id)))
            })
    }
    pub fn cell(&self, cell: &crate::trace::TableCell, trace: &PageTrace) -> Option<Text> {
        if !self
            .capture
            .prefixes
            .iter()
            .any(|p| cell.words.contains(&p.target))
        {
            return self.physical(&Line::text(cell.text.clone(), cell.words.clone()), trace);
        }
        let ordered = ordered_cell(trace, cell)?;
        let mut tokens = Vec::new();
        let mut relocated = BTreeMap::new();
        for id in ordered {
            tokens.extend(word_tokens(self.words.get(id).filter(|w| w.id == id)?));
        }
        let minus = self.capture.prefixes.iter().map(|p| p.target).collect();
        let texts = self.cols.iter().map(|c| (c.id, c.text.clone())).collect();
        let mut rendered = value(&tokens);
        let mut edits = Vec::new();
        crate::vertical::insert_source_prefixes(
            &mut rendered,
            &cell.words,
            &minus,
            &texts,
            Some(&mut edits),
        );
        for (id, at, character) in edits {
            let candidates = self
                .capture
                .prefixes
                .iter()
                .filter(|p| p.target == id)
                .collect::<Vec<_>>();
            let [prefix] = candidates.as_slice() else {
                return None;
            };
            if self
                .capture
                .prefixes
                .iter()
                .filter(|p| p.word == prefix.word || p.target == id)
                .count()
                != 1
                || !Self::unowned(trace, prefix.word)
            {
                return None;
            }
            let word = self
                .words
                .get(prefix.word)
                .filter(|w| w.id == prefix.word)?;
            let mut own = word_tokens(word);
            own.retain(|t| !t.character.is_whitespace());
            if value(&own) != prefix.text
                || own.len() != 1
                || own[0].character != character
                || relocated.insert(prefix.word, id).is_some()
            {
                return None;
            }
            for token in &mut own {
                let Origin::Word { word, scalar } = token.origin else {
                    return None;
                };
                token.origin = Origin::RelocatedWord {
                    word,
                    scalar,
                    target_word: id,
                };
            }
            let current = value(&tokens);
            let index = if at == current.len() {
                tokens.len()
            } else {
                current.char_indices().position(|(i, _)| i == at)?
            };
            tokens.splice(index..index, own);
        }
        if value(&tokens) != rendered {
            return None;
        }
        let mut line = Line::text(cell.text.clone(), cell.words.clone());
        line.words.extend(relocated.keys().copied());
        self.validate(tokens, &line, trace, &relocated)
    }
    pub fn auxiliary_locations(
        &self,
        trace: &PageTrace,
    ) -> BTreeMap<usize, (usize, usize, Option<usize>)> {
        let mut out = BTreeMap::<usize, Vec<(usize, usize, Option<usize>)>>::new();
        for (ri, r) in trace.regions.iter().enumerate() {
            for (li, l) in r.lines.iter().enumerate() {
                if let Some(table) = &l.table {
                    for (ci, cell) in table.cells.iter().enumerate() {
                        if let Some(tokens) = self.cell(cell, trace) {
                            for token in tokens {
                                if let Origin::RelocatedWord { word, .. } = token.origin {
                                    out.entry(word).or_default().push((ri, li, Some(ci)));
                                }
                            }
                        }
                    }
                }
            }
        }
        out.into_iter()
            .filter_map(|(id, v)| (v.len() == 1).then_some((id, v[0])))
            .collect()
    }
}

pub(crate) fn ordered_cell(page: &PageTrace, cell: &crate::trace::TableCell) -> Option<Vec<usize>> {
    if cell.words.iter().collect::<BTreeSet<_>>().len() != cell.words.len() {
        return None;
    }
    let cols = cell
        .words
        .iter()
        .map(|&id| {
            let w = page
                .words
                .get(id)
                .filter(|w| w.id == id && w.status == WordStatus::Used)?;
            Some(Col {
                id,
                x: w.x0,
                x1: w.x1,
                y: w.y0,
                y1: w.y1,
                text: w.text.clone(),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    let ordered = crate::shinkyu::cell_source_word_order(cols);
    (ordered.len() == cell.words.len()
        && ordered.iter().collect::<BTreeSet<_>>() == cell.words.iter().collect::<BTreeSet<_>>())
    .then_some(ordered)
}

#[cfg(test)]
#[path = "source_line_projection_tests.rs"]
mod tests;

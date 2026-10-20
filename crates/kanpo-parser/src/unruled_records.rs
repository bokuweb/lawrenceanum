//! Complete unruled records witnessed by repeated native header fields, numeric
//! key baselines, and empty gaps across every field. Captions remain outside.
use crate::{
    trace::{Line, Table, TableCell},
    vertical::Col,
};
use std::collections::BTreeSet;

type VRule = (f32, f32, f32);
type Box4 = (f32, f32, f32, f32);
#[derive(Clone)]
struct NativeLine {
    y0: f32,
    y1: f32,
    words: Vec<Col>,
}
#[derive(Clone)]
struct Header {
    at: usize,
    fields: Vec<Vec<Col>>,
    signature: Vec<String>,
}
fn middle(w: &Col) -> f32 {
    (w.x + w.x1) / 2.
}
fn numeric(t: &str) -> bool {
    let t = t.trim();
    (2..=15).contains(&t.chars().count()) && t.chars().all(|c| c.is_ascii_digit() || ('０'..='９').contains(&c))
}
fn unknown(t: &str) -> bool {
    t.chars().any(|c| {
        ('\u{e000}'..='\u{f8ff}').contains(&c) || ('\u{f0000}'..='\u{ffffd}').contains(&c) || ('\u{100000}'..='\u{10fffd}').contains(&c)
    })
}
fn native_lines(words: &[Col], em: f32) -> Vec<NativeLine> {
    let mut ws = words.to_vec();
    ws.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
    let mut ls: Vec<NativeLine> = vec![];
    for w in ws {
        match ls.last_mut() {
            Some(l) if (w.y - l.y0).abs() <= em * 0.15 => {
                l.y1 = l.y1.max(w.y1);
                l.words.push(w);
            }
            _ => ls.push(NativeLine {
                y0: w.y,
                y1: w.y1,
                words: vec![w],
            }),
        }
    }
    for l in &mut ls {
        l.words.sort_by(|a, b| a.x.total_cmp(&b.x));
    }
    ls
}
fn header(at: usize, l: &NativeLine, em: f32) -> Option<Header> {
    if l.words
        .iter()
        .any(|w| w.text.chars().any(|c| c.is_ascii_digit() || ('０'..='９').contains(&c)))
    {
        return None;
    }
    let mut fields: Vec<Vec<Col>> = vec![];
    for w in &l.words {
        match fields.last_mut() {
            Some(f) if w.x - f.last()?.x1 < em * 1.4 => f.push(w.clone()),
            _ => fields.push(vec![w.clone()]),
        }
    }
    if !(3..=8).contains(&fields.len()) {
        return None;
    }
    let signature: Vec<String> = fields
        .iter()
        .map(|f| f.iter().flat_map(|w| w.text.chars()).filter(|c| !c.is_whitespace()).collect())
        .collect();
    if signature
        .iter()
        .any(|s| !(2..=40).contains(&s.chars().count()) || !s.chars().any(char::is_alphabetic))
    {
        return None;
    }
    Some(Header { at, fields, signature })
}
fn same_header(a: &Header, b: &Header, em: f32) -> bool {
    a.signature == b.signature
        && a.fields
            .iter()
            .zip(&b.fields)
            .all(|(a, b)| (a[0].x - b[0].x).abs() <= em * 0.25 && (a.last().unwrap().x1 - b.last().unwrap().x1).abs() <= em * 0.25)
}
fn caption(l: &NativeLine, h: &Header, em: f32) -> bool {
    let s: String = l.words.iter().map(|w| w.text.as_str()).collect();
    let digit = s.chars().any(|c| c.is_ascii_digit() || ('０'..='９').contains(&c));
    digit
        && (8..=48).contains(&s.chars().count())
        && !s.starts_with(['〇', '○', '●'])
        && !s.contains(['。', '：', ':'])
        && (l.words[0].x - h.fields[0][0].x).abs() <= em * 1.25
}
fn field_text(ws: &[Col]) -> String {
    crate::horizontal::row_lines(ws.to_vec())
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}
fn cell(row: usize, col: usize, box4: Box4, ws: &[Col]) -> TableCell {
    TableCell {
        row,
        col,
        rowspan: 1,
        colspan: 1,
        x0: box4.0,
        y0: box4.1,
        x1: box4.2,
        y1: box4.3,
        text: field_text(ws),
        words: crate::horizontal::row_lines(ws.to_vec())
            .iter()
            .flat_map(|l| l.words.iter().copied())
            .collect(),
    }
}
fn build(
    words: &[Col],
    ls: &[NativeLine],
    hs: &[Header],
    em: f32,
    vrules: &[VRule],
    hrules: &[VRule],
    tables: &[Box4],
) -> Option<Vec<Line>> {
    let mut starts = vec![];
    for h in hs {
        let start = if h.at > 0
            && caption(&ls[h.at - 1], h, em)
            && ls[h.at].y0 - ls[h.at - 1].y1 >= em * 0.2
            && ls[h.at].y0 - ls[h.at - 1].y1 <= em * 2.
        {
            h.at - 1
        } else {
            h.at
        };
        starts.push(start);
    }
    // This pass must own the entire source domain, including every caption. It
    // never searches through a preceding article or skips unrecognized words.
    if starts[0] != 0 || hs.windows(2).any(|w| w[0].at >= w[1].at) {
        return None;
    }
    let mut out = vec![];
    let mut total_records = 0;
    for (k, h) in hs.iter().enumerate() {
        let body_end = starts.get(k + 1).copied().unwrap_or(ls.len());
        let first = h.at + 1;
        if body_end <= first {
            return None;
        }
        let body: Vec<Col> = ls[first..body_end].iter().flat_map(|l| l.words.iter().cloned()).collect();
        let x0 = h.fields[0][0].x;
        let x1 = body.iter().map(|w| w.x1).fold(h.fields.last()?.last()?.x1, f32::max);
        let mut xs = vec![x0];
        for pair in h.fields.windows(2) {
            xs.push((pair[0].last()?.x1 + pair[1][0].x) / 2.);
        }
        xs.push(x1);
        let col_of = |w: &Col| -> Option<usize> {
            let i = (0..xs.len() - 1).find(|&i| middle(w) >= xs[i] && middle(w) <= xs[i + 1])?;
            (w.x >= xs[i] - 0.05 && w.x1 <= xs[i + 1] + 0.05).then_some(i)
        };
        let mut columns = vec![vec![]; h.fields.len()];
        for w in &body {
            columns[col_of(w)?].push(w.clone());
        }
        let mut keys = columns[0].clone();
        keys.sort_by(|a, b| a.y.total_cmp(&b.y));
        if !(2..=64).contains(&keys.len())
            || keys
                .iter()
                .any(|w| !numeric(&w.text) || (w.x1 - h.fields[0].last().unwrap().x1).abs() > em * 0.5)
            || keys.windows(2).any(|w| w[1].y - w[0].y < em * 1.5)
        {
            return None;
        }
        total_records += keys.len();
        let mut bands = vec![];
        let mut grouped = vec![];
        let head_bottom = ls[h.at].y1;
        let head_cut = (head_bottom + keys[0].y) / 2.;
        if keys[0].y - head_bottom < em * 0.2 {
            return None;
        }
        let mut y0 = head_cut;
        for (r, key) in keys.iter().enumerate() {
            let next = keys.get(r + 1).map(|w| w.y).unwrap_or(f32::INFINITY);
            let row: Vec<Col> = body
                .iter()
                .filter(|w| w.y >= key.y - em * 0.15 && w.y < next - em * 0.15)
                .cloned()
                .collect();
            let min = row.iter().map(|w| w.y).fold(f32::INFINITY, f32::min);
            let max = row.iter().map(|w| w.y1).fold(f32::NEG_INFINITY, f32::max);
            if row.is_empty() || (min - key.y).abs() > em * 0.15 || next.is_finite() && next - max < em * 0.25 {
                return None;
            }
            let y1 = if next.is_finite() { (max + next) / 2. } else { max };
            let mut fields = vec![];
            for c in 0..h.fields.len() {
                let f: Vec<Col> = row.iter().filter(|w| col_of(w) == Some(c)).cloned().collect();
                if f.is_empty() || (f.iter().map(|w| w.y).fold(f32::INFINITY, f32::min) - key.y).abs() > em * 0.15 {
                    return None;
                }
                // These are text records, not a numeric matrix. A ditto is retained as an
                // original field; its predecessor must be a proved field in this group.
                if c > 0 {
                    let s: String = f.iter().map(|w| w.text.as_str()).collect();
                    if s.trim() == "〃" {
                        if r == 0 || f.len() != 1 {
                            return None;
                        }
                    } else if !s.chars().any(char::is_alphabetic) {
                        return None;
                    }
                }
                fields.push(f);
            }
            bands.push((y0, y1));
            grouped.push(fields);
            y0 = y1;
        }
        let source: BTreeSet<_> = body.iter().map(|w| w.id).collect();
        let got: Vec<_> = grouped.iter().flatten().flatten().map(|w| w.id).collect();
        if got.len() != body.len() || got.iter().copied().collect::<BTreeSet<_>>() != source {
            return None;
        }
        let box4 = (x0, ls[h.at].y0, x1, y0);
        if tables.iter().any(|t| t.0 < box4.2 && t.2 > box4.0 && t.1 < box4.3 && t.3 > box4.1)
            || vrules
                .iter()
                .any(|v| v.0 > box4.0 + 0.1 && v.0 < box4.2 - 0.1 && v.1 < box4.3 && v.2 > box4.1)
            || hrules
                .iter()
                .any(|v| v.0 > box4.1 + 0.1 && v.0 < box4.3 - 0.1 && v.1 < box4.2 && v.2 > box4.0)
        {
            return None;
        }
        if starts[k] < h.at {
            let l = &ls[starts[k]];
            out.push(Line::text(field_text(&l.words), l.words.iter().map(|w| w.id).collect()));
        }
        let mut cells = vec![];
        for (c, f) in h.fields.iter().enumerate() {
            cells.push(cell(0, c, (xs[c], box4.1, xs[c + 1], head_cut), f));
        }
        for (r, fields) in grouped.iter().enumerate() {
            for (c, f) in fields.iter().enumerate() {
                cells.push(cell(r + 1, c, (xs[c], bands[r].0, xs[c + 1], bands[r].1), f));
            }
        }
        let t = Table {
            x0: box4.0,
            y0: box4.1,
            x1: box4.2,
            y1: box4.3,
            rows: keys.len() + 1,
            cols: h.fields.len(),
            cells,
        };
        out.push(Line {
            text: t.render(),
            words: vec![],
            table: Some(t),
        });
    }
    if total_records < 6 {
        return None;
    }
    let expected: BTreeSet<_> = words.iter().map(|w| w.id).collect();
    let actual: Vec<_> = out
        .iter()
        .flat_map(|l| l.words.iter().chain(l.table.iter().flat_map(|t| &t.cells).flat_map(|c| &c.words)))
        .copied()
        .collect();
    (expected.len() == words.len() && actual.len() == words.len() && actual.iter().copied().collect::<BTreeSet<_>>() == expected)
        .then_some(out)
}

pub(crate) fn repeated_header_records(words: &[Col], vrules: &[VRule], hrules: &[VRule], tables: &[Box4]) -> Option<Vec<Line>> {
    if words.len() < 24
        || words
            .iter()
            .any(|w| w.text.trim().is_empty() || unknown(&w.text) || w.text.starts_with('〇'))
    {
        return None;
    }
    let mut sizes: Vec<_> = words.iter().map(|w| w.y1 - w.y).collect();
    sizes.sort_by(f32::total_cmp);
    let em = sizes[sizes.len() / 2];
    if em < 4. || words.iter().any(|w| (w.y1 - w.y - em).abs() > em * 0.2) {
        return None;
    }
    let x0 = words.iter().map(|w| w.x).fold(f32::INFINITY, f32::min);
    let x1 = words.iter().map(|w| w.x1).fold(f32::NEG_INFINITY, f32::max);
    let y0 = words.iter().map(|w| w.y).fold(f32::INFINITY, f32::min);
    let y1 = words.iter().map(|w| w.y1).fold(f32::NEG_INFINITY, f32::max);
    if vrules.iter().any(|v| v.0 > x0 + 0.1 && v.0 < x1 - 0.1 && v.1 < y1 && v.2 > y0) {
        return None;
    }
    let ls = native_lines(words, em);
    let headers: Vec<_> = ls.iter().enumerate().filter_map(|(i, l)| header(i, l, em)).collect();
    let mut seen = BTreeSet::new();
    let mut result = None;
    for h in &headers {
        let family: Vec<_> = headers.iter().filter(|n| same_header(h, n, em)).cloned().collect();
        if family.len() < 2 {
            continue;
        }
        let key: Vec<_> = family.iter().map(|h| h.at).collect();
        if !seen.insert(key) {
            continue;
        }
        if let Some(candidate) = build(words, &ls, &family, em, vrules, hrules, tables) {
            if result.is_some() {
                return None;
            }
            result = Some(candidate);
        }
    }
    result
}
#[cfg(test)]
#[path = "unruled_record_tests.rs"]
mod tests;

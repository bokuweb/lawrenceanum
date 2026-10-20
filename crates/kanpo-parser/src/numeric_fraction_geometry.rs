//! Numeric factors share real horizontal bars; native bracket pieces retain grouping.
use crate::parallel_fraction_geometry::{Atom, Bar};
use std::collections::HashSet;
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Source {
        word_ids: Vec<usize>,
        text: String,
    },
    Sequence(Vec<Expr>),
    Fraction {
        bar: Bar,
        numerator: Box<Expr>,
        denominator: Box<Expr>,
    },
    NativeParentheses {
        word_ids: [usize; 2],
        child: Box<Expr>,
    },
}
impl Expr {
    pub fn render(&self) -> String {
        match self {
            Self::Source { text, .. } => text.clone(),
            Self::Sequence(xs) => xs.iter().map(Self::render).collect(),
            Self::Fraction {
                numerator,
                denominator,
                ..
            } => format!("({}/{})", numerator.render(), denominator.render()),
            Self::NativeParentheses { child, .. } => format!("({})", child.render()),
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Formula {
    pub owner: usize,
    pub source_ids: Vec<usize>,
    pub bracket_ids: Vec<usize>,
    pub original: String,
    pub tree: Expr,
}
fn cy(a: &Atom) -> f32 {
    (a.y0 + a.y1) * 0.5
}
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
fn known(a: &Atom) -> bool {
    a.raw == a.text || (a.raw == "\u{ea74}" && a.text == ".")
}
fn number(s: &str) -> bool {
    let cs: Vec<_> = s.chars().collect();
    !cs.is_empty()
        && cs
            .iter()
            .all(|c| c.is_ascii_digit() || ('０'..='９').contains(c) || ".．".contains(*c))
        && cs
            .iter()
            .any(|c| ('1'..='9').contains(c) || ('１'..='９').contains(c))
        && cs.iter().filter(|c| ".．".contains(**c)).count() <= 1
        && !".．".contains(cs[0])
        && !".．".contains(*cs.last().unwrap())
}
fn prefix(a: &Atom) -> bool {
    a.text.strip_suffix('×').is_some_and(|s| {
        !s.is_empty()
            && !number(s)
            && s.chars()
                .all(|c| c.is_alphanumeric() || "・々ヶ".contains(c))
    })
}
fn source(xs: &[&Atom]) -> Expr {
    Expr::Source {
        word_ids: xs.iter().map(|a| a.id).collect(),
        text: xs.iter().map(|a| a.text.as_str()).collect(),
    }
}
fn numeric_row<'a>(atoms: &'a [Atom], b: Bar, em: f32, upper: bool) -> Option<Vec<&'a Atom>> {
    let mut xs: Vec<_> = atoms
        .iter()
        .filter(|a| {
            a.owner.is_some()
                && !a.text.is_empty()
                && a.x0 >= b.x0 - 0.05
                && a.x1 <= b.x1 + 0.05
                && if upper {
                    a.y1 <= b.y + 0.05 && b.y - a.y1 <= em * 0.4
                } else {
                    a.y0 >= b.y - 0.05 && a.y0 - b.y <= em * 0.5
                }
        })
        .collect();
    xs.sort_by(|a, b| a.x0.total_cmp(&b.x0));
    if xs.is_empty() || xs.iter().any(|a| (cy(a) - cy(xs[0])).abs() > em * 0.1) {
        return None;
    }
    for w in xs.windows(2) {
        let gap = w[1].x0 - w[0].x1;
        if gap > em * 0.6
            || gap < -em * 0.1
                && !(w.iter().any(|a| a.raw == "\u{ea74}" && a.text == ".") && gap >= -em * 0.6)
        {
            return None;
        }
    }
    let text: String = xs.iter().map(|a| a.text.as_str()).collect();
    let x0 = xs.first()?.x0;
    let x1 = xs.iter().map(|a| a.x1).fold(f32::NEG_INFINITY, f32::max);
    if !number(&text) || ((x0 + x1 - b.x0 - b.x1) * 0.5).abs() > em * 0.2 {
        return None;
    }
    Some(xs)
}
fn attempt(atoms: &[Atom], bars: &[Bar], p: &Atom, left: Bar) -> Option<Formula> {
    let owner = p.owner?;
    let em = p.y1 - p.y0;
    if !(3.0..=20.0).contains(&em)
        || p.x1 <= p.x0
        || !p.eligible
        || !known(p)
        || !prefix(p)
        || !left.y.is_finite()
        || !left.x0.is_finite()
        || !left.x1.is_finite()
        || (left.x0 - p.x1).abs() > em * 0.25
        || (cy(p) - left.y).abs() > em * 0.2
    {
        return None;
    }
    let mut bs: Vec<_> = bars
        .iter()
        .copied()
        .filter(|b| {
            (b.y - left.y).abs() <= em * 0.05
                && b.x0 >= left.x0 - 0.05
                && b.x1 > b.x0
                && b.x0.is_finite()
                && b.x1.is_finite()
        })
        .collect();
    bs.sort_by(|a, b| a.x0.total_cmp(&b.x0));
    let mut end = 1;
    while end < bs.len() && bs[end].x0 - bs[end - 1].x1 <= em * 2.0 {
        end += 1
    }
    bs.truncate(end);
    if !(2..=4).contains(&bs.len())
        || bs.first() != Some(&left)
        || bs.windows(2).any(|w| w[0].x1 >= w[1].x0)
    {
        return None;
    }
    let mut uppers = vec![];
    let mut lowers = vec![];
    let mut fs = vec![];
    let mut used = HashSet::new();
    used.insert(p.id);
    for &b in &bs {
        let n = numeric_row(atoms, b, em, true)?;
        let d = numeric_row(atoms, b, em, false)?;
        if n.iter().chain(&d).any(|a| {
            a.owner != Some(owner)
                || !a.eligible
                || !known(a)
                || a.x1 <= a.x0
                || (a.y1 - a.y0 - em).abs() > em * 0.15
                || !used.insert(a.id)
        }) {
            return None;
        }
        if !uppers.is_empty()
            && ((cy(n[0]) - cy(uppers[0])).abs() > em * 0.1
                || (cy(d[0]) - cy(lowers[0])).abs() > em * 0.1)
        {
            return None;
        }
        fs.push(Expr::Fraction {
            bar: b,
            numerator: Box::new(source(&n)),
            denominator: Box::new(source(&d)),
        });
        uppers.extend(n);
        lowers.extend(d);
    }
    let mut ops = vec![];
    for w in bs.windows(2) {
        let xs: Vec<_> = atoms
            .iter()
            .filter(|a| {
                a.owner == Some(owner)
                    && a.x0 >= w[0].x1 - 0.05
                    && a.x1 <= w[1].x0 + 0.05
                    && (cy(a) - left.y).abs() <= em * 0.2
                    && ["×", "−"].contains(&a.text.as_str())
            })
            .collect();
        if xs.len() != 1
            || !xs[0].eligible
            || !known(xs[0])
            || xs[0].x1 <= xs[0].x0
            || ((xs[0].y1 - xs[0].y0) - em).abs() > em * 0.15
            || !used.insert(xs[0].id)
        {
            return None;
        }
        ops.push(xs[0]);
    }
    let y0 = uppers.iter().map(|a| a.y0).fold(f32::INFINITY, f32::min);
    let y1 = lowers
        .iter()
        .map(|a| a.y1)
        .fold(f32::NEG_INFINITY, f32::max);
    let x1 = bs.last()?.x1;
    let near: Vec<_> = atoms
        .iter()
        .filter(|a| a.x0 < x1 + em * 1.2 && a.x1 > p.x0 && a.y0 < y1 && a.y1 > y0)
        .collect();
    let brackets: Vec<_> = near
        .iter()
        .copied()
        .filter(|a| a.text.is_empty() && !a.raw.is_empty())
        .collect();
    let mut bracket_ids = vec![];
    let mut group = None;
    if !brackets.is_empty() {
        if brackets.len() != 2 {
            return None;
        }
        let l = *brackets.iter().find(|a| a.raw == "\u{eae9}\u{eaea}")?;
        let r = *brackets.iter().find(|a| a.raw == "\u{eaeb}\u{eaec}")?;
        if l.owner.is_some()
            || r.owner.is_some()
            || !l.eligible
            || !r.eligible
            || l.id == r.id
            || l.x1 >= r.x0
            || [l, r].iter().any(|a| {
                ((a.y1 - a.y0) - 2.0 * em).abs() > em * 0.15 || (cy(a) - left.y).abs() > em * 0.2
            })
        {
            return None;
        }
        let inside: Vec<_> = bs
            .iter()
            .enumerate()
            .filter(|(_, b)| b.x0 >= l.x1 - em * 0.3 && b.x1 <= r.x0 + em * 0.3)
            .map(|(i, _)| i)
            .collect();
        if inside.len() < 2 {
            return None;
        }
        let a = inside[0];
        let z = *inside.last()?;
        if z - a + 1 != inside.len()
            || (bs[a].x0 - l.x1).abs() > em * 0.3
            || (r.x0 - bs[z].x1).abs() > em * 0.3
        {
            return None;
        }
        if a > 0 && !(l.x0 >= ops[a - 1].x1 - em * 0.1 && l.x1 <= bs[a].x0 + em * 0.3) {
            return None;
        }
        if l.x0 < p.x1 - em * 0.1 || (z + 1 < bs.len() && r.x1 > ops[z].x0 + em * 0.1) {
            return None;
        }
        if r.x1 > x1 + em * 1.2 {
            return None;
        }
        group = Some((a, z, [l.id, r.id]));
        bracket_ids = vec![l.id, r.id];
    }
    if ops
        .iter()
        .enumerate()
        .any(|(i, o)| o.text == "−" && !group.is_some_and(|(a, z, _)| i >= a && i < z))
    {
        return None;
    }
    // Unknown text or unknown empty PUA overlapping this exact expression prevents any partial rewrite.
    let bound = if let Some((_, _, [_, rid])) = group {
        atoms.iter().find(|a| a.id == rid)?.x1
    } else {
        x1
    };
    if atoms.iter().any(|a| {
        a.x0 < bound - 0.05
            && a.x1 > p.x0 + 0.05
            && a.y0 < y1 - 0.05
            && a.y1 > y0 + 0.05
            && !used.contains(&a.id)
            && !bracket_ids.contains(&a.id)
            && !a.raw.is_empty()
    }) {
        return None;
    }
    let mut parts = vec![source(&[p])];
    let mut i = 0;
    while i < fs.len() {
        if let Some((a, z, ids)) = group {
            if i == a {
                let mut inner = vec![fs[i].clone()];
                for j in i..z {
                    inner.push(source(&[ops[j]]));
                    inner.push(fs[j + 1].clone());
                }
                parts.push(Expr::NativeParentheses {
                    word_ids: ids,
                    child: Box::new(Expr::Sequence(inner)),
                });
                i = z + 1;
            } else {
                parts.push(fs[i].clone());
                i += 1;
            }
        } else {
            parts.push(fs[i].clone());
            i += 1;
        }
        if i < fs.len() {
            parts.push(source(&[ops[i - 1]]));
        }
    }
    let mut rs = vec![uppers, vec![p], lowers];
    rs[1].extend(ops);
    let mut original = String::new();
    for row in &mut rs {
        row.sort_by(|a, b| a.x0.total_cmp(&b.x0));
        for a in row {
            original.push_str(&compact(&a.text));
        }
    }
    let source_ids = atoms
        .iter()
        .filter(|a| used.contains(&a.id))
        .map(|a| a.id)
        .collect();
    Some(Formula {
        owner,
        source_ids,
        bracket_ids,
        original,
        tree: Expr::Sequence(parts),
    })
}
pub fn parse(atoms: &[Atom], bars: &[Bar]) -> Vec<Formula> {
    let mut ids = HashSet::new();
    if atoms
        .iter()
        .any(|a| !ids.insert(a.id) || ![a.x0, a.y0, a.x1, a.y1].iter().all(|v| v.is_finite()))
    {
        return vec![];
    }
    let mut out = vec![];
    for p in atoms.iter().filter(|a| a.owner.is_some() && prefix(a)) {
        let found: Vec<_> = bars
            .iter()
            .filter_map(|b| attempt(atoms, bars, p, *b))
            .collect();
        if found.len() == 1 {
            out.push(found.into_iter().next().unwrap());
        }
    }
    let mut used = HashSet::new();
    if out
        .iter()
        .flat_map(|f| &f.source_ids)
        .any(|id| !used.insert(id))
    {
        return vec![];
    }
    out
}
#[cfg(test)]
#[path = "numeric_fraction_geometry_tests.rs"]
pub(crate) mod tests;

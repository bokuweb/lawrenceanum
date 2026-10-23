#[derive(Clone, Debug)]
pub struct Atom {
    pub id: usize,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub raw: String,
    pub text: String,
    pub owner: Option<usize>,
    pub eligible: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bar {
    pub y: f32,
    pub x0: f32,
    pub x1: f32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Formula {
    pub owner: usize,
    pub source_ids: Vec<usize>,
    pub bracket_ids: [usize; 2],
    pub original: String,
    pub text: String,
    pub tree: Expr,
}
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
    fn render(&self) -> String {
        match self {
            Self::Source { text, .. } => text.clone(),
            Self::Sequence(parts) => parts.iter().map(Self::render).collect(),
            Self::NativeParentheses { child, .. } => format!("({})", child.render()),
            Self::Fraction {
                numerator,
                denominator,
                ..
            } => {
                let d = denominator.render();
                let d = if matches!(denominator.as_ref(),Self::Source{text,..} if numeric(text)) {
                    d
                } else {
                    format!("({d})")
                };
                format!("({})/{d}", numerator.render())
            }
        }
    }
}
fn source(atoms: &[&Atom], text: String) -> Expr {
    let mut a = atoms.to_vec();
    a.sort_by(|x, y| x.x0.total_cmp(&y.x0));
    Expr::Source {
        word_ids: a.iter().map(|w| w.id).collect(),
        text,
    }
}

fn center(a: &Atom) -> f32 {
    (a.y0 + a.y1) * 0.5
}
fn numeric(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .any(|c| c.is_ascii_digit() || ('０'..='９').contains(&c))
        && s.chars()
            .all(|c| c.is_ascii_digit() || ('０'..='９').contains(&c) || ".．".contains(c))
}
// Deliberately no quotient, grouping, relation, PUA, or punctuation in a leaf.
// Unknown compound mathematics must not be replaced with a simpler tree.
fn leaf(s: &str) -> bool {
    !s.is_empty()
        && s.chars().all(|c| {
            c.is_alphanumeric()
                || ('ぁ'..='ヿ').contains(&c)
                || ('一'..='龯').contains(&c)
                || "＋+×.．".contains(c)
        })
}
fn row(atoms: &[&Atom], em: f32) -> Option<String> {
    if atoms.is_empty() {
        return None;
    }
    let cy = center(atoms[0]);
    let mut xs = atoms.to_vec();
    xs.sort_by(|a, b| a.x0.total_cmp(&b.x0));
    if xs.iter().any(|a| (center(a) - cy).abs() > em * 0.15)
        || xs
            .windows(2)
            .any(|w| w[1].x0 - w[0].x1 < -em * 0.1 || w[1].x0 - w[0].x1 > em * 0.6)
    {
        return None;
    }
    Some(xs.iter().map(|a| a.text.as_str()).collect())
}
fn centered(atoms: &[&Atom], bar: Bar, em: f32) -> bool {
    let x0 = atoms.iter().map(|a| a.x0).fold(f32::INFINITY, f32::min);
    let x1 = atoms.iter().map(|a| a.x1).fold(f32::NEG_INFINITY, f32::max);
    x0 >= bar.x0 - em * 0.15
        && x1 <= bar.x1 + em * 0.15
        && ((x0 + x1) - (bar.x0 + bar.x1)).abs() <= em * 0.8
}
fn attempt(
    atoms: &[Atom],
    bars: &[Bar],
    left: &Atom,
    right: &Atom,
    outer: Bar,
    inner: Bar,
) -> Option<Formula> {
    let em = (left.y1 - left.y0) * 0.5;
    if !(3.0..=20.0).contains(&em)
        || left.owner.is_some()
        || right.owner.is_some()
        || !left.eligible
        || !right.eligible
        || !left.text.is_empty()
        || !right.text.is_empty()
        || ((right.y1 - right.y0) - 2.0 * em).abs() > em * 0.15
        || (center(left) - center(right)).abs() > em * 0.15
        || (center(left) - inner.y).abs() > em * 0.2
        || left.x1 >= right.x0
        || (inner.x0 - left.x1).abs() > em * 0.3
        || (inner.x1 - right.x0).abs() > em * 0.3
        || !(em * 0.9..=em * 1.8).contains(&(inner.y - outer.y))
        || inner.x0 - outer.x0 < em * 0.5
        || outer.x1 - inner.x1 < em * 0.5
        || !(em * 4.0..=em * 60.0).contains(&(outer.x1 - outer.x0))
        || !(em * 2.0..=em * 40.0).contains(&(inner.x1 - inner.x0))
    {
        return None;
    }
    // A third local bar could be another division or a table rule. Do not pick
    // an arbitrary two-bar subset of a larger expression.
    if bars
        .iter()
        .filter(|b| {
            b.x0 >= outer.x0 - em * 0.1
                && b.x1 <= outer.x1 + em * 0.1
                && b.y >= outer.y - em * 0.1
                && b.y <= inner.y + em * 1.5
        })
        .count()
        != 2
    {
        return None;
    }
    let local: Vec<_> = atoms
        .iter()
        .filter(|a| {
            a.x1 > outer.x0
                && a.x0 < outer.x1
                && center(a) >= outer.y - em * 1.3
                && center(a) <= inner.y + em * 1.5
        })
        .collect();
    let mut seen = std::collections::HashSet::new();
    let mut operand = Vec::new();
    for a in &local {
        if !seen.insert(a.id) {
            return None;
        }
        if a.id == left.id || a.id == right.id {
            continue;
        }
        if !a.eligible
            || a.text.is_empty()
            || a.raw != a.text
            || !leaf(&a.text)
            || a.owner.is_none()
            || !(em * 0.8..=em * 1.2).contains(&(a.y1 - a.y0))
            || a.x0 < outer.x0 - em * 0.15
            || a.x1 > outer.x1 + em * 0.15
        {
            return None;
        }
        operand.push(*a);
    }
    let owner = operand.first()?.owner?;
    if operand.iter().any(|a| a.owner != Some(owner)) {
        return None;
    }
    let (mut top, mut num, mut den, mut prefix, mut suffix) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for a in &operand {
        let cy = center(a);
        if cy < outer.y && a.y1 <= outer.y + em * 0.1 && outer.y - a.y1 <= em * 0.4 {
            top.push(*a)
        } else if cy > outer.y
            && cy < inner.y - em * 0.3
            && a.y1 <= inner.y + em * 0.1
            && inner.y - a.y1 <= em * 0.4
        {
            num.push(*a)
        } else if cy > inner.y + em * 0.35 && a.y0 >= inner.y && a.y0 - inner.y <= em * 0.6 {
            den.push(*a)
        } else if (cy - inner.y).abs() <= em * 0.3 && a.x1 <= left.x0 + em * 0.15 {
            prefix.push(*a)
        }
        // Composite glyph metric boxes include side bearings and overlap by ~1pt.
        // Require the suffix to start beyond the right glyph's left edge by half
        // an em; do not claim that full font metric boxes are disjoint.
        else if (cy - inner.y).abs() <= em * 0.3 && a.x0 >= right.x0 + em * 0.5 {
            suffix.push(*a)
        } else {
            return None;
        }
    }
    let t = row(&top, em)?;
    let n = row(&num, em)?;
    let d = row(&den, em)?;
    let p = row(&prefix, em)?;
    let s = row(&suffix, em)?;
    let head = p.strip_suffix('＋').or_else(|| p.strip_suffix('+'))?;
    let tail = s.strip_prefix('×')?;
    if !numeric(head)
        || !numeric(&d)
        || !leaf(tail)
        || tail.contains(['＋', '+', '×'])
        || !centered(&top, outer, em)
        || !centered(&num, inner, em)
        || !centered(&den, inner, em)
    {
        return None;
    }
    let body_x0 = operand
        .iter()
        .filter(|a| !top.iter().any(|t| t.id == a.id))
        .map(|a| a.x0)
        .fold(f32::INFINITY, f32::min);
    let body_x1 = operand
        .iter()
        .filter(|a| !top.iter().any(|t| t.id == a.id))
        .map(|a| a.x1)
        .fold(f32::NEG_INFINITY, f32::max);
    if ((body_x0 + body_x1) - (outer.x0 + outer.x1)).abs() > em * 0.8 {
        return None;
    }
    let tree = Expr::Fraction {
        bar: outer,
        numerator: Box::new(source(&top, t)),
        denominator: Box::new(Expr::Sequence(vec![
            source(&prefix, p),
            Expr::NativeParentheses {
                word_ids: [left.id, right.id],
                child: Box::new(Expr::Fraction {
                    bar: inner,
                    numerator: Box::new(source(&num, n)),
                    denominator: Box::new(source(&den, d)),
                }),
            },
            source(&suffix, s),
        ])),
    };
    let text = tree.render();
    let mut ordered = Vec::new();
    for mut group in [
        top,
        num,
        {
            let mut middle = prefix;
            middle.extend(suffix);
            middle
        },
        den,
    ] {
        group.sort_by(|a, b| a.x0.total_cmp(&b.x0));
        ordered.extend(group)
    }
    Some(Formula {
        owner,
        source_ids: ordered.iter().map(|a| a.id).collect(),
        bracket_ids: [left.id, right.id],
        original: ordered.iter().map(|a| a.text.as_str()).collect(),
        text,
        tree,
    })
}

pub fn recognize(atoms: &[Atom], bars: &[Bar]) -> Vec<Formula> {
    let mut found = Vec::new();
    for left in atoms.iter().filter(|a| a.raw == "\u{eae9}\u{eaea}") {
        for right in atoms.iter().filter(|a| a.raw == "\u{eaeb}\u{eaec}") {
            for &inner in bars {
                for &outer in bars {
                    if let Some(f) = attempt(atoms, bars, left, right, outer, inner) {
                        found.push(f)
                    }
                }
            }
        }
    }
    // Only disjoint, unambiguous partitions may be committed. Identical bars or
    // competing bracket pairs do not silently select a preferred expression.
    let keep: Vec<_> = (0..found.len())
        .filter(|&i| {
            !(0..found.len()).any(|j| {
                i != j
                    && (found[i]
                        .source_ids
                        .iter()
                        .any(|id| found[j].source_ids.contains(id))
                        || found[i]
                            .bracket_ids
                            .iter()
                            .any(|id| found[j].bracket_ids.contains(id)))
            })
        })
        .collect();
    keep.into_iter().map(|i| found[i].clone()).collect()
}

#[cfg(test)]
#[path = "nested_fraction_geometry_tests.rs"]
mod tests;

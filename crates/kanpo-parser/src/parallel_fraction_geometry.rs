#[derive(Clone, Debug)]
pub struct Atom {
    pub id: usize,
    pub owner: Option<usize>,
    pub eligible: bool,
    pub raw: String,
    pub text: String,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bar {
    pub y: f32,
    pub x0: f32,
    pub x1: f32,
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
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Formula {
    pub owner: usize,
    pub source_ids: Vec<usize>,
    pub original: String,
    pub tree: Expr,
}
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
fn numeric(s: &str) -> bool {
    let t = compact(s);
    !t.is_empty()
        && t.chars()
            .all(|c| c.is_ascii_digit() || ('０'..='９').contains(&c) || c == '.' || c == '．')
        && t.chars()
            .any(|c| ('1'..='9').contains(&c) || ('１'..='９').contains(&c))
}
fn leaf(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_alphanumeric() || c == '・' || c == '々' || c == 'ヶ')
}
fn cy(a: &Atom) -> f32 {
    (a.y0 + a.y1) / 2.0
}
fn source(a: &Atom) -> Expr {
    Expr::Source {
        word_ids: vec![a.id],
        text: a.text.clone(),
    }
}
fn fraction(b: Bar, n: &Atom, d: &Atom) -> Expr {
    Expr::Fraction {
        bar: b,
        numerator: Box::new(source(n)),
        denominator: Box::new(source(d)),
    }
}
fn centered(a: &Atom, b: Bar, em: f32) -> bool {
    a.x0 >= b.x0 - 0.05
        && a.x1 <= b.x1 + 0.05
        && ((a.x0 + a.x1 - b.x0 - b.x1) / 2.0).abs() <= em * 0.15
}
/// Two independent native bars and all six original atoms are mandatory. No
/// virtual/inferred bar, rewriting of unknown compound leaves, or number-only fallback.
pub fn parse(atoms: &[Atom], bars: &[Bar]) -> Option<Formula> {
    if atoms.len() != 6 {
        return None;
    }
    let owner = atoms.first()?.owner?;
    let mut ids = std::collections::HashSet::new();
    if atoms.iter().any(|a| {
        a.owner != Some(owner)
            || !a.eligible
            || a.raw != a.text
            || !ids.insert(a.id)
            || !a.x0.is_finite()
            || !a.y0.is_finite()
            || !a.x1.is_finite()
            || !a.y1.is_finite()
            || a.x1 <= a.x0
            || a.y1 <= a.y0
    }) {
        return None;
    }
    let em = atoms
        .iter()
        .map(|a| a.y1 - a.y0)
        .fold(f32::INFINITY, f32::min);
    if em < 3.0 || atoms.iter().any(|a| (a.y1 - a.y0) / em > 1.15) {
        return None;
    }
    let mut matches = Vec::new();
    for &left in bars {
        for &right in bars {
            if ![left.y, left.x0, left.x1, right.y, right.x0, right.x1]
                .iter()
                .all(|v| v.is_finite())
                || left.x1 <= left.x0
                || right.x1 <= right.x0
                || left.x1 >= right.x0
                || (left.y - right.y).abs() > em * 0.05
                || right.x0 - left.x1 > em * 1.5
            {
                continue;
            }
            let side = |b: Bar, upper: bool| -> Option<&Atom> {
                let xs: Vec<_> = atoms
                    .iter()
                    .filter(|a| {
                        centered(a, b, em)
                            && if upper {
                                a.y1 <= b.y + 0.05 && b.y - a.y1 <= em * 0.4
                            } else {
                                a.y0 >= b.y - 0.05 && a.y0 - b.y <= em * 0.5
                            }
                    })
                    .collect();
                if xs.len() == 1 {
                    Some(xs[0])
                } else {
                    None
                }
            };
            let (Some(ln), Some(ld), Some(rn), Some(rd)) = (
                side(left, true),
                side(left, false),
                side(right, true),
                side(right, false),
            ) else {
                continue;
            };
            if !leaf(&ln.text)
                || numeric(&ln.text)
                || !numeric(&ld.text)
                || !numeric(&rn.text)
                || !numeric(&rd.text)
            {
                continue;
            }
            if (cy(ln) - cy(rn)).abs() > em * 0.1 || (cy(ld) - cy(rd)).abs() > em * 0.1 {
                continue;
            }
            let prefix: Vec<_> = atoms
                .iter()
                .filter(|a| {
                    a.x1 <= left.x0 + 0.05
                        && left.x0 - a.x1 <= em * 0.25
                        && (cy(a) - left.y).abs() <= em * 0.2
                        && a.text.strip_suffix('×').is_some_and(leaf)
                })
                .collect();
            let between: Vec<_> = atoms
                .iter()
                .filter(|a| {
                    a.text == "×"
                        && a.x0 >= left.x1 - 0.05
                        && a.x1 <= right.x0 + 0.05
                        && (cy(a) - left.y).abs() <= em * 0.2
                })
                .collect();
            if prefix.len() != 1 || between.len() != 1 {
                continue;
            }
            let p = prefix[0];
            let m = between[0];
            let ordered = [p, ln, ld, m, rn, rd];
            let unique: std::collections::HashSet<_> = ordered.iter().map(|a| a.id).collect();
            if unique.len() != 6 {
                continue;
            }
            let tree = Expr::Sequence(vec![
                source(p),
                fraction(left, ln, ld),
                source(m),
                fraction(right, rn, rd),
            ]);
            matches.push(Formula {
                owner,
                source_ids: atoms.iter().map(|a| a.id).collect(),
                original: atoms.iter().map(|a| compact(&a.text)).collect(),
                tree,
            });
        }
    }
    if matches.len() == 1 {
        matches.pop()
    } else {
        None
    }
}

#[cfg(test)]
#[path = "parallel_fraction_geometry_tests.rs"]
mod tests;

//! Identify an open ruling strip by the source material it encloses. The same
//! endpoints must bracket the entire known masthead, and only its own source
//! glyphs and certified ruling glyphs may occupy that strip. A closed frame,
//! crossing rule, unrelated text or whole table prevents this classification.
//! All other rules retain their article-boundary role.
use super::*;
type Segment = ((f32, f32), (f32, f32));

pub(super) fn rulings(
    rules: &PageRules,
    segments: &[Segment],
    parent: &Heading,
    child: &Heading,
    page: &PageTrace,
    pi: usize,
    owner: &BTreeSet<Source>,
) -> BTreeSet<usize> {
    let a = project(parent.rect, parent.axis).unwrap();
    let b = project(child.rect, child.axis).unwrap();
    let projected = segments
        .iter()
        .map(|&((x, y), (s, t))| {
            let (x, y) = point(x, y, parent.axis).unwrap();
            let (s, t) = point(s, t, parent.axis).unwrap();
            ((x, y), (s, t))
        })
        .collect::<Vec<_>>();
    let mut groups: Vec<(f32, f32, Vec<(usize, f32)>)> = vec![];
    for (index, &((x, y), (s, t))) in projected.iter().enumerate() {
        if y != t || x.min(s) > a[0] || a[2] > x.max(s) {
            continue;
        }
        let lo = x.min(s);
        let hi = x.max(s);
        if let Some(group) = groups.iter_mut().find(|g| g.0 == lo && g.1 == hi) {
            group.2.push((index, y));
        } else {
            groups.push((lo, hi, vec![(index, y)]));
        }
    }
    let inverse = match parent.axis {
        NativeAxis::Right => NativeAxis::Right,
        NativeAxis::Down => NativeAxis::Up,
        NativeAxis::Left => NativeAxis::Left,
        NativeAxis::Up => NativeAxis::Down,
        _ => return BTreeSet::new(),
    };
    let mut own = BTreeSet::new();
    for (lo, hi, group) in groups {
        for &(_, lower) in group.iter().filter(|v| v.1 <= a[1]) {
            for &(_, upper) in group.iter().filter(|v| a[3] <= v.1 && v.1 < b[1]) {
                let strip = [lo, lower, hi, upper];
                let original = project(strip, inverse).unwrap();
                if !valid(original)
                    || rules
                        .enclosures
                        .iter()
                        .any(|&r| !valid(r) || touches(r, original))
                    || unaccounted(page, pi, owner, original, parent.axis)
                    || page
                        .tables
                        .iter()
                        .chain(
                            page.regions
                                .iter()
                                .flat_map(|r| &r.lines)
                                .filter_map(|l| l.table.as_ref()),
                        )
                        .any(|t| {
                            let r = [t.x0, t.y0, t.x1, t.y1];
                            !valid(r) || touches(original, r)
                        })
                    || projected.iter().any(|&((x, y), (s, t))| {
                        x == s && lo <= x && x <= hi && y.min(t) <= upper && lower <= y.max(t)
                    })
                {
                    continue;
                }
                for &(index, position) in &group {
                    if lower <= position && position <= upper {
                        own.insert(index);
                    }
                }
            }
        }
    }
    own
}

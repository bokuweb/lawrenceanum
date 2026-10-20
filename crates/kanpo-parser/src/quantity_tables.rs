//! 縦組みの名称・数量・備考表で、数量の列から欠けた縦罫の位置を推定する。

use crate::grid::{Grid, GridCell};
use crate::lines::PageRules;
use crate::vertical::Col;

/// 実在する横罫で囲まれた一字数量帯と、その上の名称帯・下の備考帯が揃う場合だけ、
/// 元の罫線を変更せず格子を返す。縦罫がある表は通常の格子検出へ任せる。
pub(crate) fn infer_quantity_grids(cols: &[Col], rules: &PageRules) -> Vec<Grid> {
    let center = |c: &Col| ((c.x + c.x1) / 2.0, (c.y + c.y1) / 2.0);
    let quantity = |t: &str| {
        t.chars().count() == 1 && t.chars().all(|c| c.is_ascii_digit() || ('０'..='９').contains(&c) || "〇零一二三四五六七八九十".contains(c))
    };
    let mut lines = rules.horizontal.clone();
    lines.sort_by(|a, b| a.y.total_cmp(&b.y));
    let mut grids = Vec::new();
    for upper in &lines {
        if upper.x1 - upper.x0 < 100.0 {
            continue;
        }
        let same_span = |h: &&crate::lines::HRule| (h.x0 - upper.x0).abs() <= 2.0 && (h.x1 - upper.x1).abs() <= 2.0;
        let Some(lower) = lines.iter().filter(same_span).find(|h| h.y > upper.y + 6.0) else { continue };
        if lower.y - upper.y > 64.0 {
            continue;
        }
        let within_x = |c: &&Col| {
            let (x, _) = center(c);
            upper.x0 < x && x < upper.x1
        };
        let quantities: Vec<&Col> = cols.iter().filter(within_x).filter(|c| {
            let (_, y) = center(c);
            upper.y < y && y < lower.y
        }).collect();
        if quantities.len() < 8 || quantities.iter().any(|c| !quantity(&c.text)) {
            continue;
        }
        let mut ems: Vec<f32> = quantities.iter().map(|c| (c.x1 - c.x).min(c.y1 - c.y)).collect();
        ems.sort_by(f32::total_cmp);
        let em = ems[ems.len() / 2].max(1.0);
        let (y0, y1) = quantities.iter().map(|c| center(c).1).fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), y| (lo.min(y), hi.max(y)));
        if y1 - y0 > em * 0.5 || lower.y - upper.y > em * 8.0 {
            continue;
        }
        let Some(top) = lines.iter().filter(same_span).rev().find(|h| h.y < upper.y - 6.0) else { continue };
        let Some(bottom) = lines.iter().filter(same_span).find(|h| h.y > lower.y + 6.0) else { continue };
        if upper.y - top.y < em * 4.0 || upper.y - top.y > 300.0 || bottom.y - lower.y > 160.0 {
            continue;
        }
        // 既存の罫線表と重ならない。見出しの飾り線などを表の縦罫と補完しない。
        if rules.vertical.iter().any(|v| {
            upper.x0 - 1.0 < v.x && v.x < upper.x1 + 1.0 && v.y1.min(bottom.y) - v.y0.max(top.y) > em
        }) {
            continue;
        }
        let mut xs: Vec<f32> = quantities.iter().map(|c| center(c).0).collect();
        xs.sort_by(f32::total_cmp);
        if xs.windows(2).any(|p| p[1] - p[0] < em * 1.1) {
            continue;
        }
        let names: Vec<&Col> = cols.iter().filter(within_x).filter(|c| top.y - 1.0 <= c.y && c.y1 <= upper.y + 1.0).collect();
        let mut starts = Vec::new();
        let aligned = xs.iter().all(|&x| {
            let name: Vec<&&Col> = names.iter().filter(|c| (center(c).0 - x).abs() < em * 0.4).collect();
            let chars: usize = name.iter().map(|c| c.text.chars().count()).sum();
            if chars < 2 || name.iter().all(|c| quantity(&c.text)) {
                return false;
            }
            starts.push(name.iter().map(|c| c.y).fold(f32::INFINITY, f32::min));
            true
        });
        let first = starts.iter().copied().fold(f32::INFINITY, f32::min);
        if !aligned || starts.iter().filter(|&&y| y - first < em).count() * 10 < xs.len() * 9 {
            continue;
        }
        // 備考だけが左へ2〜3列折り返す項目は、数量の列間が広い。数量の通常の列送りの半分を
        // 左隣の項目から空けて境にし、その間の折返しを右の項目へ含める（単純な中点では割れる）。
        let mut pitches: Vec<f32> = xs.windows(2).map(|p| p[1] - p[0]).collect();
        pitches.sort_by(f32::total_cmp);
        let pitch = pitches[pitches.len() / 2];
        if pitch > em * 3.0 || pitches.iter().filter(|&&p| p <= pitch * 1.15).count() * 10 < pitches.len() * 8 {
            continue;
        }
        let edges: Vec<f32> = std::iter::once(upper.x0)
            .chain(xs.iter().take(xs.len() - 1).map(|x| x + pitch / 2.0))
            .chain(std::iter::once(upper.x1))
            .collect();
        let cells = (0..xs.len()).flat_map(|x| (0..3).map(move |y| GridCell { x0: x, x1: x + 1, y0: y, y1: y + 1 })).collect();
        let grid = Grid { xs: edges, ys: vec![top.y, upper.y, lower.y, bottom.y], cells };
        if !grids.contains(&grid) {
            grids.push(grid);
        }
    }
    grids
}

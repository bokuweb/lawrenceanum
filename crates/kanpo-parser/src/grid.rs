//! 罫線から表の枠と格子（結合セルを含む）を復元する。
//!
//! 官報の表は縦横の罫線で描かれる。接している罫線をまとめて「枠」とし、枠の中の罫線の位置で
//! 格子を作る。隣り合うマスの境目に罫線が無ければ 1 つのセルに結合する（〔略〕のように
//! 上下の欄にまたがるセル）。新旧対照表の外枠（改正後/改正前の帯）も同じ仕組みで取れる。

use crate::lines::{HRule, PageRules, VRule};

/// 罫線どうしが接しているとみなす距離(pt)。
const TOUCH: f32 = 2.5;
/// これより近い平行な罫線は 1 本とみなす（二重線の枠・飾りの細線）(pt)。
const MERGE: f32 = 6.0;

/// 罫線の格子。`xs`・`ys` は罫線の位置（昇順）、`cells` は結合後のセル。
#[derive(Debug, Clone, PartialEq)]
pub struct Grid {
    pub xs: Vec<f32>,
    pub ys: Vec<f32>,
    pub cells: Vec<GridCell>,
}

/// 格子のセル。`xs[x0]..xs[x1]` × `ys[y0]..ys[y1]` の矩形（結合セルは複数のマスにまたがる）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridCell {
    pub x0: usize,
    pub x1: usize,
    pub y0: usize,
    pub y1: usize,
}

impl Grid {
    pub fn left(&self) -> f32 {
        self.xs[0]
    }
    pub fn right(&self) -> f32 {
        *self.xs.last().unwrap()
    }
    pub fn top(&self) -> f32 {
        self.ys[0]
    }
    pub fn bottom(&self) -> f32 {
        *self.ys.last().unwrap()
    }
    /// 点が枠の内側にあるか（`pad` だけ内側に縮めて判定）。
    pub fn contains(&self, x: f32, y: f32, pad: f32) -> bool {
        self.left() + pad <= x && x <= self.right() - pad && self.top() + pad <= y && y <= self.bottom() - pad
    }
    /// 他の格子の枠を（ほぼ）含むか。
    pub fn encloses(&self, other: &Grid) -> bool {
        self.left() - 1.0 <= other.left()
            && other.right() <= self.right() + 1.0
            && self.top() - 1.0 <= other.top()
            && other.bottom() <= self.bottom() + 1.0
            && self != other
    }
    /// 点が入るセル。
    pub fn cell_at(&self, x: f32, y: f32) -> Option<usize> {
        self.cells.iter().position(|c| {
            self.xs[c.x0] <= x && x < self.xs[c.x1] && self.ys[c.y0] <= y && y < self.ys[c.y1]
        })
    }
}

/// ページの罫線から枠ごとの格子を作る。`width`/`height` でクリップする（ページ外へ伸びて
/// 切られた罫線の座標が負になることがある）。
pub fn find_grids(rules: &PageRules, width: f32, height: f32) -> Vec<Grid> {
    let vs: Vec<VRule> = rules
        .vertical
        .iter()
        .filter(|v| v.x >= 0.0 && v.x <= width)
        .map(|v| VRule { x: v.x, y0: v.y0.max(0.0), y1: v.y1.min(height) })
        .filter(|v| v.y1 - v.y0 > 4.0)
        .collect();
    let hs: Vec<HRule> = rules
        .horizontal
        .iter()
        .filter(|h| h.y >= 0.0 && h.y <= height)
        .map(|h| HRule { y: h.y, x0: h.x0.max(0.0), x1: h.x1.min(width) })
        .filter(|h| h.x1 - h.x0 > 4.0)
        .collect();

    // 接している罫線を union-find でまとめる（縦線と横線の交差・端点の接触）。
    let n = vs.len() + hs.len();
    let mut parent: Vec<usize> = (0..n).collect();
    let touches = |v: &VRule, h: &HRule| {
        v.x >= h.x0 - TOUCH && v.x <= h.x1 + TOUCH && h.y >= v.y0 - TOUCH && h.y <= v.y1 + TOUCH
    };
    for (i, v) in vs.iter().enumerate() {
        for (j, h) in hs.iter().enumerate() {
            if touches(v, h) {
                let (a, b) = (find(&mut parent, i), find(&mut parent, vs.len() + j));
                parent[a] = b;
            }
        }
    }
    let mut groups: std::collections::BTreeMap<usize, (Vec<VRule>, Vec<HRule>)> = Default::default();
    for (i, v) in vs.iter().enumerate() {
        groups.entry(find(&mut parent, i)).or_default().0.push(*v);
    }
    for (j, h) in hs.iter().enumerate() {
        groups.entry(find(&mut parent, vs.len() + j)).or_default().1.push(*h);
    }
    groups.into_values().filter_map(|(v, h)| build_grid(&v, &h)).collect()
}

/// union-find の根（経路半減）。
fn find(p: &mut [usize], mut i: usize) -> usize {
    while p[i] != i {
        p[i] = p[p[i]];
        i = p[i];
    }
    i
}

/// 矩形 `[x0, x1] × [y0, y1]`（新旧対照表の帯など）の内側の罫線だけで表の枠を探す。
/// 矩形の縁に沿う罫線（帯の枠そのもの）は除く。表の罫線が帯の枠に接していて、
/// ページ全体では帯の枠と 1 つにつながってしまう場合に使う。
pub fn find_grids_inside(rules: &PageRules, x0: f32, x1: f32, y0: f32, y1: f32) -> Vec<Grid> {
    let near = |a: f32, b: f32| (a - b).abs() < MERGE;
    let inside = PageRules {
        vertical: rules
            .vertical
            .iter()
            .filter(|v| v.x > x0 - 1.0 && v.x < x1 + 1.0 && !near(v.x, x0) && !near(v.x, x1))
            .map(|v| VRule { x: v.x, y0: v.y0.max(y0), y1: v.y1.min(y1) })
            .filter(|v| v.y1 - v.y0 > 4.0)
            .collect(),
        horizontal: rules
            .horizontal
            .iter()
            .filter(|h| h.y > y0 - 1.0 && h.y < y1 + 1.0 && !near(h.y, y0) && !near(h.y, y1))
            .map(|h| HRule { y: h.y, x0: h.x0.max(x0), x1: h.x1.min(x1) })
            .filter(|h| h.x1 - h.x0 > 4.0)
            .collect(),
    };
    find_grids(&inside, f32::MAX, f32::MAX)
}

/// 近い値をまとめて代表値（平均）の昇順リストにする。連鎖的にまとめる。
fn cluster(mut vals: Vec<f32>, tol: f32) -> Vec<f32> {
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut out: Vec<Vec<f32>> = Vec::new();
    for v in vals {
        match out.last_mut() {
            Some(g) if v - *g.last().unwrap() <= tol => g.push(v),
            _ => out.push(vec![v]),
        }
    }
    out.into_iter().map(|g| g.iter().sum::<f32>() / g.len() as f32).collect()
}

/// 1 つの枠の罫線から格子を作る。縦横それぞれ 2 本以上の罫線が無ければ `None`。
fn build_grid(vs: &[VRule], hs: &[HRule]) -> Option<Grid> {
    // 枠の端に縦罫線（横罫線）が無いとき（ページの外まで伸びて切られた表など）は、
    // 横罫線（縦罫線）の端を枠の境界とする。
    let edge_x = [hs.iter().map(|h| h.x0).reduce(f32::min), hs.iter().map(|h| h.x1).reduce(f32::max)];
    let edge_y = [vs.iter().map(|v| v.y0).reduce(f32::min), vs.iter().map(|v| v.y1).reduce(f32::max)];
    let xs = cluster(vs.iter().map(|v| v.x).chain(edge_x.into_iter().flatten()).collect(), MERGE);
    let ys = cluster(hs.iter().map(|h| h.y).chain(edge_y.into_iter().flatten()).collect(), MERGE);
    if xs.len() < 2 || ys.len() < 2 {
        return None;
    }
    let (nx, ny) = (xs.len() - 1, ys.len() - 1);
    // マスの境目に罫線があるか。縦の境目 xs[i] の y 区間 j、横の境目 ys[j] の x 区間 i。
    let tol = MERGE;
    let v_wall = |i: usize, j: usize| {
        let (y0, y1) = (ys[j], ys[j + 1]);
        let mid = (y0 + y1) / 2.0;
        vs.iter().any(|v| (v.x - xs[i]).abs() <= tol && v.y0 <= mid && v.y1 >= mid)
    };
    let h_wall = |i: usize, j: usize| {
        let (x0, x1) = (xs[i], xs[i + 1]);
        let mid = (x0 + x1) / 2.0;
        hs.iter().any(|h| (h.y - ys[j]).abs() <= tol && h.x0 <= mid && h.x1 >= mid)
    };
    // マス (i, j) を union-find で結合。
    let id = |i: usize, j: usize| j * nx + i;
    let mut parent: Vec<usize> = (0..nx * ny).collect();
    for j in 0..ny {
        for i in 0..nx {
            if i + 1 < nx && !v_wall(i + 1, j) {
                let (a, b) = (find(&mut parent, id(i, j)), find(&mut parent, id(i + 1, j)));
                parent[a] = b;
            }
            if j + 1 < ny && !h_wall(i, j + 1) {
                let (a, b) = (find(&mut parent, id(i, j)), find(&mut parent, id(i, j + 1)));
                parent[a] = b;
            }
        }
    }
    // 結合したマスの外接矩形をセルにする。
    let mut rects: std::collections::BTreeMap<usize, GridCell> = Default::default();
    for j in 0..ny {
        for i in 0..nx {
            let r = find(&mut parent, id(i, j));
            let c = rects.entry(r).or_insert(GridCell { x0: i, x1: i + 1, y0: j, y1: j + 1 });
            c.x0 = c.x0.min(i);
            c.x1 = c.x1.max(i + 1);
            c.y0 = c.y0.min(j);
            c.y1 = c.y1.max(j + 1);
        }
    }
    let mut cells: Vec<GridCell> = rects.into_values().collect();
    // 右上から（縦組みの読み順: 右→左、上→下）。
    cells.sort_by(|a, b| b.x1.cmp(&a.x1).then(a.y0.cmp(&b.y0)));
    Some(Grid { xs, ys, cells })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(vs: &[(f32, f32, f32)], hs: &[(f32, f32, f32)]) -> PageRules {
        PageRules {
            vertical: vs.iter().map(|&(x, y0, y1)| VRule { x, y0, y1 }).collect(),
            horizontal: hs.iter().map(|&(y, x0, x1)| HRule { y, x0, x1 }).collect(),
        }
    }

    #[test]
    fn merges_cells_where_a_rule_is_missing() {
        // 実ページ（法務省令の別表第二）と同じ形: 4 列 × 2 段で、中段の横罫線は
        // 1 列目と 3 列目にしか無い（2・4 列目は上下に結合した〔略〕のセル）。
        let r = rules(
            &[(238.5, 83.2, 227.2), (255.8, 83.2, 227.2), (345.9, 83.2, 227.2), (363.2, 83.2, 227.2), (380.5, 83.2, 227.2)],
            &[(83.2, 238.5, 380.5), (131.2, 255.8, 345.9), (131.2, 363.2, 380.5), (227.2, 238.5, 380.5)],
        );
        let grids = find_grids(&r, 595.0, 842.0);
        assert_eq!(grids.len(), 1);
        let g = &grids[0];
        assert_eq!(g.xs.len(), 5);
        assert_eq!(g.ys.len(), 3);
        // 6 セル: 見出し列 2 + 〔略〕1 + 短期滞在の列 2 + 〔略〕1
        assert_eq!(g.cells.len(), 6);
        let merged: Vec<_> = g.cells.iter().filter(|c| c.y1 - c.y0 == 2).collect();
        assert_eq!(merged.len(), 2, "{:?}", g.cells);
        // 右上（見出し列の上段）から。
        assert_eq!(g.cells[0], GridCell { x0: 3, x1: 4, y0: 0, y1: 1 });
    }

    #[test]
    fn separates_frames_that_do_not_touch() {
        let r = rules(
            &[(10.0, 10.0, 50.0), (50.0, 10.0, 50.0), (100.0, 10.0, 50.0), (140.0, 10.0, 50.0)],
            &[(10.0, 10.0, 50.0), (50.0, 10.0, 50.0), (10.0, 100.0, 140.0), (50.0, 100.0, 140.0)],
        );
        assert_eq!(find_grids(&r, 595.0, 842.0).len(), 2);
    }
}

//! 別表（罫線で区切られた表）を 2D 論理表へ復元する。
//!
//! 縦書き官報の別表は列優先（右の列から読む）に並ぶため、罫線で作った
//! 「y 帯 × x 列」のセル行列を**転置**して論理表にする（x 列＝論理行、y 帯＝論理列）。
//! 1 本の帯（改正後 or 改正前）に複数の別表が同居する場合は、横罫線セグメントの
//! x 範囲ごとに分離する。

use crate::lines::PageRules;

/// 罫線グリッドから復元した表。`rows[r][c]` がセル文字列。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GridTable {
    /// 表の左端 x（同帯内で右→左に並べるための位置）。
    pub left_x: f32,
    pub rows: Vec<Vec<String>>,
}

/// セル割当用の語。`x` は左端、`yc` は中心 y。
pub(crate) struct PlacedWord {
    pub x: f32,
    pub yc: f32,
    pub text: String,
}

/// 値を近接（`tol`）でクラスタリングして代表値（平均）の昇順リストを返す。
fn cluster(mut vals: Vec<f32>, tol: f32) -> Vec<f32> {
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut out: Vec<Vec<f32>> = Vec::new();
    for v in vals {
        match out.last_mut() {
            Some(g) if v - *g.last().unwrap() <= tol => g.push(v),
            _ => out.push(vec![v]),
        }
    }
    out.into_iter().map(|g| g.iter().sum::<f32>() / g.len() as f32).collect()
}

/// 指定 y 帯（改正後 or 改正前）内の罫線・文字から別表群を復元する。右→左順で返す。
/// 別表が無ければ空。
pub(crate) fn reconstruct_tables(
    rules: &PageRules,
    words: &[PlacedWord],
    band_y0: f32,
    band_y1: f32,
    max_x: f32,
) -> Vec<GridTable> {
    // 帯内の横罫線。条件:
    // - 下端罫線が少し帯外に出るため +12pt 許容
    // - 改正後/改正前ラベル列や本文より右(=max_x 以降)は別表でないので除外
    // - 表の外枠(全幅に近い罫線)は別表のセル罫線でないので除外(長さ300pt超)
    let hs: Vec<_> = rules
        .horizontal
        .iter()
        .filter(|h| {
            band_y0 - 2.0 <= h.y
                && h.y <= band_y1 + 12.0
                && h.x1 <= max_x
                && (h.x1 - h.x0) <= 300.0
        })
        .collect();
    if hs.is_empty() {
        return Vec::new();
    }
    // 横罫線セグメントの x 範囲（始端 x0）でクラスタリングして別表領域に分ける。
    let starts = cluster(hs.iter().map(|h| h.x0).collect(), 12.0);
    let mut tables: Vec<GridTable> = Vec::new();
    for &x0c in &starts {
        // この領域の横罫線（始端が x0c 近傍）。
        let region_hs: Vec<_> = hs.iter().filter(|h| (h.x0 - x0c).abs() <= 12.0).collect();
        if region_hs.len() < 2 {
            continue;
        }
        let x_lo = region_hs.iter().map(|h| h.x0).fold(f32::INFINITY, f32::min);
        let x_hi = region_hs.iter().map(|h| h.x1).fold(f32::NEG_INFINITY, f32::max);
        // y 帯境界＝この領域の横罫線 y。
        let ys = cluster(region_hs.iter().map(|h| h.y).collect(), 4.0);
        // x 列境界＝領域内の縦罫線 x。
        let xs = cluster(
            rules
                .vertical
                .iter()
                .filter(|v| v.y0 <= band_y1 + 12.0 && v.y1 >= band_y0 - 2.0 && x_lo - 2.0 <= v.x && v.x <= x_hi + 2.0)
                .map(|v| v.x)
                .collect(),
            4.0,
        );
        if ys.len() < 2 || xs.len() < 2 {
            continue;
        }
        // 論理行＝x 列を右→左、論理列＝y 帯を上→下。
        let mut rows: Vec<Vec<String>> = Vec::new();
        for xi in (0..xs.len() - 1).rev() {
            let (cx0, cx1) = (xs[xi], xs[xi + 1]);
            let mut row: Vec<String> = Vec::new();
            for yi in 0..ys.len() - 1 {
                let (cy0, cy1) = (ys[yi], ys[yi + 1]);
                row.push(cell_text(words, cx0, cx1, cy0, cy1));
            }
            rows.push(row);
        }
        let rows = trim_empty_edges(rows);
        if rows.iter().any(|r| r.iter().any(|c| !c.is_empty())) {
            tables.push(GridTable { left_x: x_lo, rows });
        }
    }
    // 右→左（left_x 降順）。
    tables.sort_by(|a, b| b.left_x.partial_cmp(&a.left_x).unwrap_or(std::cmp::Ordering::Equal));
    tables
}

/// 全セルが空の行・列を表の縁から取り除く（罫線の外枠と本文の隙間で出来る空帯を除去）。
fn trim_empty_edges(mut rows: Vec<Vec<String>>) -> Vec<Vec<String>> {
    // 空行を端から削る。
    while rows.first().is_some_and(|r| r.iter().all(|c| c.is_empty())) {
        rows.remove(0);
    }
    while rows.last().is_some_and(|r| r.iter().all(|c| c.is_empty())) {
        rows.pop();
    }
    if rows.is_empty() {
        return rows;
    }
    let ncol = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let col_empty = |c: usize| rows.iter().all(|r| r.get(c).map(|s| s.is_empty()).unwrap_or(true));
    let mut lo = 0;
    while lo < ncol && col_empty(lo) {
        lo += 1;
    }
    let mut hi = ncol;
    while hi > lo && col_empty(hi - 1) {
        hi -= 1;
    }
    rows.into_iter()
        .map(|r| r.into_iter().enumerate().filter(|(i, _)| *i >= lo && *i < hi).map(|(_, c)| c).collect())
        .collect()
}

/// セル矩形 [x0,x1)×[y0,y1) に中心が入る語を縦書き順（右→左, 上→下）で連結。
fn cell_text(words: &[PlacedWord], x0: f32, x1: f32, y0: f32, y1: f32) -> String {
    let mut cs: Vec<&PlacedWord> = words
        .iter()
        .filter(|w| x0 <= w.x && w.x < x1 && y0 <= w.yc && w.yc < y1)
        .collect();
    cs.sort_by(|a, b| {
        b.x.partial_cmp(&a.x)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.yc.partial_cmp(&b.yc).unwrap_or(std::cmp::Ordering::Equal))
    });
    cs.iter().map(|w| w.text.as_str()).collect::<String>().trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lines::{HRule, PageRules, VRule};

    #[test]
    fn reconstructs_transposed_table_from_grid() {
        // 2 列(x10-30, x30-50) × 2 帯(y10-30, y30-50) のグリッド。縦書きなので
        // x 列(右→左)が論理行、y 帯(上→下)が論理列。
        let rules = PageRules {
            vertical: vec![
                VRule { x: 10.0, y0: 10.0, y1: 50.0 },
                VRule { x: 30.0, y0: 10.0, y1: 50.0 },
                VRule { x: 50.0, y0: 10.0, y1: 50.0 },
            ],
            horizontal: vec![
                HRule { y: 10.0, x0: 10.0, x1: 50.0 },
                HRule { y: 30.0, x0: 10.0, x1: 50.0 },
                HRule { y: 50.0, x0: 10.0, x1: 50.0 },
            ],
        };
        let w = |x: f32, yc: f32, t: &str| PlacedWord { x, yc, text: t.into() };
        // 右列(x30-50): 上=見出A, 下=見出B / 左列(x10-30): 上=値A, 下=値B
        let words = vec![w(40.0, 20.0, "見出A"), w(40.0, 40.0, "見出B"), w(20.0, 20.0, "値A"), w(20.0, 40.0, "値B")];
        let tables = reconstruct_tables(&rules, &words, 10.0, 50.0, 1000.0);
        assert_eq!(tables.len(), 1);
        assert_eq!(
            tables[0].rows,
            vec![
                vec!["見出A".to_string(), "見出B".to_string()], // 右列=1行目
                vec!["値A".to_string(), "値B".to_string()],     // 左列=2行目
            ]
        );
    }

    #[test]
    fn separates_two_tables_by_horizontal_segments() {
        // 横罫線が x0-20 と x40-60 の 2 セグメントに割れている → 別表 2 つに分離。
        let rules = PageRules {
            vertical: vec![
                VRule { x: 0.0, y0: 0.0, y1: 40.0 },
                VRule { x: 20.0, y0: 0.0, y1: 40.0 },
                VRule { x: 40.0, y0: 0.0, y1: 40.0 },
                VRule { x: 60.0, y0: 0.0, y1: 40.0 },
            ],
            horizontal: vec![
                HRule { y: 0.0, x0: 0.0, x1: 20.0 },
                HRule { y: 40.0, x0: 0.0, x1: 20.0 },
                HRule { y: 0.0, x0: 40.0, x1: 60.0 },
                HRule { y: 40.0, x0: 40.0, x1: 60.0 },
            ],
        };
        let w = |x: f32, t: &str| PlacedWord { x, yc: 20.0, text: t.into() };
        let words = vec![w(10.0, "左表"), w(50.0, "右表")];
        let tables = reconstruct_tables(&rules, &words, 0.0, 40.0, 1000.0);
        assert_eq!(tables.len(), 2);
        // 右→左順: 右表(x40-60)が先。
        assert_eq!(tables[0].rows[0][0], "右表");
        assert_eq!(tables[1].rows[0][0], "左表");
    }
}

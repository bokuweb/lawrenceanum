//! 罫線（縦線・横線）の抽出。描かれたパスのうち、1 本の直線だけのものを縦線・横線に分ける。
//!
//! kanpo-parser が `pdftocairo -svg` の出力から拾っていたもの（`M x y L x y` だけのパス）と
//! 同じものを、コンテンツストリームから直接拾う。同じ向きに一直線に続く線分（`m l l`）は
//! cairo と同じく 1 本にまとめる。閉じていない折れ線（表の外枠の上辺と列の境を 1 筆で引いたもの）は辺ごとに拾う。
//! 矩形（`re`）や閉じた折れ線は含めない。クリップで切り抜かれた線は
//! 見える範囲だけにする（隣の記事の表の横線が、クリップの外の別の記事の欄まで伸びて見えないように）。

use serde::{Deserialize, Serialize};

use crate::page::Page;

/// 縦罫線。`x` 位置に `y0..y1` の範囲で伸びる。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VRule {
    pub x: f32,
    pub y0: f32,
    pub y1: f32,
}

/// 横罫線。`y` 位置に `x0..x1` の範囲で伸びる。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HRule {
    pub y: f32,
    pub x0: f32,
    pub x1: f32,
}

/// 1 ページ分の罫線（ページ座標）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PageRules {
    pub vertical: Vec<VRule>,
    pub horizontal: Vec<HRule>,
    /// Native closed, stroked, unfilled rectangular article enclosures.
    /// Kept apart from table rules; missing in older serialized inputs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enclosures: Vec<[f32; 4]>,
}

/// 線の向きのずれの許容（pt）。
const SKEW: f64 = 2.0;
/// 罫線とみなす最短の長さ（pt）。
const MIN_LEN: f64 = 15.0;
/// クリップの縁に重なる線を切り落とさないための許容（pt）。
const CLIP_TOL: f64 = 1.0;

pub fn page_rules(page: &Page) -> PageRules {
    let mut rules = PageRules::default();
    for path in &page.paths {
        let [sp] = path.subpaths.as_slice() else {
            continue;
        };
        if path.stroke && !path.fill && sp.closed && !sp.curved {
            if let Some(rect) = enclosure_rect(&sp.points, path.clip) {
                rules.enclosures.push(rect);
            }
        }
        if sp.curved || sp.rect {
            continue;
        }
        // 一直線のパスは 1 本に。閉じていない折れ線（表の外枠の上辺と列の境を 1 筆で引いた「┐」）は辺ごとに。
        let segments: Vec<[[f64; 2]; 2]> = match straight(&sp.points) {
            Some(seg) => vec![seg],
            None if !sp.closed => sp.points.windows(2).map(|w| [w[0], w[1]]).collect(),
            None => continue,
        };
        let [cx0, cy0, cx1, cy1] = path.clip.unwrap_or([
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::INFINITY,
        ]);
        for [[x1, y1], [x2, y2]] in segments {
            let (dx, dy) = ((x2 - x1).abs(), (y2 - y1).abs());
            if dx < SKEW && dy > MIN_LEN {
                let x = (x1 + x2) / 2.0;
                let (y0, y1) = (y1.min(y2).max(cy0), y1.max(y2).min(cy1));
                if x < cx0 - CLIP_TOL || x > cx1 + CLIP_TOL || y1 - y0 <= MIN_LEN {
                    continue;
                }
                rules.vertical.push(VRule {
                    x: x as f32,
                    y0: y0 as f32,
                    y1: y1 as f32,
                });
            } else if dy < SKEW && dx > MIN_LEN {
                let y = (y1 + y2) / 2.0;
                let (x0, x1) = (x1.min(x2).max(cx0), x1.max(x2).min(cx1));
                if y < cy0 - CLIP_TOL || y > cy1 + CLIP_TOL || x1 - x0 <= MIN_LEN {
                    continue;
                }
                rules.horizontal.push(HRule {
                    y: y as f32,
                    x0: x0 as f32,
                    x1: x1 as f32,
                });
            }
        }
    }
    rules
}

// Require all four corners and axis-aligned edges. No bounding-box inference
// from arbitrary closed artwork, clipped rectangles or glyph-size boxes.
fn enclosure_rect(points: &[[f64; 2]], clip: Option<[f64; 4]>) -> Option<[f32; 4]> {
    let mut ps = points.to_vec();
    if ps.len() == 5 && ps.first() == ps.last() {
        ps.pop();
    }
    if ps.len() != 4 {
        return None;
    }
    let x0 = ps.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
    let y0 = ps.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
    let x1 = ps.iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max);
    let y1 = ps.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max);
    if x1 - x0 < 50.0 || y1 - y0 < 50.0 {
        return None;
    }
    let corners = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
    if !corners.iter().all(|c| {
        ps.iter()
            .any(|p| (p[0] - c[0]).abs() < 0.01 && (p[1] - c[1]).abs() < 0.01)
    }) {
        return None;
    }
    if (0..4).any(|i| {
        let a = ps[i];
        let b = ps[(i + 1) % 4];
        (a[0] - b[0]).abs() > 0.01 && (a[1] - b[1]).abs() > 0.01
    }) {
        return None;
    }
    if clip.is_some_and(|[a, b, c, d]| x0 < a - 1.0 || y0 < b - 1.0 || x1 > c + 1.0 || y1 > d + 1.0)
    {
        return None;
    }
    Some([x0 as f32, y0 as f32, x1 as f32, y1 as f32])
}

/// 同じ向きに一直線に続く頂点の並び（`m l l ...`）なら、その両端。
fn straight(points: &[[f64; 2]]) -> Option<[[f64; 2]; 2]> {
    let (&first, &last) = (points.first()?, points.last()?);
    if points.len() < 2 {
        return None;
    }
    let (dx, dy) = (last[0] - first[0], last[1] - first[1]);
    let len2 = dx * dx + dy * dy;
    if len2 < 1e-12 {
        return None;
    }
    let mut prev = 0.0;
    for p in &points[1..] {
        let (px, py) = (p[0] - first[0], p[1] - first[1]);
        // 直線からのずれ（pt）と、向きに沿った位置（0〜1、戻らない）。
        let off = (px * dy - py * dx).abs() / len2.sqrt();
        let t = (px * dx + py * dy) / len2;
        if off > 0.01 || t < prev - 1e-9 {
            return None;
        }
        prev = t;
    }
    Some([first, last])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collinear_points_make_one_rule() {
        assert_eq!(
            straight(&[[0.0, 0.0], [0.0, 216.0], [0.0, 328.0]]),
            Some([[0.0, 0.0], [0.0, 328.0]])
        );
        // 折り返す線・折れ線は 1 本の罫線にしない。
        assert_eq!(straight(&[[0.0, 0.0], [0.0, 216.0], [0.0, 100.0]]), None);
        assert_eq!(straight(&[[0.0, 0.0], [0.0, 216.0], [5.0, 300.0]]), None);
    }
}

#[cfg(test)]
mod enclosure_tests {
    use super::*;
    #[test]
    fn four_native_corners_are_required() {
        assert_eq!(
            enclosure_rect(&[[0., 0.], [100., 0.], [100., 100.], [0., 100.]], None),
            Some([0., 0., 100., 100.])
        );
        assert!(enclosure_rect(&[[0., 0.], [100., 2.], [100., 100.], [0., 100.]], None).is_none());
    }
    #[test]
    fn duplicate_closing_point_is_allowed() {
        assert!(enclosure_rect(
            &[[0., 0.], [100., 0.], [100., 100.], [0., 100.], [0., 0.]],
            None
        )
        .is_some());
    }
    #[test]
    fn clipped_rectangle_is_not_a_closed_enclosure() {
        assert!(enclosure_rect(
            &[[0., 0.], [100., 0.], [100., 100.], [0., 100.]],
            Some([0., 0., 90., 100.])
        )
        .is_none());
    }
    #[test]
    fn tiny_artwork_is_excluded() {
        assert!(enclosure_rect(&[[0., 0.], [10., 0.], [10., 10.], [0., 10.]], None).is_none());
    }
    #[test]
    fn filled_or_open_paths_never_become_article_frames() {
        use crate::page::{PathItem, SubPath};
        let mut p = Page {
            width: 595.,
            height: 842.,
            rotate: 0,
            glyphs: vec![],
            fonts: vec![],
            paths: vec![PathItem {
                subpaths: vec![SubPath {
                    points: vec![[0., 0.], [100., 0.], [100., 100.], [0., 100.]],
                    closed: true,
                    curved: false,
                    rect: true,
                }],
                stroke: true,
                fill: true,
                line_width: 1.,
                clip: None,
            }],
        };
        assert!(page_rules(&p).enclosures.is_empty());
        p.paths[0].fill = false;
        assert_eq!(page_rules(&p).enclosures.len(), 1);
        p.paths[0].subpaths[0].closed = false;
        assert!(page_rules(&p).enclosures.is_empty());
    }
    #[test]
    fn previous_rules_json_defaults_to_no_frames() {
        let r: PageRules = serde_json::from_str(r#"{"vertical":[],"horizontal":[]}"#).unwrap();
        assert!(r.enclosures.is_empty());
        assert_eq!(
            serde_json::to_string(&r).unwrap(),
            r#"{"vertical":[],"horizontal":[]}"#
        );
    }
}

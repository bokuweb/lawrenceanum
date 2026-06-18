//! 官報 PDF の罫線（縦線・横線）をベクターとして抽出する。
//!
//! `pdftotext` はテキストしか出さないため、新旧対照表中の別表（罫線で区切られた表）の
//! セル構造は取れない。一方 `pdftocairo -svg` は罫線をベクターパスとして出す。本モジュールは
//! その SVG を `transform` を解決しながら走査し、2 点の直線パス（`M x y L x y`）を縦線/横線に
//! 分類して絶対座標で返す。座標系は `pdftotext -bbox` と同じ（左上原点・y 下向き）で揃う。

use anyhow::Result;
use quick_xml::events::Event;
use quick_xml::Reader;

use crate::pdftotext::run_pdftocairo_svg;

/// 縦罫線。`x` 位置に `y0..y1` の範囲で伸びる。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct VRule {
    pub x: f32,
    pub y0: f32,
    pub y1: f32,
}

/// 横罫線。`y` 位置に `x0..x1` の範囲で伸びる。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct HRule {
    pub y: f32,
    pub x0: f32,
    pub x1: f32,
}

/// 1 ページ分の罫線。
#[derive(Debug, Clone, Default)]
pub(crate) struct PageRules {
    pub vertical: Vec<VRule>,
    pub horizontal: Vec<HRule>,
}

/// PDF（1 ページ）から罫線を抽出する。poppler `pdftocairo` を使う。
pub(crate) fn extract_rules(pdf: &[u8]) -> Result<PageRules> {
    let svg = run_pdftocairo_svg(pdf)?;
    Ok(parse_svg_rules(&svg))
}

/// 2D アフィン変換（SVG `matrix(a b c d e f)`）。点 `(x,y)` を `(a*x+c*y+e, b*x+d*y+f)` に写す。
type Affine = [f32; 6];
const IDENTITY: Affine = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

fn mul(m: &Affine, n: &Affine) -> Affine {
    let [a, b, c, d, e, f] = *m;
    let [aa, bb, cc, dd, ee, ff] = *n;
    [
        a * aa + c * bb,
        b * aa + d * bb,
        a * cc + c * dd,
        b * cc + d * dd,
        a * ee + c * ff + e,
        b * ee + d * ff + f,
    ]
}

fn apply(m: &Affine, x: f32, y: f32) -> (f32, f32) {
    let [a, b, c, d, e, f] = *m;
    (a * x + c * y + e, b * x + d * y + f)
}

/// `transform` 属性値（`matrix(...)` / `translate(...)`）をアフィン変換に変換する。
fn parse_transform(s: &str) -> Affine {
    if let Some(args) = s.split_once("matrix(").and_then(|(_, r)| r.split_once(')')) {
        let v: Vec<f32> = args.0.split([',', ' ']).filter_map(|t| t.trim().parse().ok()).collect();
        if v.len() == 6 {
            return [v[0], v[1], v[2], v[3], v[4], v[5]];
        }
    }
    if let Some(args) = s.split_once("translate(").and_then(|(_, r)| r.split_once(')')) {
        let v: Vec<f32> = args.0.split([',', ' ']).filter_map(|t| t.trim().parse().ok()).collect();
        if !v.is_empty() {
            return [1.0, 0.0, 0.0, 1.0, v[0], *v.get(1).unwrap_or(&0.0)];
        }
    }
    IDENTITY
}

/// SVG 文字列から罫線を抽出する（`pdftocairo` 非依存でテスト可能）。
pub(crate) fn parse_svg_rules(svg: &str) -> PageRules {
    let mut reader = Reader::from_str(svg);
    let mut ctm_stack: Vec<Affine> = vec![IDENTITY];
    let mut rules = PageRules::default();
    let mut buf = Vec::new();

    // 開始要素の transform を解決して CTM を返す（親 CTM × 自身の transform）。
    let resolve = |ctm_stack: &[Affine], e: &quick_xml::events::BytesStart| -> Affine {
        let parent = *ctm_stack.last().unwrap_or(&IDENTITY);
        for attr in e.attributes().flatten() {
            if attr.key.as_ref() == b"transform" {
                if let Ok(val) = std::str::from_utf8(&attr.value) {
                    return mul(&parent, &parse_transform(val));
                }
            }
        }
        parent
    };
    let process_path = |ctm: &Affine, e: &quick_xml::events::BytesStart, rules: &mut PageRules| {
        if e.local_name().as_ref() != b"path" {
            return;
        }
        let mut d = None;
        for attr in e.attributes().flatten() {
            if attr.key.as_ref() == b"d" {
                d = std::str::from_utf8(&attr.value).ok().map(|s| s.to_string());
            }
        }
        let Some(d) = d else { return };
        // 曲線を含むパス（グリフ等）は除外。直線 2 点だけ対象。
        if d.contains('C') || d.contains('Q') || d.contains('A') {
            return;
        }
        let nums: Vec<f32> = d
            .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
            .filter_map(|t| t.parse().ok())
            .collect();
        if nums.len() != 4 {
            return;
        }
        let (x1, y1) = apply(ctm, nums[0], nums[1]);
        let (x2, y2) = apply(ctm, nums[2], nums[3]);
        let (dx, dy) = ((x2 - x1).abs(), (y2 - y1).abs());
        if dx < 2.0 && dy > 15.0 {
            rules.vertical.push(VRule {
                x: (x1 + x2) / 2.0,
                y0: y1.min(y2),
                y1: y1.max(y2),
            });
        } else if dy < 2.0 && dx > 15.0 {
            rules.horizontal.push(HRule {
                y: (y1 + y2) / 2.0,
                x0: x1.min(x2),
                x1: x1.max(x2),
            });
        }
    };

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let ctm = resolve(&ctm_stack, &e);
                process_path(&ctm, &e, &mut rules);
                ctm_stack.push(ctm);
            }
            Ok(Event::Empty(e)) => {
                let ctm = resolve(&ctm_stack, &e);
                process_path(&ctm, &e, &mut rules);
            }
            Ok(Event::End(_)) => {
                if ctm_stack.len() > 1 {
                    ctm_stack.pop();
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    rules
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_transform_and_classifies_lines() {
        // <g transform="matrix(1 0 0 1 100 50)"> 内で、原点から (0,200) への縦線と
        // (300,0) への横線。CTM 適用後に絶対座標へ写り、縦/横に分類される。
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg">
          <g transform="matrix(1 0 0 1 100 50)">
            <path d="M 0 0 L 0 200 "/>
            <path d="M 0 0 L 300 0 "/>
            <path d="M 0 0 C 1 1 2 2 3 3 "/>
          </g>
        </svg>"#;
        let r = parse_svg_rules(svg);
        assert_eq!(r.vertical.len(), 1, "縦線1本");
        assert_eq!(r.horizontal.len(), 1, "横線1本");
        // translate(100,50) が効く。
        assert!((r.vertical[0].x - 100.0).abs() < 0.1);
        assert!((r.vertical[0].y0 - 50.0).abs() < 0.1);
        assert!((r.vertical[0].y1 - 250.0).abs() < 0.1);
        assert!((r.horizontal[0].y - 50.0).abs() < 0.1);
        assert!((r.horizontal[0].x1 - 400.0).abs() < 0.1);
    }

    #[test]
    fn ignores_glyph_curves_and_short_segments() {
        let svg = r#"<svg><g><path d="M 0 0 C 1 1 2 2 3 3 Z"/><path d="M 0 0 L 1 0 "/></g></svg>"#;
        let r = parse_svg_rules(svg);
        assert_eq!(r.vertical.len(), 0);
        assert_eq!(r.horizontal.len(), 0); // 長さ1は短すぎ(>15要件)で除外
    }
}

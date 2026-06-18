//! 縦書き新旧対照表（改正後=上帯 / 改正前=下帯）の検出と 2 カラム分離。
//!
//! 官報の縦書き新旧対照表は縦の罫線(描画)を持たず、改正後/改正前を **y 座標の帯**で
//! 表す。「改正後」(上) と「改正前」(下) のラベルが表の右端に縦に並ぶことを手がかりに、
//! 見出し x(=表の右端) と上下帯の境界 y を求め、本体を 2 帯に分けて復元する。

use crate::vertical::{reconstruct_page, Col};

/// 縦書き新旧対照表ページの「改正後／改正前」欄見出し列を検出する。
///
/// x 近傍でカラムをまとめ、その縦連結に両ラベルが現れる列を見出し列とみなし、
/// `(見出し x, 上下帯の境界 y)` を返す。境界は両ラベルの中心の中点。
/// 見つからなければ `None`（＝通常ページ）。
pub(crate) fn detect_shinkyu_header(cols: &[Col]) -> Option<(f32, f32)> {
    // x 近傍(±6pt)でグループ化。
    let mut idx: Vec<usize> = (0..cols.len()).collect();
    idx.sort_by(|&a, &b| cols[a].x.partial_cmp(&cols[b].x).unwrap_or(std::cmp::Ordering::Equal));
    let mut group: Vec<usize> = Vec::new();
    let mut rep = f32::NEG_INFINITY;
    let flush_and_check = |g: &mut Vec<usize>| -> Option<(f32, f32)> {
        if g.len() < 3 {
            g.clear();
            return None;
        }
        g.sort_by(|&a, &b| cols[a].y.partial_cmp(&cols[b].y).unwrap_or(std::cmp::Ordering::Equal));
        let chars: Vec<char> = g.iter().flat_map(|&i| cols[i].text.chars()).collect();
        // 各文字の y 中心（cols[i] が複数文字なら全文字に同じ中心を割当: 見出しは1文字列なので十分）。
        let centers: Vec<f32> = g
            .iter()
            .flat_map(|&i| {
                let c = (cols[i].y + cols[i].y1) / 2.0;
                cols[i].text.chars().map(move |_| c)
            })
            .collect();
        let header_x = g.iter().map(|&i| cols[i].x).fold(f32::INFINITY, f32::min);
        let go = find_run(&chars, &['改', '正', '後']);
        let mae = find_run(&chars, &['改', '正', '前']);
        g.clear();
        match (go, mae) {
            (Some(gi), Some(mi)) if gi != mi => {
                let go_c = (centers[gi] + centers[gi + 1] + centers[gi + 2]) / 3.0;
                let mae_c = (centers[mi] + centers[mi + 1] + centers[mi + 2]) / 3.0;
                Some((header_x, (go_c + mae_c) / 2.0))
            }
            _ => None,
        }
    };
    for &i in &idx {
        if !group.is_empty() && cols[i].x - rep > 6.0 {
            if let Some(found) = flush_and_check(&mut group) {
                return Some(found);
            }
        }
        if group.is_empty() {
            rep = cols[i].x;
        }
        group.push(i);
    }
    flush_and_check(&mut group)
}

/// `chars` 中で連続部分列 `pat` が最初に現れる開始位置を返す。
fn find_run(chars: &[char], pat: &[char]) -> Option<usize> {
    if pat.is_empty() || chars.len() < pat.len() {
        return None;
    }
    (0..=chars.len() - pat.len()).find(|&i| &chars[i..i + pat.len()] == pat)
}

/// 新旧対照表の段を改正後(上帯)/改正前(下帯)に分けて復元する。
///
/// 見出し列 x より右(=`x >= header_x`)は前文・標題とみなし先頭にそのまま置く。
/// 表本体(`x < header_x`)を境界 y で上下に分け、各帯を右→左に復元して
/// `改正後`／`改正前` の欄見出し行を冠して返す。見出し行はフロントの新旧対照表
/// 組み(parseShinkyu)と `detect_format` の shinkyu 判定の双方に整合する。
pub(crate) fn reconstruct_shinkyu(cols: Vec<Col>, header_x: f32, divider: f32) -> String {
    let mut preamble: Vec<Col> = Vec::new();
    let mut upper: Vec<Col> = Vec::new();
    let mut lower_all: Vec<Col> = Vec::new();
    for c in cols {
        if c.x >= header_x - 1.0 {
            // 見出しラベル自身(改/正/後/前)は捨て、それ以外(前文・標題)は前文へ。
            if matches!(c.text.as_str(), "改" | "正" | "後" | "前") && (c.x - header_x).abs() < 6.0 {
                continue;
            }
            preamble.push(c);
        } else if (c.y + c.y1) / 2.0 < divider {
            upper.push(c);
        } else {
            lower_all.push(c);
        }
    }
    // 新旧対照表は境界 y を挟んでほぼ上下対称（改正後欄の高さ ≒ 改正前欄の高さ）。
    // 改正前(下帯)が表の下端を越えて広がる場合、それは表の下に同居する別記事
    // （型式認定一覧など）の可能性が高い。上帯の最上端を表頂点とみなし、その鏡像
    // (`2*divider - table_top`) を下端として下帯を制限し、表外の記事の取り込みを防ぐ。
    // 表が紙面全体を占める通常ケースでは下端がページ外になり実質無効（＝無害）。
    let lower: Vec<Col> = if let Some(table_top) = upper
        .iter()
        .map(|c| c.y)
        .fold(None, |acc: Option<f32>, y| Some(acc.map_or(y, |a| a.min(y))))
    {
        let table_bottom = 2.0 * divider - table_top;
        lower_all
            .into_iter()
            .filter(|c| (c.y + c.y1) / 2.0 < table_bottom)
            .collect()
    } else {
        lower_all
    };
    let mut out: Vec<String> = Vec::new();
    let pre = reconstruct_page(preamble);
    if !pre.is_empty() {
        out.push(pre);
    }
    let after = reconstruct_page(upper);
    let before = reconstruct_page(lower);
    if !after.is_empty() || !before.is_empty() {
        out.push("改正後".to_string());
        out.push(after);
        out.push("改正前".to_string());
        out.push(before);
    }
    out.join("\n").trim().to_string()
}

#[cfg(test)]
mod tests {
    use crate::format::detect_format;
    use crate::reconstruct_vertical;

    #[test]
    fn finds_run_subsequence() {
        assert_eq!(super::find_run(&['改', '正', '後'], &['改', '正', '後']), Some(0));
        assert_eq!(
            super::find_run(&['x', '改', '正', '前'], &['改', '正', '前']),
            Some(1)
        );
        assert_eq!(super::find_run(&['改', '正'], &['改', '正', '後']), None);
    }

    #[test]
    fn reconstructs_vertical_shinkyu_into_two_bands() {
        // 縦書き新旧対照表の最小ケース: x=150 に「改正後」(上) /「改正前」(下) の
        // 欄見出し列、その左 x=120 に各帯の本文セル(改正後=甲種 / 改正前=乙類)。
        // 見出し列より右(x=200,210)に上下帯を跨ぐ前文カラムを置くことで、実ページ同様に
        // 段分割されず 1 段にまとまり、見出しの y 帯で改正後/改正前に分けて復元される。
        let xhtml = r#"<page width="300" height="400">
          <word xMin="200" yMin="65" xMax="210" yMax="300">前文左</word>
          <word xMin="210" yMin="65" xMax="220" yMax="300">前文右</word>
          <word xMin="150" yMin="70" xMax="160" yMax="80">改</word>
          <word xMin="150" yMin="90" xMax="160" yMax="100">正</word>
          <word xMin="150" yMin="110" xMax="160" yMax="120">後</word>
          <word xMin="150" yMin="250" xMax="160" yMax="260">改</word>
          <word xMin="150" yMin="270" xMax="160" yMax="280">正</word>
          <word xMin="150" yMin="290" xMax="160" yMax="300">前</word>
          <word xMin="120" yMin="70" xMax="130" yMax="110">甲種</word>
          <word xMin="120" yMin="250" xMax="130" yMax="290">乙類</word>
        </page>"#;
        let out = reconstruct_vertical(xhtml);
        // 欄見出し行が独立して現れ、各帯の本文が正しい側に入る。
        assert!(out.lines().any(|l| l.trim() == "改正後"), "out=\n{out}");
        assert!(out.lines().any(|l| l.trim() == "改正前"), "out=\n{out}");
        let after_idx = out.find("改正後").unwrap();
        let before_idx = out.find("改正前").unwrap();
        let kou = out.find("甲種").unwrap();
        let otsu = out.find("乙類").unwrap();
        assert!(after_idx < kou && kou < before_idx, "甲種は改正後側: {out}");
        assert!(before_idx < otsu, "乙類は改正前側: {out}");
        // 欄見出しが揃うので shinkyu と判定される。
        assert_eq!(detect_format(&out), "shinkyu");
    }
}

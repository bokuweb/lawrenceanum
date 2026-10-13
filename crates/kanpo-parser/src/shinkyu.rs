//! 縦書き新旧対照表（改正後=上帯 / 改正前=下帯）の検出と 2 カラム分離。
//!
//! 官報の縦書き新旧対照表は縦の罫線(描画)を持たず、改正後/改正前を **y 座標の帯**で
//! 表す。「改正後」(上) と「改正前」(下) のラベルが表の右端に縦に並ぶことを手がかりに、
//! 見出し x(=表の右端) と上下帯の境界 y を求め、本体を 2 帯に分けて復元する。

use crate::trace::Unit;
use crate::vertical::{cell_lines, page_lines, reconstruct_page, Col};

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

/// 見出しの無い継続ページで、上下2帯（改正後/改正前）構造を幾何＋内容類似から検出する。
///
/// 多ページ新旧対照表の 2 ページ目以降は「改正後/改正前」の欄見出しを繰り返さないため、
/// [`detect_shinkyu_header`] では拾えない。代わりに次の 3 条件で継続ページを判定する:
///   1. ページ中央付近に列がほとんど跨がない横線（谷）があり、そこを境界 y とできる
///   2. 上帯と下帯の高さがほぼ等しい（表は上下対称）
///   3. 上帯と下帯の本文がほぼ同一（改正後 ≒ 改正前。bigram Jaccard で判定）
///
/// 人事異動一覧のような「上下で別内容」の 2 段組ページは (3) で弾かれる。
/// 条件を満たせば境界 y を返す。
pub(crate) fn detect_shinkyu_bands_headerless(cols: &[Col], height: f32) -> Option<f32> {
    if cols.len() < 8 {
        return None;
    }
    let center = |c: &Col| (c.y + c.y1) / 2.0;
    // 中央域 [0.35H, 0.65H] で跨ぎ最小の cut（候補は各列の下端）。
    let (lo, hi) = (0.35 * height, 0.65 * height);
    let mut best: Option<(usize, f32)> = None;
    for c in cols {
        let cut = c.y1 + 0.5;
        if cut < lo || cut > hi {
            continue;
        }
        let crossing = cols.iter().filter(|d| d.y < cut && cut < d.y1).count();
        if best.map(|b| crossing < b.0).unwrap_or(true) {
            best = Some((crossing, cut));
        }
    }
    let (_, divider) = best?;
    let upper: Vec<Col> = cols.iter().filter(|c| center(c) < divider).cloned().collect();
    let lower: Vec<Col> = cols.iter().filter(|c| center(c) >= divider).cloned().collect();
    if upper.len() < 3 || lower.len() < 3 {
        return None;
    }
    // 上下帯の高さ対称性（表は改正後欄の高さ ≒ 改正前欄の高さ）。
    let band_height = |g: &[Col]| {
        let top = g.iter().map(|c| c.y).fold(f32::INFINITY, f32::min);
        let bot = g.iter().map(|c| c.y1).fold(f32::NEG_INFINITY, f32::max);
        bot - top
    };
    let (uh, lh) = (band_height(&upper), band_height(&lower));
    if uh.min(lh) / uh.max(lh) < 0.7 {
        return None;
    }
    // 内容類似（改正後 ≒ 改正前）。新旧対照表は両欄がほぼ同一テキストで差分のみ異なる。
    // 人事異動一覧のように上下で別内容のページはここで弾かれる。
    let ut = reconstruct_page(upper);
    let lt = reconstruct_page(lower);
    if bigram_jaccard(&ut, &lt) < 0.5 {
        return None;
    }
    Some(divider)
}

/// 2 文字 n-gram 集合の Jaccard 係数（0.0–1.0）。改正後/改正前の本文類似度に使う。
fn bigram_jaccard(a: &str, b: &str) -> f32 {
    use std::collections::HashSet;
    let bigrams = |s: &str| -> HashSet<(char, char)> {
        let chars: Vec<char> = s.chars().filter(|c| !c.is_whitespace()).collect();
        chars.windows(2).map(|w| (w[0], w[1])).collect()
    };
    let (sa, sb) = (bigrams(a), bigrams(b));
    if sa.is_empty() && sb.is_empty() {
        return 0.0;
    }
    let inter = sa.intersection(&sb).count() as f32;
    let union = sa.union(&sb).count() as f32;
    inter / union
}

/// `chars` 中で連続部分列 `pat` が最初に現れる開始位置を返す。
fn find_run(chars: &[char], pat: &[char]) -> Option<usize> {
    if pat.is_empty() || chars.len() < pat.len() {
        return None;
    }
    (0..=chars.len() - pat.len()).find(|&i| &chars[i..i + pat.len()] == pat)
}

/// 新旧対照表の段を改正後(上帯)/改正前(下帯)に分けて復元する。
pub(crate) fn reconstruct_shinkyu(cols: Vec<Col>, header_x: f32, divider: f32) -> Unit {
    ShinkyuLayout::split(cols, header_x, divider).into_unit()
}

/// 新旧対照表 1 件分のカラム配置。
///
/// 帯ごとのカラム群をページ内の段ごとに分けて持つ（同じ x に別の段の列があるため、
/// 1 つの Vec に混ぜると右→左復元で列が連結されてしまう）。
pub(crate) struct ShinkyuLayout {
    preamble: Vec<Col>,
    after: Vec<Vec<Col>>,
    before: Vec<Vec<Col>>,
    /// 表の外にある列（表の下・左に同居する別記事）。表の後ろに読み順で出す。
    trailing: Vec<Vec<Col>>,
}

impl ShinkyuLayout {
    /// 見出し列 x より右(=`x >= header_x`)は前文・標題とみなし先頭にそのまま置く。
    /// 表本体(`x < header_x`)を境界 y で上下に分ける。
    pub(crate) fn split(cols: Vec<Col>, header_x: f32, divider: f32) -> Self {
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
        // (`2*divider - table_top`) を下端として下帯を制限し、はみ出た列は表外として
        // 表の後ろへ回す。表が紙面全体を占める通常ケースでは下端がページ外になり実質無効。
        let (lower, outside): (Vec<Col>, Vec<Col>) = match upper.iter().map(|c| c.y).reduce(f32::min) {
            Some(table_top) => {
                let table_bottom = 2.0 * divider - table_top;
                lower_all.into_iter().partition(|c| (c.y + c.y1) / 2.0 < table_bottom)
            }
            None => (lower_all, Vec::new()),
        };
        ShinkyuLayout {
            preamble,
            after: vec![upper],
            before: vec![lower],
            trailing: vec![outside],
        }
    }

    /// 見出し段における改正後帯→改正前帯の y ピッチ。
    ///
    /// 新旧対照表は改正後/改正前の対応する行を同じ x に並べるので、両帯で x が一致する
    /// 列頭どうしの y 差の最頻値が帯のピッチになる。下の段へ続く表の判定に使う。
    fn band_pitch(&self) -> Option<f32> {
        let (a, b) = (self.after.first()?, self.before.first()?);
        let (ha, hb) = (column_heads(a), column_heads(b));
        let dys: Vec<f32> = hb
            .iter()
            .filter_map(|h| ha.iter().find(|g| (g.x - h.x).abs() <= 1.0).map(|g| h.y - g.y))
            .collect();
        // 許容 1.5pt で最多の y 差を採る（同数なら先勝ち）。
        let best = dys
            .iter()
            .map(|&d| {
                let near: Vec<f32> = dys.iter().copied().filter(|e| (e - d).abs() <= 1.5).collect();
                (near.len(), near.iter().sum::<f32>() / near.len() as f32)
            })
            .max_by_key(|(n, _)| *n)?;
        (best.0 >= 2).then_some(best.1)
    }

    /// 見出し段の下に続く段の組 `(a, b)` が同じ表の続き（a=改正後, b=改正前）なら取り込む。
    ///
    /// 官報は表の欄見出しを同じページの下の段では繰り返さないため、見出しの無い段の組を
    /// 次の条件で表の続きと判定する:
    ///   1. b の列頭に、x が一致し y 差が見出し段の帯ピッチに等しい a の列頭がある
    ///      （改正後/改正前の対応行が同じ x に並ぶ）。一致が 3 組以上。
    ///   2. 両段の最右列どうしが一致する（表は段の右端から始まる続き）。
    ///   3. a の最右列が記事見出し（`〇…`）で始まらない。
    ///   4. 表の範囲（一致した最左の x より右）にある b の列頭の 6 割以上が一致する。
    ///
    /// 段落の通常の段送りは段ピッチ（表の帯ピッチとは独立）で並ぶため 1. で弾かれる。
    /// 表の範囲より左の列は別記事として `trailing` に回す。取り込んだ段の組に表外の列が
    /// あれば表はそこで終わりとみなし `false`（続けて次の段の組を見ない）を返す。
    pub(crate) fn absorb_continuation(&mut self, a: &mut Vec<Col>, b: &mut Vec<Col>) -> Option<bool> {
        let pitch = self.band_pitch()?;
        let (ha, hb) = (column_heads(a), column_heads(b));
        let (ra, rb) = (ha.first()?, hb.first()?);
        if (ra.x - rb.x).abs() > 1.0 || ra.lead.starts_with('〇') {
            return None;
        }
        let matched: Vec<f32> = hb
            .iter()
            .filter(|h| ha.iter().any(|g| (g.x - h.x).abs() <= 1.0 && (h.y - g.y - pitch).abs() <= 1.5))
            .map(|h| h.x)
            .collect();
        if matched.len() < 3 || matched.first() != Some(&rb.x) {
            return None;
        }
        // 列頭の x は列間隔（≒12pt）の半分の余裕を見て表の左端とする。
        let left = matched.iter().copied().fold(f32::INFINITY, f32::min) - 6.0;
        let in_table = hb.iter().filter(|h| h.x >= left).count();
        if (matched.len() as f32) < in_table as f32 * 0.6 {
            return None;
        }
        let (a_tab, a_rest): (Vec<Col>, Vec<Col>) = std::mem::take(a).into_iter().partition(|c| c.x >= left);
        let (b_tab, b_rest): (Vec<Col>, Vec<Col>) = std::mem::take(b).into_iter().partition(|c| c.x >= left);
        let ends_here = !a_rest.is_empty() || !b_rest.is_empty();
        self.after.push(a_tab);
        self.before.push(b_tab);
        self.trailing.push(a_rest);
        self.trailing.push(b_rest);
        Some(!ends_here)
    }

    /// 帯ごとに行へ組む（テキスト化は [`Unit::text`]）。
    pub(crate) fn into_unit(self) -> Unit {
        Unit::Shinkyu {
            preamble: page_lines(self.preamble),
            after: self.after.into_iter().map(cell_lines).collect(),
            before: self.before.into_iter().map(cell_lines).collect(),
            outside: self.trailing.into_iter().map(page_lines).collect(),
        }
    }
}

/// 縦の列（x 近傍のカラム語の束）の頭。`lead` は列の先頭語。
struct ColumnHead {
    x: f32,
    y: f32,
    lead: String,
}

/// カラム語を [`reconstruct_page`] と同じ規則（右→左、x 差 10pt 以内を 1 列）で列に束ね、
/// 各列の頭（代表 x・最上端 y・先頭語）を右から順に返す。
fn column_heads(cols: &[Col]) -> Vec<ColumnHead> {
    let mut sorted: Vec<&Col> = cols.iter().collect();
    sorted.sort_by(|a, b| b.x.partial_cmp(&a.x).unwrap_or(std::cmp::Ordering::Equal));
    let mut heads: Vec<ColumnHead> = Vec::new();
    for c in sorted {
        match heads.last_mut() {
            Some(h) if h.x - c.x <= 10.0 => {
                if c.y < h.y {
                    h.y = c.y;
                    h.lead = c.text.clone();
                }
            }
            _ => heads.push(ColumnHead {
                x: c.x,
                y: c.y,
                lead: c.text.clone(),
            }),
        }
    }
    heads
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

    /// 見出し段（帯ピッチ 170）の下に、列頭の y 差が `lower_pitch` の段の組が続くページ。
    fn page_with_lower_tiers(lower_pitch: f32) -> String {
        let w = |x: f32, y: f32, h: f32, t: &str| {
            format!(r#"<word xMin="{x}" yMin="{y}" xMax="{}" yMax="{}">{t}</word>"#, x + 8.0, y + h)
        };
        // 前文は複数列で上下帯を跨ぐ（実ページ同様、帯の境界で段に割れない）。
        let mut words = vec![
            w(344.0, 60.0, 350.0, "前文一"),
            w(332.0, 60.0, 350.0, "前文二"),
            w(320.0, 60.0, 350.0, "前文三"),
            // 欄見出しは各帯の中央に縦に並ぶ。
            w(300.0, 100.0, 8.0, "改"),
            w(300.0, 116.0, 8.0, "正"),
            w(300.0, 132.0, 8.0, "後"),
            w(300.0, 270.0, 8.0, "改"),
            w(300.0, 286.0, 8.0, "正"),
            w(300.0, 302.0, 8.0, "前"),
        ];
        for (i, x) in [280.0, 268.0, 256.0].into_iter().enumerate() {
            words.push(w(x, 70.0, 100.0, &format!("甲{i}")));
            words.push(w(x, 240.0, 100.0, &format!("乙{i}")));
            words.push(w(x, 430.0, 100.0, &format!("丙{i}")));
            words.push(w(x, 430.0 + lower_pitch, 100.0, &format!("丁{i}")));
        }
        format!(r#"<page width="595" height="842">{}</page>"#, words.join(""))
    }

    fn after_block(out: &str) -> String {
        let lines: Vec<&str> = out.lines().collect();
        let go = lines.iter().position(|l| *l == "改正後").unwrap();
        let mae = lines.iter().position(|l| *l == "改正前").unwrap();
        lines[go + 1..mae].concat()
    }

    #[test]
    fn absorbs_lower_tiers_aligned_at_band_pitch() {
        // 下の段の組が見出し段と同じ帯ピッチで揃う → 表の続き（丙=改正後, 丁=改正前）。
        let out = reconstruct_vertical(&page_with_lower_tiers(170.0));
        let after = after_block(&out);
        assert!(after.contains("甲0") && after.contains("丙0"), "{out}");
        assert!(!after.contains("丁0"), "{out}");
    }

    #[test]
    fn keeps_lower_tiers_at_other_pitch_out_of_table() {
        // 段送りのピッチ（184）で揃うだけの通常記事は表に取り込まない。
        let out = reconstruct_vertical(&page_with_lower_tiers(184.0));
        let after = after_block(&out);
        assert!(after.contains("甲0") && !after.contains("丙0"), "{out}");
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

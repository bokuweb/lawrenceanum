//! 縦書き約物の正規化・私用領域(PUA)グリフ除去・ページ柱ノイズ判定。

/// 縦書き約物の正規化 + ページ柱ノイズ除去をテキスト全体に適用する。
///
/// 行単位に約物正規化・PUA 除去・柱の剥がしを行い、連続する空行を 1 行に畳む。
pub fn normalize_text(raw: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    for line in raw.lines() {
        let line: String = line
            .chars()
            .filter(|c| !is_private_use(*c))
            .map(normalize_char)
            .collect();
        let stripped = strip_margin_lead(&line);
        if is_margin_noise(&stripped) {
            continue;
        }
        lines.push(stripped.trim_end().to_string());
    }
    // 連続する空行を 1 行に畳む。
    let mut out: Vec<String> = Vec::new();
    let mut prev_blank = false;
    for l in lines {
        let blank = l.trim().is_empty();
        if blank && prev_blank {
            continue;
        }
        prev_blank = blank;
        out.push(l);
    }
    out.join("\n").trim().to_string()
}

/// 私用領域(PUA)の文字か。官報 PDF は傍線・下線などをフォント固有グリフ(U+E000–
/// U+F8FF)で埋め込むことがあり、テキストとしては意味を持たないため除去する。
pub(crate) fn is_private_use(c: char) -> bool {
    ('\u{E000}'..='\u{F8FF}').contains(&c)
}

/// ToUnicode を持たない私用 CID（`Adobe-Npb1` 等）のグリフを本来の文字へ戻す。
///
/// 官報の表中の括弧付き数字「(1)」や化学名のハイフンは、ToUnicode 無しの別フォントで
/// 組まれることがあり、poppler は CID をそのまま符号位置とみなして無関係な漢字を出す
/// （例: (1)→`挎`、(15)→`昝昫`、‐→`慺`）。フォント切替で独立した 1 語になり、
/// 1em 四方に収まる（2 桁は半角 2 グリフで 1em）ことを条件に写像する。
/// `星`・`昭` など常用漢字と衝突する符号もあるため、語全体がこの形のときだけ置き換える。
pub(crate) fn fix_private_cid_glyphs(text: &str, width: f32, height: f32) -> Option<String> {
    if height > width * 1.3 {
        return None;
    }
    let fw = |d: u32| char::from_u32('０' as u32 + d).unwrap_or('？');
    let chars: Vec<char> = text.trim().chars().collect();
    match chars.as_slice() {
        // (1)〜(10): U+630E..=U+6317
        [c] if ('\u{630E}'..='\u{6317}').contains(c) => {
            let n = *c as u32 - 0x630D;
            let digits: String = n.to_string().chars().map(|d| fw(d.to_digit(10).unwrap())).collect();
            Some(format!("（{digits}）"))
        }
        ['\u{617A}'] => Some("－".to_string()),
        // (11)〜(99): 「(十の位」U+661D..=U+6625 +「一の位)」U+6627..=U+6630（1..9,0 の順）
        [t, o] if ('\u{661D}'..='\u{6625}').contains(t) && ('\u{6627}'..='\u{6630}').contains(o) => {
            let tens = *t as u32 - 0x661C;
            let ones = (*o as u32 - 0x6626) % 10;
            Some(format!("（{}{}）", fw(tens), fw(ones)))
        }
        _ => None,
    }
}

/// 縦書き presentation form を通常の全角約物に写像する。
pub(crate) fn normalize_char(c: char) -> char {
    match c {
        '\u{FE10}' => '，',
        '\u{FE11}' => '、',
        '\u{FE12}' => '。',
        '\u{FE13}' => '：',
        '\u{FE14}' => '；',
        '\u{FE15}' => '！',
        '\u{FE16}' => '？',
        '\u{FE17}' => '〖',
        '\u{FE18}' => '〗',
        '\u{FE19}' => '…',
        '\u{FE31}' => '—',
        '\u{FE32}' => '–',
        '\u{FE33}' | '\u{FE34}' => '｜',
        '\u{FE35}' => '（',
        '\u{FE36}' => '）',
        '\u{FE37}' => '｛',
        '\u{FE38}' => '｝',
        '\u{FE39}' => '〔',
        '\u{FE3A}' => '〕',
        '\u{FE3B}' => '【',
        '\u{FE3C}' => '】',
        '\u{FE3D}' => '《',
        '\u{FE3E}' => '》',
        '\u{FE3F}' => '〈',
        '\u{FE40}' => '〉',
        '\u{FE41}' => '「',
        '\u{FE42}' => '」',
        '\u{FE43}' => '『',
        '\u{FE44}' => '』',
        '\u{FE47}' => '〔',
        '\u{FE48}' => '〕',
        other => other,
    }
}

/// 行頭にぽつんと現れるページ柱の 1 文字（「官」「報」）を、後続が大きな空白で
/// 区切られている場合に限り除去する。
fn strip_margin_lead(line: &str) -> String {
    let trimmed = line.trim_start();
    let mut chars = trimmed.chars();
    if let Some(first) = chars.next() {
        if matches!(first, '官' | '報') {
            let rest = chars.as_str();
            // 1 文字 + 連続空白(2 個以上) + 本文、という柱パターンのみ剥がす。
            if rest.starts_with("  ") {
                return rest.trim_start().to_string();
            }
            if rest.trim().is_empty() {
                return String::new();
            }
        }
    }
    line.to_string()
}

/// ページ柱（発行日・曜日・号数の余白テキスト）と思われる行か。
pub(crate) fn is_margin_noise(line: &str) -> bool {
    let compact: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.is_empty() {
        return false; // 空行は畳み処理に任せる。
    }
    // 数字（半角・全角）を除いた骨格で判定（「令和  ８  年…」等の数字ゆらぎを吸収）。
    let skeleton: String = compact
        .chars()
        .filter(|c| !c.is_ascii_digit() && !('０'..='９').contains(c))
        .collect();
    const NOISE: &[&str] = &[
        "官",
        "報",
        "令和年月日",
        "平成年月日",
        "号外第号",
        "（号外第号）",
        "(号外第号)",
        "月曜日",
        "火曜日",
        "水曜日",
        "木曜日",
        "金曜日",
        "土曜日",
        "日曜日",
    ];
    NOISE.contains(&skeleton.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_vertical_punctuation() {
        let raw = "\u{FE35}施行\u{FE36}\u{FE12}";
        assert_eq!(normalize_text(raw), "（施行）。");
    }

    #[test]
    fn drops_margin_noise_lines() {
        let raw = "本則を次のように改める\n官\n令和 ８ 年 ６ 月 15 日\n第六条";
        let out = normalize_text(raw);
        assert!(out.contains("本則を次のように改める"));
        assert!(out.contains("第六条"));
        assert!(!out.contains("令和"));
    }

    #[test]
    fn restores_private_cid_numbered_items() {
        // 1em 四方の独立語だけを写像する。
        assert_eq!(fix_private_cid_glyphs("挎", 8.0, 8.0).as_deref(), Some("（１）"));
        assert_eq!(fix_private_cid_glyphs("挗", 8.0, 8.0).as_deref(), Some("（１０）"));
        assert_eq!(fix_private_cid_glyphs("昝昫", 8.0, 8.0).as_deref(), Some("（１５）"));
        assert_eq!(fix_private_cid_glyphs("昡昰", 8.0, 8.0).as_deref(), Some("（５０）"));
        assert_eq!(fix_private_cid_glyphs("慺", 8.0, 8.0).as_deref(), Some("－"));
        // 通常の本文（2 文字で 2em の高さ・常用漢字の語）は触らない。
        assert_eq!(fix_private_cid_glyphs("星昭", 8.0, 16.0), None);
        assert_eq!(fix_private_cid_glyphs("昭和", 8.0, 8.0), None);
    }

    #[test]
    fn strips_private_use_glyphs() {
        // U+E0A8 等の PUA(傍線グリフ)は除去される。
        let raw = "本文\u{E0A8}\u{E0A8}\u{E0A8}\n令和八年";
        let out = normalize_text(raw);
        assert!(!out.contains('\u{E0A8}'));
        assert!(out.contains("本文"));
        assert!(out.contains("令和八年"));
    }
}

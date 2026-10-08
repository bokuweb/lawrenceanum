//! 例規テキストの正規化と行分類（ベンダ非依存）。
//!
//! ぎょうせい / 第一法規とも本文は「第N条」「２　」「(１)」のような行頭記号で
//! 条・項・号が表現される。HTML の class が使えない箇所はここでの文字列判定に頼る。

/// 全角数字・全角括弧・全角空白を半角へ寄せる（条番号・日付の解析用。本文表示には使わない）。
pub fn to_ascii_digits(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '０'..='９' => char::from_u32(c as u32 - '０' as u32 + '0' as u32).unwrap_or(c),
            _ => c,
        })
        .collect()
}

/// 連続空白を 1 つに畳み、前後を trim する。HTML 由来の改行・インデントを除去する。
pub fn squash_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_ws = false;
    for c in s.chars() {
        // 全角空白 (U+3000) は条番号と本文の区切りとして意味を持つので残す。
        if c.is_whitespace() && c != '\u{3000}' {
            if !prev_ws {
                out.push(' ');
            }
            prev_ws = true;
        } else {
            out.push(c);
            prev_ws = false;
        }
    }
    out.trim_matches(|c: char| c.is_whitespace()).to_string()
}

/// 漢数字（一〜九千九百九十九）を数値にする。位取り無しの「二〇」形式も受ける。
pub fn kanji_to_number(s: &str) -> Option<u32> {
    if s.is_empty() {
        return None;
    }
    let digit = |c: char| -> Option<u32> {
        Some(match c {
            '〇' | '零' => 0,
            '一' => 1,
            '二' => 2,
            '三' => 3,
            '四' => 4,
            '五' => 5,
            '六' => 6,
            '七' => 7,
            '八' => 8,
            '九' => 9,
            _ => return None,
        })
    };
    if !s.chars().any(|c| matches!(c, '十' | '百' | '千')) {
        let mut n = 0u32;
        for c in s.chars() {
            n = n * 10 + digit(c)?;
        }
        return Some(n);
    }
    let mut total = 0u32;
    let mut cur = 0u32;
    for c in s.chars() {
        let unit = match c {
            '十' => 10,
            '百' => 100,
            '千' => 1000,
            _ => {
                cur = digit(c)?;
                continue;
            }
        };
        total += if cur == 0 { 1 } else { cur } * unit;
        cur = 0;
    }
    Some(total + cur)
}

/// 算用数字（半角/全角）または漢数字を数値にする。
pub fn parse_number(s: &str) -> Option<u32> {
    let a = to_ascii_digits(s);
    if !a.is_empty() && a.chars().all(|c| c.is_ascii_digit()) {
        return a.parse().ok();
    }
    kanji_to_number(s)
}

fn is_num_char(c: char) -> bool {
    c.is_ascii_digit()
        || ('０'..='９').contains(&c)
        || matches!(
            c,
            '〇' | '一'
                | '二'
                | '三'
                | '四'
                | '五'
                | '六'
                | '七'
                | '八'
                | '九'
                | '十'
                | '百'
                | '千'
        )
}

/// 和暦/西暦の日付文字列を ISO (YYYY-MM-DD) にする。
/// 「◆平成24年3月30日」「令和５年12月13日」「平成元年4月1日」等を受ける。
pub fn parse_japanese_date(s: &str) -> Option<String> {
    const ERAS: [(&str, i32); 5] = [
        ("令和", 2018),
        ("平成", 1988),
        ("昭和", 1925),
        ("大正", 1911),
        ("明治", 1867),
    ];
    let s = to_ascii_digits(s);
    for (era, base) in ERAS {
        let Some(pos) = s.find(era) else { continue };
        let rest = &s[pos + era.len()..];
        let (y, rest) = take_num_until(rest, '年')?;
        let (m, rest) = take_num_until(rest, '月')?;
        let (d, _) = take_num_until(rest, '日')?;
        let year = base + y as i32;
        if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
            return None;
        }
        return Some(format!("{year:04}-{m:02}-{d:02}"));
    }
    // 西暦 "2024年4月1日"
    let pos = s.find(|c: char| c.is_ascii_digit())?;
    let (y, rest) = take_num_until(&s[pos..], '年')?;
    let (m, rest) = take_num_until(rest, '月')?;
    let (d, _) = take_num_until(rest, '日')?;
    if y < 1800 || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(format!("{y:04}-{m:02}-{d:02}"))
}

fn take_num_until(s: &str, end: char) -> Option<(u32, &str)> {
    let s = s.trim_start();
    let idx = s.find(end)?;
    let raw = s[..idx].trim();
    let n = if raw == "元" { 1 } else { parse_number(raw)? };
    Some((n, &s[idx + end.len_utf8()..]))
}

/// 例規番号「条例第34号」「北海道告示第1822号」から種別 (条例/規則/…) を取り出す。
pub fn reiki_kind(number: &str) -> Option<String> {
    let idx = number.find('第')?;
    let k = number[..idx].trim();
    if k.is_empty() {
        None
    } else {
        Some(k.to_string())
    }
}

/// 例規番号を本文先頭のヘッダから拾う（「規則第20号」等）。
pub fn extract_reiki_number(text: &str) -> Option<String> {
    const KINDS: [&str; 9] = [
        "条例", "規則", "規程", "要綱", "要領", "訓令", "告示", "公告", "規約",
    ];
    let mut best: Option<(usize, String)> = None;
    for kind in KINDS {
        let needle = format!("{kind}第");
        if let Some(pos) = text.find(&needle) {
            let rest = &text[pos + needle.len()..];
            let num: String = rest.chars().take_while(|c| is_num_char(*c)).collect();
            if !num.is_empty()
                && rest[num.len()..].starts_with('号')
                && best.as_ref().is_none_or(|(p, _)| pos < *p)
            {
                best = Some((pos, format!("{kind}第{}号", to_ascii_digits(&num))));
            }
        }
    }
    best.map(|(_, s)| s)
}

/// 行頭の条・項・号記号の分類結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineKind {
    /// 「第1条」「第8条の2」
    Article(String),
    /// 「第1章 総則」等の章節見出し
    Heading,
    /// 「(趣旨)」のような見出しだけの行
    Caption(String),
    /// 「附則」見出し
    Supplementary,
    /// 「2　」項番号
    Paragraph(String),
    /// 号。level 1 =「(1)」「一」, 2 =「ア」, 3 =「(ア)」
    Item(String, u8),
    Text,
}

/// 番号部と本文を分ける区切り（全角/半角空白）を探して (番号, 本文) にする。
fn split_head(line: &str) -> Option<(&str, &str)> {
    let idx = line.find(['\u{3000}', ' '])?;
    let (h, rest) = line.split_at(idx);
    Some((h, rest.trim_start_matches(['\u{3000}', ' '])))
}

/// 見出し「（趣旨）」判定。全体が括弧で閉じていて短いもの。
fn as_caption(line: &str) -> Option<String> {
    let t = line.trim();
    let open = t.starts_with('（') || t.starts_with('(');
    let close = t.ends_with('）') || t.ends_with(')');
    if open && close && t.chars().count() <= 40 {
        let inner = t
            .trim_start_matches(['（', '('])
            .trim_end_matches(['）', ')'])
            .trim();
        // 改正履歴「(平成24条例21・一部改正)」は見出しではない。
        if inner.is_empty()
            || inner.contains("改正")
            || inner.contains('・') && inner.contains("条例")
        {
            return None;
        }
        return Some(inner.to_string());
    }
    None
}

/// 「第N条(のM)*」なら条番号文字列を返す。
pub fn article_head(h: &str) -> Option<String> {
    let rest = h.strip_prefix('第')?;
    let n: String = rest.chars().take_while(|c| is_num_char(*c)).collect();
    if n.is_empty() {
        return None;
    }
    let mut rest = &rest[n.len()..];
    rest = rest.strip_prefix('条')?;
    while let Some(r) = rest.strip_prefix('の') {
        let m: String = r.chars().take_while(|c| is_num_char(*c)).collect();
        if m.is_empty() {
            return None;
        }
        rest = &r[m.len()..];
    }
    if rest.is_empty() {
        Some(h.to_string())
    } else {
        None
    }
}

fn is_heading_head(h: &str) -> bool {
    let Some(rest) = h.strip_prefix('第') else {
        return false;
    };
    let n: String = rest.chars().take_while(|c| is_num_char(*c)).collect();
    if n.is_empty() {
        return false;
    }
    matches!(
        rest[n.len()..].chars().next(),
        Some('章' | '節' | '款' | '目' | '編')
    )
}

const KATAKANA_ITEMS: &str =
    "アイウエオカキクケコサシスセソタチツテトナニヌネノハヒフヘホマミムメモヤユヨラリルレロワヲン";

fn item_level(h: &str) -> Option<u8> {
    let inner_paren = h
        .strip_prefix(['（', '('])
        .and_then(|r| r.strip_suffix(['）', ')']));
    if let Some(inner) = inner_paren {
        if !inner.is_empty() && inner.chars().all(is_num_char) {
            return Some(1);
        }
        if inner.chars().count() == 1 && KATAKANA_ITEMS.contains(inner) {
            return Some(3);
        }
        return None;
    }
    if !h.is_empty()
        && h.chars().all(|c| {
            matches!(
                c,
                '一' | '二' | '三' | '四' | '五' | '六' | '七' | '八' | '九' | '十' | '百'
            )
        })
    {
        return Some(1);
    }
    if h.chars().count() == 1 && KATAKANA_ITEMS.contains(h) {
        return Some(2);
    }
    None
}

/// 1 行を分類する。戻り値の 2 要素目は番号を除いた本文。
pub fn classify_line(line: &str) -> (LineKind, String) {
    let t = line.trim();
    if let Some(c) = as_caption(t) {
        return (LineKind::Caption(c), String::new());
    }
    let compact: String = t.chars().filter(|c| !c.is_whitespace()).collect();
    if compact == "附則" || compact.starts_with("附則(") || compact.starts_with("附則（") {
        return (LineKind::Supplementary, t.to_string());
    }
    if let Some((h, body)) = split_head(t) {
        if let Some(a) = article_head(h) {
            return (LineKind::Article(a), body.to_string());
        }
        if is_heading_head(h) {
            return (LineKind::Heading, t.to_string());
        }
        if !h.is_empty()
            && h.chars()
                .all(|c| c.is_ascii_digit() || ('０'..='９').contains(&c))
        {
            return (LineKind::Paragraph(to_ascii_digits(h)), body.to_string());
        }
        if let Some(level) = item_level(h) {
            return (LineKind::Item(h.to_string(), level), body.to_string());
        }
    } else if let Some(a) = article_head(t) {
        // 「第3条　削除」以外に、番号だけの行（本文が次要素）もある。
        return (LineKind::Article(a), String::new());
    } else if is_heading_head(t) {
        return (LineKind::Heading, t.to_string());
    }
    (LineKind::Text, t.to_string())
}

/// 条番号「第8条の2」→ ID 用 "8_2"。漢数字も算用数字にする。
pub fn article_key(article_no: &str) -> Option<String> {
    let rest = article_no.strip_prefix('第')?;
    let (main, branches) = rest.split_once('条')?;
    let mut parts = vec![parse_number(main)?.to_string()];
    for b in branches.split('の').skip(1) {
        parts.push(parse_number(b)?.to_string());
    }
    Some(parts.join("_"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(
            parse_japanese_date("◆平成24年3月30日").as_deref(),
            Some("2012-03-30")
        );
        assert_eq!(
            parse_japanese_date("令和５年12月13日").as_deref(),
            Some("2023-12-13")
        );
        assert_eq!(
            parse_japanese_date("平成元年4月1日").as_deref(),
            Some("1989-04-01")
        );
        assert_eq!(
            parse_japanese_date("昭和３５年１０月３日").as_deref(),
            Some("1960-10-03")
        );
        assert_eq!(
            parse_japanese_date("（千葉市例規集 現在 令和8年7月1日）").as_deref(),
            Some("2026-07-01")
        );
        assert_eq!(parse_japanese_date("なし"), None);
    }

    #[test]
    fn numbers() {
        assert_eq!(kanji_to_number("二十三"), Some(23));
        assert_eq!(kanji_to_number("百五"), Some(105));
        assert_eq!(kanji_to_number("十"), Some(10));
        assert_eq!(parse_number("１２"), Some(12));
        assert_eq!(article_key("第8条の2").as_deref(), Some("8_2"));
        assert_eq!(article_key("第十二条").as_deref(), Some("12"));
        assert_eq!(article_key("第１条").as_deref(), Some("1"));
    }

    #[test]
    fn reiki_numbers() {
        assert_eq!(
            extract_reiki_number("平成16年9月29日　条例第34号").as_deref(),
            Some("条例第34号")
        );
        assert_eq!(
            extract_reiki_number("令和５年12月13日条例第２３号").as_deref(),
            Some("条例第23号")
        );
        assert_eq!(
            reiki_kind("北海道告示第1822号").as_deref(),
            Some("北海道告示")
        );
    }

    #[test]
    fn classify() {
        assert_eq!(
            classify_line("第１条　この条例は…").0,
            LineKind::Article("第１条".into())
        );
        assert_eq!(
            classify_line("第8条の2 市長は").0,
            LineKind::Article("第8条の2".into())
        );
        assert_eq!(
            classify_line("２　市長は").0,
            LineKind::Paragraph("2".into())
        );
        assert_eq!(
            classify_line("(１)　当該命令").0,
            LineKind::Item("(１)".into(), 1)
        );
        assert_eq!(
            classify_line("一　公の秩序").0,
            LineKind::Item("一".into(), 1)
        );
        assert_eq!(classify_line("ア　市長").0, LineKind::Item("ア".into(), 2));
        assert_eq!(
            classify_line("(ア)　市長").0,
            LineKind::Item("(ア)".into(), 3)
        );
        assert_eq!(
            classify_line("（目的）").0,
            LineKind::Caption("目的".into())
        );
        assert_eq!(classify_line("附　則").0, LineKind::Supplementary);
        assert_eq!(
            classify_line("附　則（令和７年12月10日条例第36号）").0,
            LineKind::Supplementary
        );
        assert_eq!(classify_line("第2章　指定管理者").0, LineKind::Heading);
        assert_eq!(classify_line("(平成24条例21・一部改正)").0, LineKind::Text);
        assert_eq!(
            classify_line("この条例は、公布の日から施行する。").0,
            LineKind::Text
        );
    }
}

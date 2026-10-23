//! 本文中の法令参照 (`法令名 + 第N条`) を法令 ID に解決する共通処理。判例・質問主意書で共用する。
//!
//! - 法令名は `public/laws/index.json` の題名と、よく使われる略称 ([`ALIASES`]) の辞書で
//!   最長一致する。
//! - 民法・刑法のような短い法令名は一般語と衝突しうるため、直後に条番号が続く場合だけ採用する。
//!   条番号の無い言及を拾うかは呼び出し側が `bare_min_chars` で決める (長い題名だけ許す)。
//! - 法令名と条番号の間の法令番号の括弧書き (`…法（昭和二十六年政令第三百十九号）第五条`) は飛ばす。
//! - 「同法第N条」は直前に出た法令の条として扱う。
//!
//! LLM は呼ばない。

use anyhow::{Context, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// 判決文・質問主意書で多用される略称・通称 → 正式名。
pub(crate) const ALIASES: &[(&str, &str)] = &[
    ("憲法", "日本国憲法"),
    ("刑訴法", "刑事訴訟法"),
    ("民訴法", "民事訴訟法"),
    ("行訴法", "行政事件訴訟法"),
    ("国賠法", "国家賠償法"),
    ("労基法", "労働基準法"),
    ("労契法", "労働契約法"),
    ("道交法", "道路交通法"),
    ("独禁法", "私的独占の禁止及び公正取引の確保に関する法律"),
    ("独占禁止法", "私的独占の禁止及び公正取引の確保に関する法律"),
    ("入管法", "出入国管理及び難民認定法"),
    ("入管難民法", "出入国管理及び難民認定法"),
    (
        "障害者差別解消法",
        "障害を理由とする差別の解消の推進に関する法律",
    ),
    ("個人情報保護法", "個人情報の保護に関する法律"),
    ("情報公開法", "行政機関の保有する情報の公開に関する法律"),
    ("公選法", "公職選挙法"),
];

/// 照応「同法」を表すパターン (法令 ID の代わりに空文字を入れる)。
const ANAPHORA: &str = "同法";

pub(crate) struct LawMatcher {
    ac: aho_corasick::AhoCorasick,
    /// パターンごとの法令 ID。照応パターンは空文字。
    ids: Vec<String>,
}

impl LawMatcher {
    /// `public/laws/index.json` から辞書を作る。
    pub fn from_index(public: &Path) -> Result<Self> {
        let index_path = public.join("laws").join("index.json");
        let v: serde_json::Value = serde_json::from_slice(
            &std::fs::read(&index_path)
                .with_context(|| format!("read {}", index_path.display()))?,
        )?;
        let laws: Vec<(String, String)> = v["laws"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|law| {
                        (
                            law["law_id"].as_str().unwrap_or("").to_string(),
                            law["title"].as_str().unwrap_or("").to_string(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self::new(&laws)
    }

    /// (law_id, 題名) の組から辞書を作る。索引に正式名があるものだけ略称も足す。
    /// 最長一致なので正式名と略称が重なっても正式名が優先される。
    pub fn new(laws: &[(String, String)]) -> Result<Self> {
        let mut titles: Vec<String> = Vec::new();
        let mut ids: Vec<String> = Vec::new();
        for (id, title) in laws {
            if id.is_empty() || title.chars().count() < 2 {
                continue;
            }
            ids.push(id.clone());
            titles.push(title.clone());
            for (alias, official) in ALIASES {
                if official == title && alias != official {
                    ids.push(id.clone());
                    titles.push((*alias).to_string());
                }
            }
        }
        ids.push(String::new());
        titles.push(ANAPHORA.to_string());
        let ac = aho_corasick::AhoCorasick::builder()
            .match_kind(aho_corasick::MatchKind::LeftmostLongest)
            .build(&titles)?;
        Ok(Self { ac, ids })
    }

    /// 本文から法令ごとの参照条番号 (例: "709", "5_2") を抽出する。
    ///
    /// `bare_min_chars` が Some(n) なら、条番号を伴わない n 文字以上の法令名も
    /// 法令単位の言及 (条番号集合が空) として拾う。None なら条番号付きの参照だけ。
    pub fn scan(
        &self,
        text: &str,
        bare_min_chars: Option<usize>,
    ) -> BTreeMap<String, BTreeSet<String>> {
        let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut last_law: Option<&str> = None;
        for m in self.ac.find_iter(text) {
            let id = self.ids[m.pattern().as_usize()].as_str();
            let law = if id.is_empty() {
                match last_law {
                    Some(l) => l,
                    None => continue,
                }
            } else {
                id
            };
            let after = skip_law_number(&text[m.end()..]);
            let arts = parse_article_list(after);
            if arts.is_empty() {
                let name_chars = text[m.start()..m.end()].chars().count();
                if !id.is_empty() && bare_min_chars.is_some_and(|n| name_chars >= n) {
                    out.entry(law.to_string()).or_default();
                    last_law = Some(law);
                }
                continue;
            }
            out.entry(law.to_string()).or_default().extend(arts);
            last_law = Some(law);
        }
        out
    }

    /// 条番号付きの参照を (law_id, 条番号) の組で返す。
    pub fn article_refs(&self, text: &str) -> BTreeSet<(String, String)> {
        self.scan(text, None)
            .into_iter()
            .flat_map(|(law, arts)| arts.into_iter().map(move |a| (law.clone(), a)))
            .collect()
    }
}

/// 法令名＋該当条文の文字列を法令 ID と条番号に解決する公開窓口。
///
/// 他クレート (lawpub-cli の build-json など) が [`LawMatcher`] の辞書を使って、
/// 文書に添えられた関連法令 (「銀行法」「第10条第２項、第12条」) を解決するためのもの。
pub struct LawRefResolver {
    matcher: LawMatcher,
}

impl LawRefResolver {
    /// 判例等の本文から、条番号を伴う法令参照だけを抽出する。
    pub fn article_refs(&self, text: &str) -> BTreeSet<(String, String)> {
        self.matcher.article_refs(text)
    }

    /// `public/laws/index.json` から辞書を作る。
    pub fn from_index(public: &Path) -> Result<Self> {
        Ok(Self {
            matcher: LawMatcher::from_index(public)?,
        })
    }

    pub fn new(laws: &[(String, String)]) -> Result<Self> {
        Ok(Self {
            matcher: LawMatcher::new(laws)?,
        })
    }

    /// 法令名 `name` (と該当条文 `provisions`) を (law_id, 条番号) に解決する。
    /// 法令名が索引に無ければ None。条文が読めなければ条番号は空。
    pub fn resolve(&self, name: &str, provisions: Option<&str>) -> Option<(String, Vec<String>)> {
        let text = format!("{name}{}", provisions.unwrap_or(""));
        let found = self.matcher.scan(&text, Some(0));
        // 先頭に名前が一致した法令を優先する (条文側に別法令の名前が出ることがあるため)。
        let first = self.matcher.scan(name, Some(0)).into_keys().next()?;
        let arts = found.get(&first).cloned().unwrap_or_default();
        Some((first, arts.into_iter().collect()))
    }
}

fn kanji_digit(c: char) -> Option<u32> {
    "〇一二三四五六七八九"
        .chars()
        .position(|k| k == c)
        .map(|i| i as u32)
}

/// `１４` / `14` / `七百九` / `三百八十二` → 数値。解釈できなければ None。
pub(crate) fn parse_number(s: &str) -> Option<u32> {
    if s.is_empty() {
        return None;
    }
    if s.chars()
        .all(|c| c.is_ascii_digit() || ('０'..='９').contains(&c))
    {
        let ascii: String = s
            .chars()
            .map(|c| match c {
                '０'..='９' => char::from_u32(c as u32 - '０' as u32 + '0' as u32).unwrap(),
                c => c,
            })
            .collect();
        return ascii.parse().ok();
    }
    let (mut total, mut cur) = (0u32, 0u32);
    for c in s.chars() {
        if let Some(d) = kanji_digit(c) {
            cur = cur * 10 + d; // 〇一二… の位取り表記
        } else {
            let unit = match c {
                '十' => 10,
                '百' => 100,
                '千' => 1000,
                _ => return None,
            };
            total += if cur == 0 { 1 } else { cur } * unit;
            cur = 0;
        }
    }
    Some(total + cur)
}

fn is_num_char(c: char) -> bool {
    c.is_ascii_digit() || ('０'..='９').contains(&c) || "〇一二三四五六七八九十百千".contains(c)
}

fn take_num(s: &str) -> &str {
    let end = s
        .char_indices()
        .find(|(_, c)| !is_num_char(*c))
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    &s[..end]
}

/// `text` 先頭の `第?N条(のM)*` を読み、(条番号文字列, 消費バイト数) を返す。
pub(crate) fn parse_article_ref(text: &str) -> Option<(String, usize)> {
    let mut rest = text.strip_prefix('第').unwrap_or(text);
    let n = take_num(rest);
    let mut art = parse_number(n)?.to_string();
    rest = &rest[n.len()..];
    rest = rest.strip_prefix('条')?;
    // 枝番「条の二」「条の2の3」
    while let Some(r) = rest.strip_prefix('の') {
        let n = take_num(r);
        match parse_number(n) {
            Some(v) => {
                art.push('_');
                art.push_str(&v.to_string());
                rest = &r[n.len()..];
            }
            None => break,
        }
    }
    Some((art, text.len() - rest.len()))
}

/// 条に続く `第N項` `第N号` を読み飛ばす。
fn skip_paragraphs(mut s: &str) -> &str {
    loop {
        let r = s.strip_prefix('第').unwrap_or(s);
        let n = take_num(r);
        if n.is_empty() {
            return s;
        }
        match r[n.len()..].strip_prefix(['項', '号']) {
            Some(rest) => s = rest,
            None => return s,
        }
    }
}

/// 法令名の直後の法令番号の括弧書き `（昭和二十六年政令第三百十九号…）` を飛ばす。
fn skip_law_number(s: &str) -> &str {
    let Some(inner) = s.strip_prefix(['（', '(']) else {
        return s;
    };
    // 括弧内は短い。入れ子 (「以下「入管法」という。」) は無いものとして最初の閉じ括弧まで。
    let Some(close) = inner
        .char_indices()
        .take(80)
        .find(|(_, c)| *c == '）' || *c == ')')
    else {
        return s;
    };
    let body = &inner[..close.0];
    if body.contains('号') && body.contains('第') {
        &inner[close.0 + close.1.len_utf8()..]
    } else {
        s
    }
}

/// `第N条…、第M条…及び第K条` のように並ぶ条番号をまとめて読む。
/// 先頭が条番号でなければ空。
fn parse_article_list(mut after: &str) -> Vec<String> {
    let mut out = Vec::new();
    while let Some((art, used)) = parse_article_ref(after) {
        out.push(art);
        after = skip_paragraphs(&after[used..]);
        let next = [
            "、",
            ",",
            "，",
            "・",
            "及び",
            "並びに",
            "又は",
            "若しくは",
            "から",
        ]
        .iter()
        .find_map(|sep| after.strip_prefix(sep));
        match next {
            Some(r) if parse_article_ref(r).is_some() => after = r,
            _ => break,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matcher(laws: &[(&str, &str)]) -> LawMatcher {
        let laws: Vec<(String, String)> = laws
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        LawMatcher::new(&laws).unwrap()
    }

    #[test]
    fn numbers() {
        assert_eq!(parse_number("１４"), Some(14));
        assert_eq!(parse_number("709"), Some(709));
        assert_eq!(parse_number("七百九"), Some(709));
        assert_eq!(parse_number("三百八十二"), Some(382));
        assert_eq!(parse_number("十"), Some(10));
        assert_eq!(parse_number("二〇"), Some(20));
    }

    #[test]
    fn article_refs() {
        assert_eq!(
            parse_article_ref("第七百九条により"),
            Some(("709".into(), "第七百九条".len()))
        );
        assert_eq!(parse_article_ref("14条"), Some(("14".into(), "14条".len())));
        assert_eq!(
            parse_article_ref("５条の２の３、"),
            Some(("5_2_3".into(), "５条の２の３".len()))
        );
        assert_eq!(parse_article_ref("について"), None);
    }

    #[test]
    fn short_titles_need_article() {
        let m = matcher(&[("M", "民法"), ("S", "借地借家法"), ("K", "刑事訴訟法")]);
        let got: Vec<_> = m
            .article_refs("借地借家法14条、民法709条、710条 民法の趣旨 刑事訴訟法３８２条")
            .into_iter()
            .collect();
        assert_eq!(
            got,
            vec![
                ("K".to_string(), "382".to_string()),
                ("M".to_string(), "709".to_string()),
                ("M".to_string(), "710".to_string()),
                ("S".to_string(), "14".to_string()),
            ]
        );
    }

    #[test]
    fn law_number_paren_paragraphs_and_anaphora() {
        let m = matcher(&[("NYU", "出入国管理及び難民認定法"), ("KOK", "国会法")]);
        let r = m.scan(
            "出入国管理及び難民認定法（昭和二十六年政令第三百十九号）第五条第一項第九号及び第七条の二\
             について、同法第二十四条の規定は",
            None,
        );
        let arts: Vec<_> = r["NYU"].iter().cloned().collect();
        assert_eq!(arts, vec!["24", "5", "7_2"]);
        // 照応は直前の法令が無ければ捨てる。
        assert!(m.scan("同法第三条", None).is_empty());
        // 「同法人」は条番号が続かないので拾わない。
        assert_eq!(m.scan("国会法第七十四条 同法人", None)["KOK"].len(), 1);
    }

    #[test]
    fn resolver_resolves_name_and_provisions() {
        let laws = vec![
            ("GINKO".to_string(), "銀行法".to_string()),
            ("KINSHO".to_string(), "金融商品取引法".to_string()),
            ("KINSHO_REI".to_string(), "金融商品取引法施行令".to_string()),
        ];
        let r = LawRefResolver::new(&laws).unwrap();
        assert_eq!(
            r.resolve("銀行法", Some("第10条第２項、第12条")),
            Some((
                "GINKO".to_string(),
                vec!["10".to_string(), "12".to_string()]
            ))
        );
        assert_eq!(
            r.resolve("金融商品取引法", Some("第77条の３第１項等")),
            Some(("KINSHO".to_string(), vec!["77_3".to_string()]))
        );
        assert_eq!(
            r.resolve("金融商品取引法施行令", None),
            Some(("KINSHO_REI".to_string(), vec![]))
        );
        assert_eq!(r.resolve("保険業法", None), None);
    }

    #[test]
    fn bare_mentions_only_for_long_titles() {
        let m = matcher(&[
            ("M", "民法"),
            ("NYU", "出入国管理及び難民認定法"),
            ("SABETSU", "障害を理由とする差別の解消の推進に関する法律"),
        ]);
        let r = m.scan(
            "出入国管理及び難民認定法に基づく 民法の趣旨 障害者差別解消法の趣旨",
            Some(6),
        );
        assert!(r.contains_key("NYU"));
        assert!(r["NYU"].is_empty());
        assert!(
            r.contains_key("SABETSU"),
            "略称も 6 文字以上なら拾う: {r:?}"
        );
        assert!(!r.contains_key("M"));
        // bare 無効なら条番号付きのみ。
        assert!(m.scan("出入国管理及び難民認定法に基づく", None).is_empty());
    }
}

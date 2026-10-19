//! 法令 → 条約 クロスリンク。
//!
//! 法令本文 (`public/laws/{law_id}/current.json` の題名・本則・附則) から条約への参照を拾い、
//! `public/links/law-to-treaties/{law_id}.json` と逆引きの
//! `public/links/treaty-to-laws/{treaty_id}.json` を書き出す。
//!
//! 辞書は `public/treaties/index.json` (外務省の条約データ) から作る静的なもので、
//! aho-corasick (leftmost-longest) で照合する。LLM は呼ばない。
//!
//! - **正式名称**: 法令は条約を「国際的な子の奪取の民事上の側面に関する条約（平成二十六年
//!   条約第二号）」のように正式名称で引く。[`MIN_TITLE_CHARS`] 文字以上の正式名称を採る。
//!   同名の条約が複数あるとき (改定された万国郵便条約など) は、直後の括弧書きの条約番号で
//!   決め、無ければ最も新しく効力を生じたものを `ambiguous` 付きで採る。
//! - **条約番号**: 「平成二十六年条約第二号」(公布年と番号) 単独でも引く。番号は国会提出条約
//!   ページ (2002 年以降) から取れたものだけ。
//! - **略称**: 「日・ウクライナ租税条約」のような略称は、[`MIN_SHORT_TITLE_CHARS`] 文字以上で
//!   条約/協定/議定書/規約/憲章で終わり、他の条約と紛れないものだけ。

use aho_corasick::{AhoCorasick, MatchKind};
use anyhow::{Context, Result};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

/// 正式名称として照合する最小文字数 (「国際連合憲章」「万国郵便条約」が 6 文字)。
pub const MIN_TITLE_CHARS: usize = 6;
/// 略称として照合する最小文字数。
pub const MIN_SHORT_TITLE_CHARS: usize = 7;

#[derive(Debug, Clone, Deserialize)]
struct TreatyEntry {
    id: String,
    title: String,
    #[serde(default)]
    short_title: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    effective_on: Option<String>,
    #[serde(default)]
    promulgated_on: Option<String>,
    #[serde(default)]
    treaty_number: Option<u32>,
    #[serde(default)]
    treaty_number_label: Option<String>,
    #[serde(default)]
    source_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LinkedTreaty {
    pub treaty_id: String,
    pub title: String,
    pub short_title: Option<String>,
    pub kind: Option<String>,
    pub effective_on: Option<String>,
    pub treaty_number_label: Option<String>,
    /// 参照した条 (`art_2`、附則は `suppl1_art_1`、条に分かれない附則は `suppl1`)。
    pub articles: Vec<String>,
    /// 法令の題名に条約名が入っている (条約の実施法など)。
    pub in_title: bool,
    /// `title` / `number` / `short_title`。
    pub matched_by: Vec<String>,
    /// 同名の条約が複数あり、条約番号で決められなかった。
    pub ambiguous: bool,
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LawToTreaties {
    pub schema_version: u32,
    pub law_id: String,
    pub linked_treaties: Vec<LinkedTreaty>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LinkedLaw {
    pub law_id: String,
    pub title: String,
    pub articles: Vec<String>,
    pub in_title: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreatyToLaws {
    pub schema_version: u32,
    pub treaty_id: String,
    pub linked_laws: Vec<LinkedLaw>,
}

// ── 正規化・数字 ─────────────────────────────────────────────────

/// 照合用: 空白を除き、全角英数字を半角にする (条約データ側の題名と同じ正規化)。
fn normalize(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
            c => c,
        })
        .filter(|c| !c.is_whitespace())
        .collect()
}

/// 漢数字 (法令の書き方: 十一、二十、百五、千九百七十三)。
pub fn kanji_number(n: u32) -> String {
    const D: [&str; 10] = ["〇", "一", "二", "三", "四", "五", "六", "七", "八", "九"];
    if n == 0 {
        return D[0].to_string();
    }
    let mut s = String::new();
    let mut rest = n;
    for (unit, name) in [(10000, "万"), (1000, "千"), (100, "百"), (10, "十")] {
        let q = rest / unit;
        rest %= unit;
        if q == 0 {
            continue;
        }
        if unit == 10000 {
            s.push_str(&kanji_number(q));
        } else if q > 1 {
            s.push_str(D[q as usize]);
        }
        s.push_str(name);
    }
    if rest > 0 {
        s.push_str(D[rest as usize]);
    }
    s
}

/// ISO 日付 → 「平成二十六年」(元年は「元年」)。
fn wareki_year_kanji(iso: &str) -> Option<String> {
    let y: i32 = iso.get(0..4)?.parse().ok()?;
    let md = iso.get(5..10)?;
    let (era, base) = if (y, md) >= (2019, "05-01") {
        ("令和", 2018)
    } else if (y, md) >= (1989, "01-08") {
        ("平成", 1988)
    } else if (y, md) >= (1926, "12-25") {
        ("昭和", 1925)
    } else if (y, md) >= (1912, "07-30") {
        ("大正", 1911)
    } else {
        ("明治", 1867)
    };
    let n = (y - base) as u32;
    let year = if n == 1 {
        "元".to_string()
    } else {
        kanji_number(n)
    };
    Some(format!("{era}{year}年"))
}

/// 「平成二十六年条約第二号」。
fn number_pattern(t: &TreatyEntry) -> Option<String> {
    let n = t.treaty_number?;
    let y = wareki_year_kanji(t.promulgated_on.as_deref()?)?;
    Some(format!("{y}条約第{}号", kanji_number(n)))
}

// ── 辞書 ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PatKind {
    Title,
    Number,
    ShortTitle,
}

struct Matcher {
    ac: AhoCorasick,
    /// パターンごとの (種類, 候補の条約 index)。
    pats: Vec<(PatKind, Vec<usize>)>,
    /// 条約 index → 照合用の条約番号 (「平成二十六年条約第二号」)。
    numbers: Vec<Option<String>>,
    treaties: Vec<TreatyEntry>,
}

impl Matcher {
    fn new(treaties: Vec<TreatyEntry>) -> Result<Self> {
        let mut by_pat: BTreeMap<String, (PatKind, Vec<usize>)> = BTreeMap::new();
        let numbers: Vec<Option<String>> = treaties.iter().map(number_pattern).collect();
        for (i, t) in treaties.iter().enumerate() {
            let title = normalize(&t.title);
            if title.chars().count() >= MIN_TITLE_CHARS {
                by_pat
                    .entry(title)
                    .or_insert((PatKind::Title, vec![]))
                    .1
                    .push(i);
            }
            if let Some(n) = &numbers[i] {
                by_pat
                    .entry(n.clone())
                    .or_insert((PatKind::Number, vec![]))
                    .1
                    .push(i);
            }
        }
        // 略称は他の条約の正式名称・略称と紛れないものだけ。
        let mut shorts: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, t) in treaties.iter().enumerate() {
            let Some(s) = t.short_title.as_deref().map(normalize) else {
                continue;
            };
            if s == normalize(&t.title)
                || s.chars().count() < MIN_SHORT_TITLE_CHARS
                || !["条約", "協定", "議定書", "規約", "憲章"]
                    .iter()
                    .any(|w| s.ends_with(w))
            {
                continue;
            }
            shorts.entry(s).or_default().push(i);
        }
        for (s, idx) in shorts {
            if idx.len() == 1 && !by_pat.contains_key(&s) {
                by_pat.insert(s, (PatKind::ShortTitle, idx));
            }
        }
        let (patterns, pats): (Vec<String>, Vec<(PatKind, Vec<usize>)>) =
            by_pat.into_iter().unzip();
        let ac = AhoCorasick::builder()
            .match_kind(MatchKind::LeftmostLongest)
            .build(&patterns)
            .context("build treaty automaton")?;
        Ok(Self {
            ac,
            pats,
            numbers,
            treaties,
        })
    }

    /// テキスト中の参照: 条約 index → (照合の種類, 曖昧でない照合があったか)。
    fn scan(&self, text: &str) -> BTreeMap<usize, (BTreeSet<&'static str>, bool)> {
        let text = normalize(text);
        let mut out: BTreeMap<usize, (BTreeSet<&'static str>, bool)> = BTreeMap::new();
        for m in self.ac.find_iter(&text) {
            let (kind, cands) = &self.pats[m.pattern().as_usize()];
            let (pick, ambiguous, how) = match kind {
                PatKind::Number => (cands.clone(), false, "number"),
                PatKind::ShortTitle => (cands.clone(), false, "short_title"),
                PatKind::Title if cands.len() == 1 => (cands.clone(), false, "title"),
                PatKind::Title => {
                    // 直後の括弧書きの条約番号で決める。
                    let after = &text[m.end()..];
                    let by_number: Vec<usize> = cands
                        .iter()
                        .copied()
                        .filter(|&i| {
                            self.numbers[i].as_deref().is_some_and(|n| {
                                after.starts_with(&format!("（{n}"))
                                    || after.starts_with(&format!("({n}"))
                            })
                        })
                        .collect();
                    if by_number.len() == 1 {
                        (by_number, false, "title")
                    } else {
                        let latest = cands
                            .iter()
                            .copied()
                            .max_by(|&a, &b| {
                                self.treaties[a]
                                    .effective_on
                                    .cmp(&self.treaties[b].effective_on)
                                    .then(b.cmp(&a))
                            })
                            .into_iter()
                            .collect();
                        (latest, true, "title")
                    }
                }
            };
            for i in pick {
                let e = out.entry(i).or_default();
                e.0.insert(how);
                // 一度でも曖昧でない照合があれば確定。
                e.1 |= !ambiguous;
            }
        }
        out
    }
}

// ── 法令の読み込み ───────────────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
struct Para {
    #[serde(default)]
    text: String,
}

#[derive(Debug, Default, Deserialize)]
struct Art {
    article_id: String,
    #[serde(default)]
    caption: Option<String>,
    #[serde(default)]
    paragraphs: Vec<Para>,
}

#[derive(Debug, Default, Deserialize)]
struct Suppl {
    #[serde(default)]
    index: u32,
    #[serde(default)]
    articles: Vec<Art>,
    #[serde(default)]
    paragraphs: Vec<Para>,
}

#[derive(Debug, Default, Deserialize)]
struct LawDoc {
    #[serde(default)]
    law_id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    articles: Vec<Art>,
    #[serde(default)]
    suppl_provisions: Vec<Suppl>,
}

fn art_text(a: &Art) -> String {
    let mut s = a.caption.clone().unwrap_or_default();
    for p in &a.paragraphs {
        s.push('\n');
        s.push_str(&p.text);
    }
    s
}

/// 1 法令の中の 1 条約への参照。
#[derive(Default)]
struct Hit {
    articles: BTreeSet<String>,
    in_title: bool,
    matched_by: BTreeSet<&'static str>,
    /// 曖昧でない照合があったか。
    resolved: bool,
}

/// 1 法令の参照: 条約 index → Hit。
struct LawHits {
    law_id: String,
    title: String,
    hits: BTreeMap<usize, Hit>,
}

fn scan_law(m: &Matcher, doc: &LawDoc) -> LawHits {
    let mut hits: BTreeMap<usize, Hit> = BTreeMap::new();
    let mut add = |found: BTreeMap<usize, (BTreeSet<&'static str>, bool)>,
                   article: Option<String>,
                   in_title: bool| {
        for (i, (how, resolved)) in found {
            let e = hits.entry(i).or_default();
            if let Some(a) = &article {
                e.articles.insert(a.clone());
            }
            e.in_title |= in_title;
            e.matched_by.extend(how);
            e.resolved |= resolved;
        }
    };
    add(m.scan(&doc.title), None, true);
    for a in &doc.articles {
        add(m.scan(&art_text(a)), Some(a.article_id.clone()), false);
    }
    for sp in &doc.suppl_provisions {
        for a in &sp.articles {
            add(m.scan(&art_text(a)), Some(a.article_id.clone()), false);
        }
        if !sp.paragraphs.is_empty() {
            let text: Vec<&str> = sp.paragraphs.iter().map(|p| p.text.as_str()).collect();
            add(
                m.scan(&text.join("\n")),
                Some(format!("suppl{}", sp.index)),
                false,
            );
        }
    }
    LawHits {
        law_id: doc.law_id.clone(),
        title: doc.title.clone(),
        hits,
    }
}

fn load_treaties(public: &Path) -> Result<Option<Vec<TreatyEntry>>> {
    let path = public.join("treaties").join("index.json");
    if !path.exists() {
        return Ok(None);
    }
    let v: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&path).with_context(|| format!("read {}", path.display()))?,
    )?;
    let list = v["treaties"].as_array().cloned().unwrap_or_default();
    Ok(Some(
        list.into_iter()
            .filter_map(|t| serde_json::from_value(t).ok())
            .collect(),
    ))
}

/// `public/` を受け取り、`links/law-to-treaties/` と `links/treaty-to-laws/` を生成する。
pub fn run_link_treaties(public: &Path) -> Result<()> {
    let Some(treaties) = load_treaties(public)? else {
        tracing::warn!(
            "no treaties/index.json under {}; skipping",
            public.display()
        );
        return Ok(());
    };
    let matcher = Matcher::new(treaties)?;
    tracing::info!(
        "link-treaties: {} treaties, {} patterns",
        matcher.treaties.len(),
        matcher.pats.len()
    );
    let laws_dir = public.join("laws");
    let mut law_dirs: Vec<std::path::PathBuf> = std::fs::read_dir(&laws_dir)
        .with_context(|| format!("read {}", laws_dir.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && !p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with('.'))
        })
        .collect();
    law_dirs.sort();
    let results: Vec<LawHits> = law_dirs
        .par_iter()
        .filter_map(|dir| {
            let bytes = std::fs::read(dir.join("current.json")).ok()?;
            let mut doc: LawDoc = serde_json::from_slice(&bytes).ok()?;
            if doc.law_id.is_empty() {
                doc.law_id = dir.file_name()?.to_str()?.to_string();
            }
            let h = scan_law(&matcher, &doc);
            (!h.hits.is_empty()).then_some(h)
        })
        .collect();

    let law_out = public.join("links").join("law-to-treaties");
    let treaty_out = public.join("links").join("treaty-to-laws");
    std::fs::create_dir_all(&law_out)?;
    std::fs::create_dir_all(&treaty_out)?;
    let mut reverse: BTreeMap<usize, Vec<LinkedLaw>> = BTreeMap::new();
    for h in &results {
        let mut linked: Vec<LinkedTreaty> = Vec::new();
        for (&i, hit) in &h.hits {
            let t = &matcher.treaties[i];
            linked.push(LinkedTreaty {
                treaty_id: t.id.clone(),
                title: t.title.clone(),
                short_title: t.short_title.clone(),
                kind: t.kind.clone(),
                effective_on: t.effective_on.clone(),
                treaty_number_label: t.treaty_number_label.clone(),
                articles: hit.articles.iter().cloned().collect(),
                in_title: hit.in_title,
                matched_by: hit.matched_by.iter().map(|s| s.to_string()).collect(),
                ambiguous: !hit.resolved,
                source_url: t.source_url.clone(),
            });
            reverse.entry(i).or_default().push(LinkedLaw {
                law_id: h.law_id.clone(),
                title: h.title.clone(),
                articles: hit.articles.iter().cloned().collect(),
                in_title: hit.in_title,
            });
        }
        // 題名に出る条約 → 参照する条の多い順。
        linked.sort_by(|a, b| {
            b.in_title
                .cmp(&a.in_title)
                .then(b.articles.len().cmp(&a.articles.len()))
                .then(a.treaty_id.cmp(&b.treaty_id))
        });
        let doc = LawToTreaties {
            schema_version: 1,
            law_id: h.law_id.clone(),
            linked_treaties: linked,
        };
        std::fs::write(
            law_out.join(format!("{}.json", h.law_id)),
            serde_json::to_string_pretty(&doc)?,
        )?;
    }
    for (i, mut laws) in reverse.iter_mut().map(|(i, l)| (*i, std::mem::take(l))) {
        laws.sort_by(|a, b| {
            b.in_title
                .cmp(&a.in_title)
                .then(b.articles.len().cmp(&a.articles.len()))
                .then(a.law_id.cmp(&b.law_id))
        });
        let t = &matcher.treaties[i];
        let doc = TreatyToLaws {
            schema_version: 1,
            treaty_id: t.id.clone(),
            linked_laws: laws,
        };
        std::fs::write(
            treaty_out.join(format!("{}.json", t.id)),
            serde_json::to_string_pretty(&doc)?,
        )?;
    }
    tracing::info!(
        "link-treaties: {} laws → {} treaties linked",
        results.len(),
        reverse.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kanji_numbers() {
        assert_eq!(kanji_number(2), "二");
        assert_eq!(kanji_number(10), "十");
        assert_eq!(kanji_number(11), "十一");
        assert_eq!(kanji_number(26), "二十六");
        assert_eq!(kanji_number(105), "百五");
        assert_eq!(kanji_number(1973), "千九百七十三");
        assert_eq!(
            wareki_year_kanji("2014-01-29").as_deref(),
            Some("平成二十六年")
        );
        assert_eq!(wareki_year_kanji("2019-05-01").as_deref(), Some("令和元年"));
        assert_eq!(
            wareki_year_kanji("1979-06-21").as_deref(),
            Some("昭和五十四年")
        );
    }

    fn entry(id: &str, title: &str) -> TreatyEntry {
        TreatyEntry {
            id: id.into(),
            title: title.into(),
            short_title: None,
            kind: Some("multilateral".into()),
            effective_on: None,
            promulgated_on: None,
            treaty_number: None,
            treaty_number_label: None,
            source_url: None,
        }
    }

    #[test]
    fn links_laws_end_to_end() {
        let dir = std::env::temp_dir().join(format!("lawpub_treaty_link_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("treaties")).unwrap();
        let mut hague = entry("B-H26-001", "国際的な子の奪取の民事上の側面に関する条約");
        hague.promulgated_on = Some("2014-01-29".into());
        hague.treaty_number = Some(2);
        hague.treaty_number_label = Some("平成26年条約第2号".into());
        hague.short_title = Some("ハーグ条約".into());
        let mut upu_old = entry("B-H30-003", "万国郵便条約");
        upu_old.effective_on = Some("2018-01-01".into());
        upu_old.promulgated_on = Some("2017-12-01".into());
        upu_old.treaty_number = Some(30);
        let mut upu_new = entry("B-R4-006", "万国郵便条約");
        upu_new.effective_on = Some("2022-07-01".into());
        let mut ukraine = entry(
            "diet-pagew_000001_00003",
            "所得に対する租税に関する二重課税の除去並びに脱税及び租税回避の防止のための日本国政府とウクライナ政府との間の条約",
        );
        ukraine.short_title = Some("日・ウクライナ租税条約".into());
        ukraine.kind = None;
        let base = entry(
            "A-S46-001",
            "所得に対する租税に関する二重課税の回避のための日本国とスイスとの間の条約",
        );
        let tpp = {
            let mut t = entry(
                "B-H30-008",
                "環太平洋パートナーシップに関する包括的及び先進的な協定",
            );
            t.short_title = Some("包括的・先進的ＴＰＰ協定".into());
            t
        };
        let to_json = |t: &TreatyEntry| {
            serde_json::json!({
                "id": t.id, "title": t.title, "short_title": t.short_title, "kind": t.kind,
                "effective_on": t.effective_on, "promulgated_on": t.promulgated_on,
                "treaty_number": t.treaty_number, "treaty_number_label": t.treaty_number_label,
            })
        };
        let all = [&hague, &upu_old, &upu_new, &ukraine, &base, &tpp];
        std::fs::write(
            dir.join("treaties").join("index.json"),
            serde_json::to_vec(&serde_json::json!({
                "treaties": all.iter().map(|t| to_json(t)).collect::<Vec<_>>()
            }))
            .unwrap(),
        )
        .unwrap();
        let law = |id: &str, doc: serde_json::Value| {
            std::fs::create_dir_all(dir.join("laws").join(id)).unwrap();
            std::fs::write(
                dir.join("laws").join(id).join("current.json"),
                serde_json::to_vec(&doc).unwrap(),
            )
            .unwrap();
        };
        law(
            "425AC0000000048",
            serde_json::json!({
                "law_id": "425AC0000000048",
                "title": "国際的な子の奪取の民事上の側面に関する条約の実施に関する法律",
                "articles": [
                    {"article_id": "art_1", "article_no": "1", "caption": "（目的）", "paragraphs": [
                        {"text": "この法律は、国際的な子の奪取の民事上の側面に関する条約（以下「条約」という。）の的確な実施を確保するため、…"}
                    ]},
                    {"article_id": "art_2", "article_no": "2", "caption": null, "paragraphs": [
                        {"text": "平成二十六年条約第二号の締約国をいう。"}
                    ]},
                ],
                "suppl_provisions": [
                    {"index": 1, "articles": [], "paragraphs": [
                        {"text": "この法律は、条約が日本国について効力を生ずる日から施行する。"}
                    ]}
                ]
            }),
        );
        law(
            "322AC0000000165",
            serde_json::json!({
                "law_id": "322AC0000000165",
                "title": "郵便法",
                "articles": [
                    {"article_id": "art_7", "article_no": "7", "caption": null, "paragraphs": [
                        {"text": "万国郵便条約（平成二十九年条約第三十号）に別段の定めがある場合を除き…"}
                    ]},
                    {"article_id": "art_8", "article_no": "8", "caption": null, "paragraphs": [
                        {"text": "万国郵便条約の定めるところにより…"}
                    ]}
                ]
            }),
        );
        law(
            "999AC0000000001",
            serde_json::json!({
                "law_id": "999AC0000000001",
                "title": "租税条約等の実施に伴う所得税法、法人税法及び地方税法の特例等に関する法律",
                "articles": [
                    {"article_id": "art_3", "article_no": "3", "caption": null, "paragraphs": [
                        {"text": "日・ウクライナ租税条約及び包括的・先進的TPP協定の規定による。条約の定めるところにより。"}
                    ]}
                ]
            }),
        );
        law(
            "999AC0000000002",
            serde_json::json!({"law_id": "999AC0000000002", "title": "民法", "articles": [
                {"article_id": "art_1", "article_no": "1", "caption": null, "paragraphs": [{"text": "私権は、公共の福祉に適合しなければならない。"}]}
            ]}),
        );

        run_link_treaties(&dir).unwrap();

        let read = |id: &str| -> LawToTreaties {
            serde_json::from_slice(
                &std::fs::read(dir.join("links/law-to-treaties").join(format!("{id}.json")))
                    .unwrap(),
            )
            .unwrap()
        };
        let hague_law = read("425AC0000000048");
        assert_eq!(hague_law.linked_treaties.len(), 1);
        let l = &hague_law.linked_treaties[0];
        assert_eq!(l.treaty_id, "B-H26-001");
        assert!(l.in_title);
        assert_eq!(l.articles, vec!["art_1", "art_2"]);
        assert_eq!(l.matched_by, vec!["number", "title"]);
        assert!(!l.ambiguous);

        // 同名の万国郵便条約: 括弧書きの番号で旧版、番号の無い言及は最新版 (曖昧)。
        let post = read("322AC0000000165");
        let ids: Vec<(&str, &Vec<String>, bool)> = post
            .linked_treaties
            .iter()
            .map(|l| (l.treaty_id.as_str(), &l.articles, l.ambiguous))
            .collect();
        assert!(
            ids.contains(&("B-H30-003", &vec!["art_7".to_string()], false)),
            "{ids:?}"
        );
        assert!(
            ids.contains(&("B-R4-006", &vec!["art_8".to_string()], true)),
            "{ids:?}"
        );

        // 略称 (全角/半角の違いは正規化で吸収)。
        let tax = read("999AC0000000001");
        let ids: Vec<&str> = tax
            .linked_treaties
            .iter()
            .map(|l| l.treaty_id.as_str())
            .collect();
        assert_eq!(ids, vec!["B-H30-008", "diet-pagew_000001_00003"]);
        assert!(!dir
            .join("links/law-to-treaties/999AC0000000002.json")
            .exists());

        let rev: TreatyToLaws = serde_json::from_slice(
            &std::fs::read(dir.join("links/treaty-to-laws/B-H26-001.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(rev.linked_laws[0].law_id, "425AC0000000048");
        assert!(rev.linked_laws[0].in_title);
        std::fs::remove_dir_all(&dir).ok();
    }
}

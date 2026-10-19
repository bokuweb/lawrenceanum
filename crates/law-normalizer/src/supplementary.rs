//! 附則の構造化 (#202): 施行期日・経過措置・読替え規定を [`LawDocument`] から抽出する。
//!
//! 入力は正規化済みの附則 ([`SupplProvision`]) だけで、新規取得はしない。正規表現は使わず
//! 文字列走査で読む。LLM は呼ばない。
//!
//! ## 施行期日 ([`EnforcementRule`])
//!
//! 「…から施行する」「…ヨリ之ヲ施行ス」の文と「施行期日は、政令で定める」の文を拾い、
//! 主語 (「この法律は」= 附則全体 / 「第X条の規定は」= 一部の規定) と施行日の書き方を
//! 区別する。「次の各号に掲げる規定は、当該各号に定める日から施行する」は号ごとに
//! 「対象規定　施行日」の列を読む。施行日の書き方は次の 5 種:
//!
//! | kind | 例 | date |
//! |---|---|---|
//! | `fixed_date` | 令和五年四月一日から | その日 |
//! | `promulgation` | 公布の日から | 公布日 |
//! | `after_promulgation` | 公布の日から起算して二十日を経過した日から | 公布日から計算 |
//! | `delegated` | 公布の日から起算して六月を超えない範囲内において政令で定める日から | なし (`deadline` に期限) |
//! | `linked` | 会社法の施行の日から | 括弧書きに日付があればその日 |
//!
//! 期間の計算は民法の期間計算に従う: 「公布の日から起算して」は初日を算入し、「公布後」は
//! 初日を算入しない。月・年で定めた期間は起算日の応当日の前日に満了し (応当日が無ければ
//! その月の末日)、「経過した日」は満了日の翌日。計算で出した日は `date_source = "computed"`。
//!
//! ## 経過措置 ([`TransitionalMeasure`])
//!
//! 施行期日以外の項を、「なお従前の例による」「なお効力を有する」「…とみなす」
//! 「…について適用する」などの言い回しで分類する。
//!
//! ## 読替え規定 ([`Reading`])
//!
//! 「第X条中「A」とあるのは「B」と読み替える」の対を拾う。
//!
//! ## 参照の解決 ([`Ref`])
//!
//! 「附則第X条」「前条」「次条」は同じ附則の条へ、制定時の附則 (改正法令番号が無い附則) の
//! 「第X条」は本則の条へ article id を解決する。改正附則の「第X条」は改正法自身の条を指す
//! ため (この法令の中には無い) 解決しない。

use crate::{Article, Item, LawDocument, Paragraph, SupplProvision};
use chrono::{Datelike, Duration, Months, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

pub const SUPPLEMENTARY_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SupplementaryDocument {
    pub schema_version: u32,
    pub law_id: String,
    pub law_num: Option<String>,
    pub stats: SupplementaryStats,
    pub provisions: Vec<ProvisionAnalysis>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SupplementaryStats {
    /// 附則の数。
    pub provisions: usize,
    /// 施行期日 (附則全体か一部の規定かを問わず) を 1 つ以上読み取れた附則の数。
    pub with_enforcement: usize,
    /// 施行期日を読み取れなかった附則の数 (= `provisions - with_enforcement`)。
    pub enforcement_unresolved: usize,
    pub enforcement_rules: usize,
    pub transitional_measures: usize,
    pub readings: usize,
    pub refs: usize,
    /// article id まで解決できた参照の数。
    pub refs_resolved: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProvisionAnalysis {
    /// [`SupplProvision::index`]。
    pub index: u32,
    pub amend_law_num: Option<String>,
    pub label: Option<String>,
    /// この附則を置いた法令 (制定時の附則ならこの法令、改正附則なら改正法) の公布日。
    pub promulgation_date: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enforcement: Vec<EnforcementRule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transitional: Vec<TransitionalMeasure>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub readings: Vec<Reading>,
    /// 施行期日を 1 つも読み取れなかったら true (health の未解決件数に数える)。
    pub enforcement_unresolved: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Location {
    /// 附則の条 (`suppl{index}_art_{Num}`)。条に分かれない附則なら None。
    pub article_id: Option<String>,
    /// 項の `Num` 属性。
    pub paragraph: Option<String>,
    /// 号の番号 (ItemTitle)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnforcementScope {
    /// 附則を置いた法令全体 (「この法律は」)。
    Whole,
    /// 一部の規定 (ただし書・各号・「第X条の規定は」)。
    Partial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnforcementKind {
    FixedDate,
    Promulgation,
    AfterPromulgation,
    Delegated,
    Linked,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DateSource {
    /// 条文に日付がそのまま書かれている。
    Explicit,
    /// 公布日そのもの。
    Promulgation,
    /// 公布日から期間を計算した。
    Computed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnforcementRule {
    pub location: Location,
    pub scope: EnforcementScope,
    /// 一部の規定の施行期日なら対象規定 (e.g. "附則第十六条から第十八条まで及び第十九条第一項の規定")。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub targets: Option<String>,
    /// 附則全体の施行期日から除かれる規定 (「この法律（第三条を除く。）は」の括弧内)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub excluding: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_refs: Vec<Ref>,
    pub kind: EnforcementKind,
    /// 施行日 (ISO)。政令委任など条文から決まらないときは None。
    pub date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_source: Option<DateSource>,
    /// 政令委任の期限 (「…を超えない範囲内において」「…までの間において」の最終日)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline: Option<String>,
    /// 施行日を定める法令の種類 (e.g. "政令", "勅令", "厚生労働省令")。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegated_to: Option<String>,
    /// 施行日が連動する先 (e.g. "会社法の施行の日")。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linked_to: Option<String>,
    /// 「公布の日から起算して六月」などの期間 (原文)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub period: Option<String>,
    /// 「…から施行し、平成X年X月X日から適用する」の適用開始日。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applies_from: Option<String>,
    /// 施行日の部分の原文 (e.g. "公布の日から起算して六月を超えない範囲内において政令で定める日")。
    pub when: String,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionalKind {
    /// なお従前の例による。
    PriorLaw,
    /// 施行前の行為に対する罰則の適用は従前の例による。
    Penalty,
    /// なお効力を有する。
    ContinuedEffect,
    /// …とみなす。
    Deemed,
    /// 新しい規定を…について適用する (適用区分)。
    Application,
    /// 経過措置を政令等に委任する。
    Delegated,
    /// 検討条項 (見直し規定)。
    Review,
    /// 他の法令を廃止する。
    Repeal,
    /// 見出しが経過措置だが上のいずれでもない。
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransitionalMeasure {
    pub location: Location,
    pub kinds: Vec<TransitionalKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refs: Vec<Ref>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reading {
    pub location: Location,
    /// 読み替える規定 (「第四条第十三号中」の「第四条第十三号」)。
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_refs: Vec<Ref>,
    /// 読替え前の字句。
    pub from: String,
    /// 読替え後の字句。
    pub to: String,
    /// 読み替える場面 (「旧法第十条の規定の適用については」)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefScope {
    /// この法令の本則。
    Main,
    /// 同じ附則。
    Suppl,
    /// 改正法の本則 (この法令の中には無い)。
    AmendingLaw,
    /// 法令名などで修飾された他の法令の規定 (「新法第十条」「民法第五条」)。
    OtherLaw,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ref {
    pub text: String,
    pub scope: RefScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub article_id: Option<String>,
}

/// 法令の附則をすべて解析する。
pub fn analyze(doc: &LawDocument) -> SupplementaryDocument {
    analyze_with_dates(doc, &HashMap::new())
}

/// [`analyze`] に、改正法の公布日 ([`law_num_key`] → 公布日) を補って解析する。
/// AmendLawNum に公布日が書かれていない附則 (古い改正の一部) は e-Gov の改正履歴の
/// 公布日で補い、公布日起算の施行日を計算できるようにする。
pub fn analyze_with_dates(
    doc: &LawDocument,
    promulgation_dates: &HashMap<String, NaiveDate>,
) -> SupplementaryDocument {
    let main_ids: HashSet<&str> = doc.articles.iter().map(|a| a.article_id.as_str()).collect();
    let law_promulgation = doc
        .promulgation_date
        .as_deref()
        .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());
    let provisions: Vec<ProvisionAnalysis> = doc
        .suppl_provisions
        .iter()
        .map(|sp| {
            let known = sp
                .amend_law_num
                .as_deref()
                .and_then(law_num_key)
                .and_then(|k| promulgation_dates.get(&k).copied());
            analyze_provision(sp, &main_ids, law_promulgation, known)
        })
        .collect();
    let mut stats = SupplementaryStats {
        provisions: provisions.len(),
        ..Default::default()
    };
    for p in &provisions {
        if !p.enforcement_unresolved {
            stats.with_enforcement += 1;
        }
        stats.enforcement_rules += p.enforcement.len();
        stats.transitional_measures += p.transitional.len();
        stats.readings += p.readings.len();
        let refs = p
            .enforcement
            .iter()
            .flat_map(|e| &e.target_refs)
            .chain(p.transitional.iter().flat_map(|t| &t.refs))
            .chain(p.readings.iter().flat_map(|r| &r.target_refs));
        for r in refs {
            stats.refs += 1;
            if r.article_id.is_some() {
                stats.refs_resolved += 1;
            }
        }
    }
    stats.enforcement_unresolved = stats.provisions - stats.with_enforcement;
    SupplementaryDocument {
        schema_version: SUPPLEMENTARY_SCHEMA_VERSION,
        law_id: doc.law_id.clone(),
        law_num: doc.law_num.clone(),
        stats,
        provisions,
    }
}

/// 附則全体の施行期日 (scope = whole の最初の規則)。timeline への反映に使う。
pub fn main_enforcement(p: &ProvisionAnalysis) -> Option<&EnforcementRule> {
    p.enforcement
        .iter()
        .find(|e| e.scope == EnforcementScope::Whole && e.kind != EnforcementKind::Unknown)
}

/// timeline の改正イベント (改正法令番号 `amend_law_num`、制定なら None) に添える、
/// その附則から読んだ施行期日。対応する附則が無ければ None。
pub fn timeline_enforcement(
    doc: &SupplementaryDocument,
    amend_law_num: Option<&str>,
) -> Option<serde_json::Value> {
    let p = match amend_law_num {
        None => doc.provisions.iter().find(|p| p.amend_law_num.is_none())?,
        Some(num) => {
            let key = law_num_key(num)?;
            doc.provisions.iter().find(|p| {
                p.amend_law_num.as_deref().and_then(law_num_key).as_deref() == Some(key.as_str())
            })?
        }
    };
    let main = main_enforcement(p)?;
    let partial: Vec<serde_json::Value> = p
        .enforcement
        .iter()
        .filter(|e| e.scope == EnforcementScope::Partial && e.kind != EnforcementKind::Unknown)
        .map(|e| {
            serde_json::json!({
                "kind": e.kind,
                "date": e.date,
                "deadline": e.deadline,
                "targets": e.targets,
            })
        })
        .collect();
    Some(serde_json::json!({
        "provision_index": p.index,
        "kind": main.kind,
        "date": main.date,
        "date_source": main.date_source,
        "deadline": main.deadline,
        "delegated_to": main.delegated_to,
        "linked_to": main.linked_to,
        "when": main.when,
        "partial": partial,
    }))
}

/// 法令番号を照合用のキー ("令和6|法律|33") にする。AmendLawNum の
/// "令和六年五月二四日法律第三三号" と e-Gov の改正履歴の "令和六年法律第三十三号" が同じキーになる。
pub fn law_num_key(s: &str) -> Option<String> {
    let c: Vec<char> = s.chars().collect();
    let (era, mut i) = ERAS
        .iter()
        .find_map(|(name, _)| starts_with(&c, 0, name).then(|| (*name, name.chars().count())))?;
    let (year, j) = take_number(&c, i)?;
    if c.get(j) != Some(&'年') {
        return None;
    }
    i = j + 1;
    // 公布日 ("五月二四日") があれば読み飛ばす。
    if let Some((_, j)) = take_number(&c, i) {
        if c.get(j) == Some(&'月') {
            if let Some((_, k)) = take_number(&c, j + 1) {
                if c.get(k) == Some(&'日') {
                    i = k + 1;
                }
            }
        }
    }
    let dai = (i..c.len()).find(|&k| c[k] == '第')?;
    let kind: String = c[i..dai].iter().collect();
    let (num, k) = take_number(&c, dai + 1)?;
    if c.get(k) != Some(&'号') || kind.is_empty() {
        return None;
    }
    Some(format!("{era}{year}|{kind}|{num}"))
}

// ---------------------------------------------------------------------------
// 附則 1 本の解析

/// 解析の単位 (項)。
struct Unit<'a> {
    article: Option<&'a Article>,
    paragraph: &'a Paragraph,
}

struct Ctx<'a> {
    provision: &'a SupplProvision,
    main_ids: &'a HashSet<&'a str>,
    /// 同じ附則の条 (Num → article_id)。文書順。
    suppl_articles: Vec<(String, String)>,
    promulgation: Option<NaiveDate>,
    /// 「同年」「同日」の参照先 (直前に読んだ日付)。
    last_date: Option<NaiveDate>,
}

fn analyze_provision(
    sp: &SupplProvision,
    main_ids: &HashSet<&str>,
    law_promulgation: Option<NaiveDate>,
    known_promulgation: Option<NaiveDate>,
) -> ProvisionAnalysis {
    let promulgation = match sp.amend_law_num.as_deref() {
        Some(num) => first_date(num, None)
            .or_else(|| sp.label.as_deref().and_then(|l| first_date(l, None)))
            .or(known_promulgation),
        None => law_promulgation,
    };
    let prefix = format!("suppl{}_art_", sp.index);
    let mut ctx = Ctx {
        provision: sp,
        main_ids,
        suppl_articles: sp
            .articles
            .iter()
            .filter_map(|a| {
                a.article_id
                    .strip_prefix(&prefix)
                    .map(|n| (n.to_string(), a.article_id.clone()))
            })
            .collect(),
        promulgation,
        last_date: promulgation,
    };
    let units = sp
        .articles
        .iter()
        .flat_map(|a| {
            a.paragraphs.iter().map(move |p| Unit {
                article: Some(a),
                paragraph: p,
            })
        })
        .chain(sp.paragraphs.iter().map(|p| Unit {
            article: None,
            paragraph: p,
        }));

    let mut enforcement = Vec::new();
    let mut transitional = Vec::new();
    let mut readings = Vec::new();
    for unit in units {
        let location = Location {
            article_id: unit.article.map(|a| a.article_id.clone()),
            paragraph: unit.paragraph.paragraph_num.clone(),
            item: None,
        };
        let caption = unit.article.and_then(|a| a.caption.clone());
        let mut rest: Vec<String> = Vec::new();
        for s in sentences(unit.paragraph) {
            if let Some(rules) = enforcement_rules(&s, &unit, &location, &mut ctx) {
                enforcement.extend(rules);
                continue;
            }
            if s.contains("とあるのは") {
                readings.extend(extract_readings(&s, &location, &ctx));
            }
            rest.push(s);
        }
        if rest.is_empty() {
            continue;
        }
        let text = rest.join("");
        let mut kinds = transitional_kinds(&text);
        if kinds.is_empty()
            && !text.contains("とあるのは")
            && caption.as_deref().is_some_and(|c| c.contains("経過措置"))
        {
            kinds.push(TransitionalKind::Other);
        }
        if kinds.is_empty() {
            continue;
        }
        let mut full = text;
        for line in item_lines(&unit.paragraph.items) {
            full.push('\n');
            full.push_str(&line);
        }
        transitional.push(TransitionalMeasure {
            refs: extract_refs(&full, unit.article, &ctx),
            location,
            kinds,
            caption,
            text: full,
        });
    }
    let enforcement_unresolved = !enforcement
        .iter()
        .any(|e| e.kind != EnforcementKind::Unknown);
    ProvisionAnalysis {
        index: sp.index,
        amend_law_num: sp.amend_law_num.clone(),
        label: sp.label.clone(),
        promulgation_date: promulgation.map(iso),
        enforcement,
        transitional,
        readings,
        enforcement_unresolved,
    }
}

/// 項の文 (号の行を除く) を「。」で文に分ける。
fn sentences(p: &Paragraph) -> Vec<String> {
    let item_set: HashSet<String> = item_lines(&p.items).into_iter().collect();
    let mut out = Vec::new();
    for line in p.text.split('\n') {
        if line.is_empty() || item_set.contains(line) {
            continue;
        }
        let c: Vec<char> = line.chars().collect();
        let mut start = 0;
        let mut depth = 0i32;
        for (i, &ch) in c.iter().enumerate() {
            match ch {
                '（' | '「' => depth += 1,
                '）' | '」' => depth -= 1,
                '。' if depth <= 0 => {
                    out.push(c[start..=i].iter().collect());
                    start = i + 1;
                }
                _ => {}
            }
        }
        if start < c.len() {
            out.push(c[start..].iter().collect());
        }
    }
    out
}

fn item_lines(items: &[Item]) -> Vec<String> {
    let mut out = Vec::new();
    for it in items {
        out.push(super::render_item_line(it));
        out.extend(item_lines(&it.subitems));
    }
    out
}

// ---------------------------------------------------------------------------
// 施行期日

const LAW_KINDS: &[&str] = &[
    "法律",
    "政令",
    "内閣府令",
    "府令",
    "省令",
    "規則",
    "命令",
    "勅令",
    "告示",
    "訓令",
    "条例",
    "規程",
    "布告",
    "達",
    "法",
    "令",
];

/// 文が施行期日の規定なら規則を返す。
fn enforcement_rules(
    s: &str,
    unit: &Unit,
    location: &Location,
    ctx: &mut Ctx,
) -> Option<Vec<EnforcementRule>> {
    let c: Vec<char> = s.chars().collect();
    let flat = strip_parens(s);
    // 「この法律の施行期日は、政令で定める。」
    let delegated_whole = ["施行期日", "施行ノ期日", "施行の期日"]
        .iter()
        .any(|k| flat.contains(k))
        && (flat.contains("定める") || flat.contains("定ム"));
    let marker = enforce_marker(&c);
    if marker.is_none() && !delegated_whole {
        return None;
    }
    let (subject_end, subject) = split_subject(&c);
    let (scope, targets, excluding) = classify_subject(subject.as_deref());
    if marker.is_none() {
        let when: String = c[subject_end..].iter().collect();
        let when = when.trim_end_matches('。');
        // 「この法律の施行期日は、公布の日から起算して三箇月を経過した日とする。」
        if let Some(w) = when.strip_suffix("とする") {
            let mut r = classify_when(w, ctx);
            r.location = location.clone();
            r.scope = scope;
            r.targets = targets;
            r.excluding = excluding;
            r.text = s.to_string();
            return Some(vec![r]);
        }
        return Some(vec![EnforcementRule {
            location: location.clone(),
            scope,
            target_refs: targets
                .as_deref()
                .map(|t| extract_refs(t, unit.article, ctx))
                .unwrap_or_default(),
            targets,
            excluding,
            kind: EnforcementKind::Delegated,
            date: None,
            date_source: None,
            deadline: None,
            delegated_to: delegated_to(&flat),
            linked_to: None,
            period: None,
            applies_from: None,
            when: when.to_string(),
            text: s.to_string(),
        }]);
    }
    let (m_start, m_end) = marker.unwrap();
    // 「第三条の規定は…から、第四条の規定は…から施行する。」: 節ごとに分けて読む。
    let clauses = split_clauses(&c, m_start);
    if clauses.len() > 1 {
        let tail: String = c[m_start..].iter().collect();
        let mut rules = Vec::new();
        for (k, clause) in clauses.iter().enumerate() {
            let sentence = if k + 1 == clauses.len() {
                format!("{clause}{tail}")
            } else {
                format!("{clause}から施行する。")
            };
            for mut r in enforcement_rules(&sentence, unit, location, ctx).unwrap_or_default() {
                r.text = s.to_string();
                rules.push(r);
            }
        }
        return Some(rules);
    }
    let when: String = c[subject_end.min(m_start)..m_start].iter().collect();
    let applies_from = {
        let after: String = c[m_end..].iter().collect();
        if after.contains("適用") {
            first_date(&after, ctx.last_date).map(iso)
        } else {
            None
        }
    };

    // 「次の各号に掲げる規定は、当該各号に定める日から施行する」: 号ごとに読む。
    if is_per_item(&when) {
        let mut rules = Vec::new();
        for it in &unit.paragraph.items {
            let Some((t, w)) = split_item_columns(&it.text) else {
                continue;
            };
            let mut r = classify_when(&w, ctx);
            r.location = Location {
                item: it.title.clone(),
                ..location.clone()
            };
            r.scope = EnforcementScope::Partial;
            r.target_refs = extract_refs(&t, unit.article, ctx);
            r.targets = Some(t);
            r.text = super::render_item_line(it);
            rules.push(r);
        }
        if rules.is_empty() {
            // 号が「略」だけ等。読めなかったことを残す。
            let mut r = classify_when("", ctx);
            r.location = location.clone();
            r.scope = scope;
            r.text = s.to_string();
            r.when = when;
            rules.push(r);
        }
        return Some(rules);
    }

    let mut r = classify_when(&when, ctx);
    r.location = location.clone();
    r.scope = scope;
    r.target_refs = targets
        .as_deref()
        .map(|t| extract_refs(t, unit.article, ctx))
        .unwrap_or_default();
    r.targets = targets;
    r.excluding = excluding;
    r.applies_from = applies_from;
    r.text = s.to_string();
    Some(vec![r])
}

/// `c[..m_start]` を「…から、…の規定は」の切れ目で節に分ける (各節は末尾の「から」を含まない)。
/// 切れ目が無ければ 1 節。
fn split_clauses(c: &[char], m_start: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    for i in 0..m_start {
        match c[i] {
            '（' | '「' => depth += 1,
            '）' | '」' => depth -= 1,
            _ => {}
        }
        if depth != 0 || i < start || !starts_with(c, i, "から、") {
            continue;
        }
        let rest = &c[i + 3..m_start];
        let (_, subject) = split_subject(rest);
        if subject.is_some_and(|s| is_provision_subject(&s)) {
            out.push(c[start..i].iter().collect());
            start = i + 3;
        }
    }
    out.push(c[start..m_start].iter().collect());
    out
}

/// 「…から施行する」の「から施行す」の位置 (開始, 終了)。括弧の外だけを見る。
fn enforce_marker(c: &[char]) -> Option<(usize, usize)> {
    let mut depth = 0i32;
    for i in 0..c.len() {
        match c[i] {
            '（' | '「' => depth += 1,
            '）' | '」' => depth -= 1,
            _ => {}
        }
        if depth > 0 || !starts_with(c, i, "施行") {
            continue;
        }
        if !matches!(c.get(i + 2), Some('す' | 'ス' | 'し' | 'セ')) {
            continue;
        }
        // 「この法律を同日から施行したとしても」は施行期日の規定ではない。
        if c.get(i + 2) == Some(&'し') && c.get(i + 3) == Some(&'た') {
            continue;
        }
        // 直前の「から」「ヨリ」を探す (間の「、」「これを」「之ヲ」は読み飛ばす)。
        let mut j = i;
        loop {
            let mut moved = false;
            for skip in ["、", "これを", "之ヲ", "之を", "コレヲ"] {
                let n = skip.chars().count();
                if j >= n && ends_with(c, j, skip) {
                    j -= n;
                    moved = true;
                }
            }
            if !moved {
                break;
            }
        }
        for from in ["から", "ヨリ", "より", "カラ"] {
            if j >= 2 && ends_with(c, j, from) {
                return Some((j - 2, i + 3));
            }
        }
    }
    None
}

/// 主語 (「…は、」の前) を切り出す。(施行日の部分の開始位置, 主語)。
fn split_subject(c: &[char]) -> (usize, Option<String>) {
    let mut depth = 0i32;
    for i in 0..c.len() {
        match c[i] {
            '（' | '「' => depth += 1,
            '）' | '」' => depth -= 1,
            'は' | 'ハ' if depth == 0 => {
                let next = c.get(i + 1).copied();
                let subject: String = c[..i].iter().collect();
                if next == Some('、') {
                    return (i + 2, Some(subject));
                }
                // 「この省令は公布の日から」「本法ハ公布ノ日ヨリ」のように読点が無いものは、
                // 主語が「この法律」等か「…の規定」のときだけ主語とみなす。
                if is_whole_subject(&subject) || is_provision_subject(&subject) {
                    return (i + 1, Some(subject));
                }
            }
            _ => {}
        }
    }
    (0, None)
}

fn is_whole_subject(subject: &str) -> bool {
    let s = strip_parens(subject);
    let s = s.trim();
    for head in ["この", "此ノ", "此の", "本"] {
        if let Some(rest) = s.strip_prefix(head) {
            return LAW_KINDS.contains(&rest);
        }
    }
    false
}

/// 主語から (範囲, 対象規定, 除外規定) を決める。
/// 「第三条の規定」「…の改正規定（…を除く。）」のような一部の規定を指す主語か。
fn is_provision_subject(subject: &str) -> bool {
    strip_parens(subject).trim_end().ends_with("規定")
}

fn classify_subject(subject: Option<&str>) -> (EnforcementScope, Option<String>, Option<String>) {
    let Some(subject) = subject else {
        return (EnforcementScope::Whole, None, None);
    };
    if is_whole_subject(subject) {
        let excluding = paren_content(subject).filter(|p| p.contains("除く"));
        return (EnforcementScope::Whole, None, excluding);
    }
    let mut t = subject.trim();
    for lead in ["ただし、", "但シ", "但し、", "ただし"] {
        t = t.strip_prefix(lead).unwrap_or(t);
    }
    // 「前項の規定にかかわらず、第三条の規定は」
    if let Some(pos) = t.rfind("かかわらず、") {
        t = &t[pos + "かかわらず、".len()..];
    }
    (EnforcementScope::Partial, Some(t.to_string()), None)
}

/// 号の「対象規定　施行日」を分ける (最後の全角スペースで切る)。
fn split_item_columns(text: &str) -> Option<(String, String)> {
    let c: Vec<char> = text.chars().collect();
    let mut depth = 0i32;
    let mut last = None;
    for (i, &ch) in c.iter().enumerate() {
        match ch {
            '（' | '「' => depth += 1,
            '）' | '」' => depth -= 1,
            '\u{3000}' if depth == 0 => last = Some(i),
            _ => {}
        }
    }
    let i = last?;
    let t: String = c[..i].iter().collect();
    let w: String = c[i + 1..].iter().collect();
    (!t.is_empty() && !w.is_empty()).then_some((t, w))
}

/// 施行日の部分を分類する。
fn classify_when(when: &str, ctx: &mut Ctx) -> EnforcementRule {
    let flat = strip_parens(when);
    let mut r = EnforcementRule {
        location: Location::default(),
        scope: EnforcementScope::Whole,
        targets: None,
        excluding: None,
        target_refs: Vec::new(),
        kind: EnforcementKind::Unknown,
        date: None,
        date_source: None,
        deadline: None,
        delegated_to: None,
        linked_to: None,
        period: None,
        applies_from: None,
        when: when.trim_end_matches('。').to_string(),
        text: String::new(),
    };
    if flat.trim().is_empty() {
        return r;
    }
    let from_promulgation = [
        "公布の日から起算して",
        "公布ノ日ヨリ起算シテ",
        "公布の日から起算し",
    ]
    .iter()
    .find(|k| flat.contains(*k));
    // 「公布の日から三月を経過した日」は「起算して」が無いので初日を算入しない (民法140条)。
    let after_promulgation = [
        "公布後",
        "公布ノ後",
        "公布の後",
        "公布の日から",
        "公布ノ日ヨリ",
    ]
    .iter()
    .find(|k| flat.contains(*k));
    let period = period_after(&flat, from_promulgation.or(after_promulgation).copied());
    let promulgation = ctx.promulgation;
    let elapsed = |p: &Period, exclusive: bool| -> Option<NaiveDate> {
        let start = promulgation?;
        let start = if exclusive {
            start + Duration::days(1)
        } else {
            start
        };
        p.elapsed_from(start)
    };
    let defined = [
        "定める日",
        "定ムル日",
        "定むる日",
        "指定する日",
        "定める期日",
    ]
    .iter()
    .any(|k| flat.contains(k));

    if defined {
        r.kind = EnforcementKind::Delegated;
        r.delegated_to = delegated_to(&flat);
        if flat.contains("範囲内") || flat.contains("範囲ニ於テ") {
            if let Some(p) = &period {
                r.period = Some(p.text.clone());
                let exclusive = from_promulgation.is_none() && after_promulgation.is_some();
                r.deadline = elapsed(p, exclusive).map(|d| iso(d - Duration::days(1)));
            } else if let Some(d) = first_date(&flat, ctx.last_date) {
                // 「令和七年三月三十一日を超えない範囲内において」
                r.deadline = Some(iso(d));
            }
        } else if flat.contains("までの間") {
            if let Some(d) = first_date(&flat, ctx.last_date) {
                r.deadline = Some(iso(d));
                ctx.last_date = Some(d);
            }
        }
        return r;
    }
    if flat.contains("いずれか") {
        // 「…の施行の日又は…のいずれか遅い日」: 他の事象に連動する。
        r.kind = EnforcementKind::Linked;
        r.linked_to = Some(flat.trim_end_matches("から").to_string());
        return r;
    }
    if let (Some(p), true) = (
        &period,
        flat.contains("経過") && (from_promulgation.is_some() || after_promulgation.is_some()),
    ) {
        r.kind = EnforcementKind::AfterPromulgation;
        r.period = Some(p.text.clone());
        let exclusive = from_promulgation.is_none();
        if let Some(d) = elapsed(p, exclusive) {
            r.date = Some(iso(d));
            r.date_source = Some(DateSource::Computed);
        }
        return r;
    }
    let linked = [
        "施行の日",
        "施行日",
        "施行ノ日",
        "効力発生の日",
        "効力を生ずる日",
        "効力を生ずる時",
        "効力ヲ生ズル日",
        "効力発生日",
        "日の翌日",
    ]
    .iter()
    .any(|k| flat.contains(k))
        && !is_promulgation_only(&flat);
    if linked {
        r.kind = EnforcementKind::Linked;
        r.linked_to = Some(flat.trim().to_string());
        // 「法の施行の日（平成十年四月一日）」: 括弧書きの日付。
        if let Some(d) = first_date(when, ctx.last_date) {
            r.date = Some(iso(d));
            r.date_source = Some(DateSource::Explicit);
            ctx.last_date = Some(d);
        }
        return r;
    }
    // 「公布の日の属する月の翌月の初日（公布の日が月の初日であるときは、その日）」
    if flat.contains("公布の日の属する月の翌月の初日") {
        r.kind = EnforcementKind::AfterPromulgation;
        r.period = Some("公布の日の属する月の翌月の初日".to_string());
        if let Some(p) = promulgation {
            let d = if p.day() == 1 && when.contains("初日であるときは") {
                Some(p)
            } else {
                p.with_day(1)
                    .and_then(|d| d.checked_add_months(Months::new(1)))
            };
            if let Some(d) = d {
                r.date = Some(iso(d));
                r.date_source = Some(DateSource::Computed);
            }
        }
        return r;
    }
    if let Some(offset) = promulgation_offset(&flat) {
        if offset == 0 {
            r.kind = EnforcementKind::Promulgation;
            r.date_source = Some(DateSource::Promulgation);
        } else {
            r.kind = EnforcementKind::AfterPromulgation;
            r.period = Some(flat.trim().to_string());
            r.date_source = Some(DateSource::Computed);
        }
        match promulgation {
            Some(d) => r.date = Some(iso(d + Duration::days(offset))),
            None => r.date_source = None,
        }
        return r;
    }
    if let Some(d) = first_date(&flat, ctx.last_date) {
        r.kind = EnforcementKind::FixedDate;
        r.date = Some(iso(d));
        r.date_source = Some(DateSource::Explicit);
        ctx.last_date = Some(d);
        return r;
    }
    if flat.trim() == "同日" {
        if let Some(d) = ctx.last_date {
            r.kind = EnforcementKind::FixedDate;
            r.date = Some(iso(d));
            r.date_source = Some(DateSource::Explicit);
            return r;
        }
    }
    // 残りは「機構の成立の時」「次の常会の召集の日」「関税法…に規定する指定日」のような
    // 他の事象に連動する書き方。
    r.kind = EnforcementKind::Linked;
    r.linked_to = Some(flat.trim().to_string());
    r
}

/// 「次の各号に掲げる規定は、当該各号に定める日から」: 号ごとに施行日を定める。
fn is_per_item(when: &str) -> bool {
    when.contains("当該各号に")
        || when.contains("各号に定める")
        || when.contains("各号に掲げる日")
        || when.contains("各号ニ定ムル")
        || when.contains("各号ニ掲グル")
}

fn is_promulgation_only(flat: &str) -> bool {
    promulgation_offset(flat).is_some()
}

/// 「公布の日」なら Some(0)、「公布の日の翌日」なら Some(1)。
fn promulgation_offset(flat: &str) -> Option<i64> {
    let mut t = flat
        .trim()
        .trim_end_matches("から")
        .trim_end_matches("ヨリ");
    // 「この法律の公布の日」「本令公布ノ日」
    for head in ["この", "此ノ", "本"] {
        if let Some(rest) = t.strip_prefix(head) {
            for k in LAW_KINDS {
                if let Some(r) = rest.strip_prefix(k) {
                    t = r
                        .strip_prefix('の')
                        .or_else(|| r.strip_prefix('ノ'))
                        .unwrap_or(r);
                    break;
                }
            }
            break;
        }
    }
    match t {
        "公布の日" | "公布ノ日" | "公布日" => Some(0),
        "公布の日の翌日" | "公布ノ日ノ翌日" => Some(1),
        _ => None,
    }
}

/// 「政令で定める日」の「政令」。
fn delegated_to(flat: &str) -> Option<String> {
    let c: Vec<char> = flat.chars().collect();
    let pos = [
        "で定める",
        "ヲ以テ",
        "をもつて定める",
        "で定め",
        "が定める",
        "が指定する",
    ]
    .iter()
    .filter_map(|k| find(&c, 0, k))
    .min()?;
    let mut i = pos;
    while i > 0 && is_kanji(c[i - 1]) && pos - (i - 1) <= 10 {
        i -= 1;
    }
    (i < pos).then(|| c[i..pos].iter().collect())
}

/// 期間 (年・月・日)。
#[derive(Debug, Clone, PartialEq)]
struct Period {
    months: u32,
    days: u32,
    text: String,
}

impl Period {
    /// 起算日 `start` から期間が経過した日 (満了日の翌日)。
    fn elapsed_from(&self, start: NaiveDate) -> Option<NaiveDate> {
        let mut d = start;
        if self.months > 0 {
            let target = d.checked_add_months(Months::new(self.months))?;
            d = if target.day() != start.day() {
                // 応当日が無い (8/31 + 6 か月): 満了はその月の末日、経過はその翌日。
                target + Duration::days(1)
            } else {
                target
            };
        }
        Some(d + Duration::days(self.days as i64))
    }
}

/// `anchor` の直後にある「六月」「一年六月」「二十日」を読む。
fn period_after(flat: &str, anchor: Option<&str>) -> Option<Period> {
    let anchor = anchor?;
    let c: Vec<char> = flat.chars().collect();
    let mut i = find(&c, 0, anchor)? + anchor.chars().count();
    let start = i;
    let (mut months, mut days) = (0u32, 0u32);
    let mut any = false;
    while let Some((n, j)) = take_number(&c, i) {
        let units: &[(&str, u32, bool)] = &[
            ("箇月", 1, false),
            ("か月", 1, false),
            ("ヶ月", 1, false),
            ("ケ月", 1, false),
            ("カ月", 1, false),
            ("月", 1, false),
            ("年", 12, false),
            ("週間", 7, true),
            ("日", 1, true),
        ];
        let Some((u, mul, is_day)) = units.iter().find(|(u, _, _)| starts_with(&c, j, u)) else {
            break;
        };
        if *is_day {
            days += n * mul;
        } else {
            months += n * mul;
        }
        any = true;
        i = j + u.chars().count();
    }
    any.then(|| Period {
        months,
        days,
        text: format!("{anchor}{}", c[start..i].iter().collect::<String>()),
    })
}

// ---------------------------------------------------------------------------
// 経過措置

fn transitional_kinds(text: &str) -> Vec<TransitionalKind> {
    let flat = strip_parens(text);
    let has = |ks: &[&str]| ks.iter().any(|k| flat.contains(k));
    let mut kinds = Vec::new();
    if has(&["従前の例", "従前ノ例"]) {
        kinds.push(TransitionalKind::PriorLaw);
        if has(&["罰則の適用", "罰則ノ適用"]) {
            kinds.push(TransitionalKind::Penalty);
        }
    }
    if has(&[
        "なお効力を有",
        "なおその効力を有",
        "ナホ其ノ効力ヲ有",
        "仍其ノ効力ヲ有",
        "なほその効力を有",
        "なほ効力を有",
        "なお有効",
        "ナホ効力ヲ有",
    ]) {
        kinds.push(TransitionalKind::ContinuedEffect);
    }
    if has(&["とみな", "ト看做", "と看做"]) {
        kinds.push(TransitionalKind::Deemed);
    }
    if has(&[
        "適用する",
        "適用し、",
        "適用しない",
        "適用ス",
        "適用セズ",
        "適用セ",
        "適用があるものとする",
    ]) {
        kinds.push(TransitionalKind::Application);
    }
    if has(&["経過措置"]) && has(&["で定める", "ヲ以テ定ム", "で定め"]) {
        kinds.push(TransitionalKind::Delegated);
    }
    if has(&["検討を加え", "検討を行", "見直しを行", "検討するものとする"]) {
        kinds.push(TransitionalKind::Review);
    }
    if flat.trim_end().ends_with("廃止する。") || flat.trim_end().ends_with("廃止ス") {
        kinds.push(TransitionalKind::Repeal);
    }
    kinds
}

// ---------------------------------------------------------------------------
// 読替え規定

fn extract_readings(s: &str, location: &Location, ctx: &Ctx) -> Vec<Reading> {
    let c: Vec<char> = s.chars().collect();
    let mut out: Vec<Reading> = Vec::new();
    let mut seg_start = 0usize;
    let mut target: Option<String> = None;
    let mut context: Option<String> = None;
    let mut i = 0usize;
    while i < c.len() {
        if c[i] != '「' {
            i += 1;
            continue;
        }
        let Some(a_end) = close_quote(&c, i) else {
            break;
        };
        let after_a = a_end + 1;
        if !starts_with(&c, after_a, "とあるのは") {
            i = after_a;
            continue;
        }
        let mut j = after_a + "とあるのは".chars().count();
        if c.get(j) == Some(&'、') {
            j += 1;
        }
        if c.get(j) != Some(&'「') {
            i = j;
            continue;
        }
        let Some(b_end) = close_quote(&c, j) else {
            break;
        };
        // 読み替える規定: 前の対の後 (又は文頭) から「「」までの部分。
        let seg: String = c[seg_start..i].iter().collect();
        let seg = seg.trim_start_matches('、').trim();
        if !seg.is_empty() {
            let (ctx_part, tgt) = split_reading_target(seg);
            if ctx_part.is_some() {
                context = ctx_part;
            }
            target = tgt;
        }
        out.push(Reading {
            location: location.clone(),
            target_refs: target
                .as_deref()
                .map(|t| extract_refs(t, None, ctx))
                .unwrap_or_default(),
            target: target.clone(),
            from: c[i + 1..a_end].iter().collect(),
            to: c[j + 1..b_end].iter().collect(),
            context: context.clone(),
        });
        i = b_end + 1;
        if c.get(i) == Some(&'と') {
            i += 1;
        }
        seg_start = i;
    }
    out
}

/// 「旧法第十条の規定の適用については、同条第一項中」→ (場面, 規定)。
fn split_reading_target(seg: &str) -> (Option<String>, Option<String>) {
    let c: Vec<char> = seg.chars().collect();
    // 括弧の外の最後の「、」で場面と規定を分ける。
    let mut depth = 0i32;
    let mut cut = None;
    for (k, &ch) in c.iter().enumerate() {
        match ch {
            '（' | '「' => depth += 1,
            '）' | '」' => depth -= 1,
            '、' if depth == 0 => cut = Some(k),
            _ => {}
        }
    }
    let (head, tail): (String, String) = match cut {
        Some(k) => (c[..k].iter().collect(), c[k + 1..].iter().collect()),
        None => (String::new(), seg.to_string()),
    };
    let context = ["については", "においては", "に関しては", "の場合において"]
        .iter()
        .any(|k| head.contains(k))
        .then(|| head.trim().to_string());
    let tail = tail.trim();
    let target = tail.strip_suffix('中').map(|t| t.to_string());
    (context, target)
}

fn close_quote(c: &[char], open: usize) -> Option<usize> {
    let mut depth = 0i32;
    for (k, &ch) in c.iter().enumerate().skip(open) {
        match ch {
            '「' => depth += 1,
            '」' => {
                depth -= 1;
                if depth == 0 {
                    return Some(k);
                }
            }
            _ => {}
        }
    }
    None
}

// ---------------------------------------------------------------------------
// 参照 (第X条)

fn extract_refs(text: &str, current: Option<&Article>, ctx: &Ctx) -> Vec<Ref> {
    let c: Vec<char> = text.chars().collect();
    let mut out: Vec<Ref> = Vec::new();
    let mut prev: Option<(usize, RefScope, String)> = None; // (終了位置, 範囲, 条番号)
    let mut i = 0usize;
    // 「」の中 (読替えの字句) は参照として数えない。
    let mut quote = 0i32;
    while i < c.len() {
        match c[i] {
            '「' => {
                quote += 1;
                i += 1;
                continue;
            }
            '」' => {
                quote -= 1;
                i += 1;
                continue;
            }
            _ => {}
        }
        if quote > 0 {
            i += 1;
            continue;
        }
        // 前条・次条
        if (starts_with(&c, i, "前条") || starts_with(&c, i, "次条")) && !is_law_suffix(&c, i) {
            let id = current.and_then(|a| relative_article(ctx, a, c[i] == '前'));
            let end = i + 2;
            out.push(Ref {
                text: c[i..end].iter().collect(),
                scope: RefScope::Suppl,
                article_id: id,
            });
            prev = None;
            i = end;
            continue;
        }
        if c[i] != '第' {
            i += 1;
            continue;
        }
        let Some((num, end)) = article_num(&c, i) else {
            i += 1;
            continue;
        };
        let start = i;
        let (scope, ref_start) = if i >= 2 && ends_with(&c, i, "附則") {
            if i >= 3 && is_law_suffix(&c, i - 2) {
                (RefScope::OtherLaw, law_name_start(&c, i - 2))
            } else {
                (RefScope::Suppl, i - 2)
            }
        } else if i > 0 && is_law_suffix(&c, i) {
            (RefScope::OtherLaw, law_name_start(&c, i))
        } else if let Some((pend, pscope, pnum)) = prev.as_ref().filter(|(pend, _, _)| {
            let between: String = c[*pend..i].iter().collect();
            is_connector(&between)
        }) {
            // 「附則第三条及び第五条」「第六条から第八条まで」: 前の参照の範囲を引き継ぐ。
            let range = c[*pend..i].iter().collect::<String>() == "から";
            if range {
                if let (Ok(a), Ok(b)) = (pnum.parse::<u32>(), num.parse::<u32>()) {
                    if a < b && b - a <= 50 {
                        for n in a + 1..b {
                            out.push(Ref {
                                text: format!("第{}条", kansuji(n)),
                                scope: *pscope,
                                article_id: resolve(ctx, *pscope, &n.to_string()),
                            });
                        }
                    }
                }
            }
            (*pscope, start)
        } else if ctx.provision.amend_law_num.is_none() {
            (RefScope::Main, start)
        } else {
            (RefScope::AmendingLaw, start)
        };
        // 項・号 (「第十九条第一項」) は参照テキストに含める。
        let mut tail = end;
        while c.get(tail) == Some(&'第') {
            let Some((_, j)) = take_number(&c, tail + 1) else {
                break;
            };
            if matches!(c.get(j), Some('項' | '号')) {
                tail = j + 1;
            } else {
                break;
            }
        }
        out.push(Ref {
            text: c[ref_start..tail].iter().collect(),
            scope,
            article_id: resolve(ctx, scope, &num),
        });
        prev = Some((tail, scope, num));
        i = tail;
    }
    out
}

fn resolve(ctx: &Ctx, scope: RefScope, num: &str) -> Option<String> {
    match scope {
        RefScope::Main => {
            let id = format!("art_{num}");
            ctx.main_ids.contains(id.as_str()).then_some(id)
        }
        RefScope::Suppl => ctx
            .suppl_articles
            .iter()
            .find(|(n, _)| n == num)
            .map(|(_, id)| id.clone()),
        RefScope::AmendingLaw | RefScope::OtherLaw => None,
    }
}

fn relative_article(ctx: &Ctx, current: &Article, before: bool) -> Option<String> {
    let pos = ctx
        .suppl_articles
        .iter()
        .position(|(_, id)| *id == current.article_id)?;
    let k = if before { pos.checked_sub(1)? } else { pos + 1 };
    ctx.suppl_articles.get(k).map(|(_, id)| id.clone())
}

fn is_connector(s: &str) -> bool {
    matches!(
        s,
        "、" | "及び"
            | "並びに"
            | "から"
            | "又は"
            | "若しくは"
            | "まで、"
            | "まで及び"
            | "まで並びに"
    )
}

/// `c[i]` の直前が法令名の末尾 (「新法」「施行令」「規則」) か。
fn is_law_suffix(c: &[char], i: usize) -> bool {
    if i == 0 {
        return false;
    }
    // 「緑資源公団法（昭和三十一年法律第八十五号）第三十三条」: 法令番号の括弧書きを飛ばす。
    let i = skip_paren_back(c, i);
    if i == 0 {
        return false;
    }
    let p = c[i - 1];
    if matches!(p, '法' | '令' | '律' | '約' | '程') {
        return true;
    }
    p == '則' && !(i >= 2 && c[i - 2] == '附')
}

/// `c[..i]` が「（…）」で終わるなら、その「（」の位置。そうでなければ `i`。
fn skip_paren_back(c: &[char], i: usize) -> usize {
    if i == 0 || c[i - 1] != '）' {
        return i;
    }
    let mut depth = 0i32;
    for k in (0..i).rev() {
        match c[k] {
            '）' => depth += 1,
            '（' => {
                depth -= 1;
                if depth == 0 {
                    return k;
                }
            }
            _ => {}
        }
    }
    i
}

/// `c[..i]` の末尾の法令名 (「新民法」「改正法」) の開始位置。参照テキストに含める。
fn law_name_start(c: &[char], i: usize) -> usize {
    let i = skip_paren_back(c, i);
    let mut k = i;
    while k > 0 && is_kanji(c[k - 1]) && i - k < 12 {
        k -= 1;
    }
    k
}

/// 「第二条の二」→ ("2_2", 終了位置)。
fn article_num(c: &[char], i: usize) -> Option<(String, usize)> {
    if c.get(i) != Some(&'第') {
        return None;
    }
    let (n, mut j) = take_number(c, i + 1)?;
    if c.get(j) != Some(&'条') {
        return None;
    }
    j += 1;
    let mut parts = vec![n.to_string()];
    while c.get(j) == Some(&'の') {
        match take_number(c, j + 1) {
            Some((m, k)) => {
                parts.push(m.to_string());
                j = k;
            }
            None => break,
        }
    }
    Some((parts.join("_"), j))
}

// ---------------------------------------------------------------------------
// 日付・数字

const ERAS: &[(&str, i32)] = &[
    ("明治", 1868),
    ("大正", 1912),
    ("昭和", 1926),
    ("平成", 1989),
    ("令和", 2019),
];

fn iso(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

/// 文中の最初の日付 (「令和五年四月一日」「同年十月一日」)。
fn first_date(s: &str, last: Option<NaiveDate>) -> Option<NaiveDate> {
    let c: Vec<char> = s.chars().collect();
    for i in 0..c.len() {
        if let Some(d) = date_at(&c, i, last) {
            return Some(d);
        }
    }
    None
}

fn date_at(c: &[char], i: usize, last: Option<NaiveDate>) -> Option<NaiveDate> {
    let (year, j) = if let Some((_, base)) = ERAS.iter().find(|(name, _)| starts_with(c, i, name)) {
        let (y, j) = take_number(c, i + 2)?;
        if c.get(j) != Some(&'年') {
            return None;
        }
        (base + y as i32 - 1, j + 1)
    } else if starts_with(c, i, "同年") {
        (last?.year(), i + 2)
    } else {
        return None;
    };
    let (m, k) = take_number(c, j)?;
    if c.get(k) != Some(&'月') {
        return None;
    }
    let (d, l) = take_number(c, k + 1)?;
    if c.get(l) != Some(&'日') {
        return None;
    }
    NaiveDate::from_ymd_opt(year, m, d)
}

fn digit(ch: char) -> Option<u32> {
    match ch {
        '〇' | '零' | '０' | '0' => Some(0),
        '一' | '１' | '1' => Some(1),
        '二' | '２' | '2' => Some(2),
        '三' | '３' | '3' => Some(3),
        '四' | '４' | '4' => Some(4),
        '五' | '５' | '5' => Some(5),
        '六' | '６' | '6' => Some(6),
        '七' | '７' | '7' => Some(7),
        '八' | '８' | '8' => Some(8),
        '九' | '９' | '9' => Some(9),
        _ => None,
    }
}

/// `c[i..]` の数字 (漢数字「百二十三」/ 位取り「一二三」/ 算用数字 / 「元」) を読む。
fn take_number(c: &[char], i: usize) -> Option<(u32, usize)> {
    if c.get(i) == Some(&'元') {
        return Some((1, i + 1));
    }
    let mut j = i;
    while j < c.len() && (digit(c[j]).is_some() || matches!(c[j], '十' | '百' | '千')) {
        j += 1;
    }
    if j == i {
        return None;
    }
    Some((parse_number(&c[i..j])?, j))
}

fn parse_number(s: &[char]) -> Option<u32> {
    if s.iter().any(|ch| matches!(ch, '十' | '百' | '千')) {
        let (mut total, mut cur) = (0u32, 0u32);
        for &ch in s {
            let unit = match ch {
                '十' => 10,
                '百' => 100,
                '千' => 1000,
                _ => {
                    cur = cur * 10 + digit(ch)?;
                    continue;
                }
            };
            total += if cur == 0 { 1 } else { cur } * unit;
            cur = 0;
        }
        Some(total + cur)
    } else {
        s.iter().try_fold(0u32, |n, &ch| Some(n * 10 + digit(ch)?))
    }
}

/// 1..=9999 を漢数字にする (範囲展開した参照の表示用)。
fn kansuji(mut n: u32) -> String {
    const D: [char; 10] = ['〇', '一', '二', '三', '四', '五', '六', '七', '八', '九'];
    let mut out = String::new();
    for (unit, ch) in [(1000, '千'), (100, '百'), (10, '十')] {
        let q = n / unit;
        if q > 0 {
            if q > 1 {
                out.push(D[q as usize]);
            }
            out.push(ch);
        }
        n %= unit;
    }
    if n > 0 {
        out.push(D[n as usize]);
    }
    out
}

// ---------------------------------------------------------------------------
// 文字列の小道具

fn strip_parens(s: &str) -> String {
    let mut depth = 0i32;
    let mut out = String::new();
    for ch in s.chars() {
        match ch {
            '（' => depth += 1,
            '）' => depth = (depth - 1).max(0),
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    out
}

/// 最初の括弧書きの中身。
fn paren_content(s: &str) -> Option<String> {
    let c: Vec<char> = s.chars().collect();
    let open = c.iter().position(|&ch| ch == '（')?;
    let mut depth = 0i32;
    for (k, &ch) in c.iter().enumerate().skip(open) {
        match ch {
            '（' => depth += 1,
            '）' => {
                depth -= 1;
                if depth == 0 {
                    return Some(c[open + 1..k].iter().collect());
                }
            }
            _ => {}
        }
    }
    None
}

fn starts_with(c: &[char], i: usize, pat: &str) -> bool {
    (i..).zip(pat.chars()).all(|(k, p)| c.get(k) == Some(&p))
}

/// `c[..i]` が `pat` で終わるか。
fn ends_with(c: &[char], i: usize, pat: &str) -> bool {
    let n = pat.chars().count();
    i >= n && starts_with(c, i - n, pat)
}

fn find(c: &[char], from: usize, pat: &str) -> Option<usize> {
    (from..c.len()).find(|&i| starts_with(c, i, pat))
}

fn is_kanji(ch: char) -> bool {
    ('\u{4E00}'..='\u{9FFF}').contains(&ch) || ch == '々'
}

// ---------------------------------------------------------------------------
// 集計 (health / 未解決リスト)

/// 法令ごとの統計を足し合わせる。
pub fn add_stats(total: &mut SupplementaryStats, s: &SupplementaryStats) {
    total.provisions += s.provisions;
    total.with_enforcement += s.with_enforcement;
    total.enforcement_unresolved += s.enforcement_unresolved;
    total.enforcement_rules += s.enforcement_rules;
    total.transitional_measures += s.transitional_measures;
    total.readings += s.readings;
    total.refs += s.refs;
    total.refs_resolved += s.refs_resolved;
}

/// 施行期日の種類ごとの件数 (summary 用)。
pub fn count_kinds(doc: &SupplementaryDocument, out: &mut BTreeMap<String, usize>) {
    for p in &doc.provisions {
        for e in &p.enforcement {
            let k = serde_json::to_value(e.kind)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_default();
            *out.entry(k).or_default() += 1;
        }
    }
}

/// `public/schema/supplementary.json` (JSON Schema draft 2020-12)。
pub fn json_schema() -> serde_json::Value {
    let date = serde_json::json!({ "type": ["string", "null"], "pattern": "^[0-9]{4}-[0-9]{2}-[0-9]{2}$" });
    let opt_str = serde_json::json!({ "type": ["string", "null"] });
    serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "schema/supplementary.json",
        "title": "SupplementaryDocument",
        "description": "附則の構造化 (施行期日・経過措置・読替え規定)。laws/{law_id}/supplementary.json",
        "type": "object",
        "required": ["schema_version", "law_id", "stats", "provisions"],
        "properties": {
            "schema_version": { "type": "integer", "minimum": 1 },
            "law_id": { "type": "string" },
            "law_num": opt_str,
            "stats": {
                "type": "object",
                "required": ["provisions", "with_enforcement", "enforcement_unresolved"],
                "properties": {
                    "provisions": { "type": "integer", "minimum": 0 },
                    "with_enforcement": { "type": "integer", "minimum": 0 },
                    "enforcement_unresolved": { "type": "integer", "minimum": 0 },
                    "enforcement_rules": { "type": "integer", "minimum": 0 },
                    "transitional_measures": { "type": "integer", "minimum": 0 },
                    "readings": { "type": "integer", "minimum": 0 },
                    "refs": { "type": "integer", "minimum": 0 },
                    "refs_resolved": { "type": "integer", "minimum": 0 }
                }
            },
            "provisions": { "type": "array", "items": { "$ref": "#/$defs/provision" } }
        },
        "$defs": {
            "location": {
                "type": "object",
                "properties": {
                    "article_id": opt_str,
                    "paragraph": opt_str,
                    "item": opt_str
                }
            },
            "ref": {
                "type": "object",
                "required": ["text", "scope"],
                "properties": {
                    "text": { "type": "string" },
                    "scope": { "enum": ["main", "suppl", "amending_law", "other_law"] },
                    "article_id": opt_str
                }
            },
            "provision": {
                "type": "object",
                "required": ["index", "enforcement_unresolved"],
                "properties": {
                    "index": { "type": "integer", "minimum": 1 },
                    "amend_law_num": opt_str,
                    "label": opt_str,
                    "promulgation_date": date,
                    "enforcement_unresolved": { "type": "boolean" },
                    "enforcement": { "type": "array", "items": { "$ref": "#/$defs/enforcement" } },
                    "transitional": { "type": "array", "items": { "$ref": "#/$defs/transitional" } },
                    "readings": { "type": "array", "items": { "$ref": "#/$defs/reading" } }
                }
            },
            "enforcement": {
                "type": "object",
                "required": ["location", "scope", "kind", "date", "when", "text"],
                "properties": {
                    "location": { "$ref": "#/$defs/location" },
                    "scope": { "enum": ["whole", "partial"] },
                    "targets": opt_str,
                    "excluding": opt_str,
                    "target_refs": { "type": "array", "items": { "$ref": "#/$defs/ref" } },
                    "kind": { "enum": ["fixed_date", "promulgation", "after_promulgation", "delegated", "linked", "unknown"] },
                    "date": date,
                    "date_source": { "enum": ["explicit", "promulgation", "computed", null] },
                    "deadline": date,
                    "delegated_to": opt_str,
                    "linked_to": opt_str,
                    "period": opt_str,
                    "applies_from": date,
                    "when": { "type": "string" },
                    "text": { "type": "string" }
                }
            },
            "transitional": {
                "type": "object",
                "required": ["location", "kinds", "text"],
                "properties": {
                    "location": { "$ref": "#/$defs/location" },
                    "kinds": {
                        "type": "array",
                        "items": { "enum": ["prior_law", "penalty", "continued_effect", "deemed", "application", "delegated", "review", "repeal", "other"] }
                    },
                    "caption": opt_str,
                    "refs": { "type": "array", "items": { "$ref": "#/$defs/ref" } },
                    "text": { "type": "string" }
                }
            },
            "reading": {
                "type": "object",
                "required": ["location", "from", "to"],
                "properties": {
                    "location": { "$ref": "#/$defs/location" },
                    "target": opt_str,
                    "target_refs": { "type": "array", "items": { "$ref": "#/$defs/ref" } },
                    "from": { "type": "string" },
                    "to": { "type": "string" },
                    "context": opt_str
                }
            }
        }
    })
}

#[cfg(test)]
mod tests;

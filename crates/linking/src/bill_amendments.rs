//! 法律案の改め文 (`public/gian/amendments/`) を、公布後の法令改正差分 (law-diff) と対応づける。
//!
//! 1. 改め文の柱書きの法令番号 (`昭和二十六年法律第五十三号`) を `public/laws/index.json` で法令 ID に解決する。
//! 2. 改正対象法令の `versions.json` から、この法律案が成立した改正法令 (`amendment_law_id`) で
//!    生じた版と、それぞれの直前の版を選んで差分を取る (段階施行なら版ごと)。
//! 3. 各操作の指す条 (`art_{Num}`) が差分で変化しているかを見る。文字列の置換・挿入・削除は
//!    改正前後の条文に該当文字列があるかまで確かめる (`verified`)。条の変化だけ確認できたものは
//!    `article`、条が変化していないものは `unmatched` (同じ改正法令の未施行の版が本文未取得で残る場合は
//!    `pending`)、附則・別表・目次など本則の条でないものは `skipped`。
//! 4. 結果を改め文 JSON に書き戻し、`links/law-to-bill-amendments/{law_id}.json` (法令 → 根拠の改め文) と
//!    `_summary.json` (対応率) を出す。
//!
//! LLM は使わない。

use anyhow::{Context, Result};
use gian_client::amendment::{
    AmendmentOp, BillAmendments, LocationPart, OpKind, OpMatch, TargetLink,
};
use law_diff::{diff_documents, ArticleDiff, ParagraphDiff, TextOp};
use law_normalizer::LawDocument;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

const SUMMARY_FILE: &str = "_summary.json";

/// `links/law-to-bill-amendments/{law_id}.json`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LawToBillAmendments {
    pub schema_version: u32,
    pub law_id: String,
    pub bills: Vec<LinkedBillAmendment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkedBillAmendment {
    pub bill_id: String,
    pub session: u32,
    pub bill_title: String,
    pub law_num: Option<String>,
    pub amendment_law_id: Option<String>,
    /// 法案内の位置 (「第一条」「附則第六条」)。
    pub provision: Option<String>,
    pub status: String,
    pub from_revision_id: Option<String>,
    pub to_revision_ids: Vec<String>,
    pub source_url: String,
    /// 本文 JSON (`gian/amendments/{session}/{bill_id}.json`)。
    pub path: String,
    pub ops: usize,
    pub ops_matched: usize,
    pub sentences: Vec<LinkedSentence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkedSentence {
    pub text: String,
    /// 操作が指す条 (`art_*`) のうち、差分で変化していたもの。差分と照合できない
    /// (未成立・未施行) ときは改め文が指す本則の条。
    pub article_ids: Vec<String>,
    /// 文内の操作の対応状況の要約 (verified / article / pending / unmatched / skipped の最良)。
    /// 差分と照合できないときは改正対象法令の status (not_promulgated / pending / revision_not_found)。
    pub status: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LinkStats {
    pub bills: usize,
    pub bills_promulgated: usize,
    pub targets: usize,
    pub targets_resolved: usize,
    pub targets_linked: usize,
    /// 改正法令の版はあるが、すべて未施行で本文が無い。
    pub targets_pending: usize,
    pub ops: usize,
    pub ops_verified: usize,
    pub ops_article: usize,
    pub ops_unmatched: usize,
    /// 指す条が変化していないが、同じ改正法令の未施行の版が残っている。
    pub ops_pending: usize,
    pub ops_skipped: usize,
    /// 対応づけた版の差分で変化した条の数と、そのうち改め文の操作が指していた条の数。
    pub changed_articles: usize,
    pub explained_articles: usize,
}

// ── 法令索引 ──────────────────────────────────────────────────────

struct LawIndex {
    by_num: HashMap<String, String>,
    by_title: HashMap<String, Vec<String>>,
}

impl LawIndex {
    fn load(public: &Path) -> Result<Self> {
        let path = public.join("laws").join("index.json");
        let v: serde_json::Value = serde_json::from_slice(
            &std::fs::read(&path).with_context(|| format!("read {}", path.display()))?,
        )?;
        let mut by_num = HashMap::new();
        let mut by_title: HashMap<String, Vec<String>> = HashMap::new();
        for law in v["laws"].as_array().map(Vec::as_slice).unwrap_or(&[]) {
            let id = law["law_id"].as_str().unwrap_or("").to_string();
            if id.is_empty() {
                continue;
            }
            if let Some(num) = law["law_num"].as_str() {
                by_num.entry(compact(num)).or_insert_with(|| id.clone());
            }
            if let Some(title) = law["title"].as_str() {
                by_title.entry(compact(title)).or_default().push(id.clone());
            }
        }
        Ok(Self { by_num, by_title })
    }

    fn resolve(&self, law_num: Option<&str>, title: &str) -> Option<String> {
        if let Some(id) = law_num.and_then(|n| self.by_num.get(&compact(n))) {
            return Some(id.clone());
        }
        match self.by_title.get(&compact(title)) {
            Some(ids) if ids.len() == 1 => Some(ids[0].clone()),
            _ => None,
        }
    }
}

fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

// ── 版の選択 ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
struct VersionEntry {
    revision_id: String,
    #[serde(default)]
    effective_date: Option<String>,
    #[serde(default)]
    amendment_law_id: Option<String>,
    #[serde(default)]
    amendment_law_num: Option<String>,
    #[serde(default)]
    body_available: bool,
    #[serde(default)]
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct VersionsFile {
    versions: Vec<VersionEntry>,
}

/// 改正法令で生じた版の選択結果。
#[derive(Debug)]
struct Selected {
    /// 改正法令で生じた本文のある版ごとの (直前の版, その版)。段階施行なら複数で施行日順。
    /// 間に他の改正法令の版が挟まっても、その変化を混ぜないよう版ごとに直前と比べる。
    pairs: Vec<(VersionEntry, VersionEntry)>,
    /// 改正法令で生じたが本文がまだ無い版 (未施行で e-Gov 本文未取得)。
    pending: Vec<String>,
}

/// 改正法令で生じた版 (段階施行なら複数) と、その直前の本文のある版。
/// 改正法令の版が 1 つも無ければ None。
fn select_revisions(
    versions: &[VersionEntry],
    amendment_law_id: &str,
    amendment_law_num: Option<&str>,
) -> Option<Selected> {
    let num = amendment_law_num.map(compact);
    let is_target = |v: &VersionEntry| {
        v.amendment_law_id.as_deref() == Some(amendment_law_id)
            || v.revision_id.ends_with(&format!("_{amendment_law_id}"))
            || (num.is_some() && v.amendment_law_num.as_deref().map(compact) == num)
    };
    if !versions.iter().any(is_target) {
        return None;
    }
    let mut pending: Vec<&VersionEntry> = versions
        .iter()
        .filter(|v| is_target(v) && !v.body_available)
        .collect();
    pending.sort_by(|a, b| a.effective_date.cmp(&b.effective_date));
    let mut bodied: Vec<&VersionEntry> = versions.iter().filter(|v| v.body_available).collect();
    // build-diffs と同じ並び: 施行日昇順 (未定は末尾)。
    bodied.sort_by(
        |a, b| match (a.effective_date.as_deref(), b.effective_date.as_deref()) {
            (Some(x), Some(y)) => x.cmp(y).then_with(|| a.revision_id.cmp(&b.revision_id)),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.revision_id.cmp(&b.revision_id),
        },
    );
    let pairs = (1..bodied.len())
        .filter(|&i| is_target(bodied[i]))
        .map(|i| (bodied[i - 1].clone(), bodied[i].clone()))
        .collect();
    Some(Selected {
        pairs,
        pending: pending.into_iter().map(|v| v.revision_id.clone()).collect(),
    })
}

fn read_revision(public: &Path, law_id: &str, v: &VersionEntry) -> Result<LawDocument> {
    let path = match &v.path {
        Some(p) => public.join(p),
        None => public
            .join("laws")
            .join(law_id)
            .join("revisions")
            .join(format!("{}.json", v.revision_id)),
    };
    let bytes = std::fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))
}

// ── 差分の条ごとの集約 ────────────────────────────────────────────

/// 差分で変化した条ごとの改正前後の文字列 (見出しを含む)。空白は除いてある。
#[derive(Debug, Default)]
struct ChangedArticle {
    old: Vec<String>,
    new: Vec<String>,
}

fn collect_changes(pairs: &[(LawDocument, LawDocument)]) -> BTreeMap<String, ChangedArticle> {
    let mut out: BTreeMap<String, ChangedArticle> = BTreeMap::new();
    for (from, to) in pairs {
        let diff = diff_documents(from, to, false);
        for article in diff.articles {
            match article {
                ArticleDiff::Unchanged { .. } => {}
                ArticleDiff::Added { article_id, to } => {
                    let e = out.entry(article_id).or_default();
                    e.new.extend(to.caption.iter().map(|c| compact(c)));
                    e.new.extend(to.paragraphs.iter().map(|p| compact(&p.text)));
                }
                ArticleDiff::Removed { article_id, from } => {
                    let e = out.entry(article_id).or_default();
                    e.old.extend(from.caption.iter().map(|c| compact(c)));
                    e.old
                        .extend(from.paragraphs.iter().map(|p| compact(&p.text)));
                }
                ArticleDiff::Modified {
                    article_id,
                    from,
                    to,
                    paragraphs,
                } => {
                    let e = out.entry(article_id).or_default();
                    e.old.extend(from.caption.iter().map(|c| compact(c)));
                    e.new.extend(to.caption.iter().map(|c| compact(c)));
                    for p in paragraphs {
                        match p {
                            ParagraphDiff::Unchanged { .. } => {}
                            ParagraphDiff::Added { text, .. } => e.new.push(compact(&text)),
                            ParagraphDiff::Removed { text, .. } => e.old.push(compact(&text)),
                            ParagraphDiff::Modified { text_diff, .. } => {
                                let (mut old, mut new) = (String::new(), String::new());
                                for op in text_diff {
                                    match op {
                                        TextOp::Equal { text } => {
                                            old.push_str(&text);
                                            new.push_str(&text);
                                        }
                                        TextOp::Delete { text } => old.push_str(&text),
                                        TextOp::Insert { text } => new.push_str(&text),
                                    }
                                }
                                e.old.push(compact(&old));
                                e.new.push(compact(&new));
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

// ── 操作の対応づけ ────────────────────────────────────────────────

/// 操作が指しうる条 ID (改正前の位置 + 改正後に現れる条)。
fn candidate_ids(op: &AmendmentOp) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for a in &op.location.articles {
        ids.push(format!("art_{}", a.article));
    }
    for a in &op.new_articles {
        ids.push(format!("art_{a}"));
    }
    ids.dedup();
    ids
}

/// 操作の条 ID が差分の条 ID と一致するか。e-Gov は「第百七十二条から第百七十七条まで 削除」を
/// `art_172:177` の 1 条で持つので、範囲の端または範囲内の条も一致とみなす。
fn id_matches(candidate: &str, key: &str) -> bool {
    if candidate == key {
        return true;
    }
    let Some((start, end)) = key.strip_prefix("art_").and_then(|k| k.split_once(':')) else {
        return false;
    };
    let Some(c) = candidate.strip_prefix("art_") else {
        return false;
    };
    if c == start || c == end {
        return true;
    }
    match (c.parse::<u32>(), start.parse::<u32>(), end.parse::<u32>()) {
        (Ok(c), Ok(a), Ok(b)) => a <= c && c <= b,
        _ => false,
    }
}

fn match_op(op: &AmendmentOp, changes: &BTreeMap<String, ChangedArticle>) -> OpMatch {
    let ids = candidate_ids(op);
    if op.location.part != LocationPart::Main || ids.is_empty() {
        return OpMatch {
            status: "skipped".to_string(),
            article_ids: Vec::new(),
        };
    }
    let changed: Vec<&String> = changes
        .keys()
        .filter(|key| ids.iter().any(|id| id_matches(id, key)))
        .collect();
    if changed.is_empty() {
        return OpMatch {
            status: "unmatched".to_string(),
            article_ids: Vec::new(),
        };
    }
    let from = op.from.as_deref().map(compact);
    let to = op.to.as_deref().map(compact);
    let has = |pool: &[String], needle: &str| pool.iter().any(|t| t.contains(needle));
    let verified = changed.iter().any(|id| {
        let c = &changes[*id];
        match (op.kind, from.as_deref(), to.as_deref()) {
            (OpKind::Replace, Some(f), Some(t)) => has(&c.old, f) && has(&c.new, t),
            // 見出しの差し替え (第○条の見出しを「…」に改める)。
            (OpKind::Replace, None, Some(t)) => has(&c.new, t),
            (OpKind::InsertAfter, Some(f), Some(t)) => has(&c.new, &format!("{f}{t}")),
            (OpKind::InsertBefore, Some(f), Some(t)) => has(&c.new, &format!("{t}{f}")),
            (OpKind::DeleteText, Some(f), _) => has(&c.old, f),
            (OpKind::Add, _, Some(t)) => has(&c.new, t),
            _ => false,
        }
    });
    OpMatch {
        status: if verified { "verified" } else { "article" }.to_string(),
        article_ids: changed.into_iter().cloned().collect(),
    }
}

fn status_rank(s: &str) -> u8 {
    match s {
        "verified" => 4,
        "article" => 3,
        "pending" => 2,
        "unmatched" => 1,
        _ => 0,
    }
}

// ── 実行 ──────────────────────────────────────────────────────────

fn amendment_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(sessions) = std::fs::read_dir(dir) else {
        return files;
    };
    for session in sessions.flatten() {
        let session = session.path();
        if !session.is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(&session).into_iter().flatten().flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with('.') || name == "index.json" || !name.ends_with(".json") {
                continue;
            }
            files.push(path);
        }
    }
    files.sort();
    files
}

/// 1 法案分の対応づけ。改め文に link / match を書き込み、統計に足す。
fn link_bill(
    public: &Path,
    index: &LawIndex,
    bill: &mut BillAmendments,
    stats: &mut LinkStats,
) -> Vec<(String, LinkedBillAmendment)> {
    stats.bills += 1;
    if bill.amendment_law_id.is_some() {
        stats.bills_promulgated += 1;
    }
    let mut linked = Vec::new();
    // 同じ法案の複数の条 (第一条・第二条で段階的に改正) が同じ法令を改正するとき、差分は共通なので
    // 「改め文で説明できた条」は法令単位で合算する。
    let mut explained_by_law: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for target in &mut bill.targets {
        stats.targets += 1;
        target.law_id = index.resolve(target.law_num.as_deref(), &target.law_title);
        let mut link = TargetLink::default();
        let mut changes = BTreeMap::new();
        match (&target.law_id, &bill.amendment_law_id) {
            (None, _) => link.status = "law_unresolved".to_string(),
            (Some(_), None) => {
                stats.targets_resolved += 1;
                link.status = "not_promulgated".to_string();
            }
            (Some(law_id), Some(amendment_law_id)) => {
                stats.targets_resolved += 1;
                let versions: Option<VersionsFile> =
                    std::fs::read(public.join("laws").join(law_id).join("versions.json"))
                        .ok()
                        .and_then(|b| serde_json::from_slice(&b).ok());
                let selected = versions.as_ref().and_then(|v| {
                    select_revisions(&v.versions, amendment_law_id, bill.law_num.as_deref())
                });
                let docs = selected.as_ref().and_then(|sel| {
                    if sel.pairs.is_empty() {
                        return None;
                    }
                    sel.pairs
                        .iter()
                        .map(|(from, to)| {
                            Some((
                                read_revision(public, law_id, from).ok()?,
                                read_revision(public, law_id, to).ok()?,
                            ))
                        })
                        .collect::<Option<Vec<_>>>()
                });
                if let Some(sel) = &selected {
                    link.pending_revision_ids = sel.pending.clone();
                }
                match (selected, docs) {
                    (Some(sel), Some(docs)) => {
                        stats.targets_linked += 1;
                        link.status = "linked".to_string();
                        link.from_revision_id =
                            sel.pairs.first().map(|(from, _)| from.revision_id.clone());
                        link.to_revision_ids = sel
                            .pairs
                            .into_iter()
                            .map(|(_, to)| to.revision_id)
                            .collect();
                        changes = collect_changes(&docs);
                    }
                    (Some(sel), None) if !sel.pending.is_empty() && sel.pairs.is_empty() => {
                        stats.targets_pending += 1;
                        link.status = "pending".to_string();
                    }
                    _ => link.status = "revision_not_found".to_string(),
                }
            }
        }

        let linked_ok = link.status == "linked";
        let mut explained: BTreeSet<String> = BTreeSet::new();
        let (mut ops, mut ops_matched) = (0usize, 0usize);
        let mut sentences = Vec::new();
        for sentence in &mut target.sentences {
            let mut best = "skipped".to_string();
            let mut article_ids: Vec<String> = Vec::new();
            for op in &mut sentence.ops {
                stats.ops += 1;
                ops += 1;
                if !linked_ok {
                    // 差分と照合できない (未成立・未施行) ときは、改め文が指す本則の条だけ示す。
                    op.matched = None;
                    if op.location.part == LocationPart::Main {
                        for id in candidate_ids(op) {
                            if !article_ids.contains(&id) {
                                article_ids.push(id);
                            }
                        }
                    }
                    continue;
                }
                let mut m = match_op(op, &changes);
                // 条が変化していなくても、同じ改正法令の未施行の版 (本文未取得) があれば
                // そちらで反映される見込みなので「未対応」と区別する。
                if m.status == "unmatched" && !link.pending_revision_ids.is_empty() {
                    m.status = "pending".to_string();
                }
                match m.status.as_str() {
                    "verified" => stats.ops_verified += 1,
                    "article" => stats.ops_article += 1,
                    "unmatched" => stats.ops_unmatched += 1,
                    "pending" => stats.ops_pending += 1,
                    _ => stats.ops_skipped += 1,
                }
                if matches!(m.status.as_str(), "verified" | "article") {
                    ops_matched += 1;
                }
                for id in &m.article_ids {
                    explained.insert(id.clone());
                    if !article_ids.contains(id) {
                        article_ids.push(id.clone());
                    }
                }
                if status_rank(&m.status) > status_rank(&best) {
                    best = m.status.clone();
                }
                op.matched = Some(m);
            }
            sentences.push(LinkedSentence {
                text: sentence.text.clone(),
                article_ids,
                status: if linked_ok { best } else { link.status.clone() },
            });
        }
        if linked_ok {
            link.changed_articles = changes.keys().cloned().collect();
            if let Some(law_id) = &target.law_id {
                explained_by_law
                    .entry(law_id.clone())
                    .or_default()
                    .extend(explained);
            }
        }
        if let Some(law_id) = &target.law_id {
            linked.push((
                law_id.clone(),
                LinkedBillAmendment {
                    bill_id: bill.bill_id.clone(),
                    session: bill.session,
                    bill_title: bill.title.clone(),
                    law_num: bill.law_num.clone(),
                    amendment_law_id: bill.amendment_law_id.clone(),
                    provision: target.provision.clone(),
                    status: link.status.clone(),
                    from_revision_id: link.from_revision_id.clone(),
                    to_revision_ids: link.to_revision_ids.clone(),
                    source_url: bill.source_url.clone(),
                    path: format!("gian/amendments/{}/{}.json", bill.session, bill.bill_id),
                    ops,
                    ops_matched,
                    sentences,
                },
            ));
        }
        target.link = Some(link);
    }
    let mut counted: BTreeSet<String> = BTreeSet::new();
    for target in &mut bill.targets {
        let (Some(law_id), Some(link)) = (&target.law_id, target.link.as_mut()) else {
            continue;
        };
        let Some(explained) = explained_by_law.get(law_id) else {
            continue;
        };
        link.explained_articles = link
            .changed_articles
            .iter()
            .filter(|id| explained.contains(*id))
            .cloned()
            .collect();
        if counted.insert(law_id.clone()) {
            stats.changed_articles += link.changed_articles.len();
            stats.explained_articles += link.explained_articles.len();
        }
    }
    linked
}

/// `lawpub link-bill-amendments` — 改め文と law-diff の対応づけ。
pub fn run_link_bill_amendments(public: &Path) -> Result<()> {
    let src = public.join("gian").join("amendments");
    if !src.is_dir() {
        tracing::warn!("no {}; skipping bill amendment linking", src.display());
        return Ok(());
    }
    let index = LawIndex::load(public)?;
    let out_dir = public.join("links").join("law-to-bill-amendments");
    if out_dir.exists() {
        std::fs::remove_dir_all(&out_dir)?;
    }
    std::fs::create_dir_all(&out_dir)?;

    let mut stats = LinkStats::default();
    let mut by_law: BTreeMap<String, Vec<LinkedBillAmendment>> = BTreeMap::new();
    for path in amendment_files(&src) {
        let mut bill: BillAmendments = match std::fs::read(&path)
            .map_err(anyhow::Error::from)
            .and_then(|b| serde_json::from_slice(&b).map_err(anyhow::Error::from))
        {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("skip {}: {e:#}", path.display());
                continue;
            }
        };
        for (law_id, entry) in link_bill(public, &index, &mut bill, &mut stats) {
            by_law.entry(law_id).or_default().push(entry);
        }
        std::fs::write(&path, serde_json::to_string_pretty(&bill)?)
            .with_context(|| format!("write {}", path.display()))?;
    }
    for (law_id, mut bills) in by_law {
        // 新しい国会回次から。
        bills.sort_by(|a, b| {
            b.session
                .cmp(&a.session)
                .then_with(|| a.bill_id.cmp(&b.bill_id))
        });
        std::fs::write(
            out_dir.join(format!("{law_id}.json")),
            serde_json::to_string_pretty(&LawToBillAmendments {
                schema_version: 1,
                law_id: law_id.clone(),
                bills,
            })?,
        )?;
    }
    let decided = stats.ops_verified + stats.ops_article + stats.ops_unmatched;
    let rate = |n: usize, d: usize| {
        if d == 0 {
            None
        } else {
            Some((n as f64 / d as f64 * 1000.0).round() / 1000.0)
        }
    };
    std::fs::write(
        out_dir.join(SUMMARY_FILE),
        serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": 1,
            "stats": stats,
            // 本則の条を指す操作のうち、差分の変化と対応がついた割合。
            "op_match_rate": rate(stats.ops_verified + stats.ops_article, decided),
            "op_verified_rate": rate(stats.ops_verified, decided),
            // 差分で変化した条のうち、改め文の操作で説明できた割合。
            "article_explained_rate": rate(stats.explained_articles, stats.changed_articles),
        }))?,
    )?;
    tracing::info!(
        "link-bill-amendments: bills={} targets linked={}/{} ops verified={} article={} unmatched={} skipped={}",
        stats.bills,
        stats.targets_linked,
        stats.targets,
        stats.ops_verified,
        stats.ops_article,
        stats.ops_unmatched,
        stats.ops_skipped
    );
    Ok(())
}

/// health.json に載せる対応状況 (summary が無ければ null)。
pub fn bill_amendments_health(public: &Path) -> serde_json::Value {
    let Some(v) = std::fs::read(
        public
            .join("links")
            .join("law-to-bill-amendments")
            .join(SUMMARY_FILE),
    )
    .ok()
    .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok()) else {
        return serde_json::Value::Null;
    };
    serde_json::json!({
        "bills": v["stats"]["bills"],
        "targets": v["stats"]["targets"],
        "targets_linked": v["stats"]["targets_linked"],
        "ops_unmatched": v["stats"]["ops_unmatched"],
        "ops_pending": v["stats"]["ops_pending"],
        "op_match_rate": v["op_match_rate"],
        "article_explained_rate": v["article_explained_rate"],
        "summary": format!("links/law-to-bill-amendments/{SUMMARY_FILE}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use law_normalizer::{Article, Paragraph};

    fn doc(rev: &str, articles: &[(&str, &str)]) -> LawDocument {
        let mut d: LawDocument = serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "law_id": "326AC0000000053",
            "law_num": "昭和二十六年法律第五十三号",
            "title": "裁判所職員定員法",
            "revision_id": rev,
            "promulgation_date": null,
            "effective_date": null,
            "status": "current",
            "articles": [],
            "suppl_provisions": [],
            "source": {"provider": "test", "fetched_at": "2026-01-01T00:00:00Z", "raw_xml_sha256": null}
        }))
        .unwrap();
        d.articles = articles
            .iter()
            .map(|(num, text)| Article {
                article_id: format!("art_{num}"),
                article_no: format!("第{num}条"),
                caption: None,
                paragraphs: vec![Paragraph {
                    paragraph_no: None,
                    paragraph_num: Some("1".into()),
                    text: text.to_string(),
                    items: Vec::new(),
                }],
            })
            .collect();
        d
    }

    fn write(path: &Path, value: &serde_json::Value) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
    }

    #[test]
    fn selects_previous_and_staged_revisions() {
        let v = |id: &str, date: &str, amend: &str| VersionEntry {
            revision_id: id.into(),
            effective_date: Some(date.into()),
            amendment_law_id: Some(amend.into()),
            amendment_law_num: None,
            body_available: true,
            path: None,
        };
        let versions = vec![
            v(
                "L_20250418_507AC0000000023",
                "2025-04-18",
                "507AC0000000023",
            ),
            v(
                "L_20260507_508AC0000000014",
                "2026-05-07",
                "508AC0000000014",
            ),
            v(
                "L_20270401_508AC0000000014",
                "2027-04-01",
                "508AC0000000014",
            ),
        ];
        let sel = select_revisions(&versions, "508AC0000000014", None).unwrap();
        let ids: Vec<(&str, &str)> = sel
            .pairs
            .iter()
            .map(|(a, b)| (a.revision_id.as_str(), b.revision_id.as_str()))
            .collect();
        assert_eq!(
            ids,
            vec![
                ("L_20250418_507AC0000000023", "L_20260507_508AC0000000014"),
                ("L_20260507_508AC0000000014", "L_20270401_508AC0000000014"),
            ]
        );
        assert!(sel.pending.is_empty());
        assert!(
            select_revisions(&versions, "507AC0000000023", None)
                .unwrap()
                .pairs
                .is_empty(),
            "最初の版は直前が無い"
        );
        assert!(select_revisions(&versions, "509AC0000000001", None).is_none());

        // 未施行で本文の無い版は pending に分ける。
        let mut unenforced = v(
            "L_20280401_508AC0000000014",
            "2028-04-01",
            "508AC0000000014",
        );
        unenforced.body_available = false;
        let mut with_pending = versions.clone();
        with_pending.push(unenforced);
        let sel = select_revisions(&with_pending, "508AC0000000014", None).unwrap();
        assert_eq!(sel.pairs.len(), 2);
        assert_eq!(sel.pending, vec!["L_20280401_508AC0000000014".to_string()]);
    }

    /// 実データ (第221回 閣法第19号 裁判所職員定員法の一部を改正する法律案) の改め文を、
    /// 公布前後の版の差分と対応づける。
    #[test]
    fn links_real_bill_to_revision_diff() {
        let root = std::env::temp_dir().join(format!(
            "linking-bill-amendments-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let public = root.join("public");
        let law_id = "326AC0000000053";
        write(
            &public.join("laws/index.json"),
            &serde_json::json!({"laws": [{"law_id": law_id, "law_num": "昭和二十六年法律第五十三号", "title": "裁判所職員定員法"}]}),
        );
        let old_rev = format!("{law_id}_20250418_507AC0000000023");
        let new_rev = format!("{law_id}_20260507_508AC0000000014");
        write(
            &public.join(format!("laws/{law_id}/versions.json")),
            &serde_json::json!({"law_id": law_id, "versions": [
                {"revision_id": old_rev, "effective_date": "2025-04-18", "amendment_law_id": "507AC0000000023", "body_available": true},
                {"revision_id": new_rev, "effective_date": "2026-05-07", "amendment_law_id": "508AC0000000014", "body_available": true},
            ]}),
        );
        let article1 = "下級裁判所の裁判官の員数は、次の表のとおりとする。";
        write(
            &public.join(format!("laws/{law_id}/revisions/{old_rev}.json")),
            &serde_json::to_value(doc(
                &old_rev,
                &[
                    ("1", article1),
                    (
                        "2",
                        "裁判官以外の裁判所の職員の員数は、二万千六百六十六人とする。",
                    ),
                ],
            ))
            .unwrap(),
        );
        write(
            &public.join(format!("laws/{law_id}/revisions/{new_rev}.json")),
            &serde_json::to_value(doc(
                &new_rev,
                &[
                    ("1", article1),
                    (
                        "2",
                        "裁判官以外の裁判所の職員の員数は、二万千五百四十人とする。",
                    ),
                ],
            ))
            .unwrap(),
        );

        let html = include_str!("../../gian-client/tests/fixtures/shugiin_g22109019.htm");
        let text = gian_client::extract_document_text(html);
        let targets = gian_client::parse_amendment_text(&text);
        let bill = BillAmendments {
            schema_version: 1,
            bill_id: "1DE1F02".into(),
            session: 221,
            title: "裁判所職員定員法の一部を改正する法律案".into(),
            bill_type: Some("閣法".into()),
            law_num: Some("令和八年法律第十四号".into()),
            amendment_law_id: Some("508AC0000000014".into()),
            promulgation_date: Some("2026-05-07".into()),
            source_url: "https://www.shugiin.go.jp/internet/itdb_gian.nsf/html/gian/honbun/houan/g22109019.htm".into(),
            source_sha256: "x".into(),
            stats: gian_client::amendment::stats_of(&targets),
            targets,
        };
        write(
            &public.join("gian/amendments/221/1DE1F02.json"),
            &serde_json::to_value(&bill).unwrap(),
        );

        run_link_bill_amendments(&public).unwrap();

        let linked: BillAmendments = serde_json::from_slice(
            &std::fs::read(public.join("gian/amendments/221/1DE1F02.json")).unwrap(),
        )
        .unwrap();
        let target = &linked.targets[0];
        assert_eq!(target.law_id.as_deref(), Some(law_id));
        let link = target.link.as_ref().unwrap();
        assert_eq!(link.status, "linked");
        assert_eq!(link.from_revision_id.as_deref(), Some(old_rev.as_str()));
        assert_eq!(link.explained_articles, vec!["art_2".to_string()]);
        let m = target.sentences[0].ops[0].matched.as_ref().unwrap();
        assert_eq!(m.status, "verified");
        assert_eq!(m.article_ids, vec!["art_2".to_string()]);

        let by_law: LawToBillAmendments = serde_json::from_slice(
            &std::fs::read(public.join(format!("links/law-to-bill-amendments/{law_id}.json")))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(by_law.bills[0].to_revision_ids, vec![new_rev.clone()]);
        assert_eq!(
            by_law.bills[0].sentences[0].article_ids,
            vec!["art_2".to_string()]
        );
        let summary: serde_json::Value = serde_json::from_slice(
            &std::fs::read(public.join("links/law-to-bill-amendments/_summary.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(summary["stats"]["ops_verified"], 1);
        assert_eq!(summary["op_match_rate"], 1.0);
        assert_eq!(bill_amendments_health(&public)["targets_linked"], 1);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unchanged_article_is_unmatched_and_suppl_is_skipped() {
        let mut changes = BTreeMap::new();
        changes.insert(
            "art_3".to_string(),
            ChangedArticle {
                old: vec!["前項".into()],
                new: vec!["第一項".into()],
            },
        );
        let targets = gian_client::parse_amendment_text(
            "X法（令和元年法律第一号）の一部を次のように改正する。\n第二条中「A」を「B」に改める。\n第三条中「前項」を「第一項」に改める。\n附則第四条を削る。",
        );
        let ops: Vec<&AmendmentOp> = targets[0].sentences.iter().flat_map(|s| &s.ops).collect();
        assert_eq!(match_op(ops[0], &changes).status, "unmatched");
        assert_eq!(match_op(ops[1], &changes).status, "verified");
        assert_eq!(match_op(ops[2], &changes).status, "skipped");
    }

    #[test]
    fn deleted_article_ranges_match_by_range() {
        assert!(id_matches("art_172", "art_172:177"));
        assert!(id_matches("art_175", "art_172:177"));
        assert!(id_matches("art_177_5", "art_177_3:177_5"));
        assert!(!id_matches("art_178", "art_172:177"));
        assert!(!id_matches("art_17", "art_172"));
    }
}

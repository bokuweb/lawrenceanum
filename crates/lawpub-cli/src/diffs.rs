//! `public/laws/{id}/diff/{from}..{to}.json` と
//! `public/laws/{id}/diffs.json` を生成する。
//!
//! 入力: `public/laws/index.json` と各 `versions.json`。
//! 各 versions.json のうち `body_available: true` の revision を時系列順に並べ、
//! 隣接ペアの diff を生成する。
//!
//! 隣接 diff の選択方針:
//!   - effective_date 昇順で並べ替え (null は末尾)
//!   - 連続するペアについて from→to の diff を書き出す
//!   - 既に同じファイルが存在し、from/to の revision_id が一致するならスキップ
//!
//! 大量に走るので並列化は後回し (まず実装、計測してから rayon)。

use anyhow::{Context, Result};
use law_diff::diff_documents;
use law_normalizer::LawDocument;
use serde::Deserialize;
use serde_json::json;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct LawsIndex {
    laws: Vec<LawEntry>,
}

#[derive(Debug, Deserialize)]
struct LawEntry {
    law_id: String,
}

#[derive(Debug, Deserialize)]
struct VersionsFile {
    #[allow(dead_code)]
    law_id: String,
    versions: Vec<VersionEntry>,
}

#[derive(Debug, Deserialize, Clone)]
struct VersionEntry {
    revision_id: String,
    #[serde(default)]
    effective_date: Option<String>,
    #[serde(default)]
    promulgation_date: Option<String>,
    #[serde(default)]
    body_available: bool,
}

pub fn run_build_diffs(public: &Path) -> Result<()> {
    let index_path = public.join("laws").join("index.json");
    let index_bytes =
        fs::read(&index_path).with_context(|| format!("read {}", index_path.display()))?;
    let index: LawsIndex = serde_json::from_slice(&index_bytes)
        .with_context(|| format!("parse {}", index_path.display()))?;

    let mut total_diffs = 0usize;
    let mut total_laws_with_diffs = 0usize;

    for entry in &index.laws {
        let n = build_diffs_for_law(public, &entry.law_id)?;
        if n > 0 {
            total_laws_with_diffs += 1;
            total_diffs += n;
        }
    }

    tracing::info!(
        "build-diffs done: {} diffs across {} laws",
        total_diffs,
        total_laws_with_diffs
    );
    Ok(())
}

/// 指定法令について隣接 diff を生成し、diffs.json を書き出す。
/// 返り値 = 書き出した diff ファイル数。
fn build_diffs_for_law(public: &Path, law_id: &str) -> Result<usize> {
    let law_dir = public.join("laws").join(law_id);
    let versions_path = law_dir.join("versions.json");
    if !versions_path.exists() {
        return Ok(0);
    }
    let versions: VersionsFile = serde_json::from_slice(&fs::read(&versions_path)?)
        .with_context(|| format!("parse {}", versions_path.display()))?;

    // body_available のみ抽出
    let mut bodied: Vec<VersionEntry> = versions
        .versions
        .into_iter()
        .filter(|v| v.body_available)
        .collect();

    // effective_date 昇順 (null は末尾)
    bodied.sort_by(
        |a, b| match (a.effective_date.as_deref(), b.effective_date.as_deref()) {
            (Some(x), Some(y)) => x.cmp(y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.revision_id.cmp(&b.revision_id),
        },
    );

    if bodied.len() < 2 {
        // 1 件以下なら diff 不要 (diffs.json も書かない)
        return Ok(0);
    }

    let diff_dir = law_dir.join("diff");
    fs::create_dir_all(&diff_dir)?;

    let mut diffs_entries: Vec<serde_json::Value> = Vec::new();
    let mut written = 0usize;

    for pair in bodied.windows(2) {
        let from = &pair[0];
        let to = &pair[1];
        let file_name = format!("{}..{}.json", from.revision_id, to.revision_id);
        let out_path = diff_dir.join(&file_name);

        // 既存ファイルが同じ from/to を持つなら skip (incremental)
        if out_path.exists() {
            // 軽量チェック: from/to の revision_id だけ確認
            if let Ok(bytes) = fs::read(&out_path) {
                if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                    let f = v
                        .get("from")
                        .and_then(|x| x.get("revision_id"))
                        .and_then(|x| x.as_str());
                    let t = v
                        .get("to")
                        .and_then(|x| x.get("revision_id"))
                        .and_then(|x| x.as_str());
                    if f == Some(from.revision_id.as_str()) && t == Some(to.revision_id.as_str()) {
                        diffs_entries.push(diffs_index_entry(law_id, from, to, &file_name, &v));
                        continue;
                    }
                }
            }
        }

        // 本文を読み込んで diff。万一 body_available と実ファイルがずれていても
        // (データ不整合) その pair を skip して build 全体を止めない (防御)。
        let from_doc = match read_revision(public, law_id, &from.revision_id) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!(
                    "skip diff pair (missing from-body) {}: {e:#}",
                    from.revision_id
                );
                continue;
            }
        };
        let to_doc = match read_revision(public, law_id, &to.revision_id) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!("skip diff pair (missing to-body) {}: {e:#}", to.revision_id);
                continue;
            }
        };
        let diff = diff_documents(&from_doc, &to_doc, false);
        let value = serde_json::to_value(&diff)?;
        write_json_pretty(&out_path, &value)?;
        written += 1;

        diffs_entries.push(diffs_index_entry(law_id, from, to, &file_name, &value));
    }

    // diffs.json
    let diffs_index = json!({
        "law_id": law_id,
        "diffs": diffs_entries,
    });
    write_json_pretty(&law_dir.join("diffs.json"), &diffs_index)?;

    Ok(written)
}

fn diffs_index_entry(
    law_id: &str,
    from: &VersionEntry,
    to: &VersionEntry,
    file_name: &str,
    diff_value: &serde_json::Value,
) -> serde_json::Value {
    let summary = diff_value.get("summary").cloned().unwrap_or(json!(null));
    json!({
        "from_revision_id": from.revision_id,
        "to_revision_id": to.revision_id,
        "from_effective_date": from.effective_date,
        "to_effective_date": to.effective_date,
        "from_promulgation_date": from.promulgation_date,
        "to_promulgation_date": to.promulgation_date,
        "path": format!("laws/{}/diff/{}", law_id, file_name),
        "summary": summary,
    })
}

fn read_revision(public: &Path, law_id: &str, rev_id: &str) -> Result<LawDocument> {
    let p = public
        .join("laws")
        .join(law_id)
        .join("revisions")
        .join(format!("{}.json", rev_id));
    let bytes = fs::read(&p).with_context(|| format!("read {}", p.display()))?;
    let doc: LawDocument =
        serde_json::from_slice(&bytes).with_context(|| format!("parse {}", p.display()))?;
    Ok(doc)
}

fn write_json_pretty(path: &Path, value: &serde_json::Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(value)?;
    fs::write(path, bytes)?;
    Ok(())
}

/// 任意の 2 revision の単発 diff を stdout に出す (CLI `lawpub diff` 用)。
pub fn run_diff_pair(public: &Path, law_id: &str, from_rev: &str, to_rev: &str) -> Result<()> {
    let from_doc = read_revision(public, law_id, from_rev)?;
    let to_doc = read_revision(public, law_id, to_rev)?;
    let diff = diff_documents(&from_doc, &to_doc, false);
    let value = serde_json::to_value(&diff)?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

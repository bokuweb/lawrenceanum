//! 任意日付スナップショット `laws/{id}/at/{yyyy-mm-dd}.json` 生成。
//!
//! 各法令の versions.json を読み、`effective_date <= as_of` を満たす最新 revision を
//! 選んで軽量リダイレクト JSON を書き出す。
//!
//! `include_unenforced = true` のときは `promulgation_date <= as_of` を使う
//! (= 公布済み・未施行を含める)。
//!
//! 廃止 (`repeal_status` が None 以外) の revision が以降に存在する場合の
//! 扱いは MVP では考慮しない (= 単純に最新有効版を返す)。

use anyhow::{Context, Result};
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

pub fn run_build_snapshots(
    public: &Path,
    dates: &[String],
    include_unenforced: bool,
) -> Result<()> {
    if dates.is_empty() {
        anyhow::bail!("at least one --dates value is required");
    }
    for d in dates {
        validate_date(d)?;
    }

    let index_path = public.join("laws").join("index.json");
    let index: LawsIndex = serde_json::from_slice(&fs::read(&index_path)?)
        .with_context(|| format!("parse {}", index_path.display()))?;

    let mut written = 0usize;
    for entry in &index.laws {
        for d in dates {
            if write_snapshot_for(public, &entry.law_id, d, include_unenforced)? {
                written += 1;
            }
        }
    }

    tracing::info!("build-snapshots done: {} snapshot files written", written);
    Ok(())
}

fn write_snapshot_for(
    public: &Path,
    law_id: &str,
    as_of: &str,
    include_unenforced: bool,
) -> Result<bool> {
    let versions_path = public.join("laws").join(law_id).join("versions.json");
    if !versions_path.exists() {
        return Ok(false);
    }
    let versions: VersionsFile = serde_json::from_slice(&fs::read(&versions_path)?)
        .with_context(|| format!("parse {}", versions_path.display()))?;

    let resolved = resolve(&versions.versions, as_of, include_unenforced);

    let at_dir = public.join("laws").join(law_id).join("at");
    fs::create_dir_all(&at_dir)?;
    let out_path = at_dir.join(format!("{}.json", as_of));

    let value = match resolved {
        Some(v) => json!({
            "law_id": law_id,
            "as_of": as_of,
            "include_unenforced": include_unenforced,
            "resolved_revision_id": v.revision_id,
            "effective_date": v.effective_date,
            "promulgation_date": v.promulgation_date,
            "body_available": v.body_available,
            "current": if v.body_available {
                serde_json::Value::String(format!("laws/{}/revisions/{}.json", law_id, v.revision_id))
            } else {
                serde_json::Value::Null
            },
        }),
        None => json!({
            "law_id": law_id,
            "as_of": as_of,
            "include_unenforced": include_unenforced,
            "resolved_revision_id": null,
            "status": "not_yet_effective_or_unknown",
        }),
    };

    let bytes = serde_json::to_vec_pretty(&value)?;
    fs::write(&out_path, bytes)?;
    Ok(true)
}

fn resolve(
    versions: &[VersionEntry],
    as_of: &str,
    include_unenforced: bool,
) -> Option<VersionEntry> {
    // 候補日 (施行日 or 公布日) を取り出して as_of 以前で最大のものを選ぶ
    let mut best: Option<&VersionEntry> = None;
    let mut best_key: Option<&str> = None;
    for v in versions {
        // include_unenforced: 公布済みになった時点で「見える」とみなす
        //   → promulgation_date を優先 (なければ effective_date)
        // 通常: 施行されていることが条件
        //   → effective_date のみ
        let key = if include_unenforced {
            v.promulgation_date
                .as_deref()
                .or(v.effective_date.as_deref())
        } else {
            v.effective_date.as_deref()
        };
        let Some(k) = key else { continue };
        if k > as_of {
            continue;
        }
        match best_key {
            None => {
                best = Some(v);
                best_key = Some(k);
            }
            Some(b) if k > b => {
                best = Some(v);
                best_key = Some(k);
            }
            _ => {}
        }
    }
    best.cloned()
}

fn validate_date(s: &str) -> Result<()> {
    // YYYY-MM-DD のみ受ける (chrono は workspace dep として既にある)
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .with_context(|| format!("invalid date: {}", s))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(rev: &str, eff: Option<&str>, prom: Option<&str>, body: bool) -> VersionEntry {
        VersionEntry {
            revision_id: rev.to_string(),
            effective_date: eff.map(|s| s.to_string()),
            promulgation_date: prom.map(|s| s.to_string()),
            body_available: body,
        }
    }

    #[test]
    fn resolves_latest_effective_at_date() {
        let vs = vec![
            v("r1", Some("2017-06-01"), Some("2017-05-01"), true),
            v("r2", Some("2020-04-01"), Some("2019-12-01"), true),
            v("r3", Some("2024-04-01"), Some("2023-12-01"), true),
        ];
        let got = resolve(&vs, "2021-01-01", false).unwrap();
        assert_eq!(got.revision_id, "r2");
    }

    #[test]
    fn ignores_future_revisions() {
        let vs = vec![
            v("r1", Some("2017-06-01"), None, true),
            v("r2", Some("2030-04-01"), None, true),
        ];
        let got = resolve(&vs, "2021-01-01", false).unwrap();
        assert_eq!(got.revision_id, "r1");
    }

    #[test]
    fn include_unenforced_uses_promulgation() {
        let vs = vec![
            v("r1", Some("2017-06-01"), Some("2017-05-01"), true),
            // 未施行: effective_date=2030 だが promulgation=2024
            v("r2", Some("2030-04-01"), Some("2024-06-01"), true),
        ];
        let strict = resolve(&vs, "2025-01-01", false).unwrap();
        assert_eq!(strict.revision_id, "r1");
        let loose = resolve(&vs, "2025-01-01", true).unwrap();
        assert_eq!(loose.revision_id, "r2");
    }

    #[test]
    fn returns_none_before_any_effective_date() {
        let vs = vec![v("r1", Some("2020-01-01"), None, true)];
        assert!(resolve(&vs, "2019-01-01", false).is_none());
    }
}

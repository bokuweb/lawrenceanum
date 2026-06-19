//! `lawpub build-enforcement` — 各法令の timeline から「今後の施行予定」を集約し
//! `public/enforcement/upcoming.json` を書く。施行期日カレンダーのデータ源。
//!
//! 既存の timeline.json（effective_date / scheduled_enforcement_date）から導出するだけ
//! で、新規収集は不要。士業・法務の「この改正はいつ施行か」を一覧できる。

use anyhow::{Context, Result};
use chrono::Utc;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

const MAX_ITEMS: usize = 1000;

#[derive(Debug, Serialize)]
struct EnforcementItem {
    /// 施行(予定)日 ISO。
    date: String,
    law_id: String,
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    amending_law_title: Option<String>,
    /// "scheduled"(施行予定日) / "effective"(施行日)。
    date_kind: String,
}

fn read_json(path: &Path) -> Option<serde_json::Value> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// 法令 id → タイトル（laws/index.json から）。
fn law_titles(public: &Path) -> HashMap<String, String> {
    read_json(&public.join("laws").join("index.json"))
        .and_then(|v| v.get("laws").and_then(|l| l.as_array()).cloned())
        .map(|laws| {
            laws.iter()
                .filter_map(|l| {
                    Some((
                        l.get("law_id")?.as_str()?.to_string(),
                        l.get("title")?.as_str()?.to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn run_build(public: &Path) -> Result<()> {
    let today = Utc::now().date_naive().format("%Y-%m-%d").to_string();
    let titles = law_titles(public);
    let laws_dir = public.join("laws");

    let mut items: Vec<EnforcementItem> = Vec::new();
    if laws_dir.exists() {
        for entry in std::fs::read_dir(&laws_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let tl_path = entry.path().join("timeline.json");
            let Some(tl) = read_json(&tl_path) else { continue };
            let law_id = tl.get("law_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if law_id.is_empty() {
                continue;
            }
            let Some(events) = tl.get("events").and_then(|e| e.as_array()) else {
                continue;
            };
            for ev in events {
                // 施行予定日を優先、無ければ施行日。未来日付のみ。
                let (date, kind) = match ev
                    .get("scheduled_enforcement_date")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                {
                    Some(d) => (d.to_string(), "scheduled"),
                    None => match ev.get("effective_date").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                        Some(d) => (d.to_string(), "effective"),
                        None => continue,
                    },
                };
                if date <= today {
                    continue; // 今後の施行のみ。
                }
                items.push(EnforcementItem {
                    date,
                    law_id: law_id.clone(),
                    title: titles.get(&law_id).cloned().unwrap_or_default(),
                    amending_law_title: ev
                        .get("amending_law_title")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                        .map(String::from),
                    date_kind: kind.to_string(),
                });
            }
        }
    }

    // 施行日が近い順（昇順）。
    items.sort_by(|a, b| a.date.cmp(&b.date).then(a.law_id.cmp(&b.law_id)));
    items.truncate(MAX_ITEMS);

    let out_dir = public.join("enforcement");
    std::fs::create_dir_all(&out_dir)?;
    let out = serde_json::json!({
        "schema_version": 1,
        "generated_at": Utc::now().to_rfc3339(),
        "as_of": today,
        "count": items.len(),
        "items": items,
    });
    std::fs::write(out_dir.join("upcoming.json"), serde_json::to_string_pretty(&out)?)
        .context("write enforcement/upcoming.json")?;
    tracing::info!("build-enforcement: {} upcoming enforcements", items.len());
    Ok(())
}

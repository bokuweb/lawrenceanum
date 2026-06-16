use anyhow::{Context, Result};
use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatestState {
    pub version: u32,
    pub latest_successful_update_date: Option<String>,
    pub last_run_at: Option<String>,
    pub last_run_status: Option<String>,
    /// 直近で正常に deploy した法令数。git 追跡される state/latest.json に残るため、
    /// R2 (revisions_meta) が落ちていても参照できる「壊滅的縮小ガード」の基準線。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub law_count: Option<usize>,
}

impl Default for LatestState {
    fn default() -> Self {
        Self {
            version: 1,
            latest_successful_update_date: None,
            last_run_at: None,
            last_run_status: None,
            law_count: None,
        }
    }
}

pub fn load(path: &Path) -> Result<LatestState> {
    if !path.exists() {
        return Ok(LatestState::default());
    }
    let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    Ok(serde_json::from_slice(&bytes)?)
}

pub fn save(path: &Path, state: &LatestState) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(state)?;
    std::fs::write(path, json)?;
    Ok(())
}

/// `state.latest_successful_update_date - 3 days .. today(JST)` の幅で取得する範囲を決定する。
/// 日付が一つも無いときは `today` 単体を返す。
pub fn pick_dates(state: &LatestState, today: NaiveDate) -> Vec<String> {
    let from = match state
        .latest_successful_update_date
        .as_deref()
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
    {
        Some(d) => d - chrono::Duration::days(3),
        None => today,
    };
    let mut out = Vec::new();
    let mut cur = from;
    while cur <= today {
        out.push(cur.format("%Y-%m-%d").to_string());
        cur += chrono::Duration::days(1);
    }
    out
}

pub fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

/// `lawpub update` の 1 回ごとの結果。CI から `jq -r .changed state/last_run.json`
/// で参照することを想定。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunReport {
    pub version: u32,
    pub ran_at: String,
    pub provider: String,
    pub dates: Vec<String>,
    pub new_xmls: usize,
    pub errors: Vec<String>,
    /// 公開ツリー (public/) を再生成したか。
    /// 新たな revision が来た場合 / public/ が壊れている (or 存在しない) 場合に true。
    pub changed: bool,
}

pub fn save_run_report(path: &Path, report: &RunReport) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(report)?)?;
    Ok(())
}

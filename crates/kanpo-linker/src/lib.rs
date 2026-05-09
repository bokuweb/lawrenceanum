//! e-Gov 改正イベント ↔ 官報PDF のマッチング。
//!
//! スコアリング (docs/plan.md §8.4):
//!   +0.40 公布日一致 / +0.35 法令番号一致 / +0.15 法令名一致 / +0.10 改正対象法令名一致
//! `confidence >= 0.80` で自動リンク扱い。

use kanpo_client::KanpoIssue;
use serde::Serialize;

pub const AUTO_LINK_THRESHOLD: f64 = 0.80;

#[derive(Debug, Clone, Serialize)]
pub struct MatchResult {
    pub confidence: f64,
    pub match_reasons: Vec<&'static str>,
}

pub fn score(promulgation_match: bool, law_num_match: bool, title_match: bool, target_match: bool) -> f64 {
    let mut s = 0.0;
    if promulgation_match { s += 0.40 }
    if law_num_match      { s += 0.35 }
    if title_match        { s += 0.15 }
    if target_match       { s += 0.10 }
    s
}

/// イベント (改正情報) と1つの官報号を突合する。
pub fn match_event(
    event_promulgation_date: Option<&str>,
    event_law_num: Option<&str>,
    event_title: Option<&str>,
    target_law_title: Option<&str>,
    issue: &KanpoIssue,
) -> MatchResult {
    let mut reasons: Vec<&'static str> = Vec::new();

    let pd_match = event_promulgation_date
        .map(|d| d == issue.promulgation_date)
        .unwrap_or(false);
    if pd_match { reasons.push("promulgation_date"); }

    let ln_match = event_law_num
        .map(|n| issue.law_nums.iter().any(|x| x == n))
        .unwrap_or(false);
    if ln_match { reasons.push("law_num"); }

    let title_match = event_title
        .map(|t| issue.titles.iter().any(|x| x == t))
        .unwrap_or(false);
    if title_match { reasons.push("title"); }

    let target_match = target_law_title
        .map(|t| issue.titles.iter().any(|x| x.contains(t)))
        .unwrap_or(false);
    if target_match { reasons.push("target_law"); }

    MatchResult {
        confidence: score(pd_match, ln_match, title_match, target_match),
        match_reasons: reasons,
    }
}

//! e-Stat 統計 API クライアント。
//!
//! API キー (`LAWPUB_ESTAT_APP_ID`) が必要（無料登録）。
//! <https://www.e-stat.go.jp/api/>
//!
//! ## 対象統計（財政関連）
//!
//! | stats_data_id | 名称 |
//! |---|---|
//! | 0003410379 | 財政統計（一般会計）|
//! | 0003193543 | 国有財産統計 |
//! | 0003214957 | 法人企業統計調査（年次別） |
//!
//! ## エンドポイント
//!
//! `GET https://api.e-stat.go.jp/rest/3.0/app/json/getStatsData`
//! パラメータ: `appId`, `statsDataId`, `lang=J`, `metaGetFlg=N`, `cntGetFlg=N`

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const BASE_URL: &str = "https://api.e-stat.go.jp/rest/3.0/app/json";

/// e-Stat で財政データとして追跡する統計表。
pub const FISCAL_STATS: &[(&str, &str)] = &[
    ("0003410379", "\u{8ca1}\u{653f}\u{7d71}\u{8a08}（\u{4e00}\u{822c}\u{4f1a}\u{8a08}）"), // 財政統計（一般会計）
    ("0003193543", "\u{56fd}\u{6709}\u{8ca1}\u{7523}\u{7d71}\u{8a08}"),   // 国有財産統計
    ("0003214957", "\u{6cd5}\u{4eba}\u{4f01}\u{696d}\u{7d71}\u{8a08}\u{8abf}\u{67fb}"), // 法人企業統計調査
];

// ── 公開型 ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatValue {
    pub area: Option<String>,
    pub time: Option<String>,
    pub category: Option<String>,
    pub value: String,
    pub unit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatDataset {
    pub schema_version: u32,
    pub stats_data_id: String,
    pub title: String,
    pub values: Vec<StatValue>,
    pub source: StatSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatSource {
    pub provider: String,
    pub fetched_at: String,
    pub stats_data_id: String,
}

// ── Provider trait ────────────────────────────────────────────────

pub trait EstatProvider: Send + Sync {
    fn fetch_stats(&self, stats_data_id: &str, title: &str) -> Result<StatDataset>;
}

// ── Mock ─────────────────────────────────────────────────────────

pub struct MockProvider;

impl EstatProvider for MockProvider {
    fn fetch_stats(&self, stats_data_id: &str, title: &str) -> Result<StatDataset> {
        Ok(StatDataset {
            schema_version: 1,
            stats_data_id: stats_data_id.to_string(),
            title: title.to_string(),
            values: vec![
                StatValue {
                    area: Some("\u{5168}\u{56fd}".to_string()), // 全国
                    time: Some("2023".to_string()),
                    category: Some("\u{6b73}\u{51fa}".to_string()), // 歳出
                    value: "107528600".to_string(),
                    unit: Some("\u{767e}\u{4e07}\u{5186}".to_string()), // 百万円
                },
            ],
            source: StatSource {
                provider: "estat".to_string(),
                fetched_at: "2024-01-01T00:00:00Z".to_string(),
                stats_data_id: stats_data_id.to_string(),
            },
        })
    }
}

// ── Http ─────────────────────────────────────────────────────────

pub struct HttpProvider {
    base_url: String,
    app_id: String,
}

impl HttpProvider {
    pub fn new() -> Result<Self> {
        let app_id = std::env::var("LAWPUB_ESTAT_APP_ID")
            .context("LAWPUB_ESTAT_APP_ID is required for e-Stat API")?;
        let base_url = std::env::var("LAWPUB_ESTAT_BASE_URL")
            .unwrap_or_else(|_| BASE_URL.to_string())
            .trim_end_matches('/')
            .to_string();
        Ok(Self { base_url, app_id })
    }

    fn client() -> Result<reqwest::blocking::Client> {
        reqwest::blocking::Client::builder()
            .user_agent("lawpub/0.1 (+https://github.com/bokuweb/lawrenceanum)")
            .timeout(Duration::from_secs(60))
            .build()
            .context("build reqwest client")
    }
}

impl Default for HttpProvider {
    fn default() -> Self {
        Self::new().expect("LAWPUB_ESTAT_APP_ID must be set")
    }
}

impl EstatProvider for HttpProvider {
    fn fetch_stats(&self, stats_data_id: &str, title: &str) -> Result<StatDataset> {
        let client = Self::client()?;
        let url = format!(
            "{}/getStatsData?appId={}&statsDataId={}&lang=J&metaGetFlg=N&cntGetFlg=N",
            self.base_url, self.app_id, stats_data_id
        );
        std::thread::sleep(Duration::from_millis(500));
        let resp = client
            .get(&url)
            .send()
            .and_then(|r| r.error_for_status())
            .with_context(|| format!("GET {url}"))?;
        let v: serde_json::Value = resp.json().context("parse JSON")?;
        let fetched_at = chrono::Utc::now().to_rfc3339();
        parse_estat_response(&v, stats_data_id, title, &fetched_at)
    }
}

// ── JSON パース ───────────────────────────────────────────────────

pub fn parse_estat_response(
    v: &serde_json::Value,
    stats_data_id: &str,
    title: &str,
    fetched_at: &str,
) -> Result<StatDataset> {
    let data_inf = &v["GET_STATS_DATA"]["STATISTICAL_DATA"]["DATA_INF"];
    let value_arr = data_inf["VALUE"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    let values: Vec<StatValue> = value_arr
        .iter()
        .filter_map(|item| {
            // e-Stat v3 の VALUE 要素は "@" プレフィックスの属性と "$" テキスト
            let val = item["$"].as_str().unwrap_or("").to_string();
            if val.is_empty() {
                return None;
            }
            Some(StatValue {
                area: item["@area"].as_str().map(String::from),
                time: item["@time"].as_str().map(String::from),
                category: item["@cat01"].as_str().map(String::from),
                value: val,
                unit: item["@unit"].as_str().map(String::from),
            })
        })
        .collect();

    Ok(StatDataset {
        schema_version: 1,
        stats_data_id: stats_data_id.to_string(),
        title: title.to_string(),
        values,
        source: StatSource {
            provider: "estat".to_string(),
            fetched_at: fetched_at.to_string(),
            stats_data_id: stats_data_id.to_string(),
        },
    })
}

// ── テスト ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_provider_returns_dataset() {
        let p = MockProvider;
        let d = p.fetch_stats("0003410379", "財政統計").unwrap();
        assert_eq!(d.schema_version, 1);
        assert!(!d.values.is_empty());
        assert_eq!(d.source.provider, "estat");
    }

    #[test]
    fn parse_estat_response_sample() {
        let v = serde_json::json!({
            "GET_STATS_DATA": {
                "STATISTICAL_DATA": {
                    "DATA_INF": {
                        "VALUE": [
                            { "$": "107528600", "@area": "00000", "@time": "2023", "@cat01": "110" }
                        ]
                    }
                }
            }
        });
        let d = parse_estat_response(&v, "test_id", "テスト", "2024-01-01T00:00:00Z").unwrap();
        assert_eq!(d.values.len(), 1);
        assert_eq!(d.values[0].value, "107528600");
        assert_eq!(d.values[0].time.as_deref(), Some("2023"));
    }

    #[test]
    #[ignore]
    fn http_provider_real_fetch() {
        let p = HttpProvider::new().expect("need LAWPUB_ESTAT_APP_ID");
        let d = p.fetch_stats(FISCAL_STATS[0].0, FISCAL_STATS[0].1).unwrap();
        println!("{} values", d.values.len());
    }
}

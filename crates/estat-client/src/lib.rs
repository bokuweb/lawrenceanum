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
//! | 0003360064 | 国有財産統計（政府出資等の推移） |
//! | 0003061945 | 法人企業統計調査（全産業時系列） |
//!
//! ## エンドポイント
//!
//! `GET https://api.e-stat.go.jp/rest/3.0/app/json/getStatsData`
//! パラメータ: `appId`, `statsDataId`, `lang=J`, `metaGetFlg=Y`, `cntGetFlg=N`

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Duration;

pub const BASE_URL: &str = "https://api.e-stat.go.jp/rest/3.0/app/json";

/// e-Stat で財政データとして追跡する統計表。
pub const FISCAL_STATS: &[(&str, &str)] = &[
    ("0003410379", "\u{8ca1}\u{653f}\u{7d71}\u{8a08}（\u{4e00}\u{822c}\u{4f1a}\u{8a08}）"), // 財政統計（一般会計）
    ("0003360064", "\u{56fd}\u{6709}\u{8ca1}\u{7523}\u{7d71}\u{8a08}（\u{653f}\u{5e9c}\u{51fa}\u{8cc7}\u{7b49}\u{306e}\u{63a8}\u{79fb}）"), // 国有財産統計（政府出資等の推移）
    ("0003061945", "\u{6cd5}\u{4eba}\u{4f01}\u{696d}\u{7d71}\u{8a08}\u{8abf}\u{67fb}（\u{5168}\u{7523}\u{696d}\u{6642}\u{7cfb}\u{5217}）"), // 法人企業統計調査（全産業時系列）
];

// ── 公開型 ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatValue {
    pub area: Option<String>,
    pub time: Option<String>,
    pub category: Option<String>,
    /// e-Stat の全次元を「次元名 → 日本語ラベル」で保持する。
    /// `area` / `time` / `category` は主要次元への後方互換アクセサ。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dimensions: BTreeMap<String, String>,
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
            schema_version: 2,
            stats_data_id: stats_data_id.to_string(),
            title: title.to_string(),
            values: vec![StatValue {
                area: Some("\u{5168}\u{56fd}".to_string()), // 全国
                time: Some("2023".to_string()),
                category: Some("\u{6b73}\u{51fa}".to_string()), // 歳出
                dimensions: BTreeMap::from([
                    (
                        "\u{5730}\u{57df}".to_string(),
                        "\u{5168}\u{56fd}".to_string(),
                    ),
                    ("\u{6642}\u{9593}\u{8ef8}".to_string(), "2023".to_string()),
                    (
                        "\u{5206}\u{985e}".to_string(),
                        "\u{6b73}\u{51fa}".to_string(),
                    ),
                ]),
                value: "107528600".to_string(),
                unit: Some("\u{767e}\u{4e07}\u{5186}".to_string()), // 百万円
            }],
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
        let fetched_at = chrono::Utc::now().to_rfc3339();
        let mut all_values = Vec::new();
        let mut start_position: Option<u64> = None;
        let mut seen_positions = HashSet::new();
        let mut pagination_complete = false;

        for _ in 0..50 {
            let mut url = format!(
                "{}/getStatsData?appId={}&statsDataId={}&lang=J&metaGetFlg=Y&cntGetFlg=N&explanationGetFlg=N&annotationGetFlg=N&limit=100000",
                self.base_url, self.app_id, stats_data_id
            );
            if let Some(start) = start_position {
                url.push_str(&format!("&startPosition={start}"));
            }
            std::thread::sleep(Duration::from_millis(500));
            let resp = client
                .get(&url)
                .send()
                .and_then(|r| r.error_for_status())
                .with_context(|| format!("GET e-Stat statsDataId={stats_data_id}"))?;
            let v: serde_json::Value = resp.json().context("parse e-Stat JSON")?;
            let mut page = parse_estat_response(&v, stats_data_id, title, &fetched_at)?;
            all_values.append(&mut page.values);

            let next = v["GET_STATS_DATA"]["STATISTICAL_DATA"]["RESULT_INF"]["NEXT_KEY"]
                .as_u64()
                .or_else(|| {
                    v["GET_STATS_DATA"]["STATISTICAL_DATA"]["RESULT_INF"]["NEXT_KEY"]
                        .as_str()
                        .and_then(|value| value.parse().ok())
                });
            let Some(next) = next else {
                pagination_complete = true;
                break;
            };
            if !seen_positions.insert(next) {
                anyhow::bail!("e-Stat pagination repeated NEXT_KEY={next} for {stats_data_id}");
            }
            start_position = Some(next);
        }

        if !pagination_complete {
            anyhow::bail!("e-Stat pagination exceeded 50 pages for {stats_data_id}");
        }
        if all_values.is_empty() {
            anyhow::bail!("e-Stat returned no values for {stats_data_id} ({title})");
        }
        Ok(StatDataset {
            schema_version: 2,
            stats_data_id: stats_data_id.to_string(),
            title: title.to_string(),
            values: all_values,
            source: StatSource {
                provider: "estat".to_string(),
                fetched_at,
                stats_data_id: stats_data_id.to_string(),
            },
        })
    }
}

// ── JSON パース ───────────────────────────────────────────────────

pub fn parse_estat_response(
    v: &serde_json::Value,
    stats_data_id: &str,
    title: &str,
    fetched_at: &str,
) -> Result<StatDataset> {
    let root = &v["GET_STATS_DATA"];
    let status = root["RESULT"]["STATUS"]
        .as_i64()
        .or_else(|| {
            root["RESULT"]["STATUS"]
                .as_str()
                .and_then(|value| value.parse().ok())
        })
        .context("e-Stat response missing RESULT.STATUS")?;
    if status >= 100 {
        let message = root["RESULT"]["ERROR_MSG"]
            .as_str()
            .unwrap_or("unknown e-Stat API error");
        anyhow::bail!("e-Stat API error {status}: {message}");
    }

    let statistical_data = &root["STATISTICAL_DATA"];
    let classes = parse_class_dimensions(&statistical_data["CLASS_INF"]);
    let data_inf = &statistical_data["DATA_INF"];
    let value_arr = data_inf["VALUE"]
        .as_array()
        .cloned()
        .or_else(|| {
            data_inf["VALUE"]
                .as_object()
                .map(|_| vec![data_inf["VALUE"].clone()])
        })
        .unwrap_or_default();

    let values: Vec<StatValue> = value_arr
        .iter()
        .filter_map(|item| {
            // e-Stat v3 の VALUE 要素は "@" プレフィックスの属性と "$" テキスト
            let val = item["$"].as_str().unwrap_or("").to_string();
            if val.is_empty() {
                return None;
            }
            let dimensions = labelled_dimensions(item, &classes);
            let area = dimension_value(item, "area", &classes);
            let time = dimension_value(item, "time", &classes);
            let category = dimension_value(item, "cat01", &classes);
            let unit = item["@unit"]
                .as_str()
                .map(String::from)
                .or_else(|| dimension_unit(item, "tab", &classes));
            Some(StatValue {
                area,
                time,
                category,
                dimensions,
                value: val,
                unit,
            })
        })
        .collect();

    Ok(StatDataset {
        schema_version: 2,
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

#[derive(Default)]
struct ClassDimension {
    name: String,
    values: HashMap<String, ClassValue>,
}

#[derive(Default)]
struct ClassValue {
    name: String,
    unit: Option<String>,
}

fn one_or_many(value: &serde_json::Value) -> Vec<&serde_json::Value> {
    if let Some(values) = value.as_array() {
        values.iter().collect()
    } else if value.is_object() {
        vec![value]
    } else {
        Vec::new()
    }
}

fn parse_class_dimensions(class_inf: &serde_json::Value) -> HashMap<String, ClassDimension> {
    let mut dimensions = HashMap::new();
    for object in one_or_many(&class_inf["CLASS_OBJ"]) {
        let Some(id) = object["@id"].as_str() else {
            continue;
        };
        let mut dimension = ClassDimension {
            name: object["@name"].as_str().unwrap_or(id).to_string(),
            values: HashMap::new(),
        };
        for class in one_or_many(&object["CLASS"]) {
            let Some(code) = class["@code"].as_str() else {
                continue;
            };
            dimension.values.insert(
                code.to_string(),
                ClassValue {
                    name: class["@name"].as_str().unwrap_or(code).to_string(),
                    unit: class["@unit"].as_str().map(String::from),
                },
            );
        }
        dimensions.insert(id.to_string(), dimension);
    }
    dimensions
}

fn dimension_value(
    item: &serde_json::Value,
    dimension_id: &str,
    classes: &HashMap<String, ClassDimension>,
) -> Option<String> {
    let code = item.get(format!("@{dimension_id}"))?.as_str()?;
    Some(
        classes
            .get(dimension_id)
            .and_then(|dimension| dimension.values.get(code))
            .map(|value| value.name.clone())
            .unwrap_or_else(|| code.to_string()),
    )
}

fn dimension_unit(
    item: &serde_json::Value,
    dimension_id: &str,
    classes: &HashMap<String, ClassDimension>,
) -> Option<String> {
    let code = item.get(format!("@{dimension_id}"))?.as_str()?;
    classes.get(dimension_id)?.values.get(code)?.unit.clone()
}

fn labelled_dimensions(
    item: &serde_json::Value,
    classes: &HashMap<String, ClassDimension>,
) -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();
    for (id, dimension) in classes {
        if let Some(value) = dimension_value(item, id, classes) {
            values.insert(dimension.name.clone(), value);
        }
    }
    values
}

// ── テスト ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_provider_returns_dataset() {
        let p = MockProvider;
        let d = p.fetch_stats("0003410379", "財政統計").unwrap();
        assert_eq!(d.schema_version, 2);
        assert!(!d.values.is_empty());
        assert_eq!(d.source.provider, "estat");
    }

    #[test]
    fn parse_estat_response_sample() {
        let v = serde_json::json!({
            "GET_STATS_DATA": {
                "RESULT": {
                    "STATUS": 0,
                    "ERROR_MSG": "正常に終了しました。"
                },
                "STATISTICAL_DATA": {
                    "CLASS_INF": {
                        "CLASS_OBJ": [
                            {
                                "@id": "tab",
                                "@name": "表章項目",
                                "CLASS": { "@code": "001", "@name": "歳出", "@unit": "百万円" }
                            },
                            {
                                "@id": "area",
                                "@name": "地域",
                                "CLASS": { "@code": "00000", "@name": "全国" }
                            },
                            {
                                "@id": "time",
                                "@name": "時間軸",
                                "CLASS": { "@code": "2023", "@name": "2023年度" }
                            },
                            {
                                "@id": "cat01",
                                "@name": "会計区分",
                                "CLASS": { "@code": "110", "@name": "一般会計" }
                            }
                        ]
                    },
                    "DATA_INF": {
                        "VALUE": [
                            {
                                "$": "107528600",
                                "@tab": "001",
                                "@area": "00000",
                                "@time": "2023",
                                "@cat01": "110"
                            }
                        ]
                    }
                }
            }
        });
        let d = parse_estat_response(&v, "test_id", "テスト", "2024-01-01T00:00:00Z").unwrap();
        assert_eq!(d.values.len(), 1);
        assert_eq!(d.values[0].value, "107528600");
        assert_eq!(d.values[0].area.as_deref(), Some("全国"));
        assert_eq!(d.values[0].time.as_deref(), Some("2023年度"));
        assert_eq!(d.values[0].category.as_deref(), Some("一般会計"));
        assert_eq!(d.values[0].unit.as_deref(), Some("百万円"));
        assert_eq!(
            d.values[0].dimensions.get("表章項目").map(String::as_str),
            Some("歳出")
        );
        assert_eq!(d.schema_version, 2);
    }

    #[test]
    fn parse_estat_response_surfaces_api_error() {
        let v = serde_json::json!({
            "GET_STATS_DATA": {
                "RESULT": {
                    "STATUS": 100,
                    "ERROR_MSG": "統計表が見つかりません"
                }
            }
        });
        let err =
            parse_estat_response(&v, "stale_id", "テスト", "2024-01-01T00:00:00Z").unwrap_err();
        assert!(err.to_string().contains("e-Stat API error 100"));
        assert!(err.to_string().contains("統計表が見つかりません"));
    }

    #[test]
    #[ignore]
    fn http_provider_real_fetch() {
        let p = HttpProvider::new().expect("need LAWPUB_ESTAT_APP_ID");
        let d = p.fetch_stats(FISCAL_STATS[0].0, FISCAL_STATS[0].1).unwrap();
        println!("{} values", d.values.len());
    }
}

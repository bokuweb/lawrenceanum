//! 国会会議録検索システム API クライアント。
//!
//! `MockProvider`（組み込みサンプル）と `HttpProvider`（実 API）の二系統を提供する。
//! ベース URL は `LAWPUB_KOKKAI_BASE_URL` で上書き可能。
//!
//! ## エンドポイント
//!
//! - 会議一覧: `GET {base}/meeting_list?sessionFrom=N&sessionTo=N&recordPacking=json`
//! - 会議全文: `GET {base}/meeting?meetingId={id}&recordPacking=json`
//! - 発言単位: `GET {base}/speech?sessionFrom=N&maximumRecords=100&startRecord=N&recordPacking=json`

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const DEFAULT_BASE_URL: &str = "https://kokkai.ndl.go.jp/api";

// ── 公開データ型 ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Speech {
    pub speech_id: String,
    pub order: u32,
    pub speaker: Option<String>,
    pub speaker_id: Option<String>,
    pub speaker_group: Option<String>,
    pub speaker_position: Option<String>,
    pub speech: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meeting {
    pub schema_version: u32,
    pub meeting_id: String,
    pub session: u32,
    pub house: String,
    pub committee: Option<String>,
    pub date: String,
    pub issue: Option<String>,
    pub speeches: Vec<Speech>,
    pub source: MeetingSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingSource {
    pub provider: String,
    pub fetched_at: String,
}

/// 会議一覧の軽量エントリ（index 用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingMeta {
    pub meeting_id: String,
    pub session: u32,
    pub house: String,
    pub committee: Option<String>,
    pub date: String,
    pub issue: Option<String>,
    pub speech_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchedMeeting {
    pub meeting_id: String,
    pub raw_json: serde_json::Value,
    pub source_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionBatch {
    pub session: u32,
    pub meetings: Vec<FetchedMeeting>,
}

// ── Provider trait ────────────────────────────────────────────────

pub trait KokkaiProvider: Send + Sync {
    /// 指定国会回次の会議一覧（軽量）を取得する。
    fn fetch_meeting_list(&self, session: u32) -> Result<Vec<MeetingMeta>>;

    /// 指定 meeting_id の全文（発言含む）を取得する。
    fn fetch_meeting(&self, meeting_id: &str) -> Result<FetchedMeeting>;

    /// 指定国会回次の全会議を取得する（meeting_list → 各 meeting の順で呼ぶ）。
    fn fetch_session(&self, session: u32) -> Result<SessionBatch> {
        let metas = self.fetch_meeting_list(session)?;
        tracing::info!("session={session}: {} meetings to fetch", metas.len());
        let mut meetings = Vec::with_capacity(metas.len());
        for meta in &metas {
            match self.fetch_meeting(&meta.meeting_id) {
                Ok(m) => meetings.push(m),
                Err(e) => tracing::warn!("skip {}: {e:#}", meta.meeting_id),
            }
        }
        Ok(SessionBatch { session, meetings })
    }
}

// ── Mock ─────────────────────────────────────────────────────────

pub struct MockProvider;

impl KokkaiProvider for MockProvider {
    fn fetch_meeting_list(&self, session: u32) -> Result<Vec<MeetingMeta>> {
        Ok(vec![MeetingMeta {
            meeting_id: format!("mock_{session}_001"),
            session,
            house: "shugiin".to_string(),
            committee: Some("法務委員会".to_string()),
            date: "2024-11-01".to_string(),
            issue: Some("第1号".to_string()),
            speech_count: 3,
        }])
    }

    fn fetch_meeting(&self, meeting_id: &str) -> Result<FetchedMeeting> {
        let raw: serde_json::Value = serde_json::from_str(SAMPLE_MEETING_JSON)
            .context("parse sample meeting JSON")?;
        Ok(FetchedMeeting {
            meeting_id: meeting_id.to_string(),
            raw_json: raw,
            source_url: format!("mock://kokkai/{meeting_id}"),
        })
    }
}

// ── Http ─────────────────────────────────────────────────────────

pub struct HttpProvider {
    base_url: String,
}

impl HttpProvider {
    pub fn new() -> Self {
        let base_url = std::env::var("LAWPUB_KOKKAI_BASE_URL")
            .unwrap_or_else(|_| DEFAULT_BASE_URL.to_string())
            .trim_end_matches('/')
            .to_string();
        Self { base_url }
    }

    fn client() -> Result<reqwest::blocking::Client> {
        reqwest::blocking::Client::builder()
            .user_agent("lawpub/0.1 (+https://github.com/bokuweb/lawrenceanum)")
            .timeout(Duration::from_secs(30))
            .build()
            .context("build reqwest client")
    }

    fn get_json(client: &reqwest::blocking::Client, url: &str) -> Result<serde_json::Value> {
        let mut last_err: Option<anyhow::Error> = None;
        for attempt in 0..5u32 {
            match client.get(url).send().and_then(|r| r.error_for_status()) {
                Ok(resp) => match resp.json::<serde_json::Value>() {
                    Ok(v) => return Ok(v),
                    Err(e) => last_err = Some(anyhow::anyhow!(e)),
                },
                Err(e) => last_err = Some(anyhow::anyhow!(e)),
            }
            let secs = [1u64, 3, 6, 12, 24][attempt as usize];
            std::thread::sleep(Duration::from_secs(secs));
        }
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("unknown error")))
            .with_context(|| format!("GET {url}"))
    }
}

impl Default for HttpProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl KokkaiProvider for HttpProvider {
    fn fetch_meeting_list(&self, session: u32) -> Result<Vec<MeetingMeta>> {
        let client = Self::client()?;
        let url = format!(
            "{}/meeting_list?sessionFrom={session}&sessionTo={session}&recordPacking=json",
            self.base_url
        );
        let v = Self::get_json(&client, &url)?;
        let metas = parse_meeting_list(&v)?;
        Ok(metas)
    }

    fn fetch_meeting(&self, meeting_id: &str) -> Result<FetchedMeeting> {
        let client = Self::client()?;
        let url = format!(
            "{}/meeting?meetingId={meeting_id}&recordPacking=json",
            self.base_url
        );
        let raw = Self::get_json(&client, &url)?;
        Ok(FetchedMeeting {
            meeting_id: meeting_id.to_string(),
            raw_json: raw,
            source_url: url,
        })
    }
}

// ── 正規化 ────────────────────────────────────────────────────────

/// API の `meeting_list` レスポンスから `MeetingMeta` のリストを抽出する。
pub fn parse_meeting_list(v: &serde_json::Value) -> Result<Vec<MeetingMeta>> {
    let records = v
        .get("meetingRecord")
        .and_then(|r| r.as_array())
        .cloned()
        .unwrap_or_default();

    let metas = records
        .iter()
        .filter_map(|r| {
            let meeting_id = r["meetingId"].as_str()?.to_string();
            let session = r["session"].as_str()?.parse::<u32>().ok()?;
            let house = r["nameOfHouse"].as_str().unwrap_or("").to_string();
            Some(MeetingMeta {
                meeting_id,
                session,
                house: normalize_house(&house),
                committee: r["nameOfMeeting"].as_str().map(String::from),
                date: r["date"].as_str().unwrap_or("").to_string(),
                issue: r["issue"].as_str().map(String::from),
                speech_count: r["numberOfSpeech"]
                    .as_str()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0),
            })
        })
        .collect();

    Ok(metas)
}

/// API の `meeting` レスポンスから `Meeting` に正規化する。
pub fn normalize_meeting(raw: &FetchedMeeting, fetched_at: &str) -> Result<Meeting> {
    let v = &raw.raw_json;
    let records = v
        .get("meetingRecord")
        .and_then(|r| r.as_array())
        .and_then(|a| a.first())
        .ok_or_else(|| anyhow::anyhow!("no meetingRecord in response"))?;

    let meeting_id = records["meetingId"]
        .as_str()
        .unwrap_or(&raw.meeting_id)
        .to_string();
    let session = records["session"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let house = records["nameOfHouse"].as_str().unwrap_or("").to_string();
    let date = records["date"].as_str().unwrap_or("").to_string();

    let speeches = records["speechRecord"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|s| {
            // API は "speechID" (大文字) で返す。
            let speech_id = s["speechID"].as_str().unwrap_or("").to_string();
            if speech_id.is_empty() {
                return None;
            }
            let order = s["speechOrder"]
                .as_str()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            let speech_text = s["speech"].as_str().unwrap_or("").to_string();
            Some(Speech {
                speech_id,
                order,
                speaker: s["speaker"].as_str().map(String::from),
                speaker_id: s["speakerId"].as_str().map(String::from),
                speaker_group: s["speakerGroup"].as_str().map(String::from),
                speaker_position: s["speakerPosition"].as_str().map(String::from),
                speech: speech_text,
            })
        })
        .collect();

    Ok(Meeting {
        schema_version: 1,
        meeting_id,
        session,
        house: normalize_house(&house),
        committee: records["nameOfMeeting"].as_str().map(String::from),
        date,
        issue: records["issue"].as_str().map(String::from),
        speeches,
        source: MeetingSource {
            provider: "kokkai_ndl".to_string(),
            fetched_at: fetched_at.to_string(),
        },
    })
}

fn normalize_house(raw: &str) -> String {
    match raw {
        "衆議院" => "shugiin".to_string(),
        "参議院" => "sangiin".to_string(),
        "両院" | "両議院" => "both".to_string(),
        other => other.to_string(),
    }
}

// ── サンプルフィクスチャ ──────────────────────────────────────────

pub const SAMPLE_MEETING_JSON: &str = r#"{
  "numberOfRecords": 1,
  "numberOfReturn": 1,
  "startRecord": 1,
  "nextRecordPosition": -1,
  "meetingRecord": [
    {
      "issueID": "120020241101200X06",
      "imageKind": "会議録",
      "searchObject": 0,
      "session": "215",
      "nameOfHouse": "衆議院",
      "nameOfMeeting": "法務委員会",
      "issue": "第6号",
      "date": "2024-11-01",
      "closing": null,
      "meetingId": "120020241101200X06",
      "speechRecord": [
        {
          "speechID": "120020241101200X06_000",
          "speechOrder": "0",
          "speaker": "委員長",
          "speakerYomi": null,
          "speakerGroup": null,
          "speakerPosition": null,
          "speakerRole": null,
          "speakerId": null,
          "parliamentMemberFlag": 1,
          "governmentSpecialMemberFlag": 0,
          "otherMemberFlag": 0,
          "speech": "これより法務委員会を開会します。",
          "startPage": 1,
          "createTime": "2024-11-10T10:00:00",
          "updateTime": "2024-11-10T10:00:00",
          "speechUrl": "https://kokkai.ndl.go.jp/txt/120020241101200X06/0"
        },
        {
          "speechID": "120020241101200X06_001",
          "speechOrder": "1",
          "speaker": "山田太郎",
          "speakerYomi": "やまだたろう",
          "speakerGroup": "自由民主党",
          "speakerPosition": "委員",
          "speakerRole": null,
          "speakerId": "1234567",
          "parliamentMemberFlag": 1,
          "governmentSpecialMemberFlag": 0,
          "otherMemberFlag": 0,
          "speech": "民法改正案について質問します。",
          "startPage": 2,
          "createTime": "2024-11-10T10:00:00",
          "updateTime": "2024-11-10T10:00:00",
          "speechUrl": "https://kokkai.ndl.go.jp/txt/120020241101200X06/1"
        }
      ]
    }
  ]
}"#;

// ── テスト ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_fetched() -> FetchedMeeting {
        let raw: serde_json::Value =
            serde_json::from_str(SAMPLE_MEETING_JSON).expect("sample JSON is valid");
        FetchedMeeting {
            meeting_id: "120020241101200X06".to_string(),
            raw_json: raw,
            source_url: "mock://test".to_string(),
        }
    }

    #[test]
    fn normalize_meeting_basic_fields() {
        let fetched = sample_fetched();
        let m = normalize_meeting(&fetched, "2024-11-10T10:00:00Z").unwrap();
        assert_eq!(m.meeting_id, "120020241101200X06");
        assert_eq!(m.session, 215);
        assert_eq!(m.house, "shugiin");
        assert_eq!(m.committee.as_deref(), Some("法務委員会"));
        assert_eq!(m.date, "2024-11-01");
        assert_eq!(m.issue.as_deref(), Some("第6号"));
        assert_eq!(m.schema_version, 1);
        assert_eq!(m.source.provider, "kokkai_ndl");
    }

    #[test]
    fn normalize_meeting_speeches() {
        let fetched = sample_fetched();
        let m = normalize_meeting(&fetched, "2024-11-10T10:00:00Z").unwrap();
        assert_eq!(m.speeches.len(), 2);
        assert_eq!(m.speeches[1].speaker.as_deref(), Some("山田太郎"));
        assert_eq!(m.speeches[1].speaker_group.as_deref(), Some("自由民主党"));
        assert_eq!(m.speeches[1].speech, "民法改正案について質問します。");
    }

    #[test]
    fn mock_provider_returns_meeting() {
        let p = MockProvider;
        let metas = p.fetch_meeting_list(215).unwrap();
        assert!(!metas.is_empty());
        let fetched = p.fetch_meeting(&metas[0].meeting_id).unwrap();
        let m = normalize_meeting(&fetched, "2024-01-01T00:00:00Z").unwrap();
        assert_eq!(m.session, 215);
    }

    #[test]
    fn parse_meeting_list_from_sample() {
        let v: serde_json::Value = serde_json::from_str(SAMPLE_MEETING_LIST_JSON).unwrap();
        let metas = parse_meeting_list(&v).unwrap();
        assert_eq!(metas.len(), 1);
        assert_eq!(metas[0].house, "shugiin");
        assert_eq!(metas[0].session, 215);
    }

    const SAMPLE_MEETING_LIST_JSON: &str = r#"{
      "numberOfRecords": 1,
      "numberOfReturn": 1,
      "startRecord": 1,
      "nextRecordPosition": -1,
      "meetingRecord": [
        {
          "issueID": "120020241101200X06",
          "session": "215",
          "nameOfHouse": "衆議院",
          "nameOfMeeting": "法務委員会",
          "issue": "第6号",
          "date": "2024-11-01",
          "meetingId": "120020241101200X06",
          "numberOfSpeech": "2"
        }
      ]
    }"#;

    #[test]
    #[ignore]
    fn http_provider_real_fetch() {
        let p = HttpProvider::new();
        let metas = p.fetch_meeting_list(213).unwrap();
        assert!(!metas.is_empty());
        println!("session 213: {} meetings", metas.len());
    }
}

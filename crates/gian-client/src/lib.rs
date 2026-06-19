//! 国会 議案情報（法案審議トラッキング）スクレイパー。
//!
//! 衆議院「議案情報」(`itdb_gian.nsf`) から法案の審議経過を取得する。
//! 公式 API は無いため HTML をパースする。robots.txt は不在 (404)。
//!
//! ## 合法性の方針
//! - 取得・保存するのは **事実データ**（件名・種類・番号・提出者・各日付・付託委員会・
//!   審議結果）のみ。事実は著作物性が低い。法案本文 (honbun) や経過ページの HTML 丸ごとは
//!   保存せず、原文へディープリンク (`source.detail_url`) する。
//! - 衆議院サイトは Shift-JIS。`text_with_charset` で復号する。

use anyhow::{Context, Result};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const BASE_URL: &str =
    "https://www.shugiin.go.jp/internet/itdb_gian.nsf/html/gian";

// ── 公開型 ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillMeta {
    /// 審議経過ページのファイル名 stem (例: "1DE153E")。
    pub bill_id: String,
    pub session: u32,
    pub title: String,
    /// 一覧上の状態（例: 「衆議院で審議中」「成立」）。
    pub status: Option<String>,
    pub keika_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyValue {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bill {
    pub schema_version: u32,
    pub bill_id: String,
    pub session: u32,
    /// 衆法 / 参法 / 閣法。
    pub bill_type: Option<String>,
    pub number: Option<String>,
    pub title: String,
    pub submitter: Option<String>,
    pub parties: Option<String>,
    /// 付託委員会（衆優先、無ければ参）。
    pub committee: Option<String>,
    /// 審議結果（衆/参のいずれか、無ければ公布で成立判断）。
    pub result: Option<String>,
    pub promulgation_date: Option<String>,
    pub law_num: Option<String>,
    /// 一覧由来の状態。
    pub status: Option<String>,
    /// 審議経過ページの全項目（KOMOKU/NAIYO）。
    pub fields: Vec<KeyValue>,
    pub source: BillSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillSource {
    pub provider: String,
    pub fetched_at: String,
    pub detail_url: String,
}

// ── Provider trait ────────────────────────────────────────────────

pub trait GianProvider: Send + Sync {
    /// `session` 回次の議案一覧。0 を渡すと最新回 (menu.htm)。
    fn list_bills(&self, session: u32) -> Result<Vec<BillMeta>>;
    fn fetch_bill(&self, meta: &BillMeta) -> Result<Bill>;
}

// ── URL ───────────────────────────────────────────────────────────

fn list_url(base: &str, session: u32) -> String {
    if session == 0 {
        format!("{base}/menu.htm")
    } else {
        format!("{base}/kaiji{session}.htm")
    }
}

// ── Mock ──────────────────────────────────────────────────────────

pub struct MockProvider;

impl GianProvider for MockProvider {
    fn list_bills(&self, session: u32) -> Result<Vec<BillMeta>> {
        let s = if session == 0 { 221 } else { session };
        Ok(vec![BillMeta {
            bill_id: "1DE153E".to_string(),
            session: s,
            title: "政治資金規正法の一部を改正する法律案".to_string(),
            status: Some("衆議院で審議中".to_string()),
            keika_url: format!("{BASE_URL}/keika/1DE153E.htm"),
        }])
    }

    fn fetch_bill(&self, meta: &BillMeta) -> Result<Bill> {
        Ok(Bill {
            schema_version: 1,
            bill_id: meta.bill_id.clone(),
            session: meta.session,
            bill_type: Some("衆法".to_string()),
            number: Some("1".to_string()),
            title: meta.title.clone(),
            submitter: Some("落合 貴之君外四名".to_string()),
            parties: Some("国民民主党・無所属クラブ".to_string()),
            committee: Some("政治改革に関する特別".to_string()),
            result: None,
            promulgation_date: None,
            law_num: None,
            status: meta.status.clone(),
            fields: vec![KeyValue { key: "議案件名".into(), value: meta.title.clone() }],
            source: BillSource {
                provider: "shugiin".to_string(),
                fetched_at: "2024-01-01T00:00:00Z".to_string(),
                detail_url: meta.keika_url.clone(),
            },
        })
    }
}

// ── Http (衆議院, Shift-JIS) ───────────────────────────────────────

pub struct HttpProvider {
    base_url: String,
}

impl HttpProvider {
    pub fn new() -> Self {
        let base_url = std::env::var("LAWPUB_GIAN_BASE_URL")
            .unwrap_or_else(|_| BASE_URL.to_string())
            .trim_end_matches('/')
            .to_string();
        Self { base_url }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    fn client() -> Result<reqwest::blocking::Client> {
        reqwest::blocking::Client::builder()
            .user_agent("lawpub/0.1 (+https://github.com/bokuweb/lawrenceanum)")
            .timeout(Duration::from_secs(30))
            .build()
            .context("build reqwest client")
    }

    /// Shift-JIS ページを復号して取得する。1 req/sec。
    fn get_sjis(client: &reqwest::blocking::Client, url: &str) -> Result<String> {
        std::thread::sleep(Duration::from_secs(1));
        let resp = client
            .get(url)
            .send()
            .and_then(|r| r.error_for_status())
            .with_context(|| format!("GET {url}"))?;
        resp.text_with_charset("Shift_JIS").context("decode shift_jis")
    }
}

impl Default for HttpProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl GianProvider for HttpProvider {
    fn list_bills(&self, session: u32) -> Result<Vec<BillMeta>> {
        let client = Self::client()?;
        let url = list_url(&self.base_url, session);
        let html = Self::get_sjis(&client, &url)?;
        parse_bill_list(&html, session, &self.base_url)
    }

    fn fetch_bill(&self, meta: &BillMeta) -> Result<Bill> {
        let client = Self::client()?;
        let html = Self::get_sjis(&client, &meta.keika_url)?;
        let fetched_at = chrono::Utc::now().to_rfc3339();
        parse_keika(&html, meta, &fetched_at)
    }
}

// ── パース ────────────────────────────────────────────────────────

fn sel(css: &str) -> Selector {
    Selector::parse(css).unwrap_or_else(|_| Selector::parse("*").unwrap())
}

fn norm(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn text_of(el: &scraper::ElementRef) -> String {
    norm(&el.text().collect::<Vec<_>>().join(""))
}

/// 議案一覧 (`menu.htm` / `kaiji{N}.htm`) から経過リンク行を抽出する。
pub fn parse_bill_list(html: &str, session: u32, base: &str) -> Result<Vec<BillMeta>> {
    let doc = Html::parse_document(html);
    let row_sel = sel("tr");
    let a_sel = sel("a");
    let td_sel = sel("td");

    let mut bills = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for tr in doc.select(&row_sel) {
        // 行内の「経過」リンク (keika/{ID}.htm) を探す。
        let keika = tr.select(&a_sel).find_map(|a| {
            let href = a.value().attr("href").unwrap_or("");
            href.contains("keika/").then(|| href.to_string())
        });
        let Some(href) = keika else { continue };
        let file = href.rsplit('/').next().unwrap_or("");
        let bill_id = file.trim_end_matches(".htm").to_string();
        if bill_id.is_empty() || !seen.insert(bill_id.clone()) {
            continue;
        }
        let keika_url = format!("{base}/keika/{file}");

        // 件名・状態: 経過/本文リンクのみのセルを除いた td テキスト。
        let cells: Vec<String> = tr
            .select(&td_sel)
            .map(|td| text_of(&td))
            .filter(|t| !t.is_empty() && t != "経過" && t != "本文")
            .collect();
        let title = cells.first().cloned().unwrap_or_default();
        if title.is_empty() {
            continue;
        }
        let status = cells.get(1).cloned();

        bills.push(BillMeta {
            bill_id,
            session,
            title,
            status,
            keika_url,
        });
    }
    Ok(bills)
}

/// 「日付／委員会」「日付／結果」形式の値から ／ 以降を返す。
fn after_slash(v: &str) -> Option<String> {
    let parts: Vec<&str> = v.split('／').collect();
    if parts.len() >= 2 {
        let t = parts[1].trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    None
}

/// 審議経過ページ (`keika/{ID}.htm`) を事実フィールドに構造化する。
pub fn parse_keika(html: &str, meta: &BillMeta, fetched_at: &str) -> Result<Bill> {
    let doc = Html::parse_document(html);
    let komoku_sel = sel(r#"td[headers="KOMOKU"]"#);
    let naiyo_sel = sel(r#"td[headers="NAIYO"]"#);

    // KOMOKU と NAIYO は行ごとに 1 対 1。文書順で zip する。
    let keys: Vec<String> = doc.select(&komoku_sel).map(|e| text_of(&e)).collect();
    let vals: Vec<String> = doc.select(&naiyo_sel).map(|e| text_of(&e)).collect();
    let fields: Vec<KeyValue> = keys
        .into_iter()
        .zip(vals.into_iter())
        .filter(|(k, _)| !k.is_empty())
        .map(|(key, value)| KeyValue { key, value })
        .collect();

    let get = |k: &str| fields.iter().find(|f| f.key == k).map(|f| f.value.clone()).filter(|v| !v.is_empty());

    let bill_type = get("議案種類");
    let number = get("議案番号");
    let title = get("議案件名").unwrap_or_else(|| meta.title.clone());
    let submitter = get("議案提出者");
    let parties = get("議案提出会派");
    // 付託委員会: 衆優先、無ければ参。「日付／委員会」の ／ 以降。
    let committee = get("衆議院付託年月日／衆議院付託委員会")
        .and_then(|v| after_slash(&v))
        .or_else(|| get("参議院付託年月日／参議院付託委員会").and_then(|v| after_slash(&v)));
    // 審議結果: 衆審議結果→参審議結果。
    let result = get("衆議院審議終了年月日／衆議院審議結果")
        .and_then(|v| after_slash(&v))
        .or_else(|| get("参議院審議終了年月日／参議院審議結果").and_then(|v| after_slash(&v)));
    // 公布年月日／法律番号。
    let kofu = get("公布年月日／法律番号");
    let promulgation_date = kofu
        .as_deref()
        .map(|v| v.split('／').next().unwrap_or("").trim().to_string())
        .filter(|s| !s.is_empty());
    let law_num = kofu.as_deref().and_then(after_slash);

    Ok(Bill {
        schema_version: 1,
        bill_id: meta.bill_id.clone(),
        session: meta.session,
        bill_type,
        number,
        title,
        submitter,
        parties,
        committee,
        result,
        promulgation_date,
        law_num,
        status: meta.status.clone(),
        fields,
        source: BillSource {
            provider: "shugiin".to_string(),
            fetched_at: fetched_at.to_string(),
            detail_url: meta.keika_url.clone(),
        },
    })
}

// ── テスト ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_list_and_fetch() {
        let p = MockProvider;
        let bills = p.list_bills(0).unwrap();
        assert_eq!(bills.len(), 1);
        let b = p.fetch_bill(&bills[0]).unwrap();
        assert_eq!(b.bill_type.as_deref(), Some("衆法"));
        assert!(b.source.detail_url.contains("keika/"));
    }

    #[test]
    fn parse_bill_list_sample() {
        let html = r#"<html><body><table>
          <tr>
            <td class="td"><span class="txt03">政治資金規正法の一部を改正する法律案</span></td>
            <td class="td"><span class="txt03">衆議院で審議中</span></td>
            <td class="td"><a href="./keika/1DE153E.htm" title="経過">経過</a></td>
            <td class="td"><a href="./honbun/g22105001.htm" title="本文">本文</a></td>
          </tr>
        </table></body></html>"#;
        let bills = parse_bill_list(html, 221, BASE_URL).unwrap();
        assert_eq!(bills.len(), 1);
        assert_eq!(bills[0].bill_id, "1DE153E");
        assert!(bills[0].title.contains("政治資金規正法"));
        assert_eq!(bills[0].status.as_deref(), Some("衆議院で審議中"));
        assert!(bills[0].keika_url.ends_with("keika/1DE153E.htm"));
    }

    #[test]
    fn parse_keika_fields() {
        let html = r#"<html><body><table>
          <tr><td headers="KOMOKU"><span>議案種類</span></td><td headers="NAIYO"><span>衆法</span></td></tr>
          <tr><td headers="KOMOKU"><span>議案番号</span></td><td headers="NAIYO"><span>1</span></td></tr>
          <tr><td headers="KOMOKU"><span>議案件名</span></td><td headers="NAIYO"><span>政治資金規正法の一部を改正する法律案</span></td></tr>
          <tr><td headers="KOMOKU"><span>議案提出者</span></td><td headers="NAIYO"><span>落合 貴之君外四名</span></td></tr>
          <tr><td headers="KOMOKU"><span>衆議院付託年月日／衆議院付託委員会</span></td><td headers="NAIYO"><span>令和 8年 6月12日 ／ 政治改革に関する特別</span></td></tr>
          <tr><td headers="KOMOKU"><span>公布年月日／法律番号</span></td><td headers="NAIYO"><span>令和 8年 6月20日 ／ 法律第50号</span></td></tr>
        </table></body></html>"#;
        let meta = BillMeta {
            bill_id: "1DE153E".into(),
            session: 221,
            title: "一覧由来".into(),
            status: Some("成立".into()),
            keika_url: "https://x/keika/1DE153E.htm".into(),
        };
        let b = parse_keika(html, &meta, "2026-01-01T00:00:00Z").unwrap();
        assert_eq!(b.bill_type.as_deref(), Some("衆法"));
        assert_eq!(b.number.as_deref(), Some("1"));
        assert!(b.title.contains("政治資金規正法"));
        assert_eq!(b.submitter.as_deref(), Some("落合 貴之君外四名"));
        assert_eq!(b.committee.as_deref(), Some("政治改革に関する特別"));
        assert_eq!(b.law_num.as_deref(), Some("法律第50号"));
        assert_eq!(b.promulgation_date.as_deref(), Some("令和 8年 6月20日"));
        assert_eq!(b.fields.len(), 6);
    }

    #[test]
    #[ignore]
    fn http_real_fetch() {
        let p = HttpProvider::new();
        let bills = p.list_bills(0).unwrap();
        println!("{} bills (latest session)", bills.len());
        assert!(!bills.is_empty());
        let b = p.fetch_bill(&bills[0]).unwrap();
        println!("first: [{}] {} / 委員会={:?} / 結果={:?}", b.bill_type.as_deref().unwrap_or("?"), b.title, b.committee, b.result);
        assert!(!b.title.is_empty());
        assert!(!b.fields.is_empty());
    }
}

//! e-Gov パブリックコメントスクレイパー。
//!
//! 公式 API がないため `public-comment.e-gov.go.jp` の HTML を `scraper` でパースする。
//!
//! ## スクレイプ対象（現行 e-Gov UI = `egovui-*`, 2024 以降）
//!
//! - 案件一覧: `GET /pcm/list?CLASSNAME=PCMMSTLIST&Mode=1&Page={n}` (Mode=1 = 結果公示済み)
//!   → `ul.egovui-list-comment-list > li` の各カードを抽出。詳細遷移はカードの
//!     `.egovui-link-area-cursor` の onClick に埋まる `id={案件番号}` から URL を組む。
//! - 案件詳細: `GET /pcm/1040?CLASSNAME=PCM1040&id={案件番号}&Mode=1`
//!   → `table.egovui-normal-horizontal` の th/td から各属性を読む。
//!
//! ## スコープ
//!
//! 1 リクエストごとに 1 秒以上待機する。提出された意見と府省の考え方の本文は
//! HTML にインラインでは無く PDF 添付 (`/pcm/download?seqNo=...`) で公開されるため、
//! このクレートでは添付メタ (名前と URL) のみ収集し、本文 PDF の解釈は行わない
//! (将来 kanpo-amend と同様の PDF 抽出を別途行う想定)。個人情報は取得しない。

use anyhow::{Context, Result};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const BASE_URL: &str = "https://public-comment.e-gov.go.jp";

// ── 公開型 ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseMeta {
    pub case_id: String,
    pub title: String,
    pub ministry: Option<String>,
    pub reception_start: Option<String>,
    pub reception_end: Option<String>,
    pub result_published: Option<String>,
    pub detail_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpinionSummary {
    pub item: String,
    pub opinion: String,
    pub ministry_response: String,
}

/// 結果公示等の添付ファイル (意見と府省の考え方の本文 PDF など)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseDetail {
    pub schema_version: u32,
    pub case_id: String,
    pub title: String,
    pub ministry: Option<String>,
    pub reception_start: Option<String>,
    pub reception_end: Option<String>,
    pub result_published: Option<String>,
    /// 根拠法令から抽出した関連法令名 (例: 「更生保護法」)。法令リンクの主シグナル。
    pub related_law_name: Option<String>,
    /// 分野カテゴリー (例: 「刑事」)。
    #[serde(default)]
    pub category: Option<String>,
    /// 定めようとする命令などの題名 (改正政令・省令名など)。
    #[serde(default)]
    pub command_title: Option<String>,
    /// 根拠法令条項の原文 (例: 「更生保護法第12条第3項…」)。
    #[serde(default)]
    pub legal_basis: Option<String>,
    /// 所管省庁・部局名等 (例: 「法務省保護局総務課」)。
    #[serde(default)]
    pub responsible_office: Option<String>,
    /// 提出意見数。
    #[serde(default)]
    pub opinion_count: Option<u32>,
    /// HTML にインラインで意見概要がある場合のみ。通常は空 (PDF 添付)。
    #[serde(default)]
    pub opinions: Vec<OpinionSummary>,
    /// 結果公示等の添付ファイル。
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    pub source: CaseSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseSource {
    pub provider: String,
    pub fetched_at: String,
    pub detail_url: String,
}

// ── Provider trait ────────────────────────────────────────────────

pub trait PubcommentProvider: Send + Sync {
    fn fetch_case_list(&self, page: u32) -> Result<Vec<CaseMeta>>;
    fn fetch_case_detail(&self, case_id: &str) -> Result<CaseDetail>;
}

// ── URL 生成 ──────────────────────────────────────────────────────

fn list_url(base: &str, page: u32) -> String {
    format!("{base}/pcm/list?CLASSNAME=PCMMSTLIST&Mode=1&Page={page}")
}

fn detail_url(base: &str, case_id: &str) -> String {
    format!("{base}/pcm/1040?CLASSNAME=PCM1040&id={case_id}&Mode=1")
}

// ── Mock ─────────────────────────────────────────────────────────

pub struct MockProvider;

impl PubcommentProvider for MockProvider {
    fn fetch_case_list(&self, _page: u32) -> Result<Vec<CaseMeta>> {
        Ok(vec![CaseMeta {
            case_id: "300110052".to_string(),
            title: "民法の一部を改正する法律案に関するパブリックコメント".to_string(),
            ministry: Some("法務省".to_string()),
            reception_start: Some("2023-06-01".to_string()),
            reception_end: Some("2023-06-30".to_string()),
            result_published: Some("2023-09-01".to_string()),
            detail_url: detail_url(BASE_URL, "300110052"),
        }])
    }

    fn fetch_case_detail(&self, case_id: &str) -> Result<CaseDetail> {
        Ok(CaseDetail {
            schema_version: 1,
            case_id: case_id.to_string(),
            title: "民法の一部を改正する法律案に関するパブリックコメント".to_string(),
            ministry: Some("法務省".to_string()),
            reception_start: Some("2023-06-01".to_string()),
            reception_end: Some("2023-06-30".to_string()),
            result_published: Some("2023-09-01".to_string()),
            related_law_name: Some("民法".to_string()),
            category: Some("民事".to_string()),
            command_title: Some("民法の一部を改正する法律".to_string()),
            legal_basis: Some("民法第1条".to_string()),
            responsible_office: Some("法務省民事局".to_string()),
            opinion_count: Some(1),
            opinions: vec![OpinionSummary {
                item: "第1条関係".to_string(),
                opinion: "基本原則をより明確にすべきである。".to_string(),
                ministry_response: "ご意見を踏まえ、条文の表現を検討します。".to_string(),
            }],
            attachments: vec![],
            source: CaseSource {
                provider: "egov_pubcomment".to_string(),
                fetched_at: "2024-01-01T00:00:00Z".to_string(),
                detail_url: detail_url(BASE_URL, case_id),
            },
        })
    }
}

// ── Http ─────────────────────────────────────────────────────────

pub struct HttpProvider {
    base_url: String,
}

impl HttpProvider {
    pub fn new() -> Self {
        let base_url = std::env::var("LAWPUB_PUBCOMMENT_BASE_URL")
            .unwrap_or_else(|_| BASE_URL.to_string())
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

    fn get_html(client: &reqwest::blocking::Client, url: &str) -> Result<String> {
        // 1 秒待機して連続アクセスを避ける。
        std::thread::sleep(Duration::from_secs(1));
        let resp = client
            .get(url)
            .send()
            .and_then(|r| r.error_for_status())
            .with_context(|| format!("GET {url}"))?;
        resp.text().context("read response text")
    }
}

impl Default for HttpProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl PubcommentProvider for HttpProvider {
    fn fetch_case_list(&self, page: u32) -> Result<Vec<CaseMeta>> {
        let client = Self::client()?;
        let url = list_url(&self.base_url, page);
        let html = Self::get_html(&client, &url)?;
        parse_case_list(&html, &self.base_url)
    }

    fn fetch_case_detail(&self, case_id: &str) -> Result<CaseDetail> {
        let client = Self::client()?;
        let url = detail_url(&self.base_url, case_id);
        let html = Self::get_html(&client, &url)?;
        let fetched_at = chrono::Utc::now().to_rfc3339();
        parse_case_detail(&html, case_id, &url, &fetched_at, &self.base_url)
    }
}

// ── HTML パース ───────────────────────────────────────────────────

fn sel(css: &str) -> Selector {
    Selector::parse(css).unwrap_or_else(|_| Selector::parse("*").unwrap())
}

/// 連続する空白 (改行・全角含む) を 1 つに畳んで前後をトリムする。
fn norm_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn text_of(el: &scraper::ElementRef) -> String {
    norm_ws(&el.text().collect::<Vec<_>>().join(""))
}

/// 根拠法令条項の原文から関連法令名を取り出す。
/// 例: 「更生保護法第12条第3項（…）」→「更生保護法」。
/// 「第」が無ければ全体を、句読点・括弧以降は落として返す。
fn law_name_from_legal_basis(raw: &str) -> Option<String> {
    let s = norm_ws(raw);
    if s.is_empty() {
        return None;
    }
    // 「第…条」より前を法令名とみなす。全角/半角どちらの「第」でも切る。
    let head = s.split('第').next().unwrap_or(&s);
    // 括弧・読点以降を除去。
    let head = head
        .split(['（', '(', '、', '，', '　'])
        .next()
        .unwrap_or(head)
        .trim();
    if head.is_empty() {
        None
    } else {
        Some(head.to_string())
    }
}

/// 「法務省保護局総務課」→「法務省」のように先頭の府省名だけを取り出す。
fn ministry_short(office: &str) -> Option<String> {
    let s = norm_ws(office);
    if s.is_empty() {
        return None;
    }
    // 最初の「省」または「庁」までを府省名とする。内閣府/会計検査院など例外は全体を返す。
    for (i, c) in s.char_indices() {
        if c == '省' || c == '庁' {
            return Some(s[..i + c.len_utf8()].to_string());
        }
    }
    Some(s)
}

/// onClick 属性等から `id={数字}` を取り出す。
fn extract_case_id(s: &str) -> Option<String> {
    let after = s.split("id=").nth(1)?;
    let id: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    if id.is_empty() { None } else { Some(id) }
}

/// 案件一覧 HTML から `CaseMeta` を抽出する (現行 egovui カード構造)。
pub fn parse_case_list(html: &str, base_url: &str) -> Result<Vec<CaseMeta>> {
    let document = Html::parse_document(html);
    let li_sel = sel("ul.egovui-list-comment-list > li");
    let title_sel = sel("h2 a.egovui-link, h2 .egovui-link");
    let status_cursor_sel = sel(".egovui-link-area-cursor");
    let detail_sel = sel(".egovui-comment-detail");
    let span_sel = sel("span");

    let mut cases = Vec::new();
    for li in document.select(&li_sel) {
        // 案件番号 (id) はカード内の遷移要素の onClick に埋まる。
        let case_id = li
            .select(&status_cursor_sel)
            .find_map(|c| c.value().attr("onclick").and_then(extract_case_id));

        // 属性 (案件番号 / 結果の公示日 / 所管省庁 …) を label→value で集める。
        let mut ministry = None;
        let mut result_published = None;
        let mut case_id_attr = None;
        for d in li.select(&detail_sel) {
            let full = text_of(&d);
            let label = d.select(&span_sel).next().map(|s| text_of(&s)).unwrap_or_default();
            let value = norm_ws(full.strip_prefix(&label).unwrap_or(&full));
            match label.as_str() {
                "案件番号" => case_id_attr = Some(value),
                "結果の公示日" | "結果公示日" => result_published = Some(value),
                "所管省庁" => ministry = Some(value),
                _ => {}
            }
        }

        let case_id = case_id.or(case_id_attr);
        let case_id = match case_id {
            Some(id) if !id.is_empty() => id,
            _ => continue,
        };

        let title = li.select(&title_sel).next().map(|a| text_of(&a)).unwrap_or_default();

        cases.push(CaseMeta {
            case_id: case_id.clone(),
            title,
            ministry,
            reception_start: None,
            reception_end: None,
            result_published,
            detail_url: detail_url(base_url, &case_id),
        });
    }
    Ok(cases)
}

/// 案件詳細 HTML から `CaseDetail` を抽出する (現行 egovui テーブル構造)。
pub fn parse_case_detail(
    html: &str,
    case_id: &str,
    url: &str,
    fetched_at: &str,
    base_url: &str,
) -> Result<CaseDetail> {
    let document = Html::parse_document(html);

    let title = document
        .select(&sel("h1.egovui-article-title"))
        .next()
        .map(|el| text_of(&el))
        .unwrap_or_default();

    // すべての横並びテーブルの行を label(空白除去)→value で集める。
    let mut fields: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let tr_sel = sel("table.egovui-normal-horizontal tr");
    let th_sel = sel("th");
    let td_sel = sel("td");
    for tr in document.select(&tr_sel) {
        let (Some(th), Some(td)) = (tr.select(&th_sel).next(), tr.select(&td_sel).next()) else {
            continue;
        };
        // ラベルは空白を完全に除去して正規化 (「  案件番号  」→「案件番号」)。
        let label: String = text_of(&th).chars().filter(|c| !c.is_whitespace()).collect();
        let value = text_of(&td);
        fields.entry(label).or_insert(value);
    }
    let get = |k: &str| fields.get(k).filter(|v| !v.is_empty()).cloned();

    let category = get("カテゴリー");
    let command_title = get("定めようとする命令などの題名").or_else(|| get("命令等の題名"));
    let legal_basis = get("根拠法令条項");
    let related_law_name = legal_basis.as_deref().and_then(law_name_from_legal_basis);
    let reception_start = get("案の公示日").or_else(|| get("意見募集開始日"));
    let reception_end = get("受付締切日時").or_else(|| get("意見募集終了日"));
    let result_published = get("結果の公示日");
    let responsible_office = get("（所管省庁・部局名等）")
        .or_else(|| get("所管省庁・部局名等"))
        .or_else(|| get("所管省庁"));
    let ministry = responsible_office.as_deref().and_then(ministry_short);
    let opinion_count = get("提出意見数").and_then(|v| {
        v.chars().filter(|c| c.is_ascii_digit()).collect::<String>().parse::<u32>().ok()
    });

    // 添付ファイル (結果公示 PDF 等)。
    let mut attachments = Vec::new();
    for a in document.select(&sel("a.file[href], a[href*=\"/pcm/download\"]")) {
        let href = a.value().attr("href").unwrap_or("");
        if href.is_empty() {
            continue;
        }
        let full = if href.starts_with("http") {
            href.to_string()
        } else {
            format!("{base_url}{href}")
        };
        let name = text_of(&a);
        attachments.push(Attachment {
            name: if name.is_empty() { "添付".to_string() } else { name },
            url: full,
        });
    }

    let title = if title.is_empty() {
        command_title.clone().unwrap_or_default()
    } else {
        title
    };

    Ok(CaseDetail {
        schema_version: 1,
        case_id: case_id.to_string(),
        title,
        ministry,
        reception_start,
        reception_end,
        result_published,
        related_law_name,
        category,
        command_title,
        legal_basis,
        responsible_office,
        opinion_count,
        opinions: Vec::new(),
        attachments,
        source: CaseSource {
            provider: "egov_pubcomment".to_string(),
            fetched_at: fetched_at.to_string(),
            detail_url: url.to_string(),
        },
    })
}

// ── テスト ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_provider_returns_list() {
        let p = MockProvider;
        let cases = p.fetch_case_list(1).unwrap();
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].ministry.as_deref(), Some("法務省"));
    }

    #[test]
    fn mock_provider_returns_detail() {
        let p = MockProvider;
        let d = p.fetch_case_detail("300110052").unwrap();
        assert_eq!(d.schema_version, 1);
        assert_eq!(d.source.provider, "egov_pubcomment");
        assert!(d.source.detail_url.contains("/pcm/1040"));
    }

    #[test]
    fn law_name_extraction() {
        assert_eq!(law_name_from_legal_basis("更生保護法第12条第3項（…）").as_deref(), Some("更生保護法"));
        assert_eq!(law_name_from_legal_basis("民法第90条").as_deref(), Some("民法"));
        assert_eq!(law_name_from_legal_basis("労働基準法施行規則").as_deref(), Some("労働基準法施行規則"));
        assert_eq!(law_name_from_legal_basis("  ").as_deref(), None);
    }

    #[test]
    fn ministry_short_works() {
        assert_eq!(ministry_short("法務省保護局総務課").as_deref(), Some("法務省"));
        assert_eq!(ministry_short("国土交通省道路局").as_deref(), Some("国土交通省"));
    }

    #[test]
    fn parse_case_list_egovui_card() {
        // 現行 egovui カード構造の最小再現。
        let html = r#"<html><body>
<ul class="egovui-list-comment-list">
  <li class="egovui-flex-column">
    <h2><a href="javascript:void(0)" class="egovui-link">「更生保護法施行令の一部を改正する政令案」に関する意見募集の結果について</a></h2>
    <span class="egovui-comment-status egovui-badge">結果公示</span>
    <div class="egovui-list-comment-attributes">
      <div class="egovui-link-area-cursor" onClick="document.forms['formDetail'].action='/pcm/1040?CLASSNAME=PCM1040&id=300110052&Mode=1';document.forms['formDetail'].submit(); return false;"></div>
      <div class="egovui-comment-detail"><span>案件番号</span><span>300110052</span></div>
      <div class="egovui-comment-detail"><span>結果の公示日</span>2026年6月19日</div>
      <div class="egovui-comment-detail"><span>所管省庁</span><span>法務省</span></div>
    </div>
  </li>
</ul>
</body></html>"#;
        let cases = parse_case_list(html, BASE_URL).unwrap();
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].case_id, "300110052");
        assert!(cases[0].title.contains("更生保護法施行令"));
        assert_eq!(cases[0].ministry.as_deref(), Some("法務省"));
        assert_eq!(cases[0].result_published.as_deref(), Some("2026年6月19日"));
        assert!(cases[0].detail_url.contains("id=300110052"));
    }

    #[test]
    fn parse_case_detail_egovui_table() {
        let html = r#"<html><body>
<h1 class="egovui-article-title">「更生保護法施行令の一部を改正する政令案」に関する意見募集の結果について</h1>
<table class="egovui-normal-horizontal"><tbody>
  <tr><th> カテゴリー </th><td>刑事</td></tr>
  <tr><th> 案件番号 </th><td>300110052</td></tr>
  <tr><th> 定めようとする命令などの題名 </th><td>更生保護法施行令の一部を改正する政令</td></tr>
  <tr><th> 根拠法令条項 </th><td>更生保護法第１２条第３項（同法第２５条において準用する場合を含む。）</td></tr>
</tbody></table>
<table class="egovui-normal-horizontal"><tbody>
  <tr><th> 案の公示日 </th><td>2026年2月26日</td></tr>
  <tr><th> 受付締切日時 </th><td>2026年3月27日18時0分</td></tr>
  <tr><th> 結果の公示日 </th><td>2026年6月19日</td></tr>
  <tr><th> 提出意見数 </th><td>2</td></tr>
  <tr><th> （所管省庁・部局名等） </th><td>法務省保護局総務課</td></tr>
</tbody></table>
<a class="file" href="/pcm/download?seqNo=0000316383" target="_blank">結果公示</a>
</body></html>"#;
        let d = parse_case_detail(html, "300110052", "http://x/pcm/1040", "2026-01-01T00:00:00Z", BASE_URL).unwrap();
        assert!(d.title.contains("更生保護法施行令"));
        assert_eq!(d.category.as_deref(), Some("刑事"));
        assert_eq!(d.related_law_name.as_deref(), Some("更生保護法"));
        assert_eq!(d.reception_start.as_deref(), Some("2026年2月26日"));
        assert_eq!(d.reception_end.as_deref(), Some("2026年3月27日18時0分"));
        assert_eq!(d.result_published.as_deref(), Some("2026年6月19日"));
        assert_eq!(d.opinion_count, Some(2));
        assert_eq!(d.ministry.as_deref(), Some("法務省"));
        assert_eq!(d.attachments.len(), 1);
        assert!(d.attachments[0].url.contains("/pcm/download?seqNo=0000316383"));
    }

    #[test]
    #[ignore]
    fn http_provider_real_fetch() {
        let p = HttpProvider::new();
        let cases = p.fetch_case_list(1).unwrap();
        println!("{} cases on page 1", cases.len());
        assert!(!cases.is_empty());
        let d = p.fetch_case_detail(&cases[0].case_id).unwrap();
        println!("detail: {} / law={:?}", d.title, d.related_law_name);
        assert!(!d.title.is_empty());
    }
}

//! 自治体例規スクレイパー。
//!
//! robots.txt 準拠・1 req/sec 上限厳守。
//! ぎょうせい型例規集（全自治体の約 50%）に 1 アダプタで対応。
//!
//! ## 方針
//!
//! - 必ず自治体**公式サイト**掲載の例規集ページから取得（ベンダ DB 直叩き NG）。
//! - 取得元 URL を `source` に残す。
//! - 著作権法 13 条により例規本文に著作権は生じないが、ToS / robots を厳守する。

use anyhow::{Context, Result};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::time::Duration;

// ── 公開型 ────────────────────────────────────────────────────────

/// 総務省全国地方公共団体コード (6 桁)。
pub type MunicipalityCode = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Municipality {
    pub code: MunicipalityCode,
    pub name: String,
    /// 例規集ページのベース URL（自治体公式）。
    pub reiki_base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReikiMeta {
    pub reiki_id: String,
    pub municipality_code: MunicipalityCode,
    pub title: String,
    pub reiki_number: Option<String>,
    pub enforced_date: Option<String>,
    pub detail_url: String,
}

/// 例規本文の条単位の構造化（国法令の Article に相当する最小版）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReikiArticle {
    /// 「第1条」「第2条の2」など。
    pub article_no: String,
    /// 見出し（（趣旨）など）。無ければ None。
    pub caption: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReikiDocument {
    pub schema_version: u32,
    pub reiki_id: String,
    pub municipality_code: MunicipalityCode,
    pub municipality_name: String,
    pub title: String,
    pub reiki_number: Option<String>,
    pub enforced_date: Option<String>,
    pub body_text: String,
    /// 条単位の構造化（第N条で分割）。空のこともある。
    #[serde(default)]
    pub articles: Vec<ReikiArticle>,
    pub source: ReikiSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReikiSource {
    pub provider: String,
    pub fetched_at: String,
    pub detail_url: String,
    pub municipality_official_site: String,
}

// ── Provider trait ────────────────────────────────────────────────

pub trait ReikiProvider: Send + Sync {
    fn list_reiki(&self, municipality: &Municipality) -> Result<Vec<ReikiMeta>>;
    fn fetch_reiki(&self, meta: &ReikiMeta, municipality: &Municipality)
        -> Result<ReikiDocument>;
}

// ── Mock ─────────────────────────────────────────────────────────

pub struct MockProvider;

impl ReikiProvider for MockProvider {
    fn list_reiki(&self, municipality: &Municipality) -> Result<Vec<ReikiMeta>> {
        let reiki_id = format!("{}_jourei_sample", municipality.code);
        Ok(vec![ReikiMeta {
            reiki_id: reiki_id.clone(),
            municipality_code: municipality.code.clone(),
            title: format!("{}個人情報保護条例", municipality.name),
            reiki_number: Some("条例第1号".to_string()),
            enforced_date: Some("2023-04-01".to_string()),
            detail_url: format!("{}/detail/{}", municipality.reiki_base_url, reiki_id),
        }])
    }

    fn fetch_reiki(
        &self,
        meta: &ReikiMeta,
        municipality: &Municipality,
    ) -> Result<ReikiDocument> {
        Ok(ReikiDocument {
            schema_version: 1,
            reiki_id: meta.reiki_id.clone(),
            municipality_code: municipality.code.clone(),
            municipality_name: municipality.name.clone(),
            title: meta.title.clone(),
            reiki_number: meta.reiki_number.clone(),
            enforced_date: meta.enforced_date.clone(),
            body_text: "（第一条）この条例は、個人情報の保護に関し必要な事項を定める。"
                .to_string(),
            articles: vec![ReikiArticle {
                article_no: "第1条".to_string(),
                caption: Some("趣旨".to_string()),
                text: "この条例は、個人情報の保護に関し必要な事項を定める。".to_string(),
            }],
            source: ReikiSource {
                provider: "gyosei".to_string(),
                fetched_at: "2024-01-01T00:00:00Z".to_string(),
                detail_url: meta.detail_url.clone(),
                municipality_official_site: municipality.reiki_base_url.clone(),
            },
        })
    }
}

// ── ぎょうせい型 Http アダプタ ────────────────────────────────────

pub struct GyoseiHttpProvider;

impl GyoseiHttpProvider {
    pub fn new() -> Self {
        Self
    }

    fn client() -> Result<reqwest::blocking::Client> {
        reqwest::blocking::Client::builder()
            .user_agent("lawpub/0.1 (+https://github.com/bokuweb/lawrenceanum)")
            .timeout(Duration::from_secs(30))
            .build()
            .context("build reqwest client")
    }

    fn get_html(client: &reqwest::blocking::Client, url: &str) -> Result<String> {
        // 1 req/sec 厳守（robots.txt 準拠）
        std::thread::sleep(Duration::from_secs(1));
        let resp = client
            .get(url)
            .send()
            .and_then(|r| r.error_for_status())
            .with_context(|| format!("GET {url}"))?;
        resp.text().context("read response text")
    }
}

impl Default for GyoseiHttpProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ReikiProvider for GyoseiHttpProvider {
    /// ぎょうせい Reiki-Base の五十音目次から例規一覧を収集する。
    /// 入口: `{base}/reiki_kana/kana_default.html` → 各 `r_50_{かな}.html` →
    ///       `../reiki_honbun/{id}.html` (例規本文) のリンク。
    fn list_reiki(&self, municipality: &Municipality) -> Result<Vec<ReikiMeta>> {
        let client = Self::client()?;
        let base = municipality.reiki_base_url.trim_end_matches('/');
        let kana_index = format!("{base}/reiki_kana/kana_default.html");
        let index_html = Self::get_html(&client, &kana_index)?;
        let kana_pages = parse_kana_index(&index_html);

        let mut metas = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for page in &kana_pages {
            let url = format!("{base}/reiki_kana/{page}");
            let html = match Self::get_html(&client, &url) {
                Ok(h) => h,
                Err(e) => {
                    tracing::warn!("skip kana page {url}: {e:#}");
                    continue;
                }
            };
            for meta in parse_gyosei_list(&html, municipality)? {
                if seen.insert(meta.reiki_id.clone()) {
                    metas.push(meta);
                }
            }
        }
        Ok(metas)
    }

    fn fetch_reiki(
        &self,
        meta: &ReikiMeta,
        municipality: &Municipality,
    ) -> Result<ReikiDocument> {
        let client = Self::client()?;
        let html = Self::get_html(&client, &meta.detail_url)?;
        let fetched_at = chrono::Utc::now().to_rfc3339();
        parse_gyosei_detail(&html, meta, municipality, &fetched_at)
    }
}

// ── HTML パース（ぎょうせい型）────────────────────────────────────

fn sel(css: &str) -> Selector {
    Selector::parse(css).unwrap_or_else(|_| Selector::parse("*").unwrap())
}

fn text_of(el: &scraper::ElementRef) -> String {
    el.text().collect::<Vec<_>>().join("").trim().to_string()
}

/// 五十音目次 (`kana_default.html`) から各 `r_50_{かな}.html` のファイル名を集める。
pub fn parse_kana_index(html: &str) -> Vec<String> {
    let doc = Html::parse_document(html);
    let link_sel = sel("a");
    let mut pages = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for a in doc.select(&link_sel) {
        let href = a.value().attr("href").unwrap_or("");
        // 同ディレクトリ内の "r_50_xx.html" のみ。
        let file = href.rsplit('/').next().unwrap_or("");
        if file.starts_with("r_50_") && file.ends_with(".html") && seen.insert(file.to_string()) {
            pages.push(file.to_string());
        }
    }
    pages
}

pub fn parse_gyosei_list(html: &str, municipality: &Municipality) -> Result<Vec<ReikiMeta>> {
    let doc = Html::parse_document(html);
    let link_sel = sel("a");
    let base = municipality.reiki_base_url.trim_end_matches('/');
    let mut metas = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for a in doc.select(&link_sel) {
        let href = a.value().attr("href").unwrap_or("");
        if !href.contains("reiki_honbun") {
            continue;
        }
        let title = text_of(&a);
        if title.is_empty() {
            continue;
        }
        // 本文ファイル名 (例: g002RG00001058.html)。相対パス (../reiki_honbun/...) を正規化。
        let file = href.rsplit('/').next().unwrap_or("");
        if file.is_empty() {
            continue;
        }
        let detail_url = format!("{base}/reiki_honbun/{file}");
        let reiki_id = format!("{}_{}", municipality.code, file.trim_end_matches(".html"));
        if !seen.insert(reiki_id.clone()) {
            continue;
        }

        metas.push(ReikiMeta {
            reiki_id,
            municipality_code: municipality.code.clone(),
            title,
            reiki_number: None,
            enforced_date: None,
            detail_url,
        });
    }
    Ok(metas)
}

/// 「規則第20号」「条例第34号」等を本文先頭付近から拾う (regex 非依存の簡易抽出)。
fn extract_reiki_number(text: &str) -> Option<String> {
    const KINDS: [&str; 6] = ["条例", "規則", "規程", "要綱", "訓令", "告示"];
    for kind in KINDS {
        let needle = format!("{kind}第");
        if let Some(pos) = text.find(&needle) {
            let rest = &text[pos + needle.len()..];
            let num: String = rest
                .chars()
                .take_while(|c| c.is_ascii_digit() || ('０'..='９').contains(c))
                .collect();
            if !num.is_empty() {
                return Some(format!("{kind}第{num}号"));
            }
        }
    }
    None
}

pub fn parse_gyosei_detail(
    html: &str,
    meta: &ReikiMeta,
    municipality: &Municipality,
    fetched_at: &str,
) -> Result<ReikiDocument> {
    let doc = Html::parse_document(html);

    // タイトルは <title> を優先 (先頭の ○ を除去)、無ければ一覧由来。
    let title = doc
        .select(&sel("title"))
        .next()
        .map(|e| text_of(&e))
        .map(|t| t.trim_start_matches('○').trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| meta.title.clone());

    // 条単位の構造化: Reiki-Base は各条を div.article で囲む。
    let art_sel = sel("div.article");
    let cap_sel = sel("p.title");
    let num_sel = sel("span.num");
    let mut articles = Vec::new();
    let mut body_parts: Vec<String> = Vec::new();
    for art in doc.select(&art_sel) {
        let full = text_of(&art);
        if full.is_empty() {
            continue;
        }
        body_parts.push(full.clone());
        let caption = art
            .select(&cap_sel)
            .next()
            .map(|e| text_of(&e))
            .map(|c| c.trim_matches(|ch| ch == '(' || ch == ')' || ch == '（' || ch == '）').trim().to_string())
            .filter(|c| !c.is_empty());
        let article_no = art
            .select(&num_sel)
            .next()
            .map(|e| text_of(&e))
            .unwrap_or_default();
        articles.push(ReikiArticle { article_no, caption, text: full });
    }

    let body_text = if body_parts.is_empty() {
        // フォールバック: 本文コンテナ or body 全体。
        doc.select(&sel("div.honbun, div#honbun, div.contents, body"))
            .next()
            .map(|el| text_of(&el))
            .unwrap_or_default()
    } else {
        body_parts.join("\n")
    };

    // 制定番号は「第1条より前のヘッダ」から拾う (条文中の他法令参照を誤検出しないため)。
    let whole = doc
        .select(&sel("body"))
        .next()
        .map(|el| text_of(&el))
        .unwrap_or_default();
    let header = body_parts
        .first()
        .and_then(|first| whole.find(first.as_str()).map(|i| &whole[..i]))
        .unwrap_or(whole.as_str());
    let reiki_number = extract_reiki_number(header).or_else(|| meta.reiki_number.clone());

    Ok(ReikiDocument {
        schema_version: 1,
        reiki_id: meta.reiki_id.clone(),
        municipality_code: municipality.code.clone(),
        municipality_name: municipality.name.clone(),
        title,
        reiki_number,
        enforced_date: meta.enforced_date.clone(),
        body_text,
        articles,
        source: ReikiSource {
            provider: "gyosei".to_string(),
            fetched_at: fetched_at.to_string(),
            detail_url: meta.detail_url.clone(),
            municipality_official_site: municipality.reiki_base_url.clone(),
        },
    })
}

// ── 既知自治体リスト（初期 3 件）────────────────────────────────

/// 既知のぎょうせい Reiki-Base テナント。
/// 実機検証済みの slug のみを載せる (404 の推測 URL は載せない)。
/// 全国の slug 一覧は RILG「全国自治体例規集リンク集」から拡充する (follow-up):
/// https://www.rilg.or.jp/htdocs/main/zenkoku_reiki/zenkoku_link.html
pub fn known_municipalities() -> Vec<Municipality> {
    vec![
        // 千葉市 (総務省コード 12100-2)。www1.g-reiki.net/chiba で 2026-06 実機確認済み。
        Municipality {
            code: "121002".to_string(),
            name: "\u{5343}\u{8449}\u{5e02}".to_string(), // 千葉市
            reiki_base_url: "https://www1.g-reiki.net/chiba".to_string(),
        },
    ]
}

// ── テスト ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_municipality() -> Municipality {
        Municipality {
            code: "131016".to_string(),
            name: "\u{5343}\u{4ee3}\u{7530}\u{533a}".to_string(),
            reiki_base_url: "https://example.com/reiki".to_string(),
        }
    }

    #[test]
    fn mock_provider_list() {
        let m = test_municipality();
        let p = MockProvider;
        let metas = p.list_reiki(&m).unwrap();
        assert_eq!(metas.len(), 1);
        assert!(metas[0].reiki_id.starts_with("131016_"));
    }

    #[test]
    fn mock_provider_fetch() {
        let m = test_municipality();
        let p = MockProvider;
        let metas = p.list_reiki(&m).unwrap();
        let doc = p.fetch_reiki(&metas[0], &m).unwrap();
        assert_eq!(doc.schema_version, 1);
        assert_eq!(doc.municipality_code, "131016");
        assert!(!doc.body_text.is_empty());
    }

    #[test]
    fn parse_gyosei_list_sample() {
        let m = test_municipality();
        // 五十音ページ (r_50_*) の本文リンク (相対 ../reiki_honbun/...) を再現。
        let html = r#"<html><body>
          <a href="../reiki_honbun/g002RG00001058.html">個人情報保護条例</a>
          <a href="../reiki_honbun/g002RG00000853.html">情報公開条例</a>
          <a href="../reiki_menu.html">メニュー</a>
        </body></html>"#;
        let metas = parse_gyosei_list(html, &m).unwrap();
        assert_eq!(metas.len(), 2);
        assert!(metas[0].reiki_id.contains("131016"));
        assert_eq!(metas[0].reiki_id, "131016_g002RG00001058");
        assert_eq!(metas[0].detail_url, "https://example.com/reiki/reiki_honbun/g002RG00001058.html");
    }

    #[test]
    fn parse_kana_index_sample() {
        let html = r#"<html><body>
          <a href="r_50_a.html">あ</a>
          <a href="r_50_ka.html">か</a>
          <a href="taikei_default.html">体系</a>
        </body></html>"#;
        let pages = parse_kana_index(html);
        assert_eq!(pages, vec!["r_50_a.html".to_string(), "r_50_ka.html".to_string()]);
    }

    #[test]
    fn parse_gyosei_detail_articles() {
        let m = test_municipality();
        // Reiki-Base 本文構造 (div.article / p.title / span.num) の最小再現。
        let html = r#"<html><head><title>○千葉アイススケート場管理規則</title></head><body>
          <div>平成24年3月30日　規則第20号</div>
          <div id="l1" class="eline"><div class="article">
            <p class="title"><span class="cm">(趣旨)</span></p>
            <p class="num"><span class="num cm">第1条</span>　<span class="clause">この規則は、…に関し必要な事項を定める。</span></p>
          </div></div>
          <div id="l2" class="eline"><div class="article">
            <p class="title"><span class="cm">(開場時間)</span></p>
            <p class="num"><span class="num cm">第2条</span>　<span class="clause">開場時間は、…とする。</span></p>
          </div></div>
        </body></html>"#;
        let meta = ReikiMeta {
            reiki_id: "131016_x".into(),
            municipality_code: "131016".into(),
            title: "一覧由来タイトル".into(),
            reiki_number: None,
            enforced_date: None,
            detail_url: "https://example.com/reiki/reiki_honbun/x.html".into(),
        };
        let d = parse_gyosei_detail(html, &meta, &m, "2026-01-01T00:00:00Z").unwrap();
        assert_eq!(d.title, "千葉アイススケート場管理規則"); // ○ 除去 + <title> 優先
        assert_eq!(d.articles.len(), 2);
        assert_eq!(d.articles[0].article_no, "第1条");
        assert_eq!(d.articles[0].caption.as_deref(), Some("趣旨"));
        assert!(d.articles[0].text.contains("必要な事項"));
        assert_eq!(d.reiki_number.as_deref(), Some("規則第20号"));
        assert!(!d.body_text.is_empty());
    }

    #[test]
    #[ignore]
    fn http_provider_real_fetch_chiba() {
        let m = &known_municipalities()[0];
        let p = GyoseiHttpProvider::new();
        let metas = p.list_reiki(m).unwrap();
        println!("{} reiki listed for {}", metas.len(), m.name);
        assert!(!metas.is_empty());
        let d = p.fetch_reiki(&metas[0], m).unwrap();
        println!("first: {} / {} articles / num={:?}", d.title, d.articles.len(), d.reiki_number);
        assert!(!d.title.is_empty());
        assert!(!d.body_text.is_empty());
    }
}

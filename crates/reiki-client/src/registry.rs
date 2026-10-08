//! 収集対象自治体（テナント）の一覧。
//!
//! RILG（一般財団法人 地方自治研究機構）の「全国自治体例規集リンク集」から各自治体の
//! 例規集 URL を取り、総務省「全国地方公共団体コード」(`data/municipality_codes.csv`) と
//! 都道府県＋団体名で突き合わせる。結果は `data/tenants.json` に同梱し、
//! `lawpub reiki-discover` で再生成する。
//!
//! RILG に載る URL は自治体公式サイトからリンクされた公開例規集であり、
//! ベンダの検索 DB ではなく公開 HTML を取得するという方針 (docs/reiki-plan.md §4.2) に沿う。

use crate::{Municipality, Vendor};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const RILG_URL: &str = "https://www.rilg.or.jp/htdocs/main/zenkoku_reiki/zenkoku_link.html";

const CODES_CSV: &str = include_str!("../data/municipality_codes.csv");
const TENANTS_JSON: &str = include_str!("../data/tenants.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Registry {
    pub schema_version: u32,
    pub generated_at: String,
    pub source: String,
    /// RILG 掲載の自治体で、対応アダプタが無い（検索アプリ型など）もの。
    #[serde(default)]
    pub unsupported: Vec<UnsupportedTenant>,
    pub tenants: Vec<Municipality>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnsupportedTenant {
    pub code: String,
    pub prefecture: String,
    pub name: String,
    pub url: String,
}

/// 同梱のテナント一覧。
pub fn embedded() -> Registry {
    serde_json::from_str(TENANTS_JSON).expect("embedded tenants.json is valid")
}

/// (都道府県, 団体名) → 団体コード。都道府県自身は (県名, 県名)。
pub fn municipality_codes() -> HashMap<(String, String), String> {
    let mut m = HashMap::new();
    for line in CODES_CSV.lines().skip(1) {
        let mut it = line.split(',');
        let (Some(code), Some(pref), Some(name)) = (it.next(), it.next(), it.next()) else {
            continue;
        };
        m.insert((pref.to_string(), name.to_string()), code.to_string());
    }
    m
}

/// URL からアダプタ種別と例規集のベース URL を決める。
pub fn classify_url(url: &str) -> Option<(Vendor, String)> {
    let url = url.trim();
    let no_query = url.split(['?', '#']).next().unwrap_or(url);
    if let Some(idx) = no_query.find("/reiki_menu.html") {
        return Some((Vendor::Gyosei, no_query[..idx].to_string()));
    }
    if let Some(idx) = no_query.find("/d1w_reiki/") {
        return Some((
            Vendor::D1Static,
            no_query[..idx + "/d1w_reiki".len()].to_string(),
        ));
    }
    None
}

fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&#038;", "&")
        .replace("&nbsp;", " ")
}

/// RILG リンク集 HTML を解析する。都道府県見出し `<b>&lt;北海道&gt;</b>` 以降のリンクが
/// その都道府県の団体。コメントアウトされた旧リンクは除く。
pub fn parse_rilg(html: &str, fetched_at: &str) -> Result<Registry> {
    let codes = municipality_codes();
    // <!-- --> を除去
    let mut body = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(i) = rest.find("<!--") {
        body.push_str(&rest[..i]);
        match rest[i..].find("-->") {
            Some(j) => rest = &rest[i + j + 3..],
            None => {
                rest = "";
                break;
            }
        }
    }
    body.push_str(rest);

    let mut pref: Option<String> = None;
    let mut tenants = Vec::new();
    let mut unsupported = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut pos = 0;
    while pos < body.len() {
        let next_pref = body[pos..].find("&lt;").map(|i| pos + i);
        let next_link = body[pos..].find("<a href=\"").map(|i| pos + i);
        match (next_pref, next_link) {
            (Some(p), l) if l.is_none_or(|l| p < l) => {
                let start = p + "&lt;".len();
                let end = body[start..]
                    .find("&gt;")
                    .map(|e| start + e)
                    .unwrap_or(start);
                let name = body[start..end].trim();
                if !name.is_empty()
                    && name.chars().count() <= 5
                    && (name.ends_with(['都', '道', '府', '県']))
                {
                    pref = Some(name.to_string());
                }
                pos = end.max(start);
            }
            (_, Some(l)) => {
                let href_start = l + "<a href=\"".len();
                let Some(href_len) = body[href_start..].find('"') else {
                    break;
                };
                let href = decode_entities(&body[href_start..href_start + href_len]);
                let Some(gt) = body[href_start..].find('>') else {
                    break;
                };
                let text_start = href_start + gt + 1;
                let Some(text_len) = body[text_start..].find("</a>") else {
                    break;
                };
                let name = body[text_start..text_start + text_len].trim().to_string();
                pos = text_start + text_len;
                let Some(p) = pref.as_ref() else { continue };
                if !href.starts_with("http") || name.contains('<') {
                    continue;
                }
                let Some(code) = codes.get(&(p.clone(), name.clone())) else {
                    continue;
                };
                if !seen.insert(code.clone()) {
                    continue;
                }
                match classify_url(&href) {
                    Some((vendor, base)) => tenants.push(Municipality {
                        code: code.clone(),
                        name: name.clone(),
                        prefecture: p.clone(),
                        vendor,
                        reiki_base_url: base,
                        source_url: href.clone(),
                    }),
                    None => unsupported.push(UnsupportedTenant {
                        code: code.clone(),
                        prefecture: p.clone(),
                        name,
                        url: href,
                    }),
                }
            }
            _ => break,
        }
    }
    if tenants.is_empty() {
        anyhow::bail!("RILG page parsed but no supported tenants found (layout changed?)");
    }
    tenants.sort_by(|a, b| a.code.cmp(&b.code));
    unsupported.sort_by(|a, b| a.code.cmp(&b.code));
    Ok(Registry {
        schema_version: 1,
        generated_at: fetched_at.to_string(),
        source: RILG_URL.to_string(),
        unsupported,
        tenants,
    })
}

pub fn fetch_rilg(client: &crate::http::PoliteClient) -> Result<Registry> {
    let html = client.get_html(RILG_URL).context("fetch RILG link list")?;
    parse_rilg(&html, &chrono::Utc::now().to_rfc3339())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify() {
        assert_eq!(
            classify_url("https://www1.g-reiki.net/city.otaru/reiki_menu.html"),
            Some((Vendor::Gyosei, "https://www1.g-reiki.net/city.otaru".into()))
        );
        assert_eq!(
            classify_url(
                "http://www.city.yubari.lg.jp/contents/illustrative/reiki_int/reiki_menu.html"
            ),
            Some((
                Vendor::Gyosei,
                "http://www.city.yubari.lg.jp/contents/illustrative/reiki_int".into()
            ))
        );
        assert_eq!(
            classify_url("http://en3-jg.d1-law.com/rumoi/d1w_reiki/reiki.html"),
            Some((
                Vendor::D1Static,
                "http://en3-jg.d1-law.com/rumoi/d1w_reiki".into()
            ))
        );
        assert_eq!(
            classify_url("https://ops-jg.d1-law.com/opensearch/SrMjF01/init?jctcd=8A79F3E9AA"),
            None
        );
    }

    #[test]
    fn parse_rilg_sections() {
        let html = r##"
<a href="#01">北海道</a>
<!-- <a href="https://old.example/reiki_menu.html">札幌市</a> -->
<a name="01"></a><center><span><b>&lt;北海道&gt;</b></span></center>
<td><a href="https://ops-jg.d1-law.com/opensearch/SrMjF01/init?jctcd=8A79F3E9AA" target="_top">北海道</a></td>
<td><a href="https://www1.g-reiki.net/city.otaru/reiki_menu.html" target="_top" rel="noopener">小樽市</a></td>
<td><a href="http://en3-jg.d1-law.com/rumoi/d1w_reiki/reiki.html" target="_top">留萌市</a></td>
<td><a href="https://www1.g-reiki.net/x/reiki_menu.html">十勝圏複合事務組合</a></td>
<b>&lt;東京都&gt;</b>
<td><a href="https://www.city.fuchu.tokyo.jp/reiki_int/reiki_menu.html">府中市</a></td>
<b>&lt;広島県&gt;</b>
<td><a href="https://www.city.fuchu.hiroshima.jp/reiki_int/reiki_menu.html">府中市</a></td>
"##;
        let r = parse_rilg(html, "2026-10-08T00:00:00Z").unwrap();
        let codes: Vec<_> = r
            .tenants
            .iter()
            .map(|t| (t.code.as_str(), t.name.as_str()))
            .collect();
        assert_eq!(
            codes,
            vec![
                ("012033", "小樽市"),
                ("012122", "留萌市"),
                ("132063", "府中市"),
                ("342084", "府中市")
            ]
        );
        assert_eq!(r.tenants[1].vendor, Vendor::D1Static);
        assert_eq!(r.unsupported.len(), 1);
        assert_eq!(r.unsupported[0].code, "010006");
    }

    #[test]
    fn embedded_registry_is_consistent() {
        let r = embedded();
        assert!(!r.tenants.is_empty());
        let codes = municipality_codes();
        let valid: std::collections::HashSet<_> = codes.values().collect();
        for t in &r.tenants {
            assert!(valid.contains(&t.code), "{} {}", t.code, t.name);
            assert!(t.reiki_base_url.starts_with("http"));
        }
    }
}

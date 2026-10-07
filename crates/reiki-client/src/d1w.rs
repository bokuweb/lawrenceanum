//! 第一法規「例規類集（HTML 版）」の静的公開型 (`.../d1w_reiki/`)。
//!
//! - 入口: `{base}/reiki.html`（「内容現在：令和8年8月13日」）
//! - 五十音目次: `{base}/mokuji_index_index.html` → `index_NNN.html`
//!   各行は `javascript:OpenResDataWin('H505901010023')` / 制定日 / 番号 / 所管課
//! - 本文: `{base}/{hno}/{hno}_j.html`（`{hno}.html` はフレームセット）
//!
//! `en3-jg.d1-law.com/{slug}/d1w_reiki/` と自治体ドメイン直下の `d1w_reiki/` が同じ形式。
//! 検索アプリ型 (`ops-jg.d1-law.com/opensearch/...`) は動的検索画面のため対象外。

use crate::gyosei::{direct_cells, table_text};
use crate::structure::{block_from_line, Block};
use crate::text::{parse_japanese_date, squash_ws, to_ascii_digits};
use crate::{Municipality, ReikiMeta};
use scraper::{ElementRef, Html, Selector};
use std::collections::HashSet;

fn sel(css: &str) -> Selector {
    Selector::parse(css).expect("static selector")
}

fn text_of(el: &ElementRef) -> String {
    squash_ws(&el.text().collect::<String>())
}

/// 五十音目次の左フレームから `index_NNN.html` を集める。
pub fn parse_index_pages(html: &str) -> Vec<String> {
    let doc = Html::parse_document(html);
    let mut pages = Vec::new();
    let mut seen = HashSet::new();
    for a in doc.select(&sel("a[href]")) {
        let href = a.value().attr("href").unwrap_or("");
        let file = href.rsplit('/').next().unwrap_or("");
        if file.starts_with("index_") && file.ends_with(".html") && seen.insert(file.to_string()) {
            pages.push(file.to_string());
        }
    }
    pages
}

/// `javascript:OpenResDataWin('H505901010023')` から本文 ID を取り出す。
fn hno_of(href: &str) -> Option<&str> {
    let start = href.find("OpenResDataWin(")? + "OpenResDataWin(".len();
    let rest = href[start..].trim_start_matches(['\'', '"']);
    let end = rest.find(['\'', '"', ')'])?;
    let id = &rest[..end];
    (!id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric())).then_some(id)
}

pub fn parse_list(html: &str, m: &Municipality) -> Vec<ReikiMeta> {
    let doc = Html::parse_document(html);
    let base = m.reiki_base_url.trim_end_matches('/');
    let link = sel("a[href]");
    let mut metas = Vec::new();
    let mut seen = HashSet::new();
    for tr in doc.select(&sel("tr")) {
        let cells = direct_cells(&tr);
        // 一覧は入れ子 TABLE。外側の行（セル内に表を含む）は飛ばし、内側の行だけ読む。
        if cells
            .iter()
            .any(|c| c.select(&sel("table")).next().is_some())
        {
            continue;
        }
        let Some((idx, a)) = cells
            .iter()
            .enumerate()
            .find_map(|(i, c)| c.select(&link).next().map(|a| (i, a)))
        else {
            continue;
        };
        let Some(hno) = a.value().attr("href").and_then(hno_of) else {
            continue;
        };
        // 入れ子 TABLE の外側 TR にも同じリンクが現れるので ID で重複排除する。
        let reiki_id = format!("{}_{}", m.code, hno);
        if !seen.insert(reiki_id.clone()) {
            continue;
        }
        let title = text_of(&a);
        if title.is_empty() {
            continue;
        }
        let date = cells
            .get(idx + 1)
            .map(text_of)
            .and_then(|t| parse_japanese_date(&t));
        let number = cells
            .get(idx + 2)
            .map(|c| to_ascii_digits(&text_of(c)))
            .filter(|t| t.contains('第'));
        metas.push(ReikiMeta {
            reiki_id,
            municipality_code: m.code.clone(),
            title,
            reiki_number: number,
            promulgated_date: date,
            detail_url: format!("{base}/{hno}/{hno}_j.html"),
        });
    }
    metas
}

/// 本文 (`{hno}_j.html`) をブロック列にする。
///
/// 各行は `<div id="h:{種別}:::…">`。種別の先頭 2 文字で題名 (zA)・制定日番号 (zB)・
/// 題名の再掲 (dG) を見分け、それ以外は行頭記号で判定する。
pub fn parse_detail_blocks(html: &str) -> Vec<Block> {
    let doc = Html::parse_document(html);
    let mut out = Vec::new();
    for el in doc.select(&sel("div[id^='h:'], table")) {
        // 表のセル内の行は表としてまとめて出すので個別には拾わない。
        let inside_table = el
            .ancestors()
            .filter_map(ElementRef::wrap)
            .any(|a| a.value().name() == "table");
        if inside_table {
            continue;
        }
        if el.value().name() == "table" {
            let s = table_text(&el);
            if !s.is_empty() {
                out.push(Block::Table(s));
            }
            continue;
        }
        // 行の中に別の行 div が入れ子になっている場合は内側だけを使う。
        if el.select(&sel("div[id^='h:']")).nth(1).is_some() {
            continue;
        }
        let id = el.value().attr("id").unwrap_or("");
        let kind = id.split(':').nth(1).unwrap_or("");
        let t = text_of(&el);
        if t.is_empty() {
            continue;
        }
        match kind.get(..2).unwrap_or("") {
            "zA" => out.push(Block::Title(t)),
            "zB" => {
                // 「令和５年12月13日条例第23号」を日付と番号に分ける。
                let ascii = to_ascii_digits(&t);
                match ascii.find('日') {
                    Some(i) => {
                        let (d, n) = ascii.split_at(i + '日'.len_utf8());
                        out.push(Block::Date(d.trim().to_string()));
                        if !n.trim().is_empty() {
                            out.push(Block::Number(n.trim().to_string()));
                        }
                    }
                    None => out.push(Block::Text(t)),
                }
            }
            "dG" => {} // 題名の再掲
            _ => {
                if let Some(b) = block_from_line(&t) {
                    out.push(b);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structure::build;

    fn muni() -> Municipality {
        Municipality::test(
            "012122",
            "留萌市",
            "https://en3-jg.d1-law.com/rumoi/d1w_reiki",
        )
    }

    #[test]
    fn index_list_rows() {
        let html = include_str!("../tests/fixtures/d1w_index.html");
        let metas = parse_list(html, &muni());
        assert_eq!(metas.len(), 11);
        let m0 = &metas[0];
        assert_eq!(m0.reiki_id, "012122_H505901010023");
        assert_eq!(m0.title, "留萌市空家等の適切な管理に関する条例");
        assert_eq!(m0.promulgated_date.as_deref(), Some("2023-12-13"));
        assert_eq!(m0.reiki_number.as_deref(), Some("条例第23号"));
        assert_eq!(
            m0.detail_url,
            "https://en3-jg.d1-law.com/rumoi/d1w_reiki/H505901010023/H505901010023_j.html"
        );
    }

    #[test]
    fn index_pages() {
        let html = r#"<a HREF="index_001.html">あ</a><A HREF="index_002.html">い</A><a href="./style_comm.css">x</a>"#;
        assert_eq!(
            parse_index_pages(html),
            vec!["index_001.html", "index_002.html"]
        );
    }

    #[test]
    fn detail_structure() {
        let html = include_str!("../tests/fixtures/d1w_honbun.html");
        let s = build(parse_detail_blocks(html));
        assert_eq!(
            s.title.as_deref(),
            Some("留萌市空家等の適切な管理に関する条例")
        );
        assert_eq!(s.date.as_deref(), Some("令和5年12月13日"));
        assert_eq!(s.number.as_deref(), Some("条例第23号"));
        assert_eq!(s.articles[0].article_no, "第１条");
        assert_eq!(s.articles[0].caption.as_deref(), Some("目的"));
        let a7 = s.articles.iter().find(|a| a.article_id == "art_7").unwrap();
        assert_eq!(a7.paragraphs.len(), 5);
        assert_eq!(a7.paragraphs[4].num.as_deref(), Some("5"));
        let a8_2 = s
            .articles
            .iter()
            .find(|a| a.article_id == "art_8_2")
            .unwrap();
        assert!(a8_2.paragraphs[0].text.contains("管理不全空家等"));
        let a10 = s
            .articles
            .iter()
            .find(|a| a.article_id == "art_10")
            .unwrap();
        assert_eq!(a10.paragraphs[1].items.len(), 4);
        assert_eq!(
            a10.paragraphs[1].items[1].text,
            "当該命令の対象となった空家等の所在地"
        );
        assert_eq!(s.supplementary.len(), 2);
        assert!(s.supplementary[1].title.contains("令和７年12月10日"));
        assert_eq!(
            s.supplementary[0].articles[0].paragraphs[0].text,
            "この条例は、公布の日から施行する。"
        );
    }
}

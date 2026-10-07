//! ぎょうせい「Reiki-Base」型の例規集。
//!
//! - 入口: `{base}/reiki_menu.html`（「〜例規集 現在 令和8年7月1日」の内容現在日を含む）
//! - 五十音目次: `{base}/reiki_kana/kana_default.html` → `r_50_{かな}.html`
//! - 本文: `{base}/reiki_honbun/{id}.html`
//!
//! `www1.g-reiki.net/{slug}` のほか、自治体ドメイン直下の `reiki_int/` 等も同じエンジン。

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

fn has_class(el: &ElementRef, class: &str) -> bool {
    el.value().classes().any(|c| c == class)
}

/// `reiki_menu.html` / d1w トップから「現在 令和8年7月1日」の内容現在日を拾う。
pub fn parse_current_as_of(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let text = squash_ws(&doc.root_element().text().collect::<String>());
    let idx = text.find("現在")?;
    // 「内容現在：令和8年…」（後ろ）と「〜 現在 令和8年…」の両方がある。後ろ優先で前も見る。
    let after: String = text[idx + "現在".len()..].chars().take(24).collect();
    if let Some(d) = parse_japanese_date(&after) {
        return Some(d);
    }
    let before: String = text[..idx]
        .chars()
        .rev()
        .take(24)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    parse_japanese_date(&before)
}

/// 五十音目次 (`kana_default.html`) から各 `r_50_{かな}.html` のファイル名を集める。
pub fn parse_kana_index(html: &str) -> Vec<String> {
    let doc = Html::parse_document(html);
    let mut pages = Vec::new();
    let mut seen = HashSet::new();
    for a in doc.select(&sel("a[href]")) {
        let href = a.value().attr("href").unwrap_or("");
        let file = href.rsplit('/').next().unwrap_or("");
        if file.starts_with("r_50_") && file.ends_with(".html") && seen.insert(file.to_string()) {
            pages.push(file.to_string());
        }
    }
    pages
}

/// 五十音ページの表 (例規名称 / 制定年月日 / 種別番号) を読む。
pub fn parse_list(html: &str, m: &Municipality) -> Vec<ReikiMeta> {
    let doc = Html::parse_document(html);
    let base = m.reiki_base_url.trim_end_matches('/');
    let mut metas = Vec::new();
    let mut seen = HashSet::new();
    let link = sel("a[href]");
    let mut push = |a: ElementRef, cells: &[ElementRef]| {
        let href = a.value().attr("href").unwrap_or("");
        if !href.contains("reiki_honbun/") {
            return;
        }
        let title = text_of(&a);
        let file = href
            .rsplit('/')
            .next()
            .unwrap_or("")
            .split(['#', '?'])
            .next()
            .unwrap_or("");
        if title.is_empty() || !file.ends_with(".html") {
            return;
        }
        let stem = file.trim_end_matches(".html");
        let reiki_id = format!("{}_{}", m.code, stem);
        if !seen.insert(reiki_id.clone()) {
            return;
        }
        let date = cells
            .get(1)
            .map(text_of)
            .and_then(|t| parse_japanese_date(&t));
        let number = cells
            .get(2)
            .map(|c| to_ascii_digits(&text_of(c)))
            .filter(|t| !t.is_empty() && t.contains('第'));
        metas.push(ReikiMeta {
            reiki_id,
            municipality_code: m.code.clone(),
            title,
            reiki_number: number,
            promulgated_date: date,
            detail_url: format!("{base}/reiki_honbun/{file}"),
        });
    };
    let mut in_rows = false;
    for tr in doc.select(&sel("tr")) {
        let cells = direct_cells(&tr);
        if let Some(a) = cells.first().and_then(|c| c.select(&link).next()) {
            in_rows = true;
            push(a, &cells);
        }
    }
    if !in_rows {
        // 表構造でないテナント: 本文リンクだけ拾う。
        for a in doc.select(&link) {
            push(a, &[]);
        }
    }
    metas
}

/// `tr` 直下の `td`/`th`（入れ子テーブルのセルは含めない）。
pub(crate) fn direct_cells<'a>(tr: &ElementRef<'a>) -> Vec<ElementRef<'a>> {
    tr.children()
        .filter_map(ElementRef::wrap)
        .filter(|c| matches!(c.value().name(), "td" | "th"))
        .collect()
}

/// 表を「セル\tセル」行の改行区切りテキストにする。
pub(crate) fn table_text(table: &ElementRef) -> String {
    let mut rows = Vec::new();
    for tr in table.select(&sel("tr")) {
        let cells: Vec<String> = direct_cells(&tr).iter().map(text_of).collect();
        if cells.iter().any(|c| !c.is_empty()) {
            rows.push(cells.join("\t"));
        }
    }
    rows.join("\n")
}

/// `p.num` のような「番号 span + 本文」要素を (番号, 本文) にする。
fn num_and_body(p: &ElementRef) -> (Option<String>, String) {
    let full = text_of(p);
    let num = p
        .select(&sel("span.num"))
        .next()
        .map(|e| text_of(&e))
        .filter(|n| !n.is_empty());
    match num {
        Some(n) if full.starts_with(&n) => {
            let body = full[n.len()..]
                .trim_start_matches(['\u{3000}', ' '])
                .to_string();
            (Some(n), body)
        }
        _ => (None, full),
    }
}

fn eline_blocks(el: ElementRef, out: &mut Vec<Block>) {
    for child in el.children().filter_map(ElementRef::wrap) {
        let name = child.value().name();
        if name == "p" {
            if has_class(&child, "revise_record") || has_class(&child, "note") {
                out.push(Block::Note(text_of(&child)));
            } else if has_class(&child, "s-head") {
                out.push(Block::Supplementary(text_of(&child)));
            } else if let Some(b) = block_from_line(&text_of(&child)) {
                out.push(b);
            }
            continue;
        }
        if name != "div" {
            continue;
        }
        if has_class(&child, "head") {
            for p in child.children().filter_map(ElementRef::wrap) {
                let t = text_of(&p);
                if t.is_empty() {
                    continue;
                }
                if has_class(&p, "title-irregular") || has_class(&p, "title") {
                    out.push(Block::Title(t));
                } else if has_class(&p, "date") {
                    out.push(Block::Date(t));
                } else if has_class(&p, "number") {
                    out.push(Block::Number(to_ascii_digits(&t)));
                } else {
                    out.push(Block::Text(t));
                }
            }
        } else if has_class(&child, "table_frame")
            || child.select(&sel("table")).next().is_some() && !has_class(&child, "article")
        {
            if let Some(t) = child.select(&sel("table")).next() {
                let s = table_text(&t);
                if !s.is_empty() {
                    out.push(Block::Table(s));
                }
            }
        } else if has_class(&child, "article")
            || has_class(&child, "clause")
            || has_class(&child, "item")
        {
            let is_article = has_class(&child, "article");
            let is_item = has_class(&child, "item");
            for p in child.children().filter_map(ElementRef::wrap) {
                if has_class(&p, "title") {
                    let t = text_of(&p);
                    if let Some(Block::Caption(c)) = block_from_line(&t) {
                        out.push(Block::Caption(c));
                    } else if !t.is_empty() {
                        out.push(Block::Text(t));
                    }
                    continue;
                }
                let (num, body) = num_and_body(&p);
                if num.is_none() && body.is_empty() {
                    continue;
                }
                let blk = match (is_article, is_item, num) {
                    (true, _, Some(n)) => Block::Article { num: n, text: body },
                    (_, true, Some(n)) => {
                        let level = match block_from_line(&format!("{n}\u{3000}x")) {
                            Some(Block::Item { level, .. }) => level,
                            _ => 1,
                        };
                        Block::Item {
                            num: n,
                            level,
                            text: body,
                        }
                    }
                    (_, _, Some(n)) => Block::Paragraph {
                        num: Some(to_ascii_digits(&n)),
                        text: body,
                    },
                    (_, _, None) => Block::Paragraph {
                        num: None,
                        text: body,
                    },
                };
                out.push(blk);
            }
        } else {
            // 章見出し等: クラス名はテナント差があるので行頭記号で判定する。
            let t = text_of(&child);
            if let Some(b) = block_from_line(&t) {
                out.push(match b {
                    Block::Paragraph { .. } | Block::Article { .. } | Block::Item { .. } => b,
                    other => other,
                });
            }
        }
    }
}

/// 本文 HTML をブロック列にする。
pub fn parse_detail_blocks(html: &str) -> Vec<Block> {
    let doc = Html::parse_document(html);
    let root = doc
        .select(&sel("#primaryInner2, #primaryInner, #primary"))
        .next()
        .unwrap_or_else(|| doc.root_element());
    let mut out = Vec::new();
    for el in root.select(&sel("div.eline")) {
        eline_blocks(el, &mut out);
    }
    if out.is_empty() {
        // eline の無い古いテンプレート: <p> 単位で行判定する。
        for p in root.select(&sel("p")) {
            if let Some(b) = block_from_line(&text_of(&p)) {
                out.push(b);
            }
        }
    }
    out
}

pub fn page_title(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    doc.select(&sel("title"))
        .next()
        .map(|e| text_of(&e))
        .map(|t| t.trim_start_matches('○').trim().to_string())
        .filter(|t| !t.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structure::build;

    fn muni() -> Municipality {
        Municipality::test("121002", "千葉市", "https://www1.g-reiki.net/chiba")
    }

    #[test]
    fn list_rows_have_date_and_number() {
        let html = r#"<table><tbody>
<tr><td class="indent-list01"><ul><li>■ あ</li></ul></td><td>&nbsp;</td><td>&nbsp;</td></tr>
<tr><td><a href="../reiki_honbun/g002RG00001058.html" onclick="x">千葉アイススケート場管理規則</a></td>
<td>◆平成24年3月30日</td><td>規則第20号</td></tr>
<tr><td><a href="../reiki_honbun/g002RG00000853.html">千葉アイススケート場設置管理条例</a></td>
<td>◆平成16年9月29日</td><td>条例第34号</td></tr>
</tbody></table>"#;
        let metas = parse_list(html, &muni());
        assert_eq!(metas.len(), 2);
        assert_eq!(metas[0].reiki_id, "121002_g002RG00001058");
        assert_eq!(metas[0].promulgated_date.as_deref(), Some("2012-03-30"));
        assert_eq!(metas[0].reiki_number.as_deref(), Some("規則第20号"));
        assert_eq!(
            metas[1].detail_url,
            "https://www1.g-reiki.net/chiba/reiki_honbun/g002RG00000853.html"
        );
    }

    #[test]
    fn current_as_of_from_menu() {
        let html = "<html><body><p>（千葉市例規集　現在 令和8年7月1日）</p></body></html>";
        assert_eq!(parse_current_as_of(html).as_deref(), Some("2026-07-01"));
        let d1w = "<div id=\"current\">内容現在：令和8年8月13日</div>";
        assert_eq!(parse_current_as_of(d1w).as_deref(), Some("2026-08-13"));
    }

    /// 2026-10 時点の Reiki-Base 本文構造（千葉アイススケート場設置管理条例の抜粋）。
    #[test]
    fn detail_keeps_clauses_and_items_outside_article_div() {
        let html = include_str!("../tests/fixtures/gyosei_honbun.html");
        let s = build(parse_detail_blocks(html));
        assert_eq!(s.title.as_deref(), Some("千葉アイススケート場設置管理条例"));
        assert_eq!(s.number.as_deref(), Some("条例第34号"));
        assert_eq!(s.date.as_deref(), Some("平成16年9月29日"));
        let a1 = &s.articles[0];
        assert_eq!(a1.article_no, "第1条");
        assert_eq!(a1.caption.as_deref(), Some("設置"));
        // 第1条直後の表は第1条の本文に入る
        assert!(a1.paragraphs[0]
            .text
            .contains("千葉アイススケート場\t千葉市美浜区新港224番地1"));
        let a3 = s.articles.iter().find(|a| a.article_no == "第3条").unwrap();
        assert_eq!(a3.paragraphs[0].items.len(), 5);
        assert_eq!(
            a3.paragraphs[0].items[3].text,
            "スケート場の維持管理に関する業務"
        );
        let a4 = s.articles.iter().find(|a| a.article_no == "第4条").unwrap();
        assert_eq!(a4.paragraphs.len(), 2);
        assert_eq!(a4.paragraphs[1].num.as_deref(), Some("2"));
        assert!(a4.paragraphs[1]
            .text
            .starts_with("市長は、スケート場の管理運営上"));
        // 改正履歴は本文に混ざらない
        assert!(!a3.text().contains("一部改正"));
        // 附則
        assert_eq!(s.supplementary.len(), 4);
        let s0 = &s.supplementary[0].articles[0].paragraphs[0].text;
        assert!(
            s0.starts_with("この条例は、規則で定める日から施行する。"),
            "{s0}"
        );
        // p.note（施行日注記）は本文に混ざらない
        assert!(!s0.contains("平成17年規則第5号"));
        assert_eq!(s.supplementary[1].articles[0].article_no, "第1条");
        assert_eq!(s.supplementary[1].articles[0].paragraphs[0].items.len(), 2);
        assert!(s.supplementary[2].title.contains("平成20年12月16日"));
        assert_eq!(
            s.supplementary[2].articles[0].paragraphs[0].text,
            "この条例は、平成21年1月1日から施行する。"
        );
        let last = s.supplementary.last().unwrap();
        assert_eq!(
            last.articles[0].paragraphs[0].caption.as_deref(),
            Some("施行期日")
        );
    }
}

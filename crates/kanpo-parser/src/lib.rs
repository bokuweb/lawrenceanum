//! デジタル官報の項目別 PDF パーサ。縦書きの読み順を復元し、改め文・新旧対照表を構造化する。
//!
//! 使い方は crate 直下の `README.md` を参照。
//!
//! デジタル官報 PDF はテキスト層を持つ（OCR 不要）が、**縦書き・多段組**のため
//! `pdftotext -layout` の素の出力には次の癖がある。
//!
//! 1. 縦書き presentation form の約物（︑ ︒ ﹇ ﹈ ︵ ︶ …）
//! 2. ページ柱（「官」「報」「令和 年 月 日」等の余白テキスト）の混入
//! 3. 新旧対照表での 改正後／改正前 2 カラムの並走（罫線でなく y 座標の帯で表現）
//!
//! 本クレートは座標付きテキスト（`pdftotext -bbox-layout` と同じ形の XHTML）から縦書きの読み順を
//! 復元し、(1)(2) を整形、(3) を改正後/改正前に分離する。PDF からの座標付きテキストと罫線の
//! 取り出しは pure Rust の kanpo-pdf が行う（poppler は要らない）。
//!
//! # モジュール構成
//!
//! - `normalize` — 約物正規化・PUA 除去・ページ柱ノイズ判定
//! - `vertical` — 縦書き読み順の復元（段分割・右→左再構成）
//! - `horizontal` — 回転ページ・横書きページの読み順復元（列分割・左→右再構成）
//! - `shinkyu` — 新旧対照表（改正後=上帯/改正前=下帯）の検出と分離
//! - `segment` — 1 ページ内の複数記事の分割
//! - `format` — 改め文の形式判定（prose / shinkyu / unknown）
//! - `lines` — 罫線（kanpo-pdf で抽出）
//!
//! # 例
//! ```no_run
//! let pdf: &[u8] = b"...";
//! let ex = kanpo_parser::extract(pdf).unwrap();
//! println!("{} ({})", ex.text, ex.format);
//! ```

use anyhow::Result;

pub mod format;
pub mod grid;
mod horizontal;
pub mod lines;
pub mod model;
pub mod normalize;
pub mod segment;
pub mod shinkyu;
pub mod table;
pub mod trace;
pub mod vertical;

pub use format::detect_format_of;
pub use model::{Block, Document, Run, ShinkyuRow};
pub use normalize::normalize_text;
pub use segment::segment_articles;
pub use vertical::reconstruct_vertical;

/// 抽出結果。`text` は整形済み本文、`format` は "prose"/"shinkyu"/"unknown"。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extracted {
    pub text: String,
    pub format: String,
}

/// PDF バイト列（官報1ページ分）から、縦書きの読み順を復元したページ本文と
/// 形式判定を返す。
pub fn extract(pdf: &[u8]) -> Result<Extracted> {
    let xhtml = bbox_xhtml(pdf)?;
    let text = reconstruct_vertical(&xhtml);
    let format = format::detect_format(&text);
    Ok(Extracted { text, format })
}

/// 復元過程まで含めた解析結果（playground・デバッグ用）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct Analysis {
    /// ページごとの語・領域・行の割り当て。
    pub pages: Vec<trace::PageTrace>,
    /// [`reconstruct_vertical`] と同じ本文。
    pub text: String,
    /// 本文全体の形式判定（"prose"/"shinkyu"/"unknown"）。
    pub format: String,
    /// 記事ごとの本文と構造化結果。
    pub articles: Vec<ArticleAnalysis>,
}

/// 記事 1 件の本文と構造化結果。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ArticleAnalysis {
    pub text: String,
    pub document: Document,
}

/// `pdftotext -bbox-layout` の XHTML を解析し、本文・記事・構造化結果に加えて、各語が
/// どの領域（前文・改正後・改正前・表外・本文）のどの行に組まれたかを返す。
/// poppler を使わないので WASM でも動く。
pub fn analyze(xhtml: &str) -> Analysis {
    analyze_with_rules(xhtml, &[])
}

/// [`analyze`] に各ページの罫線（[`page_rules`]）を与える版。`rules[i]` は i ページ目の罫線。
///
/// 罫線があると、新旧対照表の外枠で表の外の列（附則など）を分け、帯の中の罫線の表
/// （別表など）をセルの結合まで含めて 2D 表として復元し、構造化結果では本文の流れの中の
/// 表（[`model::Run::table`]）として返す。複数ページの XHTML（`<page>` が複数）も渡せる。
pub fn analyze_with_rules(xhtml: &str, rules: &[Option<lines::PageRules>]) -> Analysis {
    let pages = vertical::analyze_pages_with_rules(xhtml, rules);
    let join = |f: &dyn Fn(&trace::PageTrace) -> &str| {
        pages.iter().map(f).filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n").trim().to_string()
    };
    let text = join(&|p| p.text.as_str());
    let doc_text = join(&|p| p.doc_text.as_str());
    let tables: Vec<trace::Table> = pages.iter().flat_map(|p| p.tables.iter().cloned()).collect();
    let articles = segment_articles(&doc_text)
        .into_iter()
        .map(|seg| ArticleAnalysis {
            document: Document::from_text_with_tables(&seg, &tables),
            text: render_table_markers(&seg, &tables),
        })
        .collect();
    Analysis {
        format: format::detect_format(&text),
        pages,
        text,
        articles,
    }
}

/// 項目（告示など）の 1 ページ分の入力。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PageInput {
    /// `pdftotext -bbox-layout` と同じ形の XHTML（1 ページ、[`bbox_xhtml`]）。
    pub xhtml: String,
    /// そのページの罫線（[`page_rules`]）。無ければ表の復元をしない。
    #[serde(default)]
    pub rules: Option<lines::PageRules>,
}

/// 複数ページにまたがる項目をまとめて解析する。ページで切れた新旧対照表は 1 つの表に
/// つなぎ、記事（`〇…号`）ごとに構造化する。
pub fn analyze_item(pages: &[PageInput]) -> Analysis {
    let xhtml = combine_pages(pages.iter().map(|p| p.xhtml.as_str()));
    let rules: Vec<Option<lines::PageRules>> = pages.iter().map(|p| p.rules.clone()).collect();
    analyze_with_rules(&xhtml, &rules)
}

/// 目次の標題（例: 「○○規則の一部を改正する省令（総務七七）」）に最も合う記事の添字を返す。
///
/// 本文ヘッダには対象法令名のあとに法令番号「（…）」が挿入されるため、標題そのものの
/// 完全一致では拾えない。対象法令名（「の一部を」「を廃止」より前）を鍵に、まず完全一致、
/// 次に前方一致を縮めながら探し、制定文らしい（「次のように」「改める」等を含む）長い記事を採る。
///
/// 本文は行の途中で折り返されるので空白・改行を除いて比べる。対象法令名が短い（「種苗法」）
/// ときは制定機関略号を除いた標題全体を鍵にする。それでも見つからなければ、数字（漢数字・
/// 算用数字）とダッシュを除いて比べ直す（標題「人事院規則二―四」・本文「人事院規則２－４」）。
pub fn pick_article(title: &str, articles: &[ArticleAnalysis]) -> Option<usize> {
    // ダッシュ類は標題（―）と本文（－）で字が違うことがあるので揃えて比べる。
    let unify = |s: &str| -> String {
        s.chars()
            .filter(|c| !c.is_whitespace())
            .map(|c| if matches!(c, '―' | '—' | '‐' | '‑' | '–' | '-') { '－' } else { c })
            .collect()
    };
    let loose = |s: &str| -> String {
        unify(s).chars().filter(|c| !c.is_ascii_digit() && !"０１２３４５６７８９〇一二三四五六七八九十百千－".contains(*c)).collect()
    };
    let mut core = unify(&title_core(title));
    if core.chars().count() < 4 {
        core = unify(title.rfind('（').map_or(title, |p| &title[..p]));
        if core.chars().count() < 6 {
            return None;
        }
    }
    // 前置き（「…の規定に基づき、」「…の規定により、」）の付いた標題は、その後ろでも照合する。
    let mut cores = vec![core.clone()];
    for marker in ["規定に基づき、", "規定により、"] {
        if let Some(p) = core.rfind(marker) {
            let rest = core[p + marker.len()..].to_string();
            if rest.chars().count() >= 6 {
                cores.push(rest);
            }
        }
    }
    let keys_of = |cores: &[String]| -> Vec<String> {
        let mut keys: Vec<String> = Vec::new();
        for c in cores {
            keys.push(c.clone());
            let chars: Vec<char> = c.chars().collect();
            let mut len = chars.len().saturating_sub(2);
            while len >= 6 {
                keys.push(chars[..len].iter().collect());
                len = len.saturating_sub(4);
            }
        }
        keys.retain(|k| k.chars().count() >= 4);
        keys
    };
    let body_score = |s: &str| -> usize {
        let markers = ["の規定に基づき", "次のように", "に改める", "を加える", "定める"];
        markers.iter().filter(|m| s.contains(*m)).count() * 1000 + s.chars().count()
    };
    let best = |keys: &[String], texts: &[String]| {
        articles
            .iter()
            .enumerate()
            .filter(|(i, _)| keys.iter().any(|k| texts[*i].contains(k.as_str())))
            .max_by_key(|(_, a)| body_score(&a.text))
            .map(|(i, _)| i)
    };
    let texts: Vec<String> = articles.iter().map(|a| unify(&a.text)).collect();
    best(&keys_of(&cores), &texts).or_else(|| {
        let cores: Vec<String> = cores.iter().map(|c| loose(c)).filter(|c| c.chars().count() >= 6).collect();
        let texts: Vec<String> = articles.iter().map(|a| loose(&a.text)).collect();
        best(&keys_of(&cores), &texts)
    })
}

/// 標題から突合の鍵となる「対象法令名」を取り出す（末尾の制定機関略号と「の一部を…」を除く）。
pub fn title_core(title: &str) -> String {
    let mut t = title;
    if let Some(p) = t.rfind('（') {
        t = &t[..p];
    }
    for marker in ["の一部を", "を廃止"] {
        if let Some(p) = t.find(marker) {
            return t[..p].trim().to_string();
        }
    }
    t.trim().to_string()
}

/// 1 ページずつの XHTML の `<page>` 要素を 1 つの文書にまとめる。
fn combine_pages<'a>(xhtmls: impl Iterator<Item = &'a str>) -> String {
    let mut out = String::from("<html><body><doc>");
    for x in xhtmls {
        let mut rest = x;
        while let Some(start) = rest.find("<page") {
            let Some(end) = rest[start..].find("</page>") else { break };
            out.push_str(&rest[start..start + end + "</page>".len()]);
            rest = &rest[start + end + "</page>".len()..];
        }
    }
    out.push_str("</doc></body></html>");
    out
}

/// 構造化用テキストの表の目印を、表を書き下したテキストに戻す。
fn render_table_markers(text: &str, tables: &[trace::Table]) -> String {
    text.lines()
        // 新旧対照表の終わりの目印は構造化用なので落とす。
        .filter(|l| l.trim() != trace::SHINKYU_END)
        .map(|l| match trace::parse_table_marker(l).and_then(|i| tables.get(i)) {
            Some(t) => t.render(),
            None => l.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// PDF バイト列から、`pdftotext -bbox-layout` と同じ形の座標付き XHTML を得る（kanpo-pdf）。
/// [`analyze`] / [`reconstruct_vertical`] の入力になる。
pub fn bbox_xhtml(pdf: &[u8]) -> Result<String> {
    let pages = kanpo_pdf::extract_pages(pdf)?;
    Ok(kanpo_pdf::bbox::bbox_layout(&pages))
}

/// PDF の各ページを [`analyze_item`] の入力（座標付きテキストと罫線）にする。
pub fn page_inputs(pdf: &[u8]) -> Result<Vec<PageInput>> {
    let pages = kanpo_pdf::extract_pages(pdf)?;
    Ok(pages
        .into_iter()
        .map(|p| PageInput {
            rules: Some(kanpo_pdf::page_rules(&p)),
            xhtml: kanpo_pdf::bbox::bbox_layout(std::slice::from_ref(&p)),
        })
        .collect())
}

/// PDF（1 ページ）の罫線（縦線・横線）を抽出する。[`analyze_with_rules`] に渡すと、新旧対照表の
/// 帯の中の罫線表を 2D 表として復元できる。
pub fn page_rules(pdf: &[u8]) -> Result<lines::PageRules> {
    lines::extract_rules(pdf)
}

/// PDF バイト列から構造化した [`Document`] を得る。HTML や表へ変換しやすい形。
/// 改め文をテキストではなく構造で受け取りたい呼び出し側はこちらを使う。
///
/// 新旧対照表中の別表（罫線で区切られた表）は、罫線とテキスト座標から
/// 2D 表として復元し、対応する `別表第○` 行の `after_table`/`before_table` に格納する。
pub fn extract_document(pdf: &[u8]) -> Result<Document> {
    let xhtml = bbox_xhtml(pdf)?;
    let text = reconstruct_vertical(&xhtml);
    let mut doc = Document::from_text(&text);
    attach_tables(&mut doc, pdf);
    Ok(doc)
}

/// 既存の [`Document`] に、1 ページ PDF から復元した別表 2D 表を attach する。
///
/// 複数ページにまたがる項目では、各ページの PDF についてこれを呼ぶと、まだ表の付いて
/// いない `別表第○` 行へ読み順に割り当てられる。罫線が取れなければ何もしない（best-effort）。
pub fn attach_tables(doc: &mut Document, pdf: &[u8]) {
    let Ok(rules) = lines::extract_rules(pdf) else {
        return;
    };
    let Ok(xhtml) = bbox_xhtml(pdf) else {
        return;
    };
    attach_bessho_tables(doc, &rules, &xhtml);
}

/// 罫線から別表 2D 表を復元し、Document の未充填の `別表第○` 行に順番に attach する。
fn attach_bessho_tables(doc: &mut Document, rules: &lines::PageRules, xhtml: &str) {
    // 1 ページ目（kanpo の項目別 PDF はページ単位で渡される）。
    let Some((height, cols)) = vertical::parse_page_cols(xhtml).into_iter().next() else {
        return;
    };
    // 改正後/改正前 の境界 y。見出しが無ければ別表 attach はしない。
    let Some((header_x, divider)) = shinkyu::detect_shinkyu_header(&cols) else {
        return;
    };
    let words: Vec<table::PlacedWord> = cols.iter().map(vertical::col_to_word).collect();
    let after_tables = table::reconstruct_tables(rules, &words, vertical::TOP_MARGIN, divider, header_x);
    let before_tables =
        table::reconstruct_tables(rules, &words, divider, height - vertical::BOTTOM_MARGIN, header_x);

    // Document の 別表行（after が「別表」で始まる行）に、読み順で順番に割り当てる。
    let (mut ai, mut bi) = (0usize, 0usize);
    for block in &mut doc.blocks {
        let model::Block::Shinkyu { rows } = block else { continue };
        for row in rows.iter_mut() {
            let is_bessho = |runs: &[Run]| runs.first().map(|r| model::starts_with_spaced(&r.text, "別表")).unwrap_or(false);
            // 既に別表が付いている行（前のページで充填済み）は飛ばす。
            if is_bessho(&row.after) && row.after_table.is_none() && ai < after_tables.len() {
                row.after_table = Some(to_nested(&after_tables[ai]));
                ai += 1;
            }
            if is_bessho(&row.before) && row.before_table.is_none() && bi < before_tables.len() {
                row.before_table = Some(to_nested(&before_tables[bi]));
                bi += 1;
            }
        }
    }
}

/// 復元した 2D 表（セル文字列）を Document の入れ子テーブルへ変換する。
fn to_nested(t: &table::GridTable) -> model::NestedTable {
    model::NestedTable {
        merges: Vec::new(),
        rows: t
            .rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| {
                        if cell.is_empty() {
                            Vec::new()
                        } else {
                            vec![Run::plain(cell.clone())]
                        }
                    })
                    .collect()
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arts(texts: &[&str]) -> Vec<ArticleAnalysis> {
        texts.iter().map(|t| ArticleAnalysis { text: t.to_string(), document: model::Document::from_text(t) }).collect()
    }

    #[test]
    fn picks_article_across_line_breaks() {
        let a = arts(&["〇防衛省令第十八号\n自衛隊法施行規則の一部を次のように改正する。", "〇個人情報保護委員会告示第一号\n…第九条の規定に基づき、金\n融分野における個人情報保護に関するガイドラインの一部を次のように改正し"]);
        assert_eq!(pick_article("金融分野における個人情報保護に関するガイドラインの一部を改正する件（個人情報保護委・金融庁一）", &a), Some(1));
    }

    #[test]
    fn picks_article_with_short_law_name() {
        let a = arts(&["〇農林水産省令第五十号\n種苗法施行規則の一部を次のように改正する。", "種苗法の一部を改正する法律をここに公布する。\n御名御璽"]);
        assert_eq!(pick_article("種苗法の一部を改正する法律（七二）", &a), Some(1));
    }

    #[test]
    fn picks_article_ignoring_numeral_style() {
        let a = arts(&["人事院公示第26号\n人事院は、人事院規則２４（人事院の職員に対する権限の委任）第２項の規定に基づき、昭和38年人事院公示第５号の一部を次のように改正する。"]);
        let title = "人事院規則二―四（人事院の職員に対する権限の委任）第二項の規定に基づき、昭和三十八年人事院公示第五号の一部を改正する件（人事院二六）";
        assert_eq!(pick_article(title, &a), Some(0));
    }
}

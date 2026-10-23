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
mod notice_boundaries;
mod company_document_parts;
mod facility_continuations;
mod framed_band_continuations;
#[cfg(test)]
#[path = "paired_framed_continuation_shape_tests.rs"]
mod paired_framed_continuation_shape_tests;
mod paired_framed_continuations;
mod wrapped_notice_titles;
mod area_units;
mod blank_area_units;
mod chemical_line_end;
mod coordinate_rows;
mod coordinate_symbols;
mod framed_amount_rows;
mod inline_quotes;
mod micro_prefixes;
mod native_triangle_cells;
mod range_endpoints;
mod reference_markers;

mod cell_row_dashes;
mod financial_enclosures;
pub mod format;
mod fractions;
pub mod grid;
#[cfg(test)]
mod headed_data_column_tests;
mod headed_data_columns;
mod headerless_four_bands;
mod horizontal;

pub mod lines;
pub mod model;
mod nested_fraction_geometry;
mod nested_fractions;
pub mod normalize;
mod numeric_grid_orientation;
mod owned_dashes;
mod quantity_tables;
mod record_suffix;
mod ruled_plain_dashes;
pub mod segment;
mod shared_table_labels;
pub mod shinkyu;
pub mod table;
mod table_dashes;
pub mod trace;
mod unruled_road;
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
        trace::join_page_texts(&pages, f)
    };
    let text = join(&|p| p.text.as_str());
    let doc_text = join(&|p| p.doc_text.as_str());
    let tables: Vec<trace::Table> = pages.iter().flat_map(|p| p.tables.iter().cloned()).collect();
    let mut segments = segment_articles(&doc_text);
    wrapped_notice_titles::repair_wrapped_notice_titles(&mut segments, &pages);
    financial_enclosures::repair_segments(&mut segments,&pages,rules);
    let groups=notice_boundaries::groups(&segments,&doc_text,&pages,rules);
    let mut articles=Vec::new();
    for (segment,parts)in segments.into_iter().zip(groups){
        let original=Document::from_text_with_tables(&segment,&tables);
        if parts.len()>1 {
            let desired:Vec<_>=parts.iter().map(|p|Document::from_text_with_tables(&p.text,&tables)).collect();
            if let Some(documents)=company_document_parts::repartition(&original,&desired){
                articles.extend(parts.into_iter().zip(documents).map(|(p,document)|ArticleAnalysis{text:render_table_markers(&p.text,&tables),document}));continue;
            }
        }
        articles.push(ArticleAnalysis{text:render_table_markers(&segment,&tables),document:original});
    }
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
    // 短い目次名（「内閣」）は本文中の部分一致には使えないが、独立した冒頭見出しなら照合できる。
    let exact = unify(title);
    let headings: Vec<usize> = articles.iter().enumerate().filter(|(_, a)| {
        a.text.lines().filter(|l| !l.trim().is_empty()).take_while(|l| {
            segment::is_section_label(l) || unify(l) == exact
        }).any(|l| unify(l) == exact)
    }).map(|(i, _)| i).collect();
    if headings.len() == 1 {
        return Some(headings[0]);
    }
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
    best(&keys_of(&cores), &texts)
        .or_else(|| {
            let cores: Vec<String> = cores.iter().map(|c| loose(c)).filter(|c| c.chars().count() >= 6).collect();
            let texts: Vec<String> = articles.iter().map(|a| loose(&a.text)).collect();
            best(&keys_of(&cores), &texts)
        })
        .or_else(|| pick_by_number(title, articles))
}

/// 標題の末尾の番号（「（同二八五）」「（総務二八五）」）と記事の見出しの号数（「〇総務省告示第二百八十五号」）で
/// 記事を選ぶ。目次の標題と本文の中の告示の名前が違うことがある（目次「…人体にばく露される電波の許容値に係る
/// 総務大臣が別に告示する無線設備を定める件」、本文「…第十四条の二第一項第二号等の規定に基づく総務大臣が
/// 別に告示する無線設備を定める件」）。見出しの号数が合う記事が 1 つだけのときに限る。機関名（「同」でなければ）
/// が標題にあれば、見出しにもあるものだけ。
fn pick_by_number(title: &str, articles: &[ArticleAnalysis]) -> Option<usize> {
    let open = title.rfind('（')?;
    let inner = title[open + '（'.len_utf8()..].trim_end_matches('）');
    let digit = |c: char| "〇一二三四五六七八九".find(c).map(|i| i / '〇'.len_utf8());
    // 号数は末尾の漢数字の並び（「防衛二五一～二五三」「同二五四、二五五」のように範囲・列挙のこともあり、
    // そのときは先頭の号数）。機関名が漢数字で始まる（「九州地方整備局九八」）ので、先頭の漢数字では切らない。
    let start = inner
        .char_indices()
        .find(|&(i, c)| digit(c).is_some() && inner[i..].chars().all(|c| digit(c).is_some() || "～〜、・".contains(c)))?
        .0;
    let agency = &inner[..start];
    let n = inner[start..].chars().map_while(digit).fold(0u64, |n, d| n * 10 + d as u64);
    let agencies: Vec<&str> = if agency == "同" { Vec::new() } else { agency.split('・').filter(|a| !a.is_empty()).collect() };
    let matched: Vec<usize> = articles
        .iter()
        .enumerate()
        .filter(|(_, a)| {
            let head = a.text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
            heading_number(head) == Some(n) && agencies.iter().all(|ag| head.contains(ag))
        })
        .map(|(i, _)| i)
        .collect();
    (matched.len() == 1).then(|| matched[0])
}

/// 見出しの「第二百八十五号」「第26号」の号数。
fn heading_number(head: &str) -> Option<u64> {
    let start = head.find('第')? + '第'.len_utf8();
    let end = start + head[start..].find('号')?;
    let s = &head[start..end];
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || ('０'..='９').contains(&c)) {
        return Some(s.chars().fold(0, |n, c| n * 10 + c.to_digit(10).or_else(|| Some(c as u32 - '０' as u32)).unwrap() as u64));
    }
    // 漢数字（千・百・十の位取り）。
    let (mut total, mut digit) = (0u64, None::<u64>);
    for c in s.chars() {
        match "〇一二三四五六七八九".chars().position(|d| d == c) {
            Some(d) => digit = Some(d as u64),
            None => {
                let unit = match c {
                    '十' => 10,
                    '百' => 100,
                    '千' => 1000,
                    _ => return None,
                };
                total += digit.unwrap_or(1) * unit;
                digit = None;
            }
        }
    }
    let n = total + digit.unwrap_or(0);
    (n > 0).then_some(n)
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
    let sources= kanpo_pdf::extract_pages_with_aj1_sources(pdf)?;
    Ok(kanpo_pdf::bbox::bbox_layout_with_native_styles(&sources))
}

/// PDF の各ページを [`analyze_item`] の入力（座標付きテキストと罫線）にする。
pub fn page_inputs(pdf: &[u8]) -> Result<Vec<PageInput>> {
    Ok(kanpo_pdf::extract_pages_with_aj1_sources(pdf)?
        .into_iter()
        .map(|source| PageInput {
            rules: Some(kanpo_pdf::page_rules(source.page())),
            xhtml: kanpo_pdf::bbox::bbox_layout_with_native_styles(std::slice::from_ref(&source)),
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
        texts.iter().map(|t| ArticleAnalysis { text: t.to_string(), document: model::Document::from_text(t),
            }).collect()
    }

    #[test]
    fn short_title_requires_an_independent_leading_heading() {
        let a = arts(&["国会事項\n内閣に関する質問", "人事異動\n内閣\n〇財務大臣臨時代理\n指定する",
        ]);
        assert_eq!(pick_article("内閣", &a), Some(1));
        assert_eq!(pick_article("内閣", &a[..1]), None);
    }

    #[test]
    fn picks_article_across_line_breaks() {
        let a = arts(&["〇防衛省令第十八号\n自衛隊法施行規則の一部を次のように改正する。", "〇個人情報保護委員会告示第一号\n…第九条の規定に基づき、金\n融分野における個人情報保護に関するガイドラインの一部を次のように改正し"]);
        assert_eq!(pick_article("金融分野における個人情報保護に関するガイドラインの一部を改正する件（個人情報保護委・金融庁一）", &a), Some(1));
    }

    #[test]
    fn picks_article_with_short_law_name() {
        let a = arts(&["〇農林水産省令第五十号\n種苗法施行規則の一部を次のように改正する。", "種苗法の一部を改正する法律をここに公布する。\n御名御璽",
        ]);
        assert_eq!(pick_article("種苗法の一部を改正する法律（七二）", &a), Some(1));
    }

    #[test]
    fn picks_article_ignoring_numeral_style() {
        let a = arts(&["人事院公示第26号\n人事院は、人事院規則２４（人事院の職員に対する権限の委任）第２項の規定に基づき、昭和38年人事院公示第５号の一部を次のように改正する。"]);
        let title = "人事院規則二―四（人事院の職員に対する権限の委任）第二項の規定に基づき、昭和三十八年人事院公示第五号の一部を改正する件（人事院二六）";
        assert_eq!(pick_article(title, &a), Some(0));
    }

    #[test]
    fn picks_article_by_number_when_toc_title_differs_from_body() {
        // 目次の標題と本文の中の告示の名前が違う（号外第 171 号 29 ページ）。号数で選ぶ。
        let a = arts(&[
            "〇総務省告示第二百八十四号\n八〇〇MHz帯広帯域小電力無線システムの携帯局の無線設備の送信時間制限装置を定める件の一部を次のように改正する。",
            "〇総務省告示第二百八十五号\n無線設備規則第十四条の二第一項第二号等の規定に基づく総務大臣が別に告示する無線設備を定める件の一部を次のように改正する。",
        ]);
        let title = "無線設備規則で規定する人体にばく露される電波の許容値に係る総務大臣が別に告示する無線設備を定める件の一部を改正する件（同二八五）";
        assert_eq!(pick_article(title, &a), Some(1));
        assert_eq!(pick_article(&title.replace("（同二八五）", "（総務二八五）"), &a), Some(1));
        // 機関名が見出しに無ければ選ばない。号数の合う記事が無いときも。
        assert_eq!(pick_article(&title.replace("（同二八五）", "（国土交通二八五）"), &a), None);
        assert_eq!(pick_article(&title.replace("（同二八五）", "（同二八六）"), &a), None);
        assert_eq!(heading_number("〇国土交通省告示第千百八十七号"), Some(1187));
        assert_eq!(heading_number("人事院公示第26号"), Some(26));
    }

    #[test]
    fn picks_article_by_number_when_agency_starts_with_numeral() {
        // 機関名が漢数字で始まる（「九州地方整備局九八」）。先頭の「九」を号数と取り違えない。
        let a = arts(&[
            "〇東北地方整備局告示第百一号\n次のように道路の区域を変更した。",
            "〇九州地方整備局告示第九十八号\n都市計画事業を認可したので告示する。",
        ]);
        assert_eq!(pick_article("都市計画に関する件（九州地方整備局九八）", &a), Some(1));
        // 範囲・列挙の号数は先頭の号数で選ぶ。
        let a = arts(&["〇防衛省告示第二百五十一号\n射撃訓練を実施する。", "〇防衛省告示第二百五十四号\n射撃等訓練を実施する。",
        ]);
        assert_eq!(pick_by_number("訓練の件（防衛二五一～二五三）", &a), Some(0));
        assert_eq!(pick_by_number("訓練の件（同二五四、二五五）", &a), Some(1));
    }
}

mod composite_numbers;

mod large_parentheses;

mod financial_flow;

mod circled_numbers;

mod unruled_records;

mod parallel_fraction_geometry;
mod parallel_fractions;

mod numeric_fraction_geometry;
mod numeric_fractions;

#[cfg(test)]
mod ruled_plain_dash_tests;

mod quoted_inline_tables;

mod source_aj1;

#[cfg(test)]
mod qualified_notice_title_tests;
#[cfg(test)]
mod notice_title_scope_tests;
#[cfg(test)]
mod unparsed_notice_title_tests;

#[cfg(test)]
mod native_layout_relations_tests;

#[cfg(test)]
mod native_geometry_originals_tests;

mod native_word_bounds;

#[cfg(test)]
mod notice_native_geometry_tests;

#[cfg(test)]
mod glyph_typography_originals_tests;

#[cfg(test)]
mod glyph_typography_contract_tests;

mod native_content_styles;

#[cfg(test)]
mod common_header_source_order_tests;

mod common_financial_header;

#[cfg(test)]
mod bank_frame_source_control_tests;

mod framed_native_columns;

#[cfg(test)]
mod original_director_frames_tests;

#[cfg(test)]
mod legal_role_original_tests;

#[cfg(test)]
mod closed_frame_continuation_original_tests;

mod native_word_axes;

#[cfg(test)]
mod financial_original_source_tests;

#[cfg(test)]mod native_rule_source_order_tests;

mod native_word_rulings;

#[cfg(test)]
mod source_frame_fences_original_tests;

#[cfg(test)]
mod enclosed_warichu_original_tests;

#[cfg(test)]
mod composed_inline_pipeline_tests;

#[cfg(test)]
mod frame_boundary_original_tests;

#[cfg(test)]
mod frame_projection_counterfactual_tests;

#[cfg(test)]
mod source_table_border_original_tests;

#[cfg(test)]
mod source_auxiliary_original_tests;

#[cfg(test)]
mod source_shared_note_original_tests;

#[cfg(test)]
mod source_numeric_sign_original_tests;

#[cfg(test)]
mod ruled_marker_original_tests;

#[cfg(test)]
mod ruled_marker_existing_layout_tests;

mod native_unit_dash;

#[cfg(test)]
mod source_unit_dash_original_tests;

#[cfg(test)]
mod orthogonal_gutters_original_tests;

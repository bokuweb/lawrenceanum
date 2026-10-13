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
//! 本クレートは座標付きテキスト(`pdftotext -bbox-layout`)から縦書きの読み順を復元し、
//! (1)(2) を整形、(3) を改正後/改正前に分離する。`pdftotext`(poppler) への依存を前提とする。
//!
//! # モジュール構成
//!
//! - `normalize` — 約物正規化・PUA 除去・ページ柱ノイズ判定
//! - `vertical` — 縦書き読み順の復元（段分割・右→左再構成）
//! - `shinkyu` — 新旧対照表（改正後=上帯/改正前=下帯）の検出と分離
//! - `segment` — 1 ページ内の複数記事の分割
//! - `format` — 改め文の形式判定（prose / shinkyu / unknown）
//! - `pdftotext` — poppler `pdftotext` の起動
//!
//! # 例
//! ```no_run
//! let pdf: &[u8] = b"...";
//! let ex = kanpo_parser::extract(pdf).unwrap();
//! println!("{} ({})", ex.text, ex.format);
//! ```

use anyhow::Result;

pub mod format;
pub mod lines;
pub mod model;
pub mod normalize;
pub mod pdftotext;
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
    let xhtml = pdftotext::run_pdftotext_bbox(pdf)?;
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
    let pages = vertical::analyze_pages(xhtml);
    let text = pages
        .iter()
        .map(|p| p.text.as_str())
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    let articles = segment_articles(&text)
        .into_iter()
        .map(|text| ArticleAnalysis {
            document: Document::from_text(&text),
            text,
        })
        .collect();
    Analysis {
        format: format::detect_format(&text),
        pages,
        text,
        articles,
    }
}

/// PDF バイト列を `pdftotext -bbox-layout` に通し、座標付き XHTML を得る。
/// [`analyze`] / [`reconstruct_vertical`] の入力になる（poppler が必要）。
pub fn bbox_xhtml(pdf: &[u8]) -> Result<String> {
    pdftotext::run_pdftotext_bbox(pdf)
}

/// PDF（1 ページ）の罫線（縦線・横線）を抽出する。[`analyze_with_rules`] に渡すと、新旧対照表の
/// 帯の中の罫線表を 2D 表として復元できる（poppler の `pdftocairo` が必要）。
pub fn page_rules(pdf: &[u8]) -> Result<lines::PageRules> {
    lines::extract_rules(pdf)
}

/// PDF バイト列から構造化した [`Document`] を得る。HTML や表へ変換しやすい形。
/// 改め文をテキストではなく構造で受け取りたい呼び出し側はこちらを使う。
///
/// 新旧対照表中の別表（罫線で区切られた表）は、`pdftocairo` の罫線とテキスト座標から
/// 2D 表として復元し、対応する `別表第○` 行の `after_table`/`before_table` に格納する。
pub fn extract_document(pdf: &[u8]) -> Result<Document> {
    let xhtml = pdftotext::run_pdftotext_bbox(pdf)?;
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
    let Ok(xhtml) = pdftotext::run_pdftotext_bbox(pdf) else {
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

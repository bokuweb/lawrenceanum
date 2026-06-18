//! 官報項目別 PDF からの改め文テキスト抽出。
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
//! let ex = kanpo_amend::extract(pdf).unwrap();
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

/// PDF バイト列から構造化した [`Document`] を得る。HTML や表へ変換しやすい形。
/// 改め文をテキストではなく構造で受け取りたい呼び出し側はこちらを使う。
///
/// 新旧対照表中の別表（罫線で区切られた表）は、`pdftocairo` の罫線とテキスト座標から
/// 2D 表として復元し、対応する `別表第○` 行の `after_table`/`before_table` に格納する。
pub fn extract_document(pdf: &[u8]) -> Result<Document> {
    let xhtml = pdftotext::run_pdftotext_bbox(pdf)?;
    let text = reconstruct_vertical(&xhtml);
    let mut doc = Document::from_text(&text);
    // 別表(罫線表)を復元して該当行に attach（best-effort: 罫線が取れなければ素の Document）。
    if let Ok(rules) = lines::extract_rules(pdf) {
        attach_bessho_tables(&mut doc, &rules, &xhtml);
    }
    Ok(doc)
}

/// 罫線から別表 2D 表を復元し、Document の `別表第○` 行に順番に attach する。
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
            let after_is_bessho = row
                .after
                .first()
                .map(|r| r.text.trim_start().starts_with("別表"))
                .unwrap_or(false);
            let before_is_bessho = row
                .before
                .first()
                .map(|r| r.text.trim_start().starts_with("別表"))
                .unwrap_or(false);
            if after_is_bessho && ai < after_tables.len() {
                row.after_table = Some(to_nested(&after_tables[ai]));
                ai += 1;
            }
            if before_is_bessho && bi < before_tables.len() {
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

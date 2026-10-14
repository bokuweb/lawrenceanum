//! 官報 PDF の罫線（縦線・横線）。
//!
//! 新旧対照表中の別表（罫線で区切られた表）のセル構造を復元するのに使う。抽出は kanpo-pdf が
//! コンテンツストリームから直接行う（1 本の直線だけのパスを縦線/横線に分ける）。座標系は
//! [`crate::bbox_xhtml`] と同じ（左上原点・y 下向き、pt）。

use anyhow::Result;

pub use kanpo_pdf::rules::{HRule, PageRules, VRule};

/// PDF（1 ページ目）から罫線を抽出する。
pub fn extract_rules(pdf: &[u8]) -> Result<PageRules> {
    let pages = kanpo_pdf::extract_pages(pdf)?;
    Ok(pages.first().map(kanpo_pdf::page_rules).unwrap_or_default())
}

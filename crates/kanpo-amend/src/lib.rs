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
pub mod normalize;
pub mod pdftotext;
pub mod segment;
pub mod shinkyu;
pub mod vertical;

pub use format::detect_format_of;
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

//! 官報 PDF から文字の座標と罫線を取り出す pure Rust の PDF リーダ。
//!
//! poppler（`pdftotext` / `pdftocairo`）に頼らず、WASM でも動くように必要な部分だけを持つ。

pub mod bbox;
pub mod document;
pub mod font;
pub mod object;
pub mod page;
pub mod rules;

mod cmap;
mod crypt;
mod filter;
mod lexer;

#[cfg(test)]
#[path = "source_charcode_tests.rs"]
mod source_charcode_tests;
mod inline_image;

#[cfg(test)]
#[path = "inline_image_tests.rs"]
mod inline_image_tests;

pub use document::{Document, PageRef};
pub use object::{Dict, ObjRef, Object, Stream};
pub use page::{extract_page, FontInfo, Glyph, Page, PathItem, SubPath};
pub use rules::{page_rules, HRule, PageRules, VRule};

/// PDF の全ページを解釈する。
pub fn extract_pages(pdf: &[u8]) -> Result<Vec<Page>> {
    let doc = Document::load(pdf.to_vec())?;
    Ok(doc.pages().iter().map(|p| extract_page(&doc, p)).collect())
}

#[derive(Debug)]
pub enum Error {
    /// PDF の構造が壊れている。
    Malformed(String),
    /// 空パスワードで開けない暗号化。
    Encrypted,
    /// 対応していない機能。
    Unsupported(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Malformed(m) => write!(f, "malformed PDF: {m}"),
            Error::Encrypted => write!(f, "encrypted PDF needs a password"),
            Error::Unsupported(m) => write!(f, "unsupported: {m}"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
#[path="aj1_provenance_pdf_tests.rs"]
mod aj1_provenance_pdf_tests;

mod cff_aj1;
mod source_aj1;
pub use source_aj1::Aj1PageSources;
/// Extract opaque owned pages with source evidence. The read-only native page
/// is available through `Aj1PageSources::page`; it cannot be re-paired with
/// evidence from another extraction. Legacy public values remain unchanged.
pub fn extract_pages_with_aj1_sources(pdf: &[u8]) -> Result<Vec<Aj1PageSources>> {
    let doc = Document::load(pdf.to_vec())?;
    Ok(doc
        .pages()
        .iter()
        .map(|page| page::extract_page_source_aj1(&doc, page))
        .collect())
}
#[cfg(test)]
#[path = "source_aj1_api_tests.rs"]
mod source_aj1_api_tests;

mod source_images;
pub use source_images::{ImageDraw,XObjectPageSources,CaptureGap as ImageCaptureGap,GapKind as ImageCaptureGapKind,SourceClip as ImageSourceClip,ClipOrigin as ImageClipOrigin,ClipRule as ImageClipRule};
pub fn extract_pages_with_xobject_sources(pdf:&[u8])->Result<Vec<XObjectPageSources>>{source_images::extract(pdf)}

mod source_ruling;

#[cfg(test)]mod source_ruling_pdf_tests;

mod source_lineage;

mod source_scene;

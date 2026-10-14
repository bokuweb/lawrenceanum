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

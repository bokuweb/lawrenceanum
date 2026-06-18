//! poppler `pdftotext` の起動ヘルパ。

use anyhow::{anyhow, Context, Result};
use std::io::Write;
use std::process::{Command, Stdio};

/// `pdftotext -bbox-layout -enc UTF-8 - -` を stdin/stdout で実行し、座標付き XHTML を得る。
pub(crate) fn run_pdftotext_bbox(pdf: &[u8]) -> Result<String> {
    run_pdftotext(pdf, &["-bbox-layout", "-enc", "UTF-8", "-", "-"])
}

/// `pdftotext` を任意オプションで stdin/stdout 実行する共通ヘルパ。
fn run_pdftotext(pdf: &[u8], args: &[&str]) -> Result<String> {
    let mut child = Command::new("pdftotext")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("spawn pdftotext (poppler が必要: brew install poppler / apt install poppler-utils)")?;
    child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("pdftotext stdin unavailable"))?
        .write_all(pdf)
        .context("write pdf to pdftotext stdin")?;
    let out = child.wait_with_output().context("wait pdftotext")?;
    if !out.status.success() {
        return Err(anyhow!("pdftotext exited with {}", out.status));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

//! poppler (`pdftotext` / `pdftocairo`) の起動ヘルパ。

use anyhow::{anyhow, Context, Result};
use std::io::Write;
use std::process::{Command, Stdio};

/// `pdftotext -bbox-layout -enc UTF-8 - -` を stdin/stdout で実行し、座標付き XHTML を得る。
pub(crate) fn run_pdftotext_bbox(pdf: &[u8]) -> Result<String> {
    run_piped("pdftotext", &["-bbox-layout", "-enc", "UTF-8", "-", "-"], pdf)
}

/// `pdftocairo -svg - -` を stdin/stdout で実行し、1 ページ目の SVG を得る（罫線抽出用）。
pub(crate) fn run_pdftocairo_svg(pdf: &[u8]) -> Result<String> {
    run_piped("pdftocairo", &["-svg", "-", "-"], pdf)
}

/// `cmd` を stdin に PDF を流して stdout を文字列で受け取る共通ヘルパ（poppler 系 CLI 用）。
fn run_piped(cmd: &str, args: &[&str], pdf: &[u8]) -> Result<String> {
    let mut child = Command::new(cmd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("spawn {cmd} (poppler が必要: brew install poppler / apt install poppler-utils)"))?;
    child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("{cmd} stdin unavailable"))?
        .write_all(pdf)
        .with_context(|| format!("write pdf to {cmd} stdin"))?;
    let out = child.wait_with_output().with_context(|| format!("wait {cmd}"))?;
    if !out.status.success() {
        return Err(anyhow!("{cmd} exited with {}", out.status));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

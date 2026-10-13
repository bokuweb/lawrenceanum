//! 実官報ページのコーパス（`tests/corpus/<case>/`）で、復元結果をスナップショットと照合する。
//!
//! 各ケースは次のファイルを持つ:
//! - `page.pdf` — 原本（デジタル官報の項目別 PDF 1 ページ）
//! - `page.bbox.html` — `pdftotext -bbox-layout -enc UTF-8 page.pdf page.bbox.html` の出力
//! - `case.json` — 出典・タグ・何を確かめるケースか（notes）
//! - `expected.txt` — 復元した本文（目視で正しいと確認したもの）
//! - `expected.json` — 記事ごとの構造化結果
//!
//! パーサを直して出力が変わったら、差分を目視で確かめてから
//! `UPDATE_EXPECT=1 cargo test -p kanpo-parser --test corpus` で更新する。

use std::path::{Path, PathBuf};

fn cases() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&root)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join("page.bbox.html").exists())
        .collect();
    dirs.sort();
    dirs
}

/// ToUnicode 無しの私用 CID がそのまま漢字として出たときの符号（復元漏れの検出用）。
fn garbled(c: char) -> bool {
    ('\u{630E}'..='\u{6317}').contains(&c) || ('\u{661D}'..='\u{6621}').contains(&c) || c == '\u{617A}'
}

#[test]
fn corpus_matches_expected() {
    let update = std::env::var_os("UPDATE_EXPECT").is_some();
    let mut failures: Vec<String> = Vec::new();
    for dir in cases() {
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        let case: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("case.json")).unwrap()).unwrap();
        assert!(case["title"].is_string() && case["notes"].is_string(), "{name}: case.json に title/notes");
        let xhtml = std::fs::read_to_string(dir.join("page.bbox.html")).unwrap();
        let a = kanpo_parser::analyze(&xhtml);
        let text = format!("{}\n", a.text);
        let docs: Vec<_> = a.articles.iter().map(|art| &art.document).collect();
        let json = format!("{}\n", serde_json::to_string_pretty(&docs).unwrap());

        if let Some(c) = a.text.chars().find(|c| garbled(*c)) {
            failures.push(format!("{name}: 私用 CID の復元漏れ {c:?}"));
        }
        for (file, actual) in [("expected.txt", &text), ("expected.json", &json)] {
            let path = dir.join(file);
            if update {
                std::fs::write(&path, actual).unwrap();
                continue;
            }
            match std::fs::read_to_string(&path) {
                Ok(expected) if &expected == actual => {}
                Ok(expected) => failures.push(format!("{name}/{file} が変わった:\n{}", first_diff(&expected, actual))),
                Err(_) => failures.push(format!("{name}/{file} が無い（UPDATE_EXPECT=1 で作る）")),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// 最初に食い違う行の前後を示す。
fn first_diff(expected: &str, actual: &str) -> String {
    let (e, a): (Vec<&str>, Vec<&str>) = (expected.lines().collect(), actual.lines().collect());
    let i = e.iter().zip(&a).position(|(x, y)| x != y).unwrap_or(e.len().min(a.len()));
    format!(
        "  line {}:\n  - {}\n  + {}",
        i + 1,
        e.get(i).copied().unwrap_or("<EOF>"),
        a.get(i).copied().unwrap_or("<EOF>")
    )
}

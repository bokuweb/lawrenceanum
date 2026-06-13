//! 配信用 JSON を gzip 事前圧縮する (`*.json` / `*.ndjson` → `*.gz`)。
//!
//! SPA 側 (`figma/src/app/data/api.ts` の `getJson`) は `VITE_COMPRESSED` 時に
//! `${path}.gz` を取得し `DecompressionStream('gzip')` で展開する (依存ゼロ)。
//!
//! ## 圧縮対象外
//! - `search.db*`: `sql.js-httpvfs` が無圧縮ファイルへ Range アクセスするため。
//!   外側 gzip すると全 DL になり Range の利点が消える (docs/data-compression-plan.md §4.5)。
//! - 既存の `*.gz`。

use anyhow::{Context, Result};
use flate2::write::GzEncoder;
use flate2::Compression;
use rayon::prelude::*;
use std::io::Write;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// バイト列を gzip 圧縮して返す。
pub fn gzip_bytes(data: &[u8]) -> Result<Vec<u8>> {
    let mut enc = GzEncoder::new(Vec::new(), Compression::best());
    enc.write_all(data).context("gzip write")?;
    enc.finish().context("gzip finish")
}

/// 圧縮対象とみなすか。`.json` / `.ndjson` のみ、`.gz` と `search.db*` は除外。
fn is_compress_target(p: &Path) -> bool {
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if name.starts_with("search.db") {
        return false;
    }
    matches!(
        p.extension().and_then(|e| e.to_str()),
        Some("json") | Some("ndjson")
    )
}

#[derive(Debug, Default, Clone, Copy)]
pub struct CompressStats {
    pub files: usize,
    pub bytes_in: u64,
    pub bytes_out: u64,
}

fn compress_one(p: &Path, remove_original: bool) -> Result<(u64, u64)> {
    let data = std::fs::read(p).with_context(|| format!("read {}", p.display()))?;
    let gz = gzip_bytes(&data)?;
    let out = PathBuf::from(format!("{}.gz", p.display()));
    std::fs::write(&out, &gz).with_context(|| format!("write {}", out.display()))?;
    if remove_original {
        std::fs::remove_file(p).with_context(|| format!("remove {}", p.display()))?;
    }
    Ok((data.len() as u64, gz.len() as u64))
}

/// `root` 配下の `.json` / `.ndjson` を gzip し `*.gz` を書き出す。
/// `remove_original=true` なら元ファイルを削除する (容量削減)。
pub fn run_compress(root: &Path, remove_original: bool) -> Result<CompressStats> {
    let targets: Vec<PathBuf> = WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .filter(|p| is_compress_target(p))
        .collect();

    let results: Vec<Result<(u64, u64)>> = targets
        .par_iter()
        .map(|p| compress_one(p, remove_original))
        .collect();

    let mut stats = CompressStats::default();
    for r in results {
        let (bin, bout) = r?;
        stats.files += 1;
        stats.bytes_in += bin;
        stats.bytes_out += bout;
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::read::GzDecoder;
    use std::io::Read;

    /// tempfile に依存しない一意な作業ディレクトリ。
    fn scratch(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("lawpub_compress_{}_{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn gzip_roundtrip() {
        let original = r#"{"hello":"世界","n":1}"#.as_bytes();
        let gz = gzip_bytes(original).unwrap();
        let mut dec = GzDecoder::new(&gz[..]);
        let mut out = Vec::new();
        dec.read_to_end(&mut out).unwrap();
        assert_eq!(out, original);
    }

    #[test]
    fn compresses_json_and_skips_search_db() {
        let root = scratch("skip");
        std::fs::create_dir_all(root.join("laws/x")).unwrap();
        std::fs::write(root.join("laws/x/current.json"), br#"{"a":1}"#).unwrap();
        std::fs::write(root.join("laws/all.ndjson"), b"{\"id\":1}\n").unwrap();
        std::fs::write(root.join("search.db"), b"binarydata").unwrap();

        let stats = run_compress(&root, false).unwrap();

        assert_eq!(stats.files, 2, "json + ndjson のみ対象");
        assert!(root.join("laws/x/current.json.gz").exists());
        assert!(root.join("laws/all.ndjson.gz").exists());
        assert!(
            !root.join("search.db.gz").exists(),
            "search.db は Range アクセスのため圧縮対象外"
        );
        // remove_original=false なので元ファイルは残る。
        assert!(root.join("laws/x/current.json").exists());

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn remove_original_deletes_source_and_is_idempotent() {
        let root = scratch("remove");
        std::fs::write(root.join("current.json"), br#"{"a":1}"#).unwrap();

        run_compress(&root, true).unwrap();
        assert!(
            !root.join("current.json").exists(),
            "元ファイルは削除される"
        );
        assert!(root.join("current.json.gz").exists());

        // 既に .gz だけの状態で再実行しても対象 0 件 (二重圧縮しない)。
        let again = run_compress(&root, true).unwrap();
        assert_eq!(again.files, 0, ".gz は再圧縮しない");

        std::fs::remove_dir_all(&root).ok();
    }
}

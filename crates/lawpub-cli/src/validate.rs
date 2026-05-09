use anyhow::{Context, Result};
use law_normalizer::sha256_hex;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct Manifest {
    files: Vec<ManifestEntry>,
}

#[derive(Debug, Deserialize)]
struct ManifestEntry {
    path: String,
    sha256: String,
    bytes: u64,
}

pub fn run_validate(public: &Path) -> Result<()> {
    let manifest_path = public.join("manifest.json");
    let bytes = std::fs::read(&manifest_path)
        .with_context(|| format!("read {}", manifest_path.display()))?;
    let manifest: Manifest = serde_json::from_slice(&bytes)?;

    let mut errors = Vec::new();
    for entry in &manifest.files {
        let path = public.join(&entry.path);
        match std::fs::read(&path) {
            Ok(b) => {
                let actual = sha256_hex(&b);
                if actual != entry.sha256 {
                    errors.push(format!(
                        "{}: sha256 mismatch (manifest={}, actual={})",
                        entry.path, entry.sha256, actual
                    ));
                }
                if b.len() as u64 != entry.bytes {
                    errors.push(format!(
                        "{}: size mismatch (manifest={}, actual={})",
                        entry.path,
                        entry.bytes,
                        b.len()
                    ));
                }
            }
            Err(e) => errors.push(format!("{}: {}", entry.path, e)),
        }
    }

    if errors.is_empty() {
        tracing::info!("validate ok ({} files)", manifest.files.len());
        Ok(())
    } else {
        for e in &errors {
            tracing::error!("{}", e);
        }
        anyhow::bail!("validation failed with {} errors", errors.len())
    }
}

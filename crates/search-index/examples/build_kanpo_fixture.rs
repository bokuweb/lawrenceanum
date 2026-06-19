//! e2e 用の小さな kanpo search.db フィクスチャを (再) 生成するメンテナ向けツール。
//! 法令・発言は空で、`public/kanpo/{date}/index.json` 群だけを索引する。
//!
//! 使い方:
//!   cargo run -p search-index --example build_kanpo_fixture -- \
//!     figma/tests/fixtures/public/kanpo figma/tests/fixtures/public/search.db
use std::collections::HashMap;
use std::path::Path;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let kanpo_dir = args.next().expect("arg1: kanpo dir");
    let out = args.next().expect("arg2: out search.db");
    let laws: Vec<law_normalizer::LawDocument> = Vec::new();
    let cats: HashMap<String, String> = HashMap::new();
    search_index::build_search_db(Path::new(&out), &laws, &cats, None, Some(Path::new(&kanpo_dir)))?;
    println!("built {out}");
    Ok(())
}

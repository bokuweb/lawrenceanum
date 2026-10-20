//! 法令英訳 (日本法令外国語訳データベース) の条単位英語 FTS。
//!
//! `public/laws/{law_id}/en.json` (lawpub jlt-build-json) を読み、`laws_en_fts` に 1 条 1 行で入れる。
//! 日本語用の bigram ではなく `porter unicode61` (語幹化) で英文をそのまま索引する。
//! 表示用メタは UNINDEXED で同居させ JOIN 不要にする (search_fts と同じ方針)。
use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde_json::Value;
use std::path::Path;

const BATCH: usize = 50;

pub(super) fn build(conn: &Connection, root: &Path) -> Result<()> {
    conn.execute_batch(
        "CREATE VIRTUAL TABLE laws_en_fts USING fts5(
            law_id UNINDEXED, article_id UNINDEXED, article_no UNINDEXED,
            law_title UNINDEXED, url UNINDEXED,
            title_en, caption_en, text_en,
            tokenize='porter unicode61'
        );",
    )?;
    let laws_dir = root.join("laws");
    let mut files: Vec<std::path::PathBuf> = match std::fs::read_dir(&laws_dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .map(|e| e.path().join("en.json"))
            .filter(|p| p.is_file())
            .collect(),
        Err(_) => Vec::new(),
    };
    files.sort();
    let mut count = 0usize;
    for batch in files.chunks(BATCH) {
        let tx = conn.unchecked_transaction()?;
        {
            let mut insert =
                tx.prepare("INSERT INTO laws_en_fts VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)")?;
            for path in batch {
                let v: Value = serde_json::from_slice(&std::fs::read(path)?)
                    .with_context(|| format!("parse {}", path.display()))?;
                let s =
                    |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
                let law_id = s(&v, "law_id");
                if law_id.is_empty() {
                    continue;
                }
                let law_title = s(&v, "title_en");
                for a in v
                    .get("articles")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    let text = a
                        .get("paragraphs")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(|p| p.get("text").and_then(Value::as_str))
                        .collect::<Vec<_>>()
                        .join("\n");
                    if text.is_empty() {
                        continue;
                    }
                    insert.execute(params![
                        law_id,
                        s(a, "article_id"),
                        s(a, "article_no"),
                        law_title,
                        s(a, "url"),
                        format!("{law_title} {}", s(a, "title")),
                        s(a, "caption"),
                        text,
                    ])?;
                    count += 1;
                }
            }
        }
        tx.commit()?;
    }
    conn.execute(
        "INSERT INTO meta (key, value) VALUES ('en_article_count', ?1)",
        params![count.to_string()],
    )?;
    conn.execute_batch("INSERT INTO laws_en_fts(laws_en_fts) VALUES('optimize');")?;
    tracing::info!("search.db: indexed {count} English articles");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexes_en_json_with_stemming() {
        let root = std::env::temp_dir().join(format!("search-index-en-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("laws").join("129AC0000000089");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::create_dir_all(root.join("laws").join("no-en")).unwrap();
        std::fs::write(
            dir.join("en.json"),
            r#"{"law_id":"129AC0000000089","title_en":"Civil Code","articles":[
                {"article_id":"art_1","article_no":"第一条","title":"Article 1",
                 "caption":"(Fundamental Principles)","url":"u#a",
                 "paragraphs":[{"num":"(1)","text":"Private rights must be congruent with the public welfare."},
                               {"num":"(3)","text":"Abuse of rights is not permitted."}]},
                {"article_id":"art_2","paragraphs":[]}
            ]}"#,
        )
        .unwrap();
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);")
            .unwrap();
        build(&conn, &root).unwrap();
        // "right" は porter で "rights" に当たる。
        let hits: Vec<(String, String)> = conn
            .prepare("SELECT law_id, article_id FROM laws_en_fts WHERE laws_en_fts MATCH 'right AND abuse'")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(
            hits,
            vec![("129AC0000000089".to_string(), "art_1".to_string())]
        );
        let n: String = conn
            .query_row(
                "SELECT value FROM meta WHERE key='en_article_count'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, "1");
        let _ = std::fs::remove_dir_all(&root);
    }
}

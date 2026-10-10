//! 収集済みの行政文書を検索DBに取り込む。原本URLやハッシュは検索本文に混ぜない。
use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn files(dir: &Path, nested: bool) -> Result<Vec<PathBuf>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if nested && path.is_dir() {
            out.extend(files(&path, false)?);
        } else if path.extension().and_then(|s| s.to_str()) == Some("json")
            && path.file_name().and_then(|s| s.to_str()) != Some("index.json")
        {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn body(v: &Value) -> String {
    let mut parts = Vec::new();
    for key in [
        "related_law_name",
        "command_title",
        "legal_basis",
        "agenda",
        "summary",
        "body_text",
        "minutes_text",
    ] {
        parts.push(text(v, key));
    }
    for (collection, keys) in [
        ("opinions", &["item", "opinion", "ministry_response"][..]),
        ("attachments", &["name", "label", "extracted_text"][..]),
        ("documents", &["label", "text"][..]),
        ("fields", &["key", "value"][..]),
    ] {
        if let Some(items) = v.get(collection).and_then(Value::as_array) {
            for item in items {
                for key in keys {
                    parts.push(text(item, key));
                }
            }
        }
    }
    parts.join("\n")
}

pub(super) fn build(conn: &Connection, root: &Path) -> Result<()> {
    conn.execute_batch(
        "CREATE VIRTUAL TABLE documents_fts USING fts5(
        kind UNINDEXED, document_id UNINDEXED, title UNINDEXED,
        subtitle UNINDEXED, route UNINDEXED, title_tokens, text_tokens,
        tokenize='unicode61'
    );",
    )?;
    for (kind, id_key, nested) in [
        ("pubcomment", "case_id", false),
        ("gian", "bill_id", true),
        ("shingikai", "minutes_id", true),
    ] {
        let paths = files(&root.join(kind), nested)?;
        let mut count = 0;
        for batch in paths.chunks(14) {
            let tx = conn.unchecked_transaction()?;
            {
                let mut insert =
                    tx.prepare("INSERT INTO documents_fts VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)")?;
                for path in batch {
                    let v: Value = serde_json::from_slice(&std::fs::read(path)?)
                        .with_context(|| format!("parse {}", path.display()))?;
                    let id = text(&v, id_key);
                    if id.is_empty() {
                        continue;
                    }
                    let title = text(&v, "title");
                    let subtitle = [
                        text(&v, "ministry"),
                        text(&v, "committee"),
                        text(&v, "date"),
                        text(&v, "result_published"),
                    ]
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" · ");
                    let relative = path.strip_prefix(root)?.with_extension("");
                    let route = format!("/{}", relative.to_string_lossy());
                    insert.execute(params![
                        kind,
                        id,
                        title,
                        subtitle,
                        route,
                        crate::tokenize_for_fts(&format!("{title} {subtitle}")),
                        crate::tokenize_for_fts(&body(&v))
                    ])?;
                    count += 1;
                }
            }
            tx.commit()?;
        }
        conn.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)",
            params![format!("{kind}_count"), count.to_string()],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexes_bodies_attachments_and_routes() {
        let root =
            std::env::temp_dir().join(format!("lawpub_document_search_{}", std::process::id()));
        for (path, value) in [
            (
                "pubcomment/case.json",
                serde_json::json!({"case_id":"case", "title":"募集案件", "opinions":[{"ministry_response":"行政回答"}], "attachments":[{"extracted_text":"添付原文"}]}),
            ),
            (
                "gian/221/bill.json",
                serde_json::json!({"bill_id":"bill", "title":"法律案", "documents":[{"text":"提出理由"}]}),
            ),
            (
                "shingikai/moj/meeting.json",
                serde_json::json!({"minutes_id":"meeting", "title":"審議会", "minutes_text":"会議発言", "attachments":[{"extracted_text":"配布資料"}]}),
            ),
        ] {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, value.to_string()).unwrap();
        }
        let db = root.join("search.db");
        crate::build_search_db(&db, &[], &Default::default(), None, None, None).unwrap();
        let conn = Connection::open(db).unwrap();
        for (needle, route) in [
            ("行政回答", "/pubcomment/case"),
            ("添付原文", "/pubcomment/case"),
            ("提出理由", "/gian/221/bill"),
            ("会議発言", "/shingikai/moj/meeting"),
            ("配布資料", "/shingikai/moj/meeting"),
        ] {
            let actual: String = conn
                .query_row(
                    "SELECT route FROM documents_fts WHERE documents_fts MATCH ?1",
                    [crate::tokenize_for_fts(needle)],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(actual, route);
        }
        let count: String = conn
            .query_row("SELECT value FROM meta WHERE key='gian_count'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(count, "1");
        std::fs::remove_dir_all(root).unwrap();
    }
}

//! 自治体例規の全文検索 DB (`reiki-search.db`)。
//!
//! 全国の例規は件数が法令の 100 倍規模になるため、法令用 `search.db` とは別ファイルにして
//! R2 から sql.js-httpvfs で Range 読みする。テーブル設計は次の 2 つ。
//!
//! - `reiki_docs`: 例規 1 件 = 1 行（自治体・題名・番号）。FTS 行から rowid で引く。
//! - `reiki_fts`: 例規の題名行（`article_id = ''`）と条ごとの本文行。bigram で索引する。
//!   本文は検索結果の抜粋に必要な分（条の先頭）だけを `excerpt` に持ち、索引列は
//!   bigram トークンのみ（contentless）にして容量を抑える。
//! - `reiki_municipalities`: 自治体ごとの rowid 範囲。行は自治体コード順に入れるので、
//!   自治体・都道府県（コード上 2 桁）の絞り込みは `rowid BETWEEN` で FTS5 に渡せる。
//!
//! 呼び出し側は docs を**自治体コード昇順**で渡すこと。

use crate::tokenize_for_fts;
use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::Path;

pub struct ReikiFtsArticle {
    pub article_id: String,
    pub article_no: String,
    pub caption: Option<String>,
    pub text: String,
}

pub struct ReikiFtsDoc {
    pub municipality_code: String,
    pub municipality_name: String,
    pub prefecture: String,
    pub reiki_id: String,
    pub title: String,
    pub reiki_number: Option<String>,
    pub kind: Option<String>,
    pub promulgated_date: Option<String>,
    pub articles: Vec<ReikiFtsArticle>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ReikiSearchStats {
    pub docs: usize,
    pub articles: usize,
}

/// 抜粋の最大文字数（検索結果カードに表示する分）。
const EXCERPT_CHARS: usize = 160;
const DOC_BATCH: usize = 500;

fn excerpt(text: &str) -> String {
    let flat: String = text
        .chars()
        .map(|c| if c == '\n' { ' ' } else { c })
        .collect();
    let mut out: String = flat.chars().take(EXCERPT_CHARS).collect();
    if flat.chars().count() > EXCERPT_CHARS {
        out.push('…');
    }
    out
}

pub fn build_reiki_search_db<I>(out_path: &Path, docs: I) -> Result<ReikiSearchStats>
where
    I: IntoIterator<Item = Result<ReikiFtsDoc>>,
{
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let building = out_path.with_extension("db.building");
    if building.exists() {
        std::fs::remove_file(&building)?;
    }
    let conn =
        Connection::open(&building).with_context(|| format!("open {}", building.display()))?;
    conn.execute_batch(
        r#"
        PRAGMA page_size = 65536;
        PRAGMA journal_mode = OFF;
        PRAGMA synchronous = OFF;
        PRAGMA temp_store = FILE;
        PRAGMA cache_size = -32768;

        CREATE TABLE reiki_docs (
            id INTEGER PRIMARY KEY,
            municipality_code TEXT NOT NULL,
            municipality_name TEXT NOT NULL,
            prefecture TEXT NOT NULL,
            reiki_id TEXT NOT NULL,
            title TEXT NOT NULL,
            reiki_number TEXT,
            kind TEXT,
            promulgated_date TEXT
        );

        -- rowid でテキスト列を引く contentless FTS。表示用の列は reiki_fts_meta に持つ。
        CREATE VIRTUAL TABLE reiki_fts USING fts5(
            title_tokens,
            content_tokens,
            content='',
            tokenize='unicode61'
        );

        CREATE TABLE reiki_fts_meta (
            rowid INTEGER PRIMARY KEY,
            doc_id INTEGER NOT NULL,
            article_id TEXT NOT NULL,
            article_no TEXT NOT NULL,
            caption TEXT,
            excerpt TEXT NOT NULL
        );

        CREATE TABLE reiki_municipalities (
            municipality_code TEXT PRIMARY KEY,
            municipality_name TEXT NOT NULL,
            prefecture TEXT NOT NULL,
            min_rowid INTEGER NOT NULL,
            max_rowid INTEGER NOT NULL,
            doc_count INTEGER NOT NULL
        );

        CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
        "#,
    )?;

    let mut stats = ReikiSearchStats::default();
    let mut rowid: i64 = 0;
    // (code, name, prefecture, min_rowid, max_rowid, docs)
    let mut ranges: Vec<(String, String, String, i64, i64, i64)> = Vec::new();
    let mut iter = docs.into_iter().peekable();
    while iter.peek().is_some() {
        let tx = conn.unchecked_transaction()?;
        {
            let mut ins_doc = tx.prepare(
                "INSERT INTO reiki_docs (id, municipality_code, municipality_name, prefecture, reiki_id, \
                 title, reiki_number, kind, promulgated_date) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )?;
            let mut ins_fts = tx.prepare(
                "INSERT INTO reiki_fts (rowid, title_tokens, content_tokens) VALUES (?1, ?2, ?3)",
            )?;
            let mut ins_meta = tx.prepare(
                "INSERT INTO reiki_fts_meta (rowid, doc_id, article_id, article_no, caption, excerpt) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for doc in iter.by_ref().take(DOC_BATCH) {
                let doc = doc?;
                match ranges.last_mut() {
                    Some(r) if r.0 == doc.municipality_code => r.5 += 1,
                    last => {
                        if let Some(prev) = last {
                            anyhow::ensure!(
                                prev.0 < doc.municipality_code,
                                "reiki docs must be sorted by municipality code ({} after {})",
                                doc.municipality_code,
                                prev.0
                            );
                        }
                        ranges.push((
                            doc.municipality_code.clone(),
                            doc.municipality_name.clone(),
                            doc.prefecture.clone(),
                            rowid + 1,
                            rowid + 1,
                            1,
                        ));
                    }
                }
                stats.docs += 1;
                let doc_id = stats.docs as i64;
                ins_doc.execute(params![
                    doc_id,
                    doc.municipality_code,
                    doc.municipality_name,
                    doc.prefecture,
                    doc.reiki_id,
                    doc.title,
                    doc.reiki_number,
                    doc.kind,
                    doc.promulgated_date,
                ])?;
                // 題名行: 題名で引けるようにし、抜粋は第1条の頭を出す。
                rowid += 1;
                let first = doc.articles.first().map(|a| a.text.as_str()).unwrap_or("");
                ins_fts.execute(params![rowid, tokenize_for_fts(&doc.title), ""])?;
                ins_meta.execute(params![
                    rowid,
                    doc_id,
                    "",
                    "",
                    Option::<String>::None,
                    excerpt(first)
                ])?;
                for a in &doc.articles {
                    if a.text.trim().is_empty() {
                        continue;
                    }
                    rowid += 1;
                    stats.articles += 1;
                    ins_fts.execute(params![rowid, "", tokenize_for_fts(&a.text)])?;
                    ins_meta.execute(params![
                        rowid,
                        doc_id,
                        a.article_id,
                        a.article_no,
                        a.caption,
                        excerpt(&a.text)
                    ])?;
                }
                if let Some(r) = ranges.last_mut() {
                    r.4 = rowid;
                }
            }
        }
        tx.commit()?;
        tracing::info!(
            "reiki-search.db: indexed {} docs / {} articles",
            stats.docs,
            stats.articles
        );
    }

    {
        let tx = conn.unchecked_transaction()?;
        {
            let mut st = tx.prepare(
                "INSERT INTO reiki_municipalities (municipality_code, municipality_name, prefecture, \
                 min_rowid, max_rowid, doc_count) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for r in &ranges {
                st.execute(params![r.0, r.1, r.2, r.3, r.4, r.5])?;
            }
        }
        tx.commit()?;
    }
    conn.execute_batch("CREATE INDEX reiki_docs_muni ON reiki_docs(municipality_code);")?;
    conn.execute(
        "INSERT INTO meta (key, value) VALUES ('built_at', datetime('now')), ('doc_count', ?1), \
         ('article_count', ?2), ('tokenizer', 'bigram')",
        params![stats.docs as i64, stats.articles as i64],
    )?;
    conn.execute_batch("INSERT INTO reiki_fts(reiki_fts) VALUES('optimize'); PRAGMA optimize;")?;
    drop(conn);
    std::fs::rename(&building, out_path)
        .with_context(|| format!("replace {} with {}", out_path.display(), building.display()))?;
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(code: &str, name: &str, id: &str, title: &str, body: &str) -> ReikiFtsDoc {
        ReikiFtsDoc {
            municipality_code: code.into(),
            municipality_name: name.into(),
            prefecture: "千葉県".into(),
            reiki_id: id.into(),
            title: title.into(),
            reiki_number: Some("条例第1号".into()),
            kind: Some("条例".into()),
            promulgated_date: Some("2020-04-01".into()),
            articles: vec![ReikiFtsArticle {
                article_id: "art_1".into(),
                article_no: "第1条".into(),
                caption: Some("目的".into()),
                text: body.into(),
            }],
        }
    }

    #[test]
    fn indexes_titles_and_articles() {
        let out = std::env::temp_dir().join(format!("lawpub_reiki_fts_{}.db", std::process::id()));
        let stats = build_reiki_search_db(
            &out,
            vec![
                Ok(doc(
                    "012122",
                    "留萌市",
                    "012122_b",
                    "留萌市情報公開条例",
                    "公文書の公開を請求する権利を定める。",
                )),
                Ok(doc(
                    "121002",
                    "千葉市",
                    "121002_a",
                    "千葉市空家等対策条例",
                    "空家等の適切な管理について定める。",
                )),
            ],
        )
        .unwrap();
        assert_eq!(stats.docs, 2);
        assert_eq!(stats.articles, 2);
        let conn = Connection::open(&out).unwrap();
        let q = |sql: &str, term: &str| -> Vec<(String, String)> {
            let mut st = conn.prepare(sql).unwrap();
            st.query_map([tokenize_for_fts(term)], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .map(|r| r.unwrap())
                .collect()
        };
        let sql = "SELECT d.municipality_name, m.article_id FROM reiki_fts f \
                   JOIN reiki_fts_meta m ON m.rowid = f.rowid JOIN reiki_docs d ON d.id = m.doc_id \
                   WHERE reiki_fts MATCH ?1 ORDER BY rank";
        assert_eq!(
            q(sql, "空家"),
            vec![
                ("千葉市".into(), "".into()),
                ("千葉市".into(), "art_1".into())
            ]
        );
        assert_eq!(q(sql, "公文書"), vec![("留萌市".into(), "art_1".into())]);
        let sql_title = "SELECT d.title, m.article_id FROM reiki_fts f JOIN reiki_fts_meta m ON m.rowid = f.rowid \
                         JOIN reiki_docs d ON d.id = m.doc_id WHERE reiki_fts MATCH 'title_tokens : ' || ?1";
        assert_eq!(
            q(sql_title, "情報公開"),
            vec![("留萌市情報公開条例".into(), "".into())]
        );
        // 自治体の rowid 範囲で絞り込める
        let (lo, hi): (i64, i64) = conn
            .query_row(
                "SELECT min_rowid, max_rowid FROM reiki_municipalities WHERE municipality_code = '121002'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM reiki_fts WHERE reiki_fts MATCH ?1 AND rowid BETWEEN ?2 AND ?3",
                params![tokenize_for_fts("定める"), lo, hi],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
        std::fs::remove_file(&out).ok();
    }
}

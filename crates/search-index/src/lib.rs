//! 法令検索用 SQLite (FTS5) を組み立てる。
//!
//! 設計方針 (`../ellisii` の `jp-tokenizer-bigram` + `store-sqlite` を参考):
//! - 既定の `unicode61` FTS5 トークナイザは日本語の語境界を扱えないので、取り込み
//!   時 / クエリ時に **同じ bigram トークナイザ** で前段分割し、空白区切りの
//!   文字列を FTS5 列に詰める。
//! - 出力ファイル `search.db` をブラウザの sql.js で読んで条文ベース検索する。
//! - 1 行 = 1 条文。`law_id` と `article_id` を UNINDEXED 列として持つ。

use anyhow::{Context, Result};
use law_normalizer::LawDocument;
use rusqlite::{params, Connection};
use std::path::Path;

pub fn is_cjk(c: char) -> bool {
    matches!(
        c,
        '\u{3040}'..='\u{309f}'
            | '\u{30a0}'..='\u{30ff}'
            | '\u{31f0}'..='\u{31ff}'
            | '\u{3400}'..='\u{4dbf}'
            | '\u{4e00}'..='\u{9fff}'
            | '\u{f900}'..='\u{faff}'
            | '\u{ff66}'..='\u{ff9d}'
    )
}

pub fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || is_cjk(c)
}

/// 文字 bigram トークナイズ。CJK は重なり 2-gram、ASCII/数字はそのまま 1 単語。
/// 同じ関数をブラウザ側 (TS) でも実装するので、結果が一致するよう極力単純に保つ。
pub fn tokenize(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut buf = String::new();
    let mut buf_is_cjk = false;

    let flush = |buf: &mut String, is_cjk_buf: bool, out: &mut Vec<String>| {
        if buf.is_empty() {
            return;
        }
        if is_cjk_buf {
            let chars: Vec<char> = buf.chars().collect();
            if chars.len() == 1 {
                out.push(chars[0].to_string());
            } else {
                for w in chars.windows(2) {
                    out.push(w.iter().collect::<String>());
                }
            }
        } else {
            out.push(buf.to_lowercase());
        }
        buf.clear();
    };

    for c in text.chars() {
        if !is_word_char(c) {
            flush(&mut buf, buf_is_cjk, &mut out);
            continue;
        }
        let cur_is_cjk = is_cjk(c);
        if !buf.is_empty() && cur_is_cjk != buf_is_cjk {
            flush(&mut buf, buf_is_cjk, &mut out);
        }
        buf.push(c);
        buf_is_cjk = cur_is_cjk;
    }
    flush(&mut buf, buf_is_cjk, &mut out);
    out
}

pub fn tokenize_for_fts(text: &str) -> String {
    tokenize(text).join(" ")
}

/// 「前条」「次条」相対参照の解決。current_idx の前後の article を返す。
pub fn extract_relative_article_refs(
    body: &str,
    current_idx: usize,
    articles_in_order: &[(String, String)], // (article_id, article_no)
) -> Vec<(String, String, &'static str)> {
    // (matched_text, to_article_id, ref_type)
    let mut out: Vec<(String, String, &'static str)> = Vec::new();
    let mut handle = |needle: &str, target: Option<usize>, ref_type: &'static str| {
        if !body.contains(needle) {
            return;
        }
        if let Some(idx) = target {
            if let Some((id, _)) = articles_in_order.get(idx) {
                out.push((needle.to_string(), id.clone(), ref_type));
            }
        }
    };
    if current_idx > 0 {
        handle("前条", Some(current_idx - 1), "previous_article");
    }
    handle("次条", Some(current_idx + 1), "next_article");
    out
}

/// 他法令への参照: `title + 第○条` のパターンを検出する。
/// `title_index`: 法令タイトル → law_id
/// `articles_index`: law_id → (article_no -> article_id)
/// 戻り値: (matched_text_full, to_law_id, to_article_id_opt)
pub fn extract_cross_law_refs(
    body: &str,
    self_law_id: &str,
    title_index: &std::collections::HashMap<String, String>,
    articles_index: &std::collections::HashMap<String, std::collections::HashMap<String, String>>,
) -> Vec<(String, String, Option<String>)> {
    let mut out: Vec<(String, String, Option<String>)> = Vec::new();
    // title は長いものから優先 (民法 と 民事訴訟法 など部分一致を避ける)。
    let mut titles: Vec<&String> = title_index.keys().collect();
    titles.sort_by(|a, b| b.chars().count().cmp(&a.chars().count()));
    for title in titles {
        let other_id = match title_index.get(title) {
            Some(v) => v,
            None => continue,
        };
        if other_id == self_law_id {
            continue;
        }
        let mut start = 0usize;
        while let Some(pos) = body[start..].find(title.as_str()) {
            let abs = start + pos;
            let after = abs + title.len();
            // title 直後に「第○条」が続くか確認。最大 12 文字 (第十二条の二 等) スキャン。
            let tail = &body[after..(after + 36).min(body.len())];
            if let Some(art_map) = articles_index.get(other_id) {
                if let Some((art_text, art_id)) = match_article_prefix(tail, art_map) {
                    let full = format!("{}{}", title, art_text);
                    out.push((full, other_id.clone(), Some(art_id)));
                    start = after + art_text.len();
                    continue;
                }
            }
            // article がヒットしない場合でも法令名だけは記録 (to_article_id=None)。
            out.push((title.to_string(), other_id.clone(), None));
            start = after;
        }
    }
    out
}

/// `tail` の先頭が `art_map` のいずれかの key で始まるか調べる。
/// 一致時は (matched_text, article_id) を返す。長い key を優先。
fn match_article_prefix(
    tail: &str,
    art_map: &std::collections::HashMap<String, String>,
) -> Option<(String, String)> {
    let mut keys: Vec<&String> = art_map.keys().collect();
    keys.sort_by(|a, b| b.chars().count().cmp(&a.chars().count()));
    for key in keys {
        if tail.starts_with(key.as_str()) {
            return Some((key.clone(), art_map[key].clone()));
        }
    }
    None
}

/// 法令本文中の同一法令内 article 参照を抽出する。
/// `article_no_to_id`: その法令の "第三条" 等 → "art_3" の lookup map。
/// 戻り値の `(ref_text, to_article_id)` は出現順 / 重複排除なし。呼び出し側で
/// 必要なら dedup する。
pub fn extract_self_article_refs<'a>(
    body: &'a str,
    article_no_to_id: &'a std::collections::HashMap<String, String>,
) -> Vec<(&'a str, String)> {
    let mut hits: Vec<(usize, usize, &str, String)> = Vec::new();
    // article_no が長いものから順に試すと "第百条" と "第十条" の被りを回避できる。
    let mut keys: Vec<&String> = article_no_to_id.keys().collect();
    keys.sort_by(|a, b| b.chars().count().cmp(&a.chars().count()));

    for key in keys {
        let mut start = 0usize;
        while let Some(pos) = body[start..].find(key.as_str()) {
            let abs = start + pos;
            let end = abs + key.len();
            // 既に他のキーで覆われていれば skip。
            let overlaps = hits
                .iter()
                .any(|(s, e, _, _)| !(end <= *s || abs >= *e));
            if !overlaps {
                if let Some(id) = article_no_to_id.get(key) {
                    let s = &body[abs..end];
                    hits.push((abs, end, s, id.clone()));
                }
            }
            start = end;
        }
    }
    hits.sort_by_key(|(s, _, _, _)| *s);
    hits.into_iter().map(|(_, _, t, id)| (t, id)).collect()
}

pub fn build_search_db(out_path: &Path, laws: &[LawDocument]) -> Result<()> {
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if out_path.exists() {
        std::fs::remove_file(out_path)?;
    }
    let conn = Connection::open(out_path).with_context(|| format!("open {}", out_path.display()))?;

    conn.execute_batch(
        r#"
        PRAGMA journal_mode = OFF;
        PRAGMA synchronous = OFF;

        CREATE TABLE laws (
            law_id TEXT PRIMARY KEY,
            law_num TEXT,
            title TEXT NOT NULL
        );

        CREATE VIRTUAL TABLE search_fts USING fts5(
            law_id UNINDEXED,
            article_id UNINDEXED,
            article_no UNINDEXED,
            caption UNINDEXED,
            title_tokens,
            content_tokens,
            tokenize='unicode61'
        );

        -- 条文間参照: 同一法令内の "第○条" 参照だけを Phase 1 で持つ。
        -- ref_type は "self_article"。将来 cross_law 等を足すために列だけ確保。
        CREATE TABLE refs (
            id            INTEGER PRIMARY KEY,
            from_law_id   TEXT NOT NULL,
            from_article_id TEXT NOT NULL,
            to_law_id     TEXT NOT NULL,
            to_article_id TEXT,
            ref_text      TEXT NOT NULL,
            ref_type      TEXT NOT NULL DEFAULT 'self_article'
        );
        CREATE INDEX idx_refs_from ON refs(from_law_id, from_article_id);
        CREATE INDEX idx_refs_to   ON refs(to_law_id, to_article_id);

        CREATE TABLE meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        "#,
    )
    .context("schema")?;

    let tx = conn.unchecked_transaction()?;
    {
        let mut law_stmt =
            tx.prepare("INSERT INTO laws (law_id, law_num, title) VALUES (?1, ?2, ?3)")?;
        let mut fts_stmt = tx.prepare(
            "INSERT INTO search_fts (law_id, article_id, article_no, caption, title_tokens, content_tokens) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        let mut ref_stmt = tx.prepare(
            "INSERT INTO refs (from_law_id, from_article_id, to_law_id, to_article_id, ref_text, ref_type) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;

        let mut total_articles = 0usize;
        let mut total_refs = 0usize;

        // 全法令にまたがる索引を一度だけ作って cross-law 解決に流用する。
        let mut title_index: std::collections::HashMap<String, String> = Default::default();
        let mut articles_index: std::collections::HashMap<
            String,
            std::collections::HashMap<String, String>,
        > = Default::default();
        for d in laws {
            if !d.title.is_empty() {
                title_index.insert(d.title.clone(), d.law_id.clone());
            }
            let mut m: std::collections::HashMap<String, String> = Default::default();
            for a in &d.articles {
                if !a.article_no.is_empty() {
                    m.insert(a.article_no.clone(), a.article_id.clone());
                }
            }
            articles_index.insert(d.law_id.clone(), m);
        }

        for d in laws {
            law_stmt.execute(params![d.law_id, d.law_num, d.title])?;
            let title_tokens = tokenize_for_fts(&d.title);

            let no_to_id = articles_index.get(&d.law_id).cloned().unwrap_or_default();
            let articles_in_order: Vec<(String, String)> = d
                .articles
                .iter()
                .map(|a| (a.article_id.clone(), a.article_no.clone()))
                .collect();

            for (idx, a) in d.articles.iter().enumerate() {
                let body = a
                    .paragraphs
                    .iter()
                    .map(|p| p.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                let content = format!(
                    "{} {} {}",
                    a.article_no,
                    a.caption.clone().unwrap_or_default(),
                    body
                );
                let content_tokens = tokenize_for_fts(&content);
                fts_stmt.execute(params![
                    d.law_id,
                    a.article_id,
                    a.article_no,
                    a.caption.clone().unwrap_or_default(),
                    title_tokens,
                    content_tokens,
                ])?;
                total_articles += 1;

                // 1) 自己参照 (同一法令内の "第○条")。
                let mut emitted: std::collections::HashSet<(String, String, String)> =
                    Default::default();
                for (text, to_id) in extract_self_article_refs(&body, &no_to_id) {
                    if to_id == a.article_id {
                        continue;
                    }
                    let key = (text.to_string(), to_id.clone(), "self_article".into());
                    if emitted.contains(&key) {
                        continue;
                    }
                    emitted.insert(key);
                    ref_stmt.execute(params![
                        d.law_id,
                        a.article_id,
                        d.law_id,
                        to_id,
                        text,
                        "self_article",
                    ])?;
                    total_refs += 1;
                }

                // 2) 前条/次条 相対参照。
                for (text, to_id, ref_type) in
                    extract_relative_article_refs(&body, idx, &articles_in_order)
                {
                    let key = (text.clone(), to_id.clone(), ref_type.to_string());
                    if emitted.contains(&key) {
                        continue;
                    }
                    emitted.insert(key);
                    ref_stmt.execute(params![
                        d.law_id,
                        a.article_id,
                        d.law_id,
                        to_id,
                        text,
                        ref_type,
                    ])?;
                    total_refs += 1;
                }

                // 3) 他法令参照 (例「民法第七百九条」)。
                for (text, to_law, to_art) in
                    extract_cross_law_refs(&body, &d.law_id, &title_index, &articles_index)
                {
                    let key = (text.clone(), to_art.clone().unwrap_or_default(), "cross_law".into());
                    if emitted.contains(&key) {
                        continue;
                    }
                    emitted.insert(key);
                    ref_stmt.execute(params![
                        d.law_id,
                        a.article_id,
                        to_law,
                        to_art,
                        text,
                        "cross_law",
                    ])?;
                    total_refs += 1;
                }
            }
        }
        tracing::info!(
            "search.db: indexed {} laws / {} articles / {} refs",
            laws.len(),
            total_articles,
            total_refs
        );
        tx.execute(
            "INSERT INTO meta (key, value) VALUES ('built_at', datetime('now')), \
             ('law_count', ?1), ('article_count', ?2), ('ref_count', ?3), ('tokenizer', 'bigram')",
            params![
                laws.len() as i64,
                total_articles as i64,
                total_refs as i64
            ],
        )?;
    }
    tx.commit()?;
    conn.execute_batch("VACUUM;").ok();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cjk_bigram() {
        assert_eq!(
            tokenize("東京駅"),
            vec!["東京".to_string(), "京駅".to_string()]
        );
    }
    #[test]
    fn ascii_lower() {
        assert_eq!(tokenize("Hello World"), vec!["hello", "world"]);
    }
    #[test]
    fn fts_join() {
        assert_eq!(tokenize_for_fts("東京駅前"), "東京 京駅 駅前");
        assert_eq!(tokenize_for_fts("検索"), "検索");
    }

    #[test]
    fn extracts_self_article_refs_with_overlap_priority() {
        let mut map = std::collections::HashMap::new();
        map.insert("第一条".to_string(), "art_1".to_string());
        map.insert("第十条".to_string(), "art_10".to_string());
        map.insert("第百条".to_string(), "art_100".to_string());
        let body = "第一条の規定により、第十条と第百条を準用する。";
        let hits = extract_self_article_refs(body, &map);
        let ids: Vec<&String> = hits.iter().map(|(_, id)| id).collect();
        assert_eq!(ids, vec!["art_1", "art_10", "art_100"]);
    }
}

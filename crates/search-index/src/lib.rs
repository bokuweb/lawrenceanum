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

/// 他法令参照のために事前構築する索引。Aho-Corasick オートマトンとそのスロットに
/// 対応する `(title, law_id)` 配列を保持する。法令単位で 1 度だけ作って使い回す。
pub struct CrossLawIndex {
    ac: AhoCorasick,
    entries: Vec<(String, String)>, // (title, law_id) — index = pattern_id
}

impl CrossLawIndex {
    pub fn build(title_index: &std::collections::HashMap<String, String>) -> Option<Self> {
        // 1 文字タイトルは除外 (誤マッチ多発)。"民法" "刑法" 等 2 文字は残す。
        let mut entries: Vec<(String, String)> = title_index
            .iter()
            .filter(|(t, _)| t.chars().count() >= 2)
            .map(|(t, id)| (t.clone(), id.clone()))
            .collect();
        if entries.is_empty() {
            return None;
        }
        // LeftmostLongest で最長一致 (民事訴訟法 と 民法 が混在しても長い方が勝つ)。
        entries.sort_by(|a, b| b.0.chars().count().cmp(&a.0.chars().count()));
        let patterns: Vec<&str> = entries.iter().map(|(t, _)| t.as_str()).collect();
        let ac = AhoCorasick::builder()
            .match_kind(MatchKind::LeftmostLongest)
            .build(&patterns)
            .ok()?;
        Some(Self { ac, entries })
    }
}

/// 他法令への参照: `title + 第○条` のパターンを検出する。
pub fn extract_cross_law_refs(
    body: &str,
    self_law_id: &str,
    cross_index: &CrossLawIndex,
    articles_index: &std::collections::HashMap<String, std::collections::HashMap<String, String>>,
) -> Vec<(String, String, Option<String>)> {
    let mut out: Vec<(String, String, Option<String>)> = Vec::new();
    for m in cross_index.ac.find_iter(body) {
        let (title, other_id) = &cross_index.entries[m.pattern().as_usize()];
        if other_id == self_law_id {
            continue;
        }
        let after = m.end();
        // 後続最大 16 文字を見て「第○条」を探す。byte ではなく char 数で切ること。
        let tail: String = body[after..].chars().take(16).collect();
        if let Some(art_map) = articles_index.get(other_id) {
            if let Some((art_text, art_id)) = match_article_prefix(&tail, art_map) {
                let full = format!("{}{}", title, art_text);
                out.push((full, other_id.clone(), Some(art_id)));
                continue;
            }
        }
        out.push((title.clone(), other_id.clone(), None));
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

use aho_corasick::{AhoCorasick, MatchKind};

/// 法令本文中の同一法令内 article 参照を抽出する。
/// Aho-Corasick で全 article_no を同時に LeftmostLongest マッチさせ、
/// `第百条` と `第十条` の重なりは長い方を採る。
pub fn extract_self_article_refs<'a>(
    body: &'a str,
    article_no_to_id: &'a std::collections::HashMap<String, String>,
) -> Vec<(&'a str, String)> {
    if article_no_to_id.is_empty() || body.is_empty() {
        return Vec::new();
    }
    let keys: Vec<&str> = article_no_to_id.keys().map(|s| s.as_str()).collect();
    let ac = match AhoCorasick::builder()
        .match_kind(MatchKind::LeftmostLongest)
        .build(&keys)
    {
        Ok(a) => a,
        Err(_) => return Vec::new(),
    };
    let mut out: Vec<(&str, String)> = Vec::new();
    for m in ac.find_iter(body) {
        let key = keys[m.pattern().as_usize()];
        if let Some(id) = article_no_to_id.get(key) {
            out.push((&body[m.start()..m.end()], id.clone()));
        }
    }
    out
}

/// `laws`: 索引対象の現行版 LawDocument 群。
/// `categories`: law_id → e-Gov 法令分類 (「民事」「行政組織」など) の対応表。
///   FTS5 検索結果をカテゴリで絞り込めるよう `laws.category` 列に格納する。
///   未知の law_id は NULL になる。
/// `proceedings_dir`: `public/proceedings/` へのパス。Some の場合は発言 FTS も構築。
/// `kanpo_dir`: `public/kanpo/` へのパス。Some の場合は官報記事 FTS も構築。
pub fn build_search_db(
    out_path: &Path,
    laws: &[LawDocument],
    categories: &std::collections::HashMap<String, String>,
    proceedings_dir: Option<&Path>,
    kanpo_dir: Option<&Path>,
) -> Result<()> {
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if out_path.exists() {
        std::fs::remove_file(out_path)?;
    }
    let conn = Connection::open(out_path).with_context(|| format!("open {}", out_path.display()))?;

    conn.execute_batch(
        r#"
        -- 64KB pages align with the browser-side requestChunkSize=65536 so each
        -- HTTP Range fetch from R2 retrieves exactly one SQLite page.
        PRAGMA page_size = 65536;
        PRAGMA journal_mode = OFF;
        PRAGMA synchronous = OFF;

        CREATE TABLE laws (
            law_id TEXT PRIMARY KEY,
            law_num TEXT,
            title TEXT NOT NULL,
            -- e-Gov 法令分類 (50 区分)。FTS5 検索のカテゴリ絞り込みに使う。
            category TEXT
        );
        CREATE INDEX idx_laws_category ON laws(category);

        -- 条文単位 FTS5。8800 法令で約 1.5GB になるが、ブラウザは
        -- sql.js-httpvfs 経由で必要な page (4KB 単位) のみ Range fetch するので
        -- 1 query あたり 100〜300KB しか DL しない。host は Cloudflare R2 を想定
        -- (egress 無料 + 10GB 無料枠)。
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

        -- 発言単位 FTS5。発言テキストを bigram 索引する。
        -- meeting_id / speech_id / speaker / speaker_group は検索対象外 (UNINDEXED)。
        CREATE VIRTUAL TABLE speeches_fts USING fts5(
            meeting_id UNINDEXED,
            speech_id UNINDEXED,
            speaker UNINDEXED,
            speaker_group UNINDEXED,
            speech_tokens,
            tokenize='unicode61'
        );

        -- 会議メタ (JOIN 用)。index.json の MeetingMeta に対応。
        CREATE TABLE meetings (
            meeting_id TEXT PRIMARY KEY,
            session INTEGER,
            house TEXT,
            committee TEXT,
            date TEXT,
            speech_count INTEGER
        );

        -- 官報記事 FTS5。改め文 (amend_text) と記事タイトルを bigram 索引する。
        -- 表示用メタ (date/issue_no/title/page/pdf_url/agency) は UNINDEXED で同居させ
        -- JOIN 不要にする (search_fts と同じ方針)。
        CREATE VIRTUAL TABLE kanpo_fts USING fts5(
            date UNINDEXED,
            issue_no UNINDEXED,
            title UNINDEXED,
            page UNINDEXED,
            pdf_url UNINDEXED,
            agency UNINDEXED,
            title_tokens,
            content_tokens,
            tokenize='unicode61'
        );

        CREATE TABLE meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        "#,
    )
    .context("schema")?;

    let tx = conn.unchecked_transaction()?;
    {
        let mut law_stmt = tx.prepare(
            "INSERT INTO laws (law_id, law_num, title, category) VALUES (?1, ?2, ?3, ?4)",
        )?;
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

        // cross-law 用の AC オートマトンは法令一覧から 1 度だけ構築。
        let cross_index = CrossLawIndex::build(&title_index);

        for d in laws {
            law_stmt.execute(params![
                d.law_id,
                d.law_num,
                d.title,
                categories.get(&d.law_id),
            ])?;
            let title_tokens = tokenize_for_fts(&format!(
                "{} {}",
                d.title,
                d.law_num.clone().unwrap_or_default()
            ));

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

                // 3) 他法令参照 (例「民法第七百九条」)。AC オートマトンが無い時は skip。
                let cross_iter: Vec<(String, String, Option<String>)> =
                    if let Some(ix) = cross_index.as_ref() {
                        extract_cross_law_refs(&body, &d.law_id, ix, &articles_index)
                    } else {
                        Vec::new()
                    };
                for (text, to_law, to_art) in cross_iter {
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

        // ── 発言 FTS (proceedings_dir が Some のとき) ───────────────────────
        let mut total_meetings = 0usize;
        let mut total_speeches = 0usize;
        if let Some(proc_dir) = proceedings_dir {
            let mut meeting_stmt = tx.prepare(
                "INSERT OR IGNORE INTO meetings (meeting_id, session, house, committee, date, speech_count) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            let mut speech_stmt = tx.prepare(
                "INSERT INTO speeches_fts (meeting_id, speech_id, speaker, speaker_group, speech_tokens) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;

            // index.json を読んで meetings テーブルに挿入。
            let index_path = proc_dir.join("index.json");
            if index_path.exists() {
                let raw = std::fs::read_to_string(&index_path)
                    .with_context(|| format!("read {}", index_path.display()))?;
                let index: serde_json::Value = serde_json::from_str(&raw)?;
                if let Some(meetings) = index.get("meetings").and_then(|v| v.as_array()) {
                    for m in meetings {
                        let meeting_id = m.get("meeting_id").and_then(|v| v.as_str()).unwrap_or("");
                        let session = m.get("session").and_then(|v| v.as_i64()).unwrap_or(0);
                        let house = m.get("house").and_then(|v| v.as_str()).unwrap_or("");
                        let committee = m.get("committee").and_then(|v| v.as_str());
                        let date = m.get("date").and_then(|v| v.as_str()).unwrap_or("");
                        let speech_count = m.get("speech_count").and_then(|v| v.as_i64()).unwrap_or(0);
                        meeting_stmt.execute(params![
                            meeting_id, session, house, committee, date, speech_count,
                        ])?;
                        total_meetings += 1;

                        // 個別会議 JSON を読んで発言を索引。
                        let meeting_path = proc_dir.join(format!("{}.json", meeting_id));
                        if !meeting_path.exists() {
                            continue;
                        }
                        let raw2 = std::fs::read_to_string(&meeting_path)
                            .with_context(|| format!("read {}", meeting_path.display()))?;
                        let meeting_json: serde_json::Value = serde_json::from_str(&raw2)?;
                        if let Some(speeches) = meeting_json.get("speeches").and_then(|v| v.as_array()) {
                            for s in speeches {
                                let speech_id = s.get("speech_id").and_then(|v| v.as_str()).unwrap_or("");
                                let speaker = s.get("speaker").and_then(|v| v.as_str());
                                let speaker_group = s.get("speaker_group").and_then(|v| v.as_str());
                                let speech_text = s.get("speech").and_then(|v| v.as_str()).unwrap_or("");
                                if speech_text.is_empty() {
                                    continue;
                                }
                                let tokens = tokenize_for_fts(speech_text);
                                speech_stmt.execute(params![
                                    meeting_id, speech_id, speaker, speaker_group, tokens,
                                ])?;
                                total_speeches += 1;
                            }
                        }
                    }
                }
            }
            tracing::info!(
                "search.db: indexed {} meetings / {} speeches",
                total_meetings,
                total_speeches
            );
        }

        // ── 官報記事 FTS (kanpo_dir が Some のとき) ─────────────────────────
        let mut total_kanpo = 0usize;
        if let Some(kdir) = kanpo_dir {
            let mut kanpo_stmt = tx.prepare(
                "INSERT INTO kanpo_fts \
                 (date, issue_no, title, page, pdf_url, agency, title_tokens, content_tokens) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;
            // `public/kanpo/{date}/index.json` を日付昇順に走査する。
            let mut date_dirs: Vec<std::path::PathBuf> = std::fs::read_dir(kdir)
                .map(|rd| {
                    rd.filter_map(|e| e.ok())
                        .map(|e| e.path())
                        .filter(|p| p.is_dir())
                        .collect()
                })
                .unwrap_or_default();
            date_dirs.sort();
            for ddir in &date_dirs {
                let index_path = ddir.join("index.json");
                if !index_path.exists() {
                    continue;
                }
                let raw = std::fs::read_to_string(&index_path)
                    .with_context(|| format!("read {}", index_path.display()))?;
                let v: serde_json::Value = serde_json::from_str(&raw)?;
                let date = v.get("date").and_then(|x| x.as_str()).unwrap_or("");
                let Some(issues) = v.get("issues").and_then(|x| x.as_array()) else {
                    continue;
                };
                for issue in issues {
                    let issue_no = issue.get("issue_no").and_then(|x| x.as_str()).unwrap_or("");
                    let issue_pdf = issue.get("pdf_url").and_then(|x| x.as_str()).unwrap_or("");
                    let Some(items) = issue.get("items").and_then(|x| x.as_array()) else {
                        continue;
                    };
                    for item in items {
                        let title = item.get("title").and_then(|x| x.as_str()).unwrap_or("");
                        if title.is_empty() {
                            continue;
                        }
                        let page = item.get("page").and_then(|x| x.as_i64()).unwrap_or(0);
                        let pdf_url = item
                            .get("pdf_url")
                            .and_then(|x| x.as_str())
                            .filter(|s| !s.is_empty())
                            .unwrap_or(issue_pdf);
                        let agency = item.get("agency_hint").and_then(|x| x.as_str()).unwrap_or("");
                        let amend = item.get("amend_text").and_then(|x| x.as_str()).unwrap_or("");
                        let title_tokens = tokenize_for_fts(title);
                        let content_tokens = tokenize_for_fts(amend);
                        kanpo_stmt.execute(params![
                            date,
                            issue_no,
                            title,
                            page,
                            pdf_url,
                            agency,
                            title_tokens,
                            content_tokens,
                        ])?;
                        total_kanpo += 1;
                    }
                }
            }
            tracing::info!("search.db: indexed {} kanpo items", total_kanpo);
        }

        tx.execute(
            "INSERT INTO meta (key, value) VALUES ('built_at', datetime('now')), \
             ('law_count', ?1), ('article_count', ?2), ('ref_count', ?3), \
             ('speech_count', ?4), ('tokenizer', 'bigram')",
            params![
                laws.len() as i64,
                total_articles as i64,
                total_refs as i64,
                total_speeches as i64,
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

    #[test]
    fn cross_law_refs_handle_utf8_boundary_at_lookahead() {
        // 旧実装は body[after..(after + 36).min(body.len())] でバイトカットしていて、
        // 「第百条」直後にマルチバイト文字が並ぶと panic していた。
        let mut titles = std::collections::HashMap::new();
        titles.insert("民法".to_string(), "129AC0000000089".to_string());

        let mut art_map_minpo = std::collections::HashMap::new();
        art_map_minpo.insert("第三条".to_string(), "art_3".to_string());
        let mut articles_idx = std::collections::HashMap::new();
        articles_idx.insert("129AC0000000089".to_string(), art_map_minpo);

        // 民法 のすぐ後に長めの日本語が続き、後続走査の終端がマルチバイト文字の中に当たる
        // ようなケース。
        let body = "施行日前にされた国等の事務に係る処分であって、当該処分をした行政庁（以下この条において「処分庁」という。）に施行日前に行政不服審査法に規定する民法上級行政庁";
        let cross = CrossLawIndex::build(&titles).unwrap();
        // panic しないこと自体がテスト。返り値は最低 1 件 (民法だけマッチ → article 解決失敗で None) を期待。
        let hits = extract_cross_law_refs(body, "OTHER", &cross, &articles_idx);
        assert!(hits.iter().any(|(_, lid, _)| lid == "129AC0000000089"));
    }

    #[test]
    fn kanpo_fts_indexes_and_matches() {
        // 一時ディレクトリに public/kanpo/{date}/index.json を作り、build_search_db で
        // 索引 → kanpo_fts を MATCH 検索して改め文がヒットすることを確認する。
        let root = std::env::temp_dir().join("lawpub_kanpo_fts_test");
        let _ = std::fs::remove_dir_all(&root);
        let kanpo_dir = root.join("kanpo").join("2026-04-02");
        std::fs::create_dir_all(&kanpo_dir).unwrap();
        let index = serde_json::json!({
            "date": "2026-04-02",
            "issues": [{
                "issue_no": "第1678号",
                "pdf_url": "https://www.kanpo.go.jp/20260402/20260402h01678/",
                "items": [{
                    "title": "郵便法施行規則の一部を改正する省令（総務五八）",
                    "page": 2,
                    "pdf_url": "https://www.kanpo.go.jp/20260402/x.pdf",
                    "agency_hint": "総務五八",
                    "amend_text": "郵便法施行規則の一部を改正する省令\n郵便法施行規則（平成十五年総務省令第五号）の一部を次のように改正する。"
                }]
            }]
        });
        std::fs::write(
            kanpo_dir.join("index.json"),
            serde_json::to_string(&index).unwrap(),
        )
        .unwrap();

        let db_path = root.join("search.db");
        let cats = std::collections::HashMap::new();
        build_search_db(&db_path, &[], &cats, None, Some(&root.join("kanpo"))).unwrap();

        let conn = Connection::open(&db_path).unwrap();
        // 「施行規則」を bigram トークン化して MATCH。
        let match_expr = tokenize_for_fts("施行規則");
        let title: String = conn
            .query_row(
                "SELECT title FROM kanpo_fts WHERE kanpo_fts MATCH ?1 LIMIT 1",
                params![match_expr],
                |r| r.get(0),
            )
            .unwrap();
        assert!(title.contains("郵便法施行規則"), "got: {title}");

        // 本文(改め文)側のトークンでもヒットする。
        let cnt: i64 = conn
            .query_row(
                "SELECT count(*) FROM kanpo_fts WHERE kanpo_fts MATCH ?1",
                params![tokenize_for_fts("総務省令")],
                |r| r.get(0),
            )
            .unwrap();
        assert!(cnt >= 1, "amend_text 検索でヒットせず");

        let _ = std::fs::remove_dir_all(&root);
    }
}

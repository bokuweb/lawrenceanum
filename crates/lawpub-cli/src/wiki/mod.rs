//! LLM wiki (OKF: Open Knowledge Format) の決定的な部分。
//!
//! 配信済みの正規化コーパス (Pages の静的 JSON) を正本とし、wiki はその下流の派生物とする。
//! LLM はこの CLI を呼ばない。役割分担は次のとおり:
//!
//! - `wiki-plan`: 未処理の会議から法令リンクのあるものを上限付きで選び、LLM 用の
//!   ソースバンドル (言及箇所の抜粋) とページの雛形を作る。
//! - (LLM): `<!-- llm:begin -->` ～ `<!-- llm:end -->` 区間と frontmatter の
//!   `description` / `tags` だけを書く。topics/ は丸ごと書いてよい。
//! - `wiki-finalize`: 時系列表・人物ページ・index.md・log.md を frontmatter から再生成し、
//!   未完了タスクの雛形を巻き戻して state を進める。
//! - `wiki-check`: OKF frontmatter、相対リンク、引用 (発言 ID の実在と原文一致) を検証する。
//!
//! frontmatter は `key: <JSON リテラル>` の 1 行 1 キーに限定する。JSON の文字列・配列・
//! オブジェクトは YAML の flow 表記としても正しいので、OKF (YAML frontmatter) と互換の
//! まま YAML パーサ無しで機械的に読み書きできる。

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

pub mod check;
pub mod finalize;
pub mod plan;

pub const STATE_PATH: &str = ".lawpub/state.json";
pub const LLM_BEGIN: &str = "<!-- llm:begin -->";
pub const LLM_END: &str = "<!-- llm:end -->";

/// JST の今日。
pub fn today_jst() -> chrono::NaiveDate {
    let jst = chrono::FixedOffset::east_opt(9 * 3600).expect("valid offset");
    chrono::Utc::now().with_timezone(&jst).date_naive()
}

pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

// ── 正規化コーパスの読み出し ─────────────────────────────────────

/// 配信済み JSON の取得元。`http(s)://` なら Pages、それ以外はローカルの `public/`。
/// どちらも `{path}.gz` (配信時の事前圧縮) を優先し、無ければ素の `{path}` を読む。
pub struct Source {
    base: String,
    client: Option<reqwest::blocking::Client>,
}

impl Source {
    pub fn new(base: &str) -> Result<Self> {
        let base = base.trim_end_matches('/').to_string();
        let client = if base.starts_with("http://") || base.starts_with("https://") {
            Some(
                reqwest::blocking::Client::builder()
                    .user_agent("lawpub-wiki/0.1 (+https://github.com/bokuweb/lawrenceanum)")
                    .timeout(std::time::Duration::from_secs(60))
                    .build()?,
            )
        } else {
            None
        };
        Ok(Self { base, client })
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    /// 見つからなければ `Ok(None)`。通信・展開・JSON の失敗は `Err` (未処理のまま残すため区別する)。
    pub fn get_json(&self, rel: &str) -> Result<Option<Value>> {
        for (candidate, gz) in [(format!("{rel}.gz"), true), (rel.to_string(), false)] {
            let Some(bytes) = self.get_bytes(&candidate)? else {
                continue;
            };
            let bytes = if gz {
                let mut out = Vec::new();
                flate2::read::GzDecoder::new(bytes.as_slice())
                    .read_to_end(&mut out)
                    .with_context(|| format!("gunzip {candidate}"))?;
                out
            } else {
                bytes
            };
            let value =
                serde_json::from_slice(&bytes).with_context(|| format!("parse {candidate}"))?;
            return Ok(Some(value));
        }
        Ok(None)
    }

    fn get_bytes(&self, rel: &str) -> Result<Option<Vec<u8>>> {
        match &self.client {
            Some(client) => {
                let url = format!("{}/{rel}", self.base);
                let resp = client
                    .get(&url)
                    .send()
                    .with_context(|| format!("GET {url}"))?;
                if resp.status() == reqwest::StatusCode::NOT_FOUND {
                    return Ok(None);
                }
                if !resp.status().is_success() {
                    bail!("GET {url}: HTTP {}", resp.status());
                }
                Ok(Some(resp.bytes()?.to_vec()))
            }
            None => {
                let path = Path::new(&self.base).join(rel);
                if !path.exists() {
                    return Ok(None);
                }
                Ok(Some(
                    std::fs::read(&path).with_context(|| format!("read {}", path.display()))?,
                ))
            }
        }
    }
}

// ── 会議 → 引用単位 (発言 / 発言ターン) ───────────────────────────

pub const KIND_KOKKAI: &str = "kokkai";
pub const KIND_SHINGIKAI: &str = "shingikai";

/// 引用できる最小単位。国会は 1 発言、審議会は議事録の `○` で始まる 1 ターン。
#[derive(Debug, Clone)]
pub struct Unit {
    /// `kokkai:{speech_id}` / `shingikai:{minutes_id}#{turn}`。
    pub reference: String,
    pub url: String,
    pub speaker: Option<String>,
    pub group: Option<String>,
    pub position: Option<String>,
    pub text: String,
}

/// 国会の会議録情報 (出席者・付議案件の一覧) は発言者ではない。
pub const KOKKAI_HEADER_SPEAKER: &str = "会議録情報";

pub fn kokkai_units(doc: &Value) -> Vec<Unit> {
    let meeting_id = doc["meeting_id"].as_str().unwrap_or("");
    doc["speeches"]
        .as_array()
        .map(|speeches| {
            speeches
                .iter()
                .filter_map(|s| {
                    let speech_id = s["speech_id"].as_str()?;
                    let order = s["order"].as_u64().unwrap_or(0);
                    Some(Unit {
                        reference: format!("{KIND_KOKKAI}:{speech_id}"),
                        url: format!("https://kokkai.ndl.go.jp/txt/{meeting_id}/{order}"),
                        speaker: s["speaker"].as_str().map(String::from),
                        group: s["speaker_group"].as_str().map(String::from),
                        position: s["speaker_position"].as_str().map(String::from),
                        text: s["speech"].as_str().unwrap_or("").to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn shingikai_units(doc: &Value) -> Vec<Unit> {
    let minutes_id = doc["minutes_id"].as_str().unwrap_or("");
    let url = doc["source"]["detail_url"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let text = doc["minutes_text"]
        .as_str()
        .filter(|t| !t.trim().is_empty())
        .or_else(|| doc["body_text"].as_str())
        .unwrap_or("");

    let mut turns: Vec<(Option<String>, String)> = vec![(None, String::new())];
    for line in text.lines() {
        let trimmed = line.trim_start_matches([' ', '\u{3000}', '\t']);
        if let Some(rest) = trimmed.strip_prefix('○') {
            let speaker: String = rest.chars().take_while(|c| !c.is_whitespace()).collect();
            turns.push(((!speaker.is_empty()).then_some(speaker), String::new()));
        }
        let current = &mut turns.last_mut().expect("non-empty").1;
        current.push_str(line);
        current.push('\n');
    }
    turns
        .into_iter()
        .enumerate()
        .filter(|(_, (_, t))| !t.trim().is_empty())
        .map(|(i, (speaker, text))| Unit {
            reference: format!("{KIND_SHINGIKAI}:{minutes_id}#{i}"),
            url: url.clone(),
            speaker,
            group: None,
            position: None,
            text,
        })
        .collect()
}

/// 引用照合用の正規化: 空白 (全角空白・改行を含む) をすべて除く。
pub fn normalize_for_quote(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

// ── wiki の状態 ─────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct State {
    pub schema_version: u32,
    /// `kokkai:{meeting_id}` / `shingikai:{minutes_id}` → 処理結果。
    pub processed: BTreeMap<String, Processed>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Processed {
    /// `linked` (wiki に反映) / `no_link` (法令リンク無し) / `no_excerpt` (引用できる言及無し)。
    pub status: String,
    pub date: String,
}

impl State {
    pub fn load(wiki: &Path) -> Result<Self> {
        let path = wiki.join(STATE_PATH);
        if !path.exists() {
            return Ok(Self {
                schema_version: 1,
                ..Default::default()
            });
        }
        let bytes = std::fs::read(&path).with_context(|| format!("read {}", path.display()))?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub fn save(&self, wiki: &Path) -> Result<()> {
        let path = wiki.join(STATE_PATH);
        std::fs::create_dir_all(path.parent().expect("has parent"))?;
        std::fs::write(&path, serde_json::to_string_pretty(self)? + "\n")?;
        Ok(())
    }

    pub fn is_processed(&self, key: &str) -> bool {
        self.processed.contains_key(key)
    }

    pub fn mark(&mut self, key: &str, status: &str, date: &str) {
        self.processed.insert(
            key.to_string(),
            Processed {
                status: status.to_string(),
                date: date.to_string(),
            },
        );
    }
}

// ── plan.json (plan → LLM → finalize の受け渡し) ──────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Plan {
    pub schema_version: u32,
    pub date: String,
    pub base_url: String,
    pub tasks: Vec<Task>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// state のキー (`kokkai:{id}` など)。
    pub key: String,
    pub kind: String,
    pub id: String,
    pub date: String,
    pub title: String,
    /// wiki ルートからの相対パス。
    pub page: String,
    /// 作業ディレクトリ基準のソースバンドル。
    pub source: String,
    pub laws: Vec<TaskLaw>,
    /// この plan で新規作成したページ (未完了なら finalize が削除する)。
    pub created: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskLaw {
    pub law_id: String,
    pub title: String,
    pub page: String,
}

impl Plan {
    pub fn load(work: &Path) -> Result<Option<Self>> {
        let path = work.join("plan.json");
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_slice(&std::fs::read(&path)?)?))
    }
}

pub fn meeting_page(kind: &str, id: &str) -> String {
    format!("meetings/{kind}/{}.md", file_safe(id))
}

pub fn law_page(law_id: &str) -> String {
    format!("laws/{}.md", file_safe(law_id))
}

pub fn person_page(name: &str) -> String {
    format!("people/{}.md", file_safe(name))
}

/// ファイル名に使えない文字と空白を除く。
pub fn file_safe(s: &str) -> String {
    s.chars()
        .filter(|c| {
            !c.is_whitespace()
                && !matches!(
                    c,
                    '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '#'
                )
        })
        .collect()
}

// ── ページ (frontmatter + 本文) ─────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct Page {
    pub frontmatter: Vec<(String, Value)>,
    pub body: String,
}

impl Page {
    pub fn parse(text: &str) -> Result<Self> {
        let text = text.replace("\r\n", "\n");
        let Some(rest) = text.strip_prefix("---\n") else {
            bail!("frontmatter がありません (先頭は `---` 行)");
        };
        let Some(end) = rest
            .find("\n---\n")
            .or_else(|| rest.strip_suffix("\n---").map(|r| r.len()))
        else {
            bail!("frontmatter の終端 `---` がありません");
        };
        let mut frontmatter = Vec::new();
        for (i, line) in rest[..end].lines().enumerate() {
            if line.trim().is_empty() || line.trim_start().starts_with('#') {
                continue;
            }
            let Some((key, raw)) = line.split_once(':') else {
                bail!(
                    "frontmatter {} 行目: `key: value` 形式ではありません",
                    i + 2
                );
            };
            let value: Value = serde_json::from_str(raw.trim()).with_context(|| {
                format!(
                    "frontmatter {} 行目 `{}`: 値は JSON リテラル (\"文字列\" / [配列]) で書きます",
                    i + 2,
                    key.trim()
                )
            })?;
            frontmatter.push((key.trim().to_string(), value));
        }
        let body = rest.get(end + 5..).unwrap_or("").to_string();
        Ok(Self { frontmatter, body })
    }

    pub fn read(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        Self::parse(&text).with_context(|| path.display().to_string())
    }

    pub fn render(&self) -> String {
        let mut out = String::from("---\n");
        for (k, v) in &self.frontmatter {
            out.push_str(k);
            out.push_str(": ");
            out.push_str(&serde_json::to_string(v).expect("serializable"));
            out.push('\n');
        }
        out.push_str("---\n");
        out.push_str(&self.body);
        out
    }

    pub fn write(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.render()).with_context(|| format!("write {}", path.display()))
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.frontmatter
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    pub fn get_str(&self, key: &str) -> &str {
        self.get(key).and_then(Value::as_str).unwrap_or("")
    }

    pub fn set(&mut self, key: &str, value: Value) {
        match self.frontmatter.iter_mut().find(|(k, _)| k == key) {
            Some((_, v)) => *v = value,
            None => self.frontmatter.push((key.to_string(), value)),
        }
    }
}

/// `<!-- lawpub:begin {name} -->` ～ `<!-- lawpub:end {name} -->` の中身を差し替える。
/// マーカーが無ければ `None`。
pub fn replace_block(body: &str, name: &str, content: &str) -> Option<String> {
    let begin = format!("<!-- lawpub:begin {name} -->");
    let end = format!("<!-- lawpub:end {name} -->");
    let b = body.find(&begin)? + begin.len();
    let e = b + body[b..].find(&end)?;
    let mut inner = String::from("\n");
    if !content.is_empty() {
        inner.push_str(content.trim_end());
        inner.push('\n');
    }
    Some(format!("{}{}{}", &body[..b], inner, &body[e..]))
}

/// LLM 区間の中身を順に返す。
pub fn llm_blocks(body: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = body;
    while let Some(b) = rest.find(LLM_BEGIN) {
        let after = &rest[b + LLM_BEGIN.len()..];
        let Some(e) = after.find(LLM_END) else { break };
        out.push(&after[..e]);
        rest = &after[e + LLM_END.len()..];
    }
    out
}

/// wiki ルート相対の `from` ページから `to` ページへの相対リンク。
pub fn rel_link(from: &str, to: &str) -> String {
    let depth = from.matches('/').count();
    format!("{}{}", "../".repeat(depth), to)
}

/// Markdown 表のセル用エスケープ。
pub fn cell(s: &str) -> String {
    s.replace('|', "\\|").replace(['\n', '\r'], " ")
}

pub fn walk_md(root: &Path, sub: &str) -> Vec<PathBuf> {
    let dir = root.join(sub);
    if !dir.exists() {
        return Vec::new();
    }
    let mut files: Vec<PathBuf> = walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("md"))
        .collect();
    files.sort();
    files
}

/// wiki ルートからの相対パス (区切りは `/`)。
pub fn rel_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
pub(crate) fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "lawpub_wiki_{tag}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn frontmatter_roundtrip_is_okf_compatible() {
        let text = "---\ntype: \"law\"\ntitle: \"民法: 総則\"\ntags: [\"契約\"]\n---\n# 民法\n";
        let mut page = Page::parse(text).unwrap();
        assert_eq!(page.get_str("type"), "law");
        assert_eq!(page.get_str("title"), "民法: 総則");
        page.set("description", json!("説明"));
        let out = page.render();
        assert!(out.ends_with("description: \"説明\"\n---\n# 民法\n"));
        assert_eq!(Page::parse(&out).unwrap().get_str("description"), "説明");
    }

    #[test]
    fn frontmatter_rejects_bare_yaml_scalars() {
        let err = Page::parse("---\ntype: law\n---\n").unwrap_err();
        assert!(format!("{err:#}").contains("JSON リテラル"));
    }

    #[test]
    fn replace_block_keeps_surroundings() {
        let body = "a\n<!-- lawpub:begin t -->\nold\n<!-- lawpub:end t -->\nb\n";
        let out = replace_block(body, "t", "new").unwrap();
        assert_eq!(
            out,
            "a\n<!-- lawpub:begin t -->\nnew\n<!-- lawpub:end t -->\nb\n"
        );
        assert!(replace_block(body, "missing", "x").is_none());
    }

    #[test]
    fn llm_blocks_are_extracted_in_order() {
        let body = "x<!-- llm:begin -->one<!-- llm:end -->y<!-- llm:begin -->two<!-- llm:end -->";
        assert_eq!(llm_blocks(body), vec!["one", "two"]);
    }

    #[test]
    fn rel_link_walks_up_from_nested_pages() {
        assert_eq!(
            rel_link("laws/a.md", "meetings/kokkai/b.md"),
            "../meetings/kokkai/b.md"
        );
        assert_eq!(
            rel_link("meetings/kokkai/b.md", "laws/a.md"),
            "../../laws/a.md"
        );
        assert_eq!(rel_link("index.md", "laws/a.md"), "laws/a.md");
    }

    #[test]
    fn shingikai_minutes_split_on_speaker_marks() {
        let doc = json!({
            "minutes_id": "m1",
            "source": {"detail_url": "https://example.go.jp/m1"},
            "minutes_text": "議事録\n○神作部会長　開会します。\n続き\n○森委員　会社法について意見です。\n",
            "body_text": ""
        });
        let units = shingikai_units(&doc);
        assert_eq!(units.len(), 3);
        assert_eq!(units[1].reference, "shingikai:m1#1");
        assert_eq!(units[1].speaker.as_deref(), Some("神作部会長"));
        assert!(units[1].text.contains("続き"));
        assert_eq!(units[2].speaker.as_deref(), Some("森委員"));
    }

    #[test]
    fn kokkai_units_link_to_ndl_speech_urls() {
        let doc = json!({
            "meeting_id": "M1",
            "speeches": [{"speech_id": "M1_001", "order": 1, "speaker": "山田太郎",
                          "speaker_group": "無所属", "speaker_position": null, "speech": "本文"}]
        });
        let units = kokkai_units(&doc);
        assert_eq!(units[0].reference, "kokkai:M1_001");
        assert_eq!(units[0].url, "https://kokkai.ndl.go.jp/txt/M1/1");
    }
}

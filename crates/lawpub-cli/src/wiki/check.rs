//! `lawpub wiki-check`: wiki を決定的に検証する。LLM の誤りを push 前に止める最後の関門。
//!
//! - OKF: すべてのページに frontmatter と `type` がある (値は JSON リテラル)。
//! - 置き場所: ルートの index.md / log.md / README.md と laws/ meetings/ people/ topics/ 以外は不可。
//! - 相対リンクの参照先が存在する。
//! - 脚注定義はすべて引用形式 `[^n]: [kokkai:{speech_id}](url) 「原文」` であり、
//!   発言 ID が実在し、URL が発言の URL と一致し、引用が発言本文の部分文字列 (空白を無視) である。
//! - 会議・法令ページの LLM 区間、topics/ のページには少なくとも 1 つの引用がある。
//!
//! 引用の照合は `--changed` なら git で変更のあったページだけに行う (会議本体の取得を減らす)。

use super::*;
use std::collections::{HashMap, HashSet};

pub struct CheckArgs {
    pub wiki: PathBuf,
    pub work: PathBuf,
    pub base_url: String,
    pub changed_only: bool,
}

const ROOT_FILES: [&str; 3] = ["index.md", "log.md", "README.md"];
const DIRS: [&str; 4] = ["laws/", "meetings/", "people/", "topics/"];
const MIN_QUOTE_CHARS: usize = 8;
const MAX_QUOTE_CHARS: usize = 200;

#[derive(Debug, Clone, PartialEq)]
pub struct Citation {
    pub label: String,
    /// `kokkai:{speech_id}` / `shingikai:{minutes_id}#{turn}`。
    pub reference: String,
    pub url: String,
    pub quote: String,
    /// テキスト内の 1 始まりの行番号。
    pub line: usize,
}

/// 脚注定義行 `[^label]: ...` を (label, 残り, 行番号) で返す。
fn footnote_defs(text: &str) -> Vec<(String, String, usize)> {
    text.lines()
        .enumerate()
        .filter_map(|(i, line)| {
            let rest = line.trim_start().strip_prefix("[^")?;
            let (label, rest) = rest.split_once("]:")?;
            Some((label.to_string(), rest.trim().to_string(), i + 1))
        })
        .collect()
}

fn parse_citation(rest: &str) -> Option<(String, String, String)> {
    let rest = rest.strip_prefix('[')?;
    let (reference, rest) = rest.split_once("](")?;
    let (url, rest) = rest.split_once(')')?;
    let rest = rest.trim();
    let quote = rest.strip_prefix('「')?.strip_suffix('」')?;
    if !(reference.starts_with("kokkai:") || reference.starts_with("shingikai:")) {
        return None;
    }
    Some((reference.to_string(), url.to_string(), quote.to_string()))
}

/// テキスト中の正しい形式の引用。
pub fn citations_in(text: &str) -> Vec<Citation> {
    footnote_defs(text)
        .into_iter()
        .filter_map(|(label, rest, line)| {
            let (reference, url, quote) = parse_citation(&rest)?;
            Some(Citation {
                label,
                reference,
                url,
                quote,
                line,
            })
        })
        .collect()
}

/// 本文中の脚注参照 `[^label]` (定義行を除く)。
fn footnote_refs(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        if line.trim_start().starts_with("[^") && line.contains("]:") {
            continue;
        }
        let mut rest = line;
        while let Some(i) = rest.find("[^") {
            let after = &rest[i + 2..];
            let Some(j) = after.find(']') else { break };
            out.push(after[..j].to_string());
            rest = &after[j + 1..];
        }
    }
    out
}

/// 相対リンク `](target)` の参照先 (外部 URL とアンカーのみは除く)。
fn relative_links(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("](") {
        let after = &rest[i + 2..];
        let Some(j) = after.find(')') else { break };
        let target = after[..j].trim();
        rest = &after[j + 1..];
        let target = target.split_whitespace().next().unwrap_or("");
        if target.is_empty()
            || target.starts_with('#')
            || target.contains("://")
            || target.starts_with("mailto:")
        {
            continue;
        }
        let target = target.split('#').next().unwrap_or("");
        out.push(percent_decode(target));
    }
    out
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(b) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// 引用元の発言を、作業ディレクトリに残した会議本体 → 配信 JSON の順に引く。
struct Resolver<'a> {
    work: &'a Path,
    source: Source,
    docs: HashMap<String, Option<HashMap<String, Unit>>>,
    ministries: Option<HashMap<String, String>>,
}

impl Resolver<'_> {
    fn unit(&mut self, reference: &str) -> Result<Option<Unit>> {
        let (kind, id) = reference.split_once(':').unwrap_or(("", ""));
        let doc_id = match kind {
            KIND_KOKKAI => id
                .rsplit_once('_')
                .map(|(m, _)| m)
                .unwrap_or(id)
                .to_string(),
            KIND_SHINGIKAI => id.split('#').next().unwrap_or(id).to_string(),
            _ => return Ok(None),
        };
        let key = format!("{kind}:{doc_id}");
        if !self.docs.contains_key(&key) {
            let doc = self.load_doc(kind, &doc_id)?;
            let units = doc.map(|d| {
                let units = if kind == KIND_KOKKAI {
                    kokkai_units(&d)
                } else {
                    shingikai_units(&d)
                };
                units
                    .into_iter()
                    .map(|u| (u.reference.clone(), u))
                    .collect()
            });
            self.docs.insert(key.clone(), units);
        }
        Ok(self.docs[&key]
            .as_ref()
            .and_then(|u| u.get(reference).cloned()))
    }

    fn load_doc(&mut self, kind: &str, id: &str) -> Result<Option<Value>> {
        let local = self.work.join("docs").join(kind).join(format!("{id}.json"));
        if local.exists() {
            return Ok(Some(serde_json::from_slice(&std::fs::read(&local)?)?));
        }
        if kind == KIND_KOKKAI {
            return self.source.get_json(&format!("proceedings/{id}.json"));
        }
        if self.ministries.is_none() {
            let index = self
                .source
                .get_json("shingikai/index.json")?
                .unwrap_or(Value::Null);
            self.ministries = Some(
                index["minutes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|m| {
                        Some((
                            m["minutes_id"].as_str()?.to_string(),
                            m["ministry"].as_str()?.to_string(),
                        ))
                    })
                    .collect(),
            );
        }
        match self.ministries.as_ref().and_then(|m| m.get(id)) {
            Some(ministry) => self
                .source
                .get_json(&format!("shingikai/{ministry}/{id}.json")),
            None => Ok(None),
        }
    }
}

fn changed_pages(wiki: &Path) -> Option<HashSet<String>> {
    // quotePath=false: 日本語のファイル名 (people/ topics/) を八進エスケープさせない。
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(wiki)
        .args([
            "-c",
            "core.quotePath=false",
            "status",
            "--porcelain",
            "--untracked-files=all",
            "--no-renames",
            ".",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    // git は wiki がリポジトリ直下でなくてもリポジトリルート相対で返すため、prefix を落とす。
    let prefix = std::process::Command::new("git")
        .arg("-C")
        .arg(wiki)
        .args(["rev-parse", "--show-prefix"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    Some(
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| l.get(3..))
            .map(|p| p.trim_matches('"').to_string())
            .map(|p| p.strip_prefix(&prefix).map(String::from).unwrap_or(p))
            .collect(),
    )
}

pub fn run_check(args: &CheckArgs) -> Result<()> {
    let wiki = &args.wiki;
    let changed = if args.changed_only {
        changed_pages(wiki)
    } else {
        None
    };
    let mut resolver = Resolver {
        work: &args.work,
        source: Source::new(&args.base_url)?,
        docs: HashMap::new(),
        ministries: None,
    };
    let mut errors: Vec<String> = Vec::new();
    let mut checked_citations = 0usize;

    let pages: Vec<PathBuf> = walk_md(wiki, "")
        .into_iter()
        .filter(|p| !rel_path(wiki, p).starts_with(".lawpub/"))
        .collect();
    for path in &pages {
        let rel = rel_path(wiki, path);
        let mut err = |line: usize, msg: String| errors.push(format!("{rel}:{line}: {msg}"));

        if !(ROOT_FILES.contains(&rel.as_str()) || DIRS.iter().any(|d| rel.starts_with(d))) {
            err(
                1,
                format!(
                    "置き場所が不正です（{} またはルートの {} のみ）",
                    DIRS.join(" "),
                    ROOT_FILES.join(" ")
                ),
            );
        }
        let text = std::fs::read_to_string(path)?;
        let page = match Page::parse(&text) {
            Ok(p) => p,
            Err(e) => {
                err(1, format!("{e:#}"));
                continue;
            }
        };
        let ty = page.get_str("type");
        if ty.is_empty() {
            err(1, "OKF の必須フィールド `type` がありません".into());
        }
        if matches!(ty, "law" | "meeting" | "person" | "topic") && page.get_str("title").is_empty()
        {
            err(1, "`title` がありません".into());
        }
        if text.matches(LLM_BEGIN).count() != text.matches(LLM_END).count() {
            err(
                1,
                "LLM 区間のマーカー (llm:begin / llm:end) が対応していません".into(),
            );
        }

        for target in relative_links(&text) {
            let resolved = path.parent().unwrap_or(wiki).join(&target);
            if !resolved.exists() {
                err(1, format!("リンク先がありません: {target}"));
            }
        }

        // 脚注: 定義はすべて引用形式、参照と定義が対応、ラベルの重複なし。
        let defs = footnote_defs(&text);
        let citations = citations_in(&text);
        let mut labels = HashSet::new();
        for (label, rest, line) in &defs {
            if !labels.insert(label.clone()) {
                err(*line, format!("脚注 [^{label}] が重複しています"));
            }
            if parse_citation(rest).is_none() {
                err(*line, format!("脚注 [^{label}] が引用形式ではありません: `[^n]: [kokkai:発言ID](URL) 「原文の連続した一節」`"));
            }
        }
        for label in footnote_refs(&text) {
            if !labels.contains(&label) {
                err(1, format!("脚注 [^{label}] の定義がありません"));
            }
        }

        let llm_text: String = llm_blocks(&page.body).concat();
        let needs_citation = match ty {
            "meeting" | "law" => !llm_text.trim().is_empty(),
            "topic" => true,
            _ => false,
        };
        let cited = if ty == "topic" {
            !citations.is_empty()
        } else {
            !citations_in(&llm_text).is_empty()
        };
        if needs_citation && !cited {
            err(
                1,
                "要約には少なくとも 1 つの引用 (発言 ID + 原文) が必要です".into(),
            );
        }

        if changed.as_ref().is_some_and(|c| !c.contains(&rel)) {
            continue;
        }
        for c in &citations {
            checked_citations += 1;
            let quote = normalize_for_quote(&c.quote);
            let n = quote.chars().count();
            if !(MIN_QUOTE_CHARS..=MAX_QUOTE_CHARS).contains(&n) {
                err(c.line, format!("引用は {MIN_QUOTE_CHARS}〜{MAX_QUOTE_CHARS} 文字にしてください（{n} 文字）"));
                continue;
            }
            let unit = match resolver.unit(&c.reference) {
                Ok(Some(u)) => u,
                Ok(None) => {
                    err(c.line, format!("発言 {} が見つかりません", c.reference));
                    continue;
                }
                Err(e) => {
                    err(
                        c.line,
                        format!("発言 {} を取得できません: {e:#}", c.reference),
                    );
                    continue;
                }
            };
            if c.url != unit.url {
                err(
                    c.line,
                    format!(
                        "{} の URL は {} です（記載: {}）",
                        c.reference, unit.url, c.url
                    ),
                );
            }
            if !normalize_for_quote(&unit.text).contains(&quote) {
                err(c.line, format!("引用「{}」は {} の本文にありません（要約ではなく原文をそのまま引用してください）", c.quote, c.reference));
            }
        }
    }

    let report = if errors.is_empty() {
        format!(
            "# wiki-check: OK\n\n{} pages, {checked_citations} citations verified.\n",
            pages.len()
        )
    } else {
        let mut r = format!(
            "# wiki-check: {} error(s)\n\n{} pages, {checked_citations} citations checked.\n\n",
            errors.len(),
            pages.len()
        );
        for e in &errors {
            r.push_str(&format!("- {e}\n"));
        }
        r
    };
    if args.work.exists() {
        std::fs::write(args.work.join("check-report.md"), &report)?;
    }
    print!("{report}");
    if !errors.is_empty() {
        bail!("wiki-check: {} error(s)", errors.len());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn citation_format_is_parsed_strictly() {
        let text = "本文[^1]\n\n[^1]: [kokkai:M1_001](https://kokkai.ndl.go.jp/txt/M1/1) 「救済を拡充すべきです」\n[^2]: 出典なし\n";
        let c = citations_in(text);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].reference, "kokkai:M1_001");
        assert_eq!(c[0].quote, "救済を拡充すべきです");
        assert_eq!(c[0].line, 3);
        assert_eq!(footnote_refs(text), vec!["1"]);
    }

    #[test]
    fn relative_links_skip_external_and_decode() {
        let text = "[a](../laws/L1.md) [b](https://x.jp/y) [c](#top) [d](people/%E5%B1%B1.md#x)";
        assert_eq!(relative_links(text), vec!["../laws/L1.md", "people/山.md"]);
    }

    fn fixture(summary: &str) -> (PathBuf, CheckArgs) {
        let root = temp_dir("check");
        let wiki = root.join("wiki");
        let work = root.join("work");
        std::fs::create_dir_all(wiki.join("meetings/kokkai")).unwrap();
        std::fs::create_dir_all(work.join("docs/kokkai")).unwrap();
        std::fs::write(
            work.join("docs/kokkai/M1.json"),
            serde_json::to_vec(&json!({
                "meeting_id": "M1",
                "speeches": [{"speech_id": "M1_001", "order": 1, "speaker": "山田太郎",
                              "speech": "予防接種法の改正で、副反応の\r\n　救済を拡充すべきです。"}]
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            wiki.join("meetings/kokkai/M1.md"),
            format!("---\ntype: \"meeting\"\ntitle: \"会議\"\n---\n# 会議\n\n{LLM_BEGIN}\n{summary}\n{LLM_END}\n"),
        )
        .unwrap();
        let args = CheckArgs {
            wiki,
            work: work.clone(),
            base_url: root.join("public").display().to_string(),
            changed_only: false,
        };
        (root, args)
    }

    #[test]
    fn verbatim_quote_across_line_breaks_passes() {
        let (root, args) = fixture("救済の拡充を求めた[^1]。\n\n[^1]: [kokkai:M1_001](https://kokkai.ndl.go.jp/txt/M1/1) 「副反応の救済を拡充すべきです」");
        run_check(&args).unwrap();
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn paraphrased_quote_unknown_speech_and_missing_citation_fail() {
        for summary in [
            "要求した[^1]。\n\n[^1]: [kokkai:M1_001](https://kokkai.ndl.go.jp/txt/M1/1) 「救済制度を抜本的に見直すべきだ」",
            "要求した[^1]。\n\n[^1]: [kokkai:M1_099](https://kokkai.ndl.go.jp/txt/M1/99) 「副反応の救済を拡充すべきです」",
            "出典の無い要約。",
            "要求した[^9]。",
        ] {
            let (root, args) = fixture(summary);
            assert!(run_check(&args).is_err(), "should fail: {summary}");
            std::fs::remove_dir_all(root).ok();
        }
    }
}

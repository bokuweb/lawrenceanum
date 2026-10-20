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
//! 官報 (`kanpo:{date}/{slug}`) の引用は、配信側ではなく wiki の項目ページに保存した本文と照合する。

use super::kanpo::{kanpo_unit, KIND_KANPO};
use super::pubcomment::{pubcomment_units, KIND_PUBCOMMENT};
use super::*;
use std::collections::{HashMap, HashSet};

pub struct CheckArgs {
    pub wiki: PathBuf,
    pub work: PathBuf,
    pub base_url: String,
    pub changed_only: bool,
}

const ROOT_FILES: [&str; 3] = ["index.md", "log.md", "README.md"];
const DIRS: [&str; 10] = ["laws/", "meetings/", "people/", "topics/", "bills/", "committees/", "parties/", "pubcomments/", "kanpo/", "enforcements/"];
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
    if !["kokkai:", "shingikai:", "gian:", "pubcomment:", "kanpo:", "enforcement:"].iter().any(|p| reference.starts_with(p)) {
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
pub(crate) fn relative_links(text: &str) -> Vec<String> {
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

/// 国会会議録の発言単位の URL (`https://kokkai.ndl.go.jp/txt/{会議}/{発言番号}`) へのリンクを (行番号, URL) で返す。
/// 会議単位の URL (`…/txt/{会議}`) は含まない。
fn speech_links(text: &str) -> Vec<(usize, String)> {
    const PREFIX: &str = "https://kokkai.ndl.go.jp/txt/";
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        // 脚注定義の URL は引用そのもの。
        if line.trim_start().starts_with("[^") && line.contains("]:") {
            continue;
        }
        let mut rest = line;
        while let Some(j) = rest.find("](") {
            let after = &rest[j + 2..];
            let Some(k) = after.find(')') else { break };
            let url = after[..k].trim();
            rest = &after[k + 1..];
            if url.strip_prefix(PREFIX).is_some_and(|p| p.trim_end_matches('/').contains('/')) {
                out.push((i + 1, url.to_string()));
            }
        }
    }
    out
}

/// 引用元の発言を、作業ディレクトリに残した会議本体 → 配信 JSON の順に引く。
struct Resolver<'a> {
    work: &'a Path,
    wiki: &'a Path,
    source: Source,
    docs: HashMap<String, Option<HashMap<String, Unit>>>,
    /// 審議会の議事録全文 (正規化済み)。発言の区切り方を変える前の引用番号も受け付けるため。
    full_texts: HashMap<String, String>,
    ministries: Option<HashMap<String, String>>,
}

impl Resolver<'_> {
    fn unit(&mut self, reference: &str) -> Result<Option<Unit>> {
        let (kind, id) = reference.split_once(':').unwrap_or(("", ""));
        if kind == KIND_KANPO {
            return kanpo_unit(self.wiki, reference);
        }
        let doc_id = match kind {
            KIND_KOKKAI => id
                .rsplit_once('_')
                .map(|(m, _)| m)
                .unwrap_or(id)
                .to_string(),
            KIND_SHINGIKAI | KIND_GIAN | KIND_PUBCOMMENT | enforcements::KIND => id.split('#').next().unwrap_or(id).to_string(),
            _ => return Ok(None),
        };
        let key = format!("{kind}:{doc_id}");
        if !self.docs.contains_key(&key) {
            let doc = self.load_doc(kind, &doc_id)?;
            let units = doc.map(|d| {
                let units = match kind {
                    KIND_KOKKAI => kokkai_units(&d),
                    KIND_PUBCOMMENT => pubcomment_units(&d),
                    enforcements::KIND => enforcements::units(&d),
                    KIND_GIAN => gian_units(&d["detail"], d["resolutions"].as_array().map(Vec::as_slice).unwrap_or(&[])),
                    _ => shingikai_units(&d),
                };
                if kind == KIND_SHINGIKAI {
                    let full: String = units.iter().map(|u| u.text.as_str()).collect();
                    self.full_texts.insert(key.clone(), normalize_for_quote(&full));
                }
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

    /// 審議会の引用文が、発言番号に関係なく同じ議事録の本文にあるか (`unit` を先に呼んで読み込み済みであること)。
    fn in_shingikai_minutes(&self, reference: &str, normalized_quote: &str) -> bool {
        let Some(id) = reference.strip_prefix("shingikai:") else {
            return false;
        };
        let key = format!("{KIND_SHINGIKAI}:{}", id.split('#').next().unwrap_or(id));
        self.full_texts
            .get(&key)
            .is_some_and(|t| t.contains(normalized_quote))
    }

    fn load_doc(&mut self, kind: &str, id: &str) -> Result<Option<Value>> {
        // 議案は `{session}/{bill_id}` を `{session}_{bill_id}.json` として保存している。
        let local = self.work.join("docs").join(kind).join(format!("{}.json", id.replace('/', "_")));
        if local.exists() {
            return Ok(Some(serde_json::from_slice(&std::fs::read(&local)?)?));
        }
        if kind == enforcements::KIND {
            return enforcements::load_bundle(&self.source, id);
        }
        if kind == KIND_KOKKAI {
            return self.source.get_json(&format!("proceedings/{id}.json"));
        }
        if kind == KIND_PUBCOMMENT {
            return self.source.get_json(&format!("pubcomment/{id}.json"));
        }
        if kind == KIND_GIAN {
            let Some((session, bill_id)) = id.split_once('/') else { return Ok(None) };
            let Ok(session) = session.parse::<u64>() else { return Ok(None) };
            return fetch_gian_doc(&self.source, session, bill_id);
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
    let plan = Plan::load(&args.work)?;
    let state = State::load(wiki)?;
    let changed = if args.changed_only {
        changed_pages(wiki)
    } else {
        None
    };
    let mut resolver = Resolver {
        work: &args.work,
        wiki,
        source: Source::new(&args.base_url)?,
        docs: HashMap::new(),
        full_texts: HashMap::new(),
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
        if matches!(ty, "law" | "meeting" | "person" | "topic" | "bill" | "committee" | "party" | "pubcomment" | "kanpo" | "enforcement") && page.get_str("title").is_empty()
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
            "meeting" | "law" | "person" | "committee" | "party" | "bill" | "pubcomment" | "kanpo" => !llm_text.trim().is_empty(),
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

        if page.get("evidence_version").is_some() && !llm_text.trim().is_empty() {
            for message in evidence::claim_errors(&page) { err(1, message); }
        }
        // LLM が編集しないソースバンドルを基準に、日付・抽出範囲の書き換えも検出する。
        if page.get("evidence_version").is_some() || plan.as_ref().is_some_and(|p| p.tasks.iter()
            .any(|t| t.page == rel && matches!(t.kind.as_str(), KIND_KOKKAI | KIND_SHINGIKAI))) {
            if let Some(task) = plan.as_ref().and_then(|p| p.tasks.iter().find(|t| t.page == rel)) {
                let bundle = std::fs::read(args.work.join(&task.source))
                    .with_context(|| format!("read source bundle {}", task.source))?;
                let bundle: Value = serde_json::from_slice(&bundle)?;
                for (field, original) in [("date", "date"), ("source_scope", "scope"), ("evidence_sources", "attributions")] {
                    if page.get(field) != bundle.get(original) {
                        err(1, format!("機械生成の {field} がソースバンドルと一致しません"));
                    }
                }
                if bundle.get("source_fingerprint").is_some()
                    && !evidence::source_fields_match(&page, &bundle, state.is_processed(&task.key)) {
                    err(1, "機械生成の原資料・要約の版がソースバンドルと一致しません".into());
                }
            }
        }

        if ty == enforcements::KIND {
            // 空の未完了ページは再生成待ち。書かれた説明は節ごとに検証する。
            if !llm_text.trim().is_empty() {
                for message in enforcements::explanation_errors(&page) { err(1, message); }
            }
        }

        // 要約から発言へのリンクは、検証済みの引用と同じ発言を指すこと (根拠の無いリンクを防ぐ)。
        let cited_urls: HashSet<&str> = citations.iter().map(|c| c.url.as_str()).collect();
        // 機械が載せる抜粋 (lawpub 区間) のリンクは対象外。LLM 区間だけを見る。
        for (_, url) in speech_links(&llm_text) {
            if !cited_urls.contains(url.as_str()) {
                err(1, format!("発言へのリンク {url} に対応する引用（脚注）がありません"));
            }
        }

        if changed.as_ref().is_some_and(|c| !c.contains(&rel)) {
            continue;
        }
        // 主張用の出典は発言単位で厳密に照合する。審議会の全文 fallback は誤帰属を起こすため使わない。
        if let Some(sources) = page.get("evidence_sources").and_then(Value::as_array) {
            for source in sources {
                let reference = source["ref"].as_str().unwrap_or("");
                match resolver.unit(reference) {
                    Ok(Some(u)) if *source == evidence::attribution(&u, page.get_str("date")) => {}
                    Ok(_) => err(1, format!("出典の発言者・役職・URL・役割が原資料と一致しません: {reference}")),
                    Err(e) => err(1, format!("出典情報を取得できません: {reference}: {e:#}")),
                }
            }
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
                // 審議会は発言の区切り方を改めたことがあるので、番号が合わなくても同じ議事録に引用文があればよい。
                Ok(None) if page.get("evidence_version").is_none() && resolver.in_shingikai_minutes(&c.reference, &quote) => continue,
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
            if !normalize_for_quote(&unit.text).contains(&quote) && !(page.get("evidence_version").is_none() && resolver.in_shingikai_minutes(&c.reference, &quote)) {
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
    fn matching_quote_cannot_forge_the_speakers_government_role() {
        let (root, args) = fixture("救済拡充を求めた[^1]。\n\n[^1]: [kokkai:M1_001](https://kokkai.ndl.go.jp/txt/M1/1) 「副反応の救済を拡充すべきです」");
        let path = args.wiki.join("meetings/kokkai/M1.md");
        let doc: Value = serde_json::from_slice(&std::fs::read(args.work.join("docs/kokkai/M1.json")).unwrap()).unwrap();
        let u = kokkai_units(&doc).remove(0);
        let mut p = Page::read(&path).unwrap();
        p.set("evidence_version", json!(1));
        p.set("date", json!("2026-10-01"));
        p.set("evidence_sources", json!([evidence::attribution(&u, "2026-10-01")]));
        p.set("claims", json!([{"text": "救済拡充を求めた", "kind": "request", "attribution": u.reference, "evidence": ["1"], "review_status": "unreviewed", "review_note": ""}]));
        p.write(&path).unwrap();
        run_check(&args).unwrap();
        let mut forged = p.get("evidence_sources").unwrap().clone();
        forged[0]["role"] = json!("government");
        p.set("evidence_sources", forged);
        p.write(&path).unwrap();
        assert!(run_check(&args).is_err());
        assert!(std::fs::read_to_string(args.work.join("check-report.md")).unwrap().contains("役割が原資料と一致しません"));
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn shingikai_quote_with_stale_turn_number_passes_if_in_same_minutes() {
        let root = temp_dir("check_shingikai");
        let wiki = root.join("wiki");
        let work = root.join("work");
        std::fs::create_dir_all(wiki.join("meetings/shingikai")).unwrap();
        std::fs::create_dir_all(work.join("docs/shingikai")).unwrap();
        std::fs::write(
            work.join("docs/shingikai/S1.json"),
            serde_json::to_vec(&json!({
                "minutes_id": "S1",
                "source": {"detail_url": "https://example.go.jp/s1"},
                "minutes_text": "議事内容 ○古藤補佐 開会します。 ○森光健康・生活衛生局長 公平かつ安定的な医療費助成の制度確立を図る。"
            }))
            .unwrap(),
        )
        .unwrap();
        let page = |r: &str, q: &str| {
            format!("---\ntype: \"meeting\"\ntitle: \"会議\"\n---\n{LLM_BEGIN}\n述べた[^1]。\n\n[^1]: [shingikai:{r}](https://example.go.jp/s1) 「{q}」\n{LLM_END}\n")
        };
        let args = || CheckArgs { wiki: wiki.clone(), work: work.clone(), base_url: root.join("public").display().to_string(), changed_only: false };
        // 旧来の「#0 = 全文」の番号でも、同じ議事録にあれば通す。
        std::fs::write(wiki.join("meetings/shingikai/S1.md"), page("S1#0", "公平かつ安定的な医療費助成の制度確立")).unwrap();
        run_check(&args()).unwrap();
        // 議事録に無い文は従来どおり落とす。
        std::fs::write(wiki.join("meetings/shingikai/S1.md"), page("S1#0", "医療費助成を全面的に廃止する")).unwrap();
        assert!(run_check(&args()).is_err());
        // 主張の帰属情報がある新形式では、別の発言者の引用を全文一致で通さない。
        let path = wiki.join("meetings/shingikai/S1.md");
        let mut p = Page::parse(&page("S1#0", "公平かつ安定的な医療費助成の制度確立")).unwrap();
        p.set("evidence_version", json!(1));
        let doc: Value = serde_json::from_slice(&std::fs::read(work.join("docs/shingikai/S1.json")).unwrap()).unwrap();
        let u = shingikai_units(&doc).remove(0);
        p.set("evidence_sources", json!([evidence::attribution(&u, "")]));
        p.set("claims", json!([{"text": "述べた", "kind": "unknown", "attribution": "shingikai:S1#0", "evidence": ["1"], "review_status": "unreviewed", "review_note": ""}]));
        p.write(&path).unwrap();
        assert!(run_check(&args()).is_err());
        assert!(std::fs::read_to_string(work.join("check-report.md")).unwrap().contains("S1#0 の本文にありません"));
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn kanpo_citations_are_verified_against_text_stored_in_the_wiki() {
        let root = temp_dir("check_kanpo");
        let wiki = root.join("wiki");
        let work = root.join("work");
        std::fs::create_dir_all(&work).unwrap();
        ensure_law_page(&wiki, "https://example.test", "L1", "輸出貿易管理令").unwrap();
        let item = super::super::kanpo::KanpoItem {
            date: "2026-10-09".into(),
            issue: "号外第223号".into(),
            page_no: Some(2),
            title: "輸出貿易管理令の一部を改正する政令（三二五）".into(),
            pdf_url: "https://k/p2.pdf".into(),
            format: "prose".into(),
            text: "輸出貿易管理令の一部を次のように改正する。\n別表第一の一の項中「武器」を「武器及び弾薬」に改める。".into(),
            amends: vec!["L1".into()],
            basis: vec![],
        };
        let titles: HashMap<String, String> = [("L1".to_string(), "輸出貿易管理令".to_string())].into();
        let rel = item.page();
        let reference = format!("kanpo:2026-10-09/{}", item.slug());
        let write = |url: &str, quote: &str| {
            std::fs::remove_file(wiki.join(&rel)).ok();
            super::super::kanpo::write_page(&wiki, &item, &titles).unwrap();
            let mut page = Page::read(&wiki.join(&rel)).unwrap();
            page.body = page.body.replace(
                &format!("{LLM_BEGIN}\n{LLM_END}"),
                &format!("{LLM_BEGIN}\n別表第一を改める[^1]。\n\n[^1]: [{reference}]({url}) 「{quote}」\n{LLM_END}"),
            );
            page.write(&wiki.join(&rel)).unwrap();
        };
        // 配信側 (public/) に官報が無くても、ページに保存した本文と照合する。
        let args = || CheckArgs { wiki: wiki.clone(), work: work.clone(), base_url: root.join("public").display().to_string(), changed_only: false };
        write("https://k/p2.pdf", "別表第一の一の項中「武器」を「武器及び弾薬」に改める");
        run_check(&args()).unwrap();
        write("https://k/p2.pdf", "別表第一を全面的に改める");
        assert!(run_check(&args()).is_err(), "本文に無い引用は落とす");
        write("https://k/other.pdf", "別表第一の一の項中「武器」を");
        assert!(run_check(&args()).is_err(), "URL は官報の PDF と一致させる");
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn bill_citations_are_verified_against_bill_documents() {
        let root = temp_dir("check_gian");
        let wiki = root.join("wiki");
        let work = root.join("work");
        std::fs::create_dir_all(wiki.join("bills/221")).unwrap();
        std::fs::create_dir_all(work.join("docs/gian")).unwrap();
        std::fs::write(
            work.join("docs/gian/221_B1.json"),
            serde_json::to_vec(&json!({
                "detail": {"session": 221, "bill_id": "B1", "documents": [
                    {"kind": "bill_text", "url": "https://x/houan", "text": "理 由\n投票人の投票しやすい環境を整える必要がある。"}
                ]},
                "resolutions": []
            }))
            .unwrap(),
        )
        .unwrap();
        let page = |q: &str| {
            format!("---\ntype: \"bill\"\ntitle: \"法案\"\n---\n{LLM_BEGIN}\n提出理由[^1]。\n\n[^1]: [gian:221/B1#reason](https://x/houan) 「{q}」\n{LLM_END}\n")
        };
        let args = || CheckArgs { wiki: wiki.clone(), work: work.clone(), base_url: root.join("public").display().to_string(), changed_only: false };
        std::fs::write(wiki.join("bills/221/B1.md"), page("投票人の投票しやすい環境を整える")).unwrap();
        run_check(&args()).unwrap();
        std::fs::write(wiki.join("bills/221/B1.md"), page("投票所を全面的に廃止する")).unwrap();
        assert!(run_check(&args()).is_err());
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn speech_links_must_point_at_a_cited_speech() {
        let def = "[^1]: [kokkai:M1_001](https://kokkai.ndl.go.jp/txt/M1/1) 「副反応の救済を拡充すべきです」";
        let (root, args) = fixture(&format!("- 救済拡充を求めた（[発言](https://kokkai.ndl.go.jp/txt/M1/1)）[^1]。\n\n{def}"));
        run_check(&args).unwrap();
        std::fs::remove_dir_all(root).ok();
        // 会議単位の URL は見出しのリンクに使うので対象外。
        let (root, args) = fixture(&format!("### [会議](https://kokkai.ndl.go.jp/txt/M1)\n- 求めた[^1]。\n\n{def}"));
        run_check(&args).unwrap();
        std::fs::remove_dir_all(root).ok();
        let (root, args) = fixture(&format!("- 救済拡充を求めた（[発言](https://kokkai.ndl.go.jp/txt/M1/9)）[^1]。\n\n{def}"));
        assert!(run_check(&args).is_err(), "引用していない発言へのリンクは落とす");
        std::fs::remove_dir_all(root).ok();
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

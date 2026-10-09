//! `lawpub wiki-plan`: 今日 LLM に渡す仕事を決定的に決める。
//!
//! 1. 国会会議録・審議会議事録の index から、未処理かつ `lookback_days` 以内の会議を新しい順に並べる。
//! 2. 会議 → 法令リンク (`links/meeting-to-laws`, `links/shingikai-to-laws`) を引き、
//!    リンクが無ければ `no_link` として処理済みにする (以後は引かない)。
//! 3. リンクがあれば会議本体を取得し、法令名を含む発言だけを前後の文脈付きで抜粋する。
//!    LLM は会議全文ではなくこの抜粋 (ソースバンドル) だけを読む。
//! 4. 会議ページ・法令ページの雛形を作り、`plan.json` / `plan.md` を書く。
//!
//! `max_items` 件の会議を集めるか、`max_probes` 件のリンクを引いたら止める (コスト上限)。

use super::*;
use serde_json::json;
use std::collections::{BTreeSet, HashMap};

pub struct PlanArgs {
    pub base_url: String,
    pub wiki: PathBuf,
    pub work: PathBuf,
    pub max_items: usize,
    pub max_probes: usize,
    pub lookback_days: i64,
    pub today: Option<String>,
    /// 1 回の plan で LLM に要約させる議案の上限 (会議とは別枠)。
    pub max_bills: usize,
    /// 1 回の plan で LLM に要約させるパブコメ (結果公示済み) の上限 (別枠)。
    pub max_pubcomments: usize,
    /// 自治体例規の配信元 (R2 の `{public}/reiki`)。空なら例規の照合をしない。
    pub reiki_base_url: String,
}

/// 1 発言から抜き出す文脈幅 (文字数)。
const WINDOW_BEFORE: usize = 200;
const WINDOW_AFTER: usize = 500;
/// 1 会議あたりのソースバンドルの上限。
const MAX_EXCERPTS_PER_MEETING: usize = 12;
const MAX_CHARS_PER_MEETING: usize = 8_000;
/// 答弁・締めの発言として足す文脈の上限 (1 発言あたり / 1 会議あたり)。
const CONTEXT_CHARS: usize = 600;
const MAX_CONTEXT_CHARS_PER_MEETING: usize = 3_000;
/// 審議会の議事録を要点化するときの上限 (1 会議あたり) と、法令に触れない発言 1 つあたりの長さ。
const DIGEST_CHARS_PER_MEETING: usize = 12_000;
const DIGEST_MIN_TURN_CHARS: usize = 80;
const DIGEST_MAX_TURN_CHARS: usize = 300;
/// 会議ページの frontmatter (speakers 等) の形式。上げると既存ページも LLM なしで作り直す。
pub(crate) const MEETING_RENDER_VERSION: u64 = 2;
/// 1 回の plan で作り直す既存会議ページの上限 (取得数の上限)。
const MAX_REFRESH_PER_RUN: usize = 60;

/// 審議会の発言者表記のうち、官職で終わるもの (姓＋官職でほぼ 1 人に決まる)。
/// 「森委員」「神作部会長」のような委員は姓だけで同定できないので会議体ページに載せる。
const OFFICIAL_TITLES: [&str; 14] = [
    "局長", "審議官", "課長", "室長", "大臣", "政務官", "長官", "次長", "統括官", "参事官", "官房長", "次官", "総長", "部長",
];

struct Candidate {
    kind: &'static str,
    id: String,
    date: String,
    ministry: Option<String>,
    /// 処理済みだが経緯の形式 (NARRATIVE_VERSION) が古く、書き足しのために再投入した会議。
    stale: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Excerpt {
    #[serde(rename = "ref")]
    pub reference: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<String>,
    /// この発言が言及した法令 ID。
    pub laws: Vec<String>,
    /// 法令名を含まないが経緯に要る発言: `reply` (直後の答弁) / `closing` (会議の締め)。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone)]
struct LinkedLaw {
    law_id: String,
    title: String,
    patterns: Vec<String>,
}

pub fn run_plan(args: &PlanArgs) -> Result<()> {
    let source = Source::new(&args.base_url)?;
    let today = match &args.today {
        Some(d) => {
            chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").context("--today は YYYY-MM-DD")?
        }
        None => today_jst(),
    };
    let today_s = today.format("%Y-%m-%d").to_string();
    let since = (today - chrono::Duration::days(args.lookback_days))
        .format("%Y-%m-%d")
        .to_string();

    let mut state = State::load(&args.wiki)?;
    let candidates = collect_candidates(&source, &args.wiki, &state, &since)?;
    tracing::info!(
        "wiki-plan: {} unprocessed meetings since {since}",
        candidates.len()
    );

    if args.work.exists() {
        std::fs::remove_dir_all(&args.work)?;
    }
    std::fs::create_dir_all(args.work.join("sources"))?;
    std::fs::create_dir_all(args.work.join("docs"))?;

    let mut tasks = Vec::new();
    let mut probes = 0usize;
    for c in candidates {
        if tasks.len() >= args.max_items || probes >= args.max_probes {
            break;
        }
        probes += 1;
        let key = format!("{}:{}", c.kind, c.id);
        let link_path = match c.kind {
            KIND_KOKKAI => format!("links/meeting-to-laws/{}.json", c.id),
            _ => format!("links/shingikai-to-laws/{}.json", c.id),
        };
        let laws = match source.get_json(&link_path) {
            Ok(Some(v)) => linked_laws(&v),
            Ok(None) => Vec::new(),
            Err(e) => {
                tracing::warn!("wiki-plan: {key}: {e:#} — 次回に再試行");
                continue;
            }
        };
        // 審議会は資料だけでは中身がわからないので、法令リンクが無くても議事録を要点化する。
        // 国会は会議数が多いので、法令に言及した会議だけを扱う。
        if laws.is_empty() && c.kind == KIND_KOKKAI {
            state.mark(&key, "no_link", &today_s);
            retire_stale(&args.wiki, &c)?;
            continue;
        }

        let doc_path = match (c.kind, &c.ministry) {
            (KIND_KOKKAI, _) => format!("proceedings/{}.json", c.id),
            (_, Some(ministry)) => format!("shingikai/{ministry}/{}.json", c.id),
            (_, None) => continue,
        };
        let doc = match source.get_json(&doc_path) {
            Ok(Some(d)) => d,
            Ok(None) => {
                tracing::warn!("wiki-plan: {key}: {doc_path} が見つかりません — 次回に再試行");
                continue;
            }
            Err(e) => {
                tracing::warn!("wiki-plan: {key}: {e:#} — 次回に再試行");
                continue;
            }
        };
        let units = match c.kind {
            KIND_KOKKAI => kokkai_units(&doc),
            _ => shingikai_units(&doc),
        };
        let excerpts = if c.kind == KIND_SHINGIKAI {
            shingikai_digest(&units, &laws)
        } else {
            build_excerpts(c.kind, &units, &laws)
        };
        // 会議録情報 (付議案件の一覧) と委員長・議長の議事進行にしか法令名が出ない会議は、
        // LLM に渡しても「形式的言及」にしかならないので渡さない (トークン節約)。
        if c.kind == KIND_KOKKAI
            && !excerpts.is_empty()
            && excerpts.iter().filter(|e| e.context.is_none()).all(is_procedural)
        {
            state.mark(&key, "formal_only", &today_s);
            retire_stale(&args.wiki, &c)?;
            continue;
        }
        if excerpts.is_empty() {
            // 法令名が添付資料にだけ現れる等、発言として引用できる言及が無い。
            state.mark(&key, "no_excerpt", &today_s);
            retire_stale(&args.wiki, &c)?;
            continue;
        }

        // wiki-check がオフラインで引用を照合できるよう会議本体を残す。
        let doc_dir = args.work.join("docs").join(c.kind);
        std::fs::create_dir_all(&doc_dir)?;
        std::fs::write(
            doc_dir.join(format!("{}.json", c.id)),
            serde_json::to_vec(&doc)?,
        )?;

        let task = write_task(args, &source, &c, &doc, &laws, &excerpts)?;
        tasks.push(task);
    }

    // 古い形式の会議ページ (発言者の分類が無い等) を LLM なしで作り直す。
    match refresh_meetings(&source, &args.wiki, &plan_pages(&tasks)) {
        Ok(n) if n > 0 => println!("wiki-plan: refreshed {n} meeting page(s)"),
        Ok(_) => {}
        Err(e) => tracing::warn!("wiki-plan: 会議ページの更新に失敗: {e:#}"),
    }

    // 議案と改正履歴 (LLM 不要) を取り込む。失敗しても会議のタスクは続ける。
    match structured::sync(&source, &args.wiki, today) {
        Ok(st) => println!(
            "wiki-plan: {} bill(s) ({} fetched), {} pubcomment(s), {} new law page(s), {} law(s) with revisions",
            st.bills, st.bills_fetched, st.pubcomments, st.laws_created, st.laws_with_revisions
        ),
        Err(e) => tracing::warn!("wiki-plan: 議案・改正履歴の取り込みに失敗: {e:#}"),
    }
    match plan_bills(args, &source) {
        Ok(bill_tasks) => tasks.extend(bill_tasks),
        Err(e) => tracing::warn!("wiki-plan: 議案の要約タスクの作成に失敗: {e:#}"),
    }
    match super::reiki::sync(&args.reiki_base_url, source.base(), &args.wiki, today) {
        Ok(Some(n)) => println!("wiki-plan: reiki links refreshed ({n} law page(s) updated)"),
        Ok(None) => {}
        Err(e) => tracing::warn!("wiki-plan: 例規の照合に失敗: {e:#}"),
    }
    match super::pubcomment::plan_tasks(&source, &args.wiki, &args.work, args.max_pubcomments) {
        Ok(pc_tasks) => tasks.extend(pc_tasks),
        Err(e) => tracing::warn!("wiki-plan: パブコメの要約タスクの作成に失敗: {e:#}"),
    }

    state.save(&args.wiki)?;
    let plan = Plan {
        schema_version: 1,
        date: today_s,
        base_url: source.base().to_string(),
        tasks,
    };
    std::fs::write(
        args.work.join("plan.json"),
        serde_json::to_string_pretty(&plan)?,
    )?;
    std::fs::write(
        args.work.join("plan.md"),
        render_plan_md(&plan, &args.wiki, &args.work),
    )?;
    println!(
        "wiki-plan: {} task(s), {probes} link probe(s)",
        plan.tasks.len()
    );
    Ok(())
}

fn collect_candidates(source: &Source, wiki: &Path, state: &State, since: &str) -> Result<Vec<Candidate>> {
    let mut out = Vec::new();
    let mut ministries: HashMap<String, String> = HashMap::new();
    if let Some(index) = source.get_json("proceedings/index.json")? {
        for m in index["meetings"].as_array().into_iter().flatten() {
            let (Some(id), Some(date)) = (m["meeting_id"].as_str(), m["date"].as_str()) else {
                continue;
            };
            if date >= since && !state.is_processed(&format!("{KIND_KOKKAI}:{id}")) {
                out.push(Candidate {
                    kind: KIND_KOKKAI,
                    id: id.into(),
                    date: date.into(),
                    ministry: None,
                    stale: false,
                });
            }
        }
    }
    if let Some(index) = source.get_json("shingikai/index.json")? {
        for m in index["minutes"].as_array().into_iter().flatten() {
            let (Some(id), Some(date)) = (m["minutes_id"].as_str(), m["date"].as_str()) else {
                continue;
            };
            if let Some(ministry) = m["ministry"].as_str() {
                ministries.insert(id.to_string(), ministry.to_string());
            }
            // 議事録が公開されるまでは引用元が無い。公開後の run で拾う。
            if !m["has_minutes"].as_bool().unwrap_or(false) {
                continue;
            }
            // 以前は法令リンクの無い回を no_link として飛ばしていたが、今は要点化するので拾い直す。
            let done = state
                .processed
                .get(&format!("{KIND_SHINGIKAI}:{id}"))
                .is_some_and(|p| p.status != "no_link" && p.status != "no_excerpt");
            if date >= since && !done {
                out.push(Candidate {
                    kind: KIND_SHINGIKAI,
                    id: id.into(),
                    date: date.into(),
                    ministry: m["ministry"].as_str().map(String::from),
                    stale: false,
                });
            }
        }
    }
    out.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.id.cmp(&b.id)));

    // 新しい会議の後ろに、経緯の形式が古い処理済み会議を新しい順に並べる。
    let mut stale = Vec::new();
    for path in walk_md(wiki, "meetings") {
        let page = Page::read(&path)?;
        if page.get("llm_version").and_then(Value::as_u64).unwrap_or(0) >= NARRATIVE_VERSION {
            continue;
        }
        let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
        let (kind, ministry) = match page.get_str("corpus") {
            KIND_KOKKAI => (KIND_KOKKAI, None),
            KIND_SHINGIKAI => (KIND_SHINGIKAI, ministries.get(&id).cloned()),
            _ => continue,
        };
        stale.push(Candidate { kind, id, date: page.get_str("date").to_string(), ministry, stale: true });
    }
    stale.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.id.cmp(&b.id)));
    out.extend(stale);
    Ok(out)
}

/// 再投入した会議を LLM に渡さずに終える場合は、印を進めて毎日再投入されないようにする。
fn retire_stale(wiki: &Path, c: &Candidate) -> Result<()> {
    if !c.stale {
        return Ok(());
    }
    let path = wiki.join(meeting_page(c.kind, &c.id));
    if path.exists() {
        let mut page = Page::read(&path)?;
        page.set("llm_version", json!(NARRATIVE_VERSION));
        page.write(&path)?;
    }
    Ok(())
}

fn linked_laws(links: &Value) -> Vec<LinkedLaw> {
    links["linked_laws"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|l| {
            let law_id = l["law_id"].as_str()?.to_string();
            let title = l["title"].as_str().unwrap_or("").to_string();
            let mut patterns: BTreeSet<String> = l["match_reasons"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|r| r.as_str().map(String::from))
                .collect();
            patterns.insert(title.clone());
            let patterns: Vec<String> = patterns
                .into_iter()
                .filter(|p| p.chars().count() >= 2)
                .collect();
            (!patterns.is_empty()).then_some(LinkedLaw {
                law_id,
                title,
                patterns,
            })
        })
        .collect()
}

/// 法令名を含む発言を、マッチ位置の前後を残して抜粋する。
pub(crate) fn build_excerpts_for(units: &[Unit], laws: &[(String, Vec<String>)]) -> Vec<Excerpt> {
    let mut out = Vec::new();
    let mut total = 0usize;
    for unit in units {
        let chars: Vec<char> = unit.text.chars().collect();
        let mut spans: Vec<(usize, usize)> = Vec::new();
        let mut hit_laws = Vec::new();
        for (law_id, patterns) in laws {
            let mut hit = false;
            for p in patterns {
                for (byte_pos, _) in unit.text.match_indices(p.as_str()) {
                    let start = unit.text[..byte_pos].chars().count();
                    let end = start + p.chars().count();
                    spans.push((
                        start.saturating_sub(WINDOW_BEFORE),
                        (end + WINDOW_AFTER).min(chars.len()),
                    ));
                    hit = true;
                }
            }
            if hit {
                hit_laws.push(law_id.clone());
            }
        }
        if spans.is_empty() {
            continue;
        }
        spans.sort();
        let mut merged: Vec<(usize, usize)> = Vec::new();
        for (s, e) in spans {
            match merged.last_mut() {
                Some(last) if s <= last.1 => last.1 = last.1.max(e),
                _ => merged.push((s, e)),
            }
        }
        let mut text = String::new();
        for (i, (s, e)) in merged.iter().enumerate() {
            if i > 0 || *s > 0 {
                text.push('…');
            }
            text.extend(&chars[*s..*e]);
        }
        if merged
            .last()
            .map(|(_, e)| *e < chars.len())
            .unwrap_or(false)
        {
            text.push('…');
        }
        let len = text.chars().count();
        if total + len > MAX_CHARS_PER_MEETING && !out.is_empty() {
            break;
        }
        total += len;
        out.push(Excerpt {
            reference: unit.reference.clone(),
            url: unit.url.clone(),
            speaker: unit.speaker.clone(),
            group: unit.group.clone(),
            position: unit.position.clone(),
            laws: hit_laws,
            context: None,
            text,
        });
        if out.len() >= MAX_EXCERPTS_PER_MEETING {
            break;
        }
    }
    out
}

/// 抜粋から発言者を集める。国会は議事進行 (会議録情報・委員長/議長) を除いた全員、
/// 審議会は官職者 (`official`) と委員 (`member`) に分ける。
pub(crate) fn speakers_from_excerpts(kind: &str, excerpts: &[Excerpt]) -> Vec<Value> {
    let mut speakers: BTreeMap<String, (Value, BTreeSet<String>)> = BTreeMap::new();
    for e in excerpts {
        let Some(name) = e.speaker.as_deref().filter(|n| !n.is_empty()) else { continue };
        let meta = if kind == KIND_KOKKAI {
            if is_procedural(e) {
                continue;
            }
            json!({"name": name, "group": e.group, "position": e.position, "role": "kokkai"})
        } else {
            let role = if is_official_label(name) { "official" } else { "member" };
            json!({"name": name, "role": role})
        };
        let entry = speakers.entry(name.to_string()).or_insert((meta, BTreeSet::new()));
        entry.1.extend(e.laws.iter().cloned());
    }
    speakers
        .into_values()
        .map(|(mut meta, laws)| {
            meta["laws"] = json!(laws);
            meta
        })
        .collect()
}

pub(crate) fn is_official_label(label: &str) -> bool {
    // 「補佐」(課長補佐) や「委員」は姓だけの表記なので官職者として扱わない。
    !label.ends_with("補佐") && OFFICIAL_TITLES.iter().any(|t| label.ends_with(t) && label.chars().count() > t.chars().count() + 1)
}

/// 国会の議事進行の発言 (会議録情報、委員長・議長) か。発言冒頭の「○國場委員長　」で判定する。
pub(crate) fn is_procedural(e: &Excerpt) -> bool {
    if e.speaker.as_deref() == Some(KOKKAI_HEADER_SPEAKER) {
        return true;
    }
    let head: String = e
        .text
        .trim_start_matches('…')
        .chars()
        .take_while(|c| !c.is_whitespace())
        .collect();
    head.starts_with('○') && (head.contains("委員長") || head.contains("議長") || head.contains("会長"))
}

fn build_excerpts(kind: &str, units: &[Unit], laws: &[LinkedLaw]) -> Vec<Excerpt> {
    let pairs: Vec<(String, Vec<String>)> = laws
        .iter()
        .map(|l| (l.law_id.clone(), l.patterns.clone()))
        .collect();
    let matched = build_excerpts_for(units, &pairs);
    with_context(kind, units, matched)
}

/// 審議会の議事録を、会議全体の要点がわかる大きさに縮める。法令名を含む発言は前後の文脈付きで、
/// それ以外の発言は冒頭だけ、会議の最後の 2 発言 (結論・次回予定) は末尾を残し、発言順に並べる。
fn shingikai_digest(units: &[Unit], laws: &[LinkedLaw]) -> Vec<Excerpt> {
    let pairs: Vec<(String, Vec<String>)> = laws.iter().map(|l| (l.law_id.clone(), l.patterns.clone())).collect();
    let matched: HashMap<String, Excerpt> = build_excerpts_for(units, &pairs)
        .into_iter()
        .map(|e| (e.reference.clone(), e))
        .collect();
    let matched_chars: usize = matched.values().map(|e| e.text.chars().count()).sum();
    let closing_from = units.len().saturating_sub(2);
    let rest = units.len().saturating_sub(matched.len()).max(1);
    // 残りの発言は、全体が上限に収まる長さで冒頭を残す (短すぎると意味が取れないので下限あり)。
    let per_turn = (DIGEST_CHARS_PER_MEETING.saturating_sub(matched_chars) / rest).clamp(DIGEST_MIN_TURN_CHARS, DIGEST_MAX_TURN_CHARS);
    let mut out = Vec::new();
    let mut total = 0usize;
    for (i, u) in units.iter().enumerate() {
        let (text, context, laws) = if let Some(e) = matched.get(&u.reference) {
            (e.text.clone(), None, e.laws.clone())
        } else {
            let chars: Vec<char> = u.text.chars().collect();
            if chars.len() < 12 {
                continue;
            }
            if i >= closing_from {
                let n = CONTEXT_CHARS.min(chars.len());
                let tail: String = chars[chars.len() - n..].iter().collect();
                (if n < chars.len() { format!("…{tail}") } else { tail }, Some("closing".to_string()), vec![])
            } else {
                let n = per_turn.min(chars.len());
                let head: String = chars[..n].iter().collect();
                (if n < chars.len() { format!("{head}…") } else { head }, Some("turn".to_string()), vec![])
            }
        };
        let n = text.chars().count();
        if total + n > DIGEST_CHARS_PER_MEETING + CONTEXT_CHARS * 2 && i < closing_from {
            continue;
        }
        total += n;
        out.push(Excerpt {
            reference: u.reference.clone(),
            url: u.url.clone(),
            speaker: u.speaker.clone(),
            group: u.group.clone(),
            position: u.position.clone(),
            laws,
            context,
            text,
        });
    }
    out
}

/// 「どうなったか」は法令名を繰り返さない発言にあることが多いので、文脈を足す。
/// 国会は法令名を含む発言の直後の発言 (答弁)、審議会は会議の最後の 2 発言 (結論・次回予定)。
/// 足した後は会議の発言順に並べ直す。
pub(crate) fn with_context(kind: &str, units: &[Unit], matched: Vec<Excerpt>) -> Vec<Excerpt> {
    if matched.is_empty() {
        return matched;
    }
    let index: HashMap<&str, usize> = units.iter().enumerate().map(|(i, u)| (u.reference.as_str(), i)).collect();
    let included: BTreeSet<usize> = matched.iter().filter_map(|e| index.get(e.reference.as_str()).copied()).collect();
    let mut extra: BTreeSet<(usize, &'static str)> = BTreeSet::new();
    if kind == KIND_KOKKAI {
        for e in matched.iter().filter(|e| !is_procedural(e)) {
            let Some(&i) = index.get(e.reference.as_str()) else { continue };
            if i + 1 < units.len() && !included.contains(&(i + 1)) {
                extra.insert((i + 1, "reply"));
            }
        }
    } else {
        for i in units.len().saturating_sub(2)..units.len() {
            if !included.contains(&i) {
                extra.insert((i, "closing"));
            }
        }
    }
    let mut out: Vec<(usize, Excerpt)> = matched
        .into_iter()
        .map(|e| (index.get(e.reference.as_str()).copied().unwrap_or(usize::MAX), e))
        .collect();
    let mut budget = MAX_CONTEXT_CHARS_PER_MEETING;
    for (i, ctx) in extra {
        let u = &units[i];
        let chars: Vec<char> = u.text.chars().collect();
        // 答弁は冒頭、締めは末尾が肝心。
        let text: String = if chars.len() <= CONTEXT_CHARS {
            u.text.clone()
        } else if ctx == "reply" {
            chars[..CONTEXT_CHARS].iter().collect::<String>() + "…"
        } else {
            "…".to_string() + &chars[chars.len() - CONTEXT_CHARS..].iter().collect::<String>()
        };
        let e = Excerpt {
            reference: u.reference.clone(),
            url: u.url.clone(),
            speaker: u.speaker.clone(),
            group: u.group.clone(),
            position: u.position.clone(),
            laws: vec![],
            context: Some(ctx.to_string()),
            text,
        };
        // 議事進行 (委員長の「次に〜君」等) は文脈にならない。
        if ctx == "reply" && is_procedural(&e) {
            continue;
        }
        let n = e.text.chars().count();
        if n > budget {
            break;
        }
        budget -= n;
        out.push((i, e));
    }
    out.sort_by_key(|(i, _)| *i);
    out.into_iter().map(|(_, e)| e).collect()
}

fn house_ja(house: &str) -> &str {
    match house {
        "shugiin" => "衆議院",
        "sangiin" => "参議院",
        other => other,
    }
}

fn ministry_ja(ministry: &str) -> &str {
    match ministry {
        "moj" => "法務省",
        "mhlw" => "厚生労働省",
        "mlit" => "国土交通省",
        "cao" => "内閣府",
        other => other,
    }
}

fn write_task(
    args: &PlanArgs,
    source: &Source,
    c: &Candidate,
    doc: &Value,
    laws: &[LinkedLaw],
    excerpts: &[Excerpt],
) -> Result<Task> {
    // 実際に抜粋に現れた法令だけをページ化する (リンク判定はタイトル一致なので広め)。
    let cited: BTreeSet<&str> = excerpts
        .iter()
        .flat_map(|e| e.laws.iter().map(String::as_str))
        .collect();
    let laws: Vec<&LinkedLaw> = laws
        .iter()
        .filter(|l| cited.contains(l.law_id.as_str()))
        .collect();

    let (title, organization, committee, resource) = match c.kind {
        KIND_KOKKAI => {
            let house = house_ja(doc["house"].as_str().unwrap_or(""));
            let committee = doc["committee"].as_str().unwrap_or("");
            let issue = doc["issue"].as_str().unwrap_or("");
            (
                format!("{house} {committee} {issue}").trim().to_string(),
                house.to_string(),
                committee.to_string(),
                format!("https://kokkai.ndl.go.jp/txt/{}", c.id),
            )
        }
        _ => {
            let ministry = ministry_ja(doc["ministry"].as_str().unwrap_or(""));
            (
                doc["title"].as_str().unwrap_or(&c.id).to_string(),
                ministry.to_string(),
                doc["committee"].as_str().unwrap_or("").to_string(),
                doc["source"]["detail_url"]
                    .as_str()
                    .unwrap_or("")
                    .to_string(),
            )
        }
    };

    let page = meeting_page(c.kind, &c.id);
    let mut created = Vec::new();

    // 会議ページ (既存なら LLM 区間を温存する)。
    let page_path = args.wiki.join(&page);
    if !page_path.exists() {
        created.push(page.clone());
    }
    let mut meeting = if page_path.exists() {
        Page::read(&page_path)?
    } else {
        Page::default()
    };
    let speakers_json = speakers_from_excerpts(c.kind, excerpts);
    let description = meeting.get_str("description").to_string();
    let tags = meeting.get("tags").cloned().unwrap_or(json!([]));
    meeting.frontmatter = vec![
        ("type".into(), json!("meeting")),
        ("title".into(), json!(title)),
        ("description".into(), json!(description)),
        ("resource".into(), json!(resource)),
        ("timestamp".into(), json!(now_rfc3339())),
        ("date".into(), json!(c.date)),
        ("corpus".into(), json!(c.kind)),
        ("organization".into(), json!(organization)),
        ("committee".into(), json!(committee)),
        (
            "laws".into(),
            json!(laws.iter().map(|l| &l.law_id).collect::<Vec<_>>()),
        ),
        ("speakers".into(), Value::Array(speakers_json.clone())),
        ("tags".into(), tags),
        ("render_version".into(), json!(MEETING_RENDER_VERSION)),
    ];
    let law_links: Vec<String> = laws
        .iter()
        .map(|l| format!("[{}]({})", l.title, rel_link(&page, &law_page(&l.law_id))))
        .collect();
    let source_label = if c.kind == KIND_KOKKAI {
        "国会会議録検索システム"
    } else {
        "会議ページ"
    };
    let meta = format!(
        "| 項目 | 内容 |\n|---|---|\n| 日付 | {} |\n| 会議 | {} |\n| 原文 | [{source_label}]({resource}) |\n| 言及法令 | {} |",
        c.date,
        cell(&title),
        law_links.join("、"),
    );
    meeting.body = match replace_block(&meeting.body, "meta", &meta) {
        Some(body) => body,
        None => format!(
            "\n# {title}\n\n<!-- lawpub:begin meta -->\n{meta}\n<!-- lawpub:end meta -->\n\n## 要点\n\n{LLM_BEGIN}\n{LLM_END}\n"
        ),
    };
    meeting.write(&page_path)?;

    // 法令ページの雛形 (時系列は finalize が埋める)。
    let mut task_laws = Vec::new();
    for law in &laws {
        let lp = law_page(&law.law_id);
        if ensure_law_page(&args.wiki, source.base(), &law.law_id, &law.title)? {
            created.push(lp.clone());
        }
        ensure_law_synthesis(&args.wiki, &law.law_id)?;
        task_laws.push(TaskLaw {
            law_id: law.law_id.clone(),
            title: law.title.clone(),
            page: lp,
        });
    }

    // 経緯を書き足す人物・会議体ページ (無ければ雛形を作る)。
    let mut task_people = Vec::new();
    for sp in speakers_json.iter().filter(|sp| sp["role"] != "member") {
        let Some(name) = sp["name"].as_str() else { continue };
        if ensure_person_page(&args.wiki, name)? {
            created.push(person_page(name));
        }
        task_people.push(person_page(name));
    }
    let task_committee = match committee_title(c.kind, &organization, &committee) {
        Some(t) => {
            if ensure_committee_page(&args.wiki, &t)? {
                created.push(committee_page(&t));
            }
            Some(committee_page(&t))
        }
        None => None,
    };

    let source_rel = format!("sources/{}_{}.json", c.kind, file_safe(&c.id));
    let bundle = json!({
        "kind": c.kind,
        "id": c.id,
        "title": title,
        "date": c.date,
        "organization": organization,
        "committee": committee,
        "agenda": doc["agenda"],
        "url": resource,
        "laws": task_laws,
        "excerpts": excerpts,
    });
    std::fs::write(
        args.work.join(&source_rel),
        serde_json::to_string_pretty(&bundle)?,
    )?;

    Ok(Task {
        key: format!("{}:{}", c.kind, c.id),
        kind: c.kind.to_string(),
        id: c.id.clone(),
        date: c.date.clone(),
        title,
        page,
        source: source_rel,
        laws: task_laws,
        people: task_people,
        committee: task_committee,
        created,
    })
}

/// 議案の引用単位をソースバンドルに載せるときの上限 (字)。
const BILL_REASON_CHARS: usize = 1_500;
const BILL_OUTLINE_CHARS: usize = 2_500;
const BILL_RESOLUTION_CHARS: usize = 1_500;
const MAX_RESOLUTIONS: usize = 3;

fn head(text: &str, n: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= n {
        text.to_string()
    } else {
        chars[..n].iter().collect::<String>() + "…"
    }
}

/// 経過が進んだ (または未要約の) 議案を、新しい順に最大 `max_bills` 件 LLM に渡す。
/// 議案ページの `llm_latest_date` が `latest_date` と同じなら要約済み。
fn plan_bills(args: &PlanArgs, source: &Source) -> Result<Vec<Task>> {
    if args.max_bills == 0 {
        return Ok(Vec::new());
    }
    let mut pending: Vec<(String, PathBuf, Page)> = Vec::new();
    for path in walk_md(&args.wiki, "bills") {
        let page = Page::read(&path)?;
        let latest = page.get_str("latest_date");
        if latest.is_empty() || page.get_str("llm_latest_date") == latest {
            continue;
        }
        pending.push((page.get_str("date").to_string(), path, page));
    }
    pending.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

    let mut tasks = Vec::new();
    for (_, path, mut page) in pending {
        if tasks.len() >= args.max_bills {
            break;
        }
        let (Some(session), bill_id) = (page.get("session").and_then(Value::as_u64), page.get_str("bill_id").to_string()) else {
            continue;
        };
        let doc = match fetch_gian_doc(source, session, &bill_id) {
            Ok(Some(d)) => d,
            Ok(None) => continue,
            Err(e) => {
                tracing::warn!("wiki-plan: 議案 {session}/{bill_id}: {e:#}");
                continue;
            }
        };
        let resolutions: Vec<Value> = doc["resolutions"].as_array().cloned().unwrap_or_default();
        let units = gian_units(&doc["detail"], &resolutions);
        if units.is_empty() {
            // 要約の材料 (理由・要綱・附帯決議) が無い議案は、表の情報だけで足りる。
            let latest = page.get_str("latest_date").to_string();
            page.set("llm_latest_date", json!(latest));
            page.write(&path)?;
            continue;
        }
        let dir = args.work.join("docs").join(KIND_GIAN);
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join(format!("{session}_{}.json", file_safe(&bill_id))), serde_json::to_vec(&doc)?)?;

        let mut n_res = 0;
        let excerpts: Vec<Value> = units
            .iter()
            .filter_map(|u| {
                let anchor = u.reference.rsplit('#').next().unwrap_or("");
                let cap = match anchor {
                    "reason" => BILL_REASON_CHARS,
                    "outline" => BILL_OUTLINE_CHARS,
                    a if a.starts_with("res-") => {
                        n_res += 1;
                        if n_res > MAX_RESOLUTIONS {
                            return None;
                        }
                        BILL_RESOLUTION_CHARS
                    }
                    _ => return None,
                };
                Some(json!({"ref": u.reference, "url": u.url, "label": u.speaker, "text": head(&u.text, cap)}))
            })
            .collect();
        let laws: Vec<TaskLaw> = page
            .get("laws")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str())
            .map(|id| {
                let lp = law_page(id);
                if let Err(e) = ensure_law_synthesis(&args.wiki, id) {
                    tracing::warn!("wiki-plan: 全体像の区間を追加できません {id}: {e:#}");
                }
                let title = Page::read(&args.wiki.join(&lp)).map(|p| p.get_str("title").to_string()).unwrap_or_default();
                TaskLaw { law_id: id.to_string(), title, page: lp }
            })
            .collect();
        let rel = rel_path(&args.wiki, &path);
        let fields: serde_json::Map<String, Value> = doc["detail"]["fields"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|f| Some((f["key"].as_str()?.to_string(), f["value"].clone())))
            .filter(|(k, _)| k.contains("提出") || k.contains("会派") || k.contains("結果"))
            .collect();
        let source_rel = format!("sources/bill_{session}_{}.json", file_safe(&bill_id));
        let bundle = json!({
            "kind": "bill",
            "id": bill_id,
            "session": session,
            "title": page.get_str("title"),
            "bill_type": page.get_str("bill_type"),
            "result": page.get_str("result"),
            "stages": page.get("stages"),
            "fields": fields,
            "laws": laws,
            "already_summarized_until": page.get("llm_latest_date"),
            "excerpts": excerpts,
        });
        std::fs::write(args.work.join(&source_rel), serde_json::to_string_pretty(&bundle)?)?;
        tasks.push(Task {
            key: format!("{KIND_GIAN}:{session}/{bill_id}"),
            kind: "bill".into(),
            id: bill_id.clone(),
            date: page.get_str("date").to_string(),
            title: page.get_str("title").to_string(),
            page: rel,
            source: source_rel,
            laws,
            people: Vec::new(),
            committee: None,
            created: Vec::new(),
        });
    }
    Ok(tasks)
}

fn plan_pages(tasks: &[Task]) -> BTreeSet<String> {
    tasks.iter().map(|t| t.page.clone()).collect()
}

/// `render_version` が古い会議ページの frontmatter (speakers・laws) を、会議本体と法令リンクから
/// 作り直す。LLM の書いた description・tags・要点はそのまま残す。
fn refresh_meetings(source: &Source, wiki: &Path, skip: &BTreeSet<String>) -> Result<usize> {
    let mut refreshed = 0;
    let mut shingikai_ministry: Option<HashMap<String, String>> = None;
    for path in walk_md(wiki, "meetings") {
        if refreshed >= MAX_REFRESH_PER_RUN {
            break;
        }
        let rel = rel_path(wiki, &path);
        if skip.contains(&rel) {
            continue;
        }
        let mut page = Page::read(&path)?;
        if page.get("render_version").and_then(Value::as_u64) == Some(MEETING_RENDER_VERSION) {
            continue;
        }
        let kind = page.get_str("corpus").to_string();
        let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
        let (link_path, doc_path) = if kind == KIND_KOKKAI {
            (format!("links/meeting-to-laws/{id}.json"), format!("proceedings/{id}.json"))
        } else {
            if shingikai_ministry.is_none() {
                let index = source.get_json("shingikai/index.json")?.unwrap_or(Value::Null);
                shingikai_ministry = Some(
                    index["minutes"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|m| Some((m["minutes_id"].as_str()?.to_string(), m["ministry"].as_str()?.to_string())))
                        .collect(),
                );
            }
            let Some(ministry) = shingikai_ministry.as_ref().and_then(|m| m.get(&id)) else { continue };
            (format!("links/shingikai-to-laws/{id}.json"), format!("shingikai/{ministry}/{id}.json"))
        };
        let (Some(links), Some(doc)) = (source.get_json(&link_path)?, source.get_json(&doc_path)?) else { continue };
        let units = if kind == KIND_KOKKAI { kokkai_units(&doc) } else { shingikai_units(&doc) };
        let excerpts = build_excerpts(&kind, &units, &linked_laws(&links));
        page.set("speakers", Value::Array(speakers_from_excerpts(&kind, &excerpts)));
        page.set("render_version", json!(MEETING_RENDER_VERSION));
        page.write(&path)?;
        refreshed += 1;
    }
    Ok(refreshed)
}

fn render_plan_md(plan: &Plan, wiki: &Path, work: &Path) -> String {
    let wiki = wiki.display();
    let work = work.display();
    let mut out = format!("# 本日の wiki 更新タスク ({})\n\n", plan.date);
    if plan.tasks.is_empty() {
        out.push_str("今日は対象の会議がありません。\n");
        return out;
    }
    out.push_str(&format!(
        "{} 件。各タスクのソースバンドルを読み、指示書に従ってページを更新してください。\n\n",
        plan.tasks.len()
    ));
    for (i, t) in plan.tasks.iter().enumerate() {
        out.push_str(&format!("## {}. {} ({})\n\n", i + 1, t.title, t.date));
        out.push_str(&format!("- ソースバンドル: `{work}/{}`\n", t.source));
        let new = |p: &str| {
            if t.created.iter().any(|c| c == p) {
                "（新規）"
            } else {
                "（既存・追記）"
            }
        };
        let page_label = match t.kind.as_str() {
            "bill" => "議案ページ",
            "pubcomment" => "パブコメページ",
            _ => "会議ページ",
        };
        out.push_str(&format!("- {page_label}: `{wiki}/{}`{}\n", t.page, new(&t.page)));
        for l in &t.laws {
            out.push_str(&format!(
                "- 法令ページ: `{wiki}/{}` {}{}\n",
                l.page,
                l.title,
                new(&l.page)
            ));
        }
        if let Some(cp) = &t.committee {
            out.push_str(&format!("- 会議体ページ: `{wiki}/{cp}`{}\n", new(cp)));
        }
        for pp in &t.people {
            out.push_str(&format!("- 人物ページ: `{wiki}/{pp}`{}（実質的な発言をした場合だけ追記）\n", new(pp)));
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(reference: &str, text: &str) -> Unit {
        Unit {
            reference: reference.into(),
            url: "u".into(),
            speaker: Some("山田太郎".into()),
            group: None,
            position: None,
            text: text.into(),
        }
    }

    #[test]
    fn excerpts_keep_only_mentioning_units_with_context() {
        let filler = "あ".repeat(2000);
        let units = vec![
            unit("kokkai:a", "関係ない発言です。"),
            unit(
                "kokkai:b",
                &format!("{filler}予防接種法の改正について伺います。{filler}"),
            ),
        ];
        let laws = vec![("L1".to_string(), vec!["予防接種法".to_string()])];
        let out = build_excerpts_for(&units, &laws);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].reference, "kokkai:b");
        assert_eq!(out[0].laws, vec!["L1"]);
        assert!(out[0].text.starts_with('…') && out[0].text.ends_with('…'));
        assert!(out[0].text.contains("予防接種法の改正について伺います。"));
        assert!(out[0].text.chars().count() < 1100);
    }

    #[test]
    fn speakers_are_classified_by_corpus() {
        let ex = |speaker: &str, position: Option<&str>, text: &str| Excerpt {
            reference: "r".into(),
            url: "u".into(),
            speaker: Some(speaker.into()),
            group: None,
            position: position.map(String::from),
            laws: vec!["L1".into()],
            context: None,
            text: text.into(),
        };
        let kokkai = speakers_from_excerpts(
            KIND_KOKKAI,
            &[
                ex("会議録情報", None, "本日の会議に付した案件"),
                ex("國場幸之助", None, "○國場委員長　これより会議を開きます"),
                ex("築山信彦", Some("衆議院事務総長"), "○築山事務総長　日本国憲法施行八十周年記念行事の経費"),
            ],
        );
        assert_eq!(kokkai.len(), 1);
        assert_eq!(kokkai[0]["name"], "築山信彦");
        assert_eq!(kokkai[0]["position"], "衆議院事務総長");

        let shingikai = speakers_from_excerpts(
            KIND_SHINGIKAI,
            &[ex("森光健康・生活衛生局長", None, "x"), ex("森委員", None, "x"), ex("古藤補佐", None, "x")],
        );
        let roles: Vec<(&str, &str)> =
            shingikai.iter().map(|s| (s["name"].as_str().unwrap(), s["role"].as_str().unwrap())).collect();
        assert_eq!(roles, vec![("古藤補佐", "member"), ("森光健康・生活衛生局長", "official"), ("森委員", "member")]);
    }

    #[test]
    fn replies_and_closings_are_added_as_context() {
        let u = |r: &str, text: &str| Unit {
            reference: r.into(),
            url: "u".into(),
            speaker: Some("x".into()),
            group: None,
            position: None,
            text: text.into(),
        };
        let laws = vec![("L1".to_string(), vec!["予防接種法".to_string()])];
        // 国会: 質問の直後の答弁を足す (法令名を含まなくても)。
        let units = vec![
            u("k:1", "○山田太郎君　予防接種法の救済を拡充すべきでは。"),
            u("k:2", "○佐藤大臣　検討会で年内に結論を得ます。"),
            u("k:3", "○別の議員君　別の話題です。"),
        ];
        let out = with_context(KIND_KOKKAI, &units, build_excerpts_for(&units, &laws));
        let refs: Vec<(&str, Option<&str>)> = out.iter().map(|e| (e.reference.as_str(), e.context.as_deref())).collect();
        assert_eq!(refs, vec![("k:1", None), ("k:2", Some("reply"))]);

        // 審議会: 会議の最後の 2 発言を足す。
        let units = vec![
            u("s:0", "議事録"),
            u("s:1", "○局長 予防接種法に基づき説明します。"),
            u("s:2", "○委員 意見です。"),
            u("s:3", "○委員長 次回は11月に開催します。"),
        ];
        let out = with_context(KIND_SHINGIKAI, &units, build_excerpts_for(&units, &laws));
        let refs: Vec<&str> = out.iter().map(|e| e.reference.as_str()).collect();
        assert_eq!(refs, vec!["s:1", "s:2", "s:3"]);
        assert_eq!(out[2].context.as_deref(), Some("closing"));
    }

    #[test]
    fn shingikai_without_law_links_is_summarized_from_a_digest() {
        let root = temp_dir("plan_shingikai");
        let public = root.join("public");
        let wiki = root.join("wiki");
        let work = root.join("work");
        let write = |rel: &str, v: Value| {
            let p = public.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, serde_json::to_vec(&v).unwrap()).unwrap();
        };
        write("shingikai/index.json", json!({"minutes": [
            {"minutes_id": "S1", "ministry": "mhlw", "date": "2026-10-01", "has_minutes": true}
        ]}));
        let long = "あ".repeat(2_000);
        write("shingikai/mhlw/S1.json", json!({
            "minutes_id": "S1", "ministry": "mhlw", "committee": "疾病対策部会指定難病検討委員会",
            "title": "第69回", "agenda": "新規の疾病追加について",
            "source": {"detail_url": "https://www.mhlw.go.jp/s1"},
            "minutes_text": format!("議事内容 ○持田委員長 開会します。 ○西垣補佐 新規の疾病追加について説明します。{long} ○山田委員 患者数の要件を確認したい。 ○持田委員長 次回は12月に開催します。")
        }));
        // 法令リンク (links/shingikai-to-laws/S1.json) は無い。

        run_plan(&PlanArgs {
            base_url: public.display().to_string(),
            wiki: wiki.clone(),
            work: work.clone(),
            max_items: 5,
            max_probes: 10,
            lookback_days: 30,
            today: Some("2026-10-08".into()),
            max_bills: 0,
            max_pubcomments: 0,
            reiki_base_url: String::new(),
        })
        .unwrap();
        let plan = Plan::load(&work).unwrap().unwrap();
        assert_eq!(plan.tasks.len(), 1, "法令リンクが無くても審議会は要点化する");
        let bundle: Value = serde_json::from_slice(&std::fs::read(work.join(&plan.tasks[0].source)).unwrap()).unwrap();
        assert_eq!(bundle["agenda"], "新規の疾病追加について");
        let ex = bundle["excerpts"].as_array().unwrap();
        let speakers: Vec<&str> = ex.iter().filter_map(|e| e["speaker"].as_str()).collect();
        assert_eq!(speakers, vec!["持田委員長", "西垣補佐", "山田委員", "持田委員長"]);
        // 長い説明は冒頭だけ、締めは全文 (短い) が入る。
        assert!(ex[1]["text"].as_str().unwrap().chars().count() <= DIGEST_MAX_TURN_CHARS + 1);
        assert_eq!(ex[3]["context"], "closing");
        assert!(ex[3]["text"].as_str().unwrap().contains("次回は12月に開催します"));
        assert!(wiki.join("committees/疾病対策部会指定難病検討委員会.md").exists());
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn procedural_speeches_are_detected() {
        let ex = |speaker: &str, text: &str| Excerpt {
            reference: "r".into(),
            url: "u".into(),
            speaker: Some(speaker.into()),
            group: None,
            position: None,
            laws: vec![],
            context: None,
            text: text.into(),
        };
        assert!(is_procedural(&ex("会議録情報", "本日の会議に付した案件 地方自治法")));
        assert!(is_procedural(&ex("國場幸之助", "○國場委員長　地方自治法第九十九条の規定に基づく意見書")));
        assert!(!is_procedural(&ex("山田太郎", "○山田太郎君　予防接種法の改正について伺います")));
        // 抜粋が発言の途中から始まる場合は、冒頭の話者表示が無いので議事進行とみなさない。
        assert!(!is_procedural(&ex("國場幸之助", "…地方自治法の改正について")));
    }

    #[test]
    fn plan_against_local_public_creates_bundle_and_skeletons() {
        let root = temp_dir("plan");
        let public = root.join("public");
        let wiki = root.join("wiki");
        let work = root.join("work");
        let write = |rel: &str, v: Value| {
            let p = public.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, serde_json::to_vec(&v).unwrap()).unwrap();
        };
        write(
            "proceedings/index.json",
            json!({"meetings": [
                {"meeting_id": "M1", "date": "2026-10-01"},
                {"meeting_id": "M2", "date": "2026-10-02"},
            ]}),
        );
        write(
            "proceedings/M1.json",
            json!({
                "meeting_id": "M1", "house": "sangiin", "committee": "厚生労働委員会", "issue": "第1号",
                "date": "2026-10-01",
                "speeches": [
                    {"speech_id": "M1_000", "order": 0, "speaker": "会議録情報", "speech": "予防接種法"},
                    {"speech_id": "M1_001", "order": 1, "speaker": "山田太郎", "speaker_group": "無所属",
                     "speech": "予防接種法の改正で副反応の救済を拡充すべきです。"}
                ]
            }),
        );
        write(
            "links/meeting-to-laws/M1.json",
            json!({"linked_laws": [
                {"law_id": "323AC0000000068", "title": "予防接種法", "match_reasons": ["予防接種法"]}
            ]}),
        );

        run_plan(&PlanArgs {
            base_url: public.display().to_string(),
            wiki: wiki.clone(),
            work: work.clone(),
            max_items: 5,
            max_probes: 10,
            lookback_days: 30,
            today: Some("2026-10-08".into()),
            max_bills: 0,
            max_pubcomments: 0,
            reiki_base_url: String::new(),
        })
        .unwrap();

        let plan = Plan::load(&work).unwrap().unwrap();
        assert_eq!(plan.tasks.len(), 1);
        assert_eq!(plan.tasks[0].page, "meetings/kokkai/M1.md");
        let meeting = Page::read(&wiki.join("meetings/kokkai/M1.md")).unwrap();
        assert_eq!(meeting.get_str("title"), "参議院 厚生労働委員会 第1号");
        assert_eq!(meeting.get("speakers").unwrap()[0]["name"], "山田太郎");
        assert!(meeting
            .body
            .contains("[予防接種法](../../laws/323AC0000000068.md)"));
        assert!(wiki.join("laws/323AC0000000068.md").exists());
        let state = State::load(&wiki).unwrap();
        assert_eq!(state.processed["kokkai:M2"].status, "no_link");
        assert!(
            !state.is_processed("kokkai:M1"),
            "linked はfinalize まで未処理のまま"
        );

        // 人物 (議事進行・会議録情報を除く) と会議体のページも作業対象に入る。
        let t = &plan.tasks[0];
        assert_eq!(t.people, vec!["people/山田太郎.md"]);
        assert_eq!(t.committee.as_deref(), Some("committees/参議院厚生労働委員会.md"));
        assert!(wiki.join("people/山田太郎.md").exists());
        assert!(wiki.join("committees/参議院厚生労働委員会.md").exists());

        // 処理済みでも経緯の形式が古い会議は、翌日の plan で再投入される。
        let mut state = State::load(&wiki).unwrap();
        state.mark("kokkai:M1", "linked", "2026-10-08");
        state.save(&wiki).unwrap();
        run_plan(&PlanArgs {
            base_url: public.display().to_string(),
            wiki: wiki.clone(),
            work: work.clone(),
            max_items: 5,
            max_probes: 10,
            lookback_days: 30,
            today: Some("2026-10-09".into()),
            max_bills: 0,
            max_pubcomments: 0,
            reiki_base_url: String::new(),
        })
        .unwrap();
        let again = Plan::load(&work).unwrap().unwrap();
        let ids: Vec<&str> = again.tasks.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["M1"]);
        std::fs::remove_dir_all(root).ok();
    }
}

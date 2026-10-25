//! `lawpub wiki-plan`: 今日 LLM に渡す仕事を決定的に決める。
//!
//! 1. 国会会議録・審議会議事録の index から、未処理かつ `lookback_days` 以内の会議を新しい順に並べる。
//! 2. 会議 → 法令リンク (`links/meeting-to-laws`, `links/shingikai-to-laws`) を引き、
//!    リンクが無い会議も政策形成の入口として、同じ件数上限内で読む。
//! 3. 会議本体を取得し、法令名を含む発言と前後の文脈、または会議全体のダイジェストを抜粋する。
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
    /// 1 回の plan で LLM に要約させる官報の項目 (改め文) の上限 (別枠)。
    pub max_kanpo: usize,
    /// 1 回の plan で LLM に全体像と経緯を書かせる人物の上限 (別枠)。
    pub max_people: usize,
    /// 1 回の plan で新しく作る発言者名簿の上限 (LLM とは無関係。会議 1 件あたり HTTP 数回)。
    pub max_roster: usize,
    /// 自治体例規の配信元 (R2 の `{public}/reiki`)。空なら例規の照合をしない。
    pub reiki_base_url: String,
    /// LLM に 1 回で渡すタスク数 (`.wiki-work/chunks/NNN.md` ごとに別々の実行にする)。
    pub chunk_size: usize,
    pub max_enforcements: usize,
    /// 導入後に取得された判例の要約上限（別枠）。0 でも新規 ID は保留する。
    pub max_hanrei: usize,
    pub max_hanrei_backfill: usize,
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
const QUALIFIERS: [&str; 8] = ["ただし", "但し", "一方", "しかし", "もっとも", "条件", "反対", "慎重"];
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
    /// 補った文脈: `preceding` / `reply` (前後の実質的発言)、`closing` (締め)、`turn` (審議会の発言)。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    /// この抜粋の前後関係を調べる起点の発言 ID。応答・因果関係の確定ではない。
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub context_for: Vec<String>,
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
        "wiki-plan: {} candidate meetings since {since}",
        candidates.len()
    );

    if args.work.exists() {
        std::fs::remove_dir_all(&args.work)?;
    }
    std::fs::create_dir_all(args.work.join("sources"))?;
    std::fs::create_dir_all(args.work.join("docs"))?;

    let mut tasks = Vec::new();
    let mut intake: BTreeMap<String, String> = candidates.iter()
        .map(|c| (format!("{}:{}", c.kind, c.id), "budget_pending".into())).collect();
    let mut probes = 0usize;
    let mut probe_order = state.meeting_probes.values().copied().max().unwrap_or(0);
    for c in candidates {
        if tasks.len() >= args.max_items || probes >= args.max_probes {
            break;
        }
        probes += 1;
        let key = format!("{}:{}", c.kind, c.id);
        probe_order += 1;
        state.meeting_probes.insert(key.clone(), probe_order);
        intake.insert(key.clone(), "source_unavailable".into());
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
        let fingerprint = meeting_fingerprint(&c, &doc, &laws, &units);
        let page_path = args.wiki.join(meeting_page(c.kind, &c.id));
        if page_path.exists() {
            let mut page = Page::read(&page_path)?;
            if state.is_processed(&key)
                && page.get_str("source_fingerprint") == fingerprint
                && page.get_str("llm_source_fingerprint") == fingerprint
                && page.get("llm_version").and_then(Value::as_u64).unwrap_or(0) >= NARRATIVE_VERSION
                && page.get("render_version").and_then(Value::as_u64) == Some(MEETING_RENDER_VERSION)
            {
                intake.insert(key, "source_unchanged".into());
                continue;
            }
            // 要約できる本文が無くなった訂正でも、以前の要約を最新版として残さない。
            page.set("source_fingerprint", json!(fingerprint));
            page.write(&page_path)?;
            state.mark(&key, "summary_pending", &today_s);
        }
        let matched = build_excerpts(c.kind, &units, &laws);
        // 法令名がまだ無い政策形成、議事進行にしか法令名が無い回も会議から取り込む。
        let excerpts = if c.kind == KIND_SHINGIKAI || matched.is_empty()
            || matched.iter().filter(|e| e.context.is_none()).all(is_procedural)
        {
            meeting_digest(c.kind, &units, &laws)
        } else {
            matched
        };
        if excerpts.is_empty() {
            intake.insert(key, "no_substantive_text".into());
            continue;
        }
        intake.insert(key, "planned".into());

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

    // 議案と改正履歴・官報 (LLM 不要) を取り込む。失敗しても会議のタスクは続ける。
    match structured::sync(&source, &args.wiki, today) {
        Ok(st) => println!(
            "wiki-plan: {} bill(s) ({} fetched), {} pubcomment(s), {} new law page(s), {} law(s) with revisions, {} law(s) with kanpo ({} new kanpo page(s))",
            st.bills, st.bills_fetched, st.pubcomments, st.laws_created, st.laws_with_revisions, st.laws_with_kanpo, st.kanpo_pages_created
        ),
        Err(e) => tracing::warn!("wiki-plan: 議案・改正履歴の取り込みに失敗: {e:#}"),
    }
    match plan_bills(args, &source) {
        Ok(bill_tasks) => tasks.extend(bill_tasks),
        Err(e) => tracing::warn!("wiki-plan: 議案の要約タスクの作成に失敗: {e:#}"),
    }
    // 発言者名簿 (人物・会議体ページの元) は LLM と切り離し、コーパスの全会議から作る。
    match super::roster::sync(&source, &args.wiki, args.max_roster) {
        Ok(st) => println!(
            "wiki-plan: roster {} built ({} with speakers), {} pending of {} meeting(s)",
            st.built, st.with_speakers, st.pending, st.total
        ),
        Err(e) => tracing::warn!("wiki-plan: 発言者名簿の更新に失敗: {e:#}"),
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
    match super::kanpo::plan_tasks(&args.wiki, &args.work, args.max_kanpo) {
        Ok(k_tasks) => tasks.extend(k_tasks),
        Err(e) => tracing::warn!("wiki-plan: 官報の要約タスクの作成に失敗: {e:#}"),
    }
    // 人物は名簿 (上で更新済み) から選ぶ。今日の会議タスクで経緯を書き足す人は翌日に回す。
    let busy: BTreeSet<String> = tasks.iter().flat_map(|t| t.people.iter().cloned()).collect();
    match super::person::plan_tasks(&source, &args.wiki, &args.work, args.max_people, &busy) {
        Ok(p_tasks) => tasks.extend(p_tasks),
        Err(e) => tracing::warn!("wiki-plan: 人物の要約タスクの作成に失敗: {e:#}"),
    }

    tasks.extend(enforcements::plan(args, &source, &today_s)?);
    tasks.extend(super::hanrei::plan_tasks_with_backfill(
        &source, &args.wiki, &args.work, args.max_hanrei, args.max_hanrei_backfill, args.max_probes, &today_s,
    )?);

    let before = tasks
        .iter()
        .map(|t| {
            let page = Page::read(&args.wiki.join(&t.page))?;
            Ok((t.page.clone(), llm_hash(&page.body)))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    std::fs::create_dir_all(args.wiki.join(".lawpub"))?;
    std::fs::write(args.wiki.join(".lawpub/intake.json"), serde_json::to_vec_pretty(&json!({
        "date": today_s, "since": since, "entries": intake,
        "scope": "日付窓内の未完了会議・処理済み会議の原資料再取得と旧形式ページ。取得・要約件数に上限があり、窓外の現行形式ページ・未収集資料は集計外。source_unchanged は取得時点の比較結果、planned は完了ではない。"
    }))?)?;
    state.save(&args.wiki)?;
    let plan = Plan {
        schema_version: 2,
        before,
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
    // LLM には数件ずつ別々に渡す (一度に何十件も渡すと途中で作業を切り上げてしまうため)。
    let chunks = args.work.join("chunks");
    std::fs::create_dir_all(&chunks)?;
    for (i, part) in plan.tasks.chunks(args.chunk_size.max(1)).enumerate() {
        let sub = Plan { tasks: part.to_vec(), ..plan.clone() };
        std::fs::write(chunks.join(format!("{:03}.md", i + 1)), render_plan_md(&sub, &args.wiki, &args.work))?;
    }
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
            if date >= since {
                out.push(Candidate {
                    kind: KIND_KOKKAI,
                    id: id.into(),
                    date: date.into(),
                    ministry: None,
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
            if date >= since {
                out.push(Candidate {
                    kind: KIND_SHINGIKAI,
                    id: id.into(),
                    date: date.into(),
                    ministry: m["ministry"].as_str().map(String::from),
                });
            }
        }
    }
    // 新規・未完了を先にし、処理済みは再取得した順が古いものから巡回する。
    out.sort_by(|a, b| {
        let ak = format!("{}:{}", a.kind, a.id);
        let bk = format!("{}:{}", b.kind, b.id);
        state.is_processed(&ak).cmp(&state.is_processed(&bk))
            .then_with(|| {
                if state.is_processed(&ak) {
                    state.meeting_probes.get(&ak).copied().unwrap_or(0)
                        .cmp(&state.meeting_probes.get(&bk).copied().unwrap_or(0))
                } else { std::cmp::Ordering::Equal }
            })
            .then_with(|| b.date.cmp(&a.date)).then_with(|| a.id.cmp(&b.id))
    });

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
        stale.push(Candidate { kind, id, date: page.get_str("date").to_string(), ministry });
    }
    stale.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.id.cmp(&b.id)));
    out.extend(stale);
    let mut seen = BTreeSet::new();
    out.retain(|c| seen.insert((c.kind, c.id.clone())));
    Ok(out)
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

/// 会議 → 法令リンクを (law_id, 題名, 照合パターン) に畳む。名簿 (`roster`) 用。
pub(crate) fn linked_law_entries(links: &Value) -> Vec<(String, String, Vec<String>)> {
    linked_laws(links)
        .into_iter()
        .map(|l| (l.law_id, l.title, l.patterns))
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
        // 法令名の近傍だけでは、後半の留保・結論を落とす。全文の最初の留保候補と末尾も足す。
        if let Some(byte_pos) = QUALIFIERS.iter().filter_map(|m| unit.text.find(m)).min() {
            let start = unit.text[..byte_pos].chars().count();
            spans.push((start.saturating_sub(60), (start + 240).min(chars.len())));
        }
        spans.push((chars.len().saturating_sub(300), chars.len()));
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
        let remaining = MAX_CHARS_PER_MEETING.saturating_sub(total);
        if remaining < 80 { break; }
        let text = context_text(&text, remaining);
        let len = text.chars().count();
        total += len;
        out.push(Excerpt {
            reference: unit.reference.clone(),
            url: unit.url.clone(),
            speaker: unit.speaker.clone(),
            group: unit.group.clone(),
            position: unit.position.clone(),
            laws: hit_laws,
            context: None,
            context_for: Vec::new(),
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
    is_procedural_text(e.speaker.as_deref(), &e.text)
}

pub(crate) fn is_procedural_text(speaker: Option<&str>, text: &str) -> bool {
    if speaker == Some(KOKKAI_HEADER_SPEAKER) {
        return true;
    }
    let head: String = text
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

/// 会議全体を上限付きで抜粋する。冒頭・留保候補・末尾を残し、最後の 2 発言に予算を予約する。
fn meeting_digest(kind: &str, units: &[Unit], laws: &[LinkedLaw]) -> Vec<Excerpt> {
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
        if kind == KIND_KOKKAI && is_procedural_text(u.speaker.as_deref(), &u.text) { continue; }
        let (text, context, laws) = if let Some(e) = matched.get(&u.reference) {
            (e.text.clone(), None, e.laws.clone())
        } else {
            let chars: Vec<char> = u.text.chars().collect();
            if chars.len() < 12 {
                continue;
            }
            let limit = if i >= closing_from { CONTEXT_CHARS } else { per_turn };
            (context_text(&u.text, limit), Some(if i >= closing_from { "closing" } else { "turn" }.to_string()), vec![])
        };
        let n = text.chars().count();
        if total + n > DIGEST_CHARS_PER_MEETING.saturating_sub(if i < closing_from { CONTEXT_CHARS * 2 } else { 0 }) {
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
            context_for: Vec::new(),
            text,
        });
    }
    out
}

/// 法令名を繰り返さない質問・答弁を、議事進行を飛ばして前後から補う。
/// 近接しているだけでは応答と確定できないため、起点の発言 ID を明示する。
/// 探索は片側 4 発言まで。別の議題への横断とソース量の増大を抑える。
pub(crate) fn with_context(kind: &str, units: &[Unit], matched: Vec<Excerpt>) -> Vec<Excerpt> {
    if matched.is_empty() {
        return matched;
    }
    let index: HashMap<&str, usize> = units.iter().enumerate().map(|(i, u)| (u.reference.as_str(), i)).collect();
    let included: BTreeSet<usize> = matched.iter().filter_map(|e| index.get(e.reference.as_str()).copied()).collect();
    let mut extra: BTreeMap<usize, (&str, BTreeSet<String>)> = BTreeMap::new();
    if kind == KIND_KOKKAI {
        for e in matched.iter().filter(|e| !is_procedural(e)) {
            let Some(&i) = index.get(e.reference.as_str()) else { continue };
            for (ctx, candidates) in [
                ("preceding", (i.saturating_sub(4)..i).rev().collect::<Vec<_>>()),
                ("reply", (i + 1..units.len().min(i + 5)).collect::<Vec<_>>()),
            ] {
                if let Some(j) = candidates.into_iter().find(|&j| {
                    !is_procedural_text(units[j].speaker.as_deref(), &units[j].text)
                }) {
                    extra.entry(j).or_insert_with(|| (ctx, BTreeSet::new())).1.insert(e.reference.clone());
                }
            }
        }
    } else {
        for i in units.len().saturating_sub(2)..units.len() {
            if !included.contains(&i) {
                extra.insert(i, ("closing", BTreeSet::new()));
            }
        }
    }
    let mut out: Vec<(usize, Excerpt)> = matched
        .into_iter()
        .map(|mut e| {
            let i = index.get(e.reference.as_str()).copied().unwrap_or(usize::MAX);
            if let Some((_, refs)) = extra.remove(&i) {
                e.context_for = refs.into_iter().collect();
            }
            (i, e)
        })
        .collect();
    let mut budget = MAX_CONTEXT_CHARS_PER_MEETING;
    for (i, (ctx, refs)) in extra {
        let u = &units[i];
        // 冒頭・留保候補・末尾を残す。全文確認の代わりにはならない。
        let text = context_text(&u.text, CONTEXT_CHARS);
        let n = text.chars().count();
        if n > budget {
            continue;
        }
        budget -= n;
        out.push((i, Excerpt {
            reference: u.reference.clone(),
            url: u.url.clone(),
            speaker: u.speaker.clone(),
            group: u.group.clone(),
            position: u.position.clone(),
            // 文脈から法令との直接のリンクを捏造しない。
            laws: vec![],
            context: Some(ctx.to_string()),
            context_for: refs.into_iter().collect(),
            text,
        }));
    }
    out.sort_by_key(|(i, _)| *i);
    out.into_iter().map(|(_, e)| e).collect()
}

/// 予算内で冒頭・末尾と最初の留保候補を保持する。候補が無いことは留保が無い証明ではない。
pub(crate) fn context_text(text: &str, limit: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= limit { return text.to_string(); }
    if limit < 6 { return chars.into_iter().take(limit).collect(); }
    let width = (limit - 3) / 3;
    let pivot = QUALIFIERS
        .iter().filter_map(|m| text.find(m)).min()
        .map(|i| text[..i].chars().count()).unwrap_or(width);
    let mut spans = vec![(0, width), (pivot.saturating_sub(width / 4), (pivot + width * 3 / 4).min(chars.len())), (chars.len() - width, chars.len())];
    spans.sort();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (a,b) in spans { if let Some(last) = merged.last_mut().filter(|last| a <= last.1) { last.1 = last.1.max(b); } else { merged.push((a,b)); } }
    merged.iter().map(|(a,b)| chars[*a..*b].iter().collect::<String>()).collect::<Vec<_>>().join("…")
}

/// 会議の表示名: (タイトル, 組織, 会議体, 原文 URL)。会議ページと名簿で同じ表記にする。
pub(crate) fn meeting_labels(kind: &str, id: &str, doc: &Value) -> (String, String, String, String) {
    match kind {
        KIND_KOKKAI => {
            let house = house_ja(doc["house"].as_str().unwrap_or(""));
            let committee = doc["committee"].as_str().unwrap_or("");
            let issue = doc["issue"].as_str().unwrap_or("");
            (
                format!("{house} {committee} {issue}").trim().to_string(),
                house.to_string(),
                committee.to_string(),
                format!("https://kokkai.ndl.go.jp/txt/{id}"),
            )
        }
        _ => {
            let ministry = ministry_ja(doc["ministry"].as_str().unwrap_or(""));
            (
                doc["title"].as_str().unwrap_or(id).to_string(),
                ministry.to_string(),
                doc["committee"].as_str().unwrap_or("").to_string(),
                doc["source"]["detail_url"].as_str().unwrap_or("").to_string(),
            )
        }
    }
}

fn house_ja(house: &str) -> &str {
    match house {
        "shugiin" => "衆議院",
        "sangiin" => "参議院",
        // 国家基本政策委員会合同審査会 (党首討論) などは両院合同。
        "both" => "両院",
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

/// 取得時刻等に影響されず、抜粋外を含む引用単位・帰属・議題・法令対応を比較する。
fn meeting_fingerprint(c: &Candidate, doc: &Value, laws: &[LinkedLaw], units: &[Unit]) -> String {
    let labels = meeting_labels(c.kind, &c.id, doc);
    let laws: BTreeMap<_, _> = laws.iter().map(|l| (&l.law_id, (&l.title, &l.patterns))).collect();
    super::report::revision(&json!({"version": 1, "kind": c.kind, "date": c.date,
        "labels": labels, "agenda": doc["agenda"], "units": units, "laws": laws}))
}

fn write_task(
    args: &PlanArgs,
    source: &Source,
    c: &Candidate,
    doc: &Value,
    laws: &[LinkedLaw],
    excerpts: &[Excerpt],
) -> Result<Task> {
    let units = if c.kind == KIND_KOKKAI { kokkai_units(doc) } else { shingikai_units(doc) };
    let fingerprint = meeting_fingerprint(c, doc, laws, &units);
    // 実際に抜粋に現れた法令だけをページ化する (リンク判定はタイトル一致なので広め)。
    let cited: BTreeSet<&str> = excerpts
        .iter()
        .flat_map(|e| e.laws.iter().map(String::as_str))
        .collect();
    let laws: Vec<&LinkedLaw> = laws
        .iter()
        .filter(|l| cited.contains(l.law_id.as_str()))
        .collect();

    let (title, organization, committee, resource) = meeting_labels(c.kind, &c.id, doc);

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
    let scope = super::evidence::source_scope(&units, excerpts);
    // 以前の主張との対応を保持する。本文の更新と claims の構造検査を満たすまで完了にはしない。

    let existing_claims = meeting.get("claims").cloned().unwrap_or(json!([]));
    let previous_fingerprint = meeting.get("llm_source_fingerprint").cloned().unwrap_or(json!(""));
    let llm_version = meeting.get("llm_version").cloned().unwrap_or(json!(0));
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
        ("evidence_version".into(), json!(1)),
        ("source_fingerprint".into(), json!(fingerprint)),
        ("llm_source_fingerprint".into(), previous_fingerprint.clone()),
        ("llm_version".into(), llm_version),
        ("source_scope".into(), scope.clone()),
        ("claims".into(), existing_claims),
        ("evidence_sources".into(), json!(units.iter().filter(|u| excerpts.iter().any(|e| e.reference == u.reference))
            .map(|u| super::evidence::attribution(u, &c.date)).collect::<Vec<_>>())),
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
        "scope": scope,
        "attributions": units.iter().filter(|u| excerpts.iter().any(|e| e.reference == u.reference))
            .map(|u| super::evidence::attribution(u, &c.date)).collect::<Vec<_>>(),
        "source_fingerprint": fingerprint,
        "previous_summary_fingerprint": previous_fingerprint,
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
        person: None,
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
            person: None,
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
            "kanpo" => "官報ページ",
            "person" => "人物ページ",
            "enforcement" => "施行予定解説ページ",
            "hanrei" => "判例ページ",
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
        assert!(out[0].text.starts_with('…'));
        assert!(out[0].text.contains("予防接種法の改正について伺います。"));
        assert!(out[0].text.ends_with(&"あ".repeat(300)), "発言の末尾も残す");
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
            context_for: Vec::new(),
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
    fn context_skips_chair_turns_and_links_the_question_to_the_answer() {
        let mut units = vec![
            unit("k:1", "○山田委員　予防接種法の対象拡充はいつ判断するのですか。"),
            unit("k:2", "○田中委員長　佐藤大臣。"),
            unit("k:3", "○田中委員長　どうぞ。"),
            unit("k:4", "○佐藤大臣　専門家会議で年度内に検討します。"),
        ];
        units[0].speaker = Some("山田".into());
        units[3].speaker = Some("佐藤".into());
        units[3].position = Some("厚生労働大臣".into());
        let laws = vec![("L1".into(), vec!["予防接種法".into()])];
        let out = with_context(KIND_KOKKAI, &units, build_excerpts_for(&units, &laws));
        assert_eq!(out.iter().map(|e| e.reference.as_str()).collect::<Vec<_>>(), vec!["k:1", "k:4"]);
        assert_eq!(out[1].context.as_deref(), Some("reply"));
        assert_eq!(out[1].context_for, vec!["k:1"]);
        assert_eq!(out[1].position.as_deref(), Some("厚生労働大臣"));
        assert!(out[1].laws.is_empty(), "近接だけで法令との直接リンクを作らない");
        assert_eq!(serde_json::to_value(&out[1]).unwrap()["context_for"], json!(["k:1"]));
    }

    #[test]
    fn a_matching_answer_keeps_its_preceding_question_without_duplicating_it() {
        let units = vec![
            unit("k:1", "○山田委員　対象を拡充すべきではないですか。"),
            unit("k:2", "○田中委員長　佐藤大臣。"),
            unit("k:3", "○佐藤大臣　予防接種法の対象拡充は専門家会議で検討します。"),
        ];
        let laws = vec![("L1".into(), vec!["予防接種法".into()])];
        let out = with_context(KIND_KOKKAI, &units, build_excerpts_for(&units, &laws));
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].reference, "k:1");
        assert_eq!(out[0].context.as_deref(), Some("preceding"));
        assert_eq!(out[0].context_for, vec!["k:3"]);

        let laws = vec![("L1".into(), vec!["拡充".into()])];
        let out = with_context(KIND_KOKKAI, &units, build_excerpts_for(&units, &laws));
        assert_eq!(out.len(), 2, "両方が法令言及に当たっても重複させない");
        assert_eq!(out[0].context_for, vec!["k:3"]);
        assert_eq!(out[1].context_for, vec!["k:1"]);
        assert_eq!(out[1].laws, vec!["L1"]);
    }

    #[test]
    fn context_search_and_character_budget_are_bounded() {
        let mut units = vec![unit("k:0", "○山田委員　予防接種法について伺います。")];
        for _ in 0..4 {
            units.push(unit("chair", "○田中委員長　暫時休憩します。"));
        }
        units.push(unit("k:5", "○佐藤大臣　検討します。"));
        let laws = vec![("L1".into(), vec!["予防接種法".into()])];
        let out = with_context(KIND_KOKKAI, &units, build_excerpts_for(&units, &laws));
        assert_eq!(out.len(), 1, "遠くの発言まで探索しない");

        let mut units = Vec::new();
        for n in 0..12 {
            units.push(unit(&format!("q:{n}"), "○山田委員　予防接種法について伺います。"));
            units.push(unit(&format!("a:{n}"), &format!("○佐藤大臣　{}", "検討中です。".repeat(150))));
        }
        let out = with_context(KIND_KOKKAI, &units, build_excerpts_for(&units, &laws));
        let chars: usize = out.iter().filter(|e| e.context.is_some()).map(|e| e.text.chars().count()).sum();
        assert!(chars > 0 && chars <= MAX_CONTEXT_CHARS_PER_MEETING);
    }

    #[test]
    fn long_law_matched_answer_keeps_late_qualification_and_conclusion_within_budget() {
        let text = format!("○佐藤大臣　予防接種法について説明します。{}ただし財源の確保が条件です。{}実施時期は未定です。", "背景説明。".repeat(300), "検討過程。".repeat(300));
        let out = build_excerpts_for(&[unit("k:1", &text)], &[("L1".into(), vec!["予防接種法".into()])]);
        assert_eq!(out.len(), 1);
        for fragment in ["予防接種法", "ただし財源の確保が条件です", "実施時期は未定です"] {
            assert!(out[0].text.contains(fragment));
        }
        assert!(out[0].text.chars().count() <= MAX_CHARS_PER_MEETING);
        for fragment in out[0].text.split('…').filter(|s| !s.is_empty()) {
            assert!(text.contains(fragment), "原文の連続した部分だけを使う");
        }
        // 大量の法令言及があっても、最初の発言だけで予算を超過しない。
        let repeated = unit("k:2", &"予防接種法に言及する非常に長い説明。".repeat(2_000));
        let out = build_excerpts_for(&[repeated], &[("L1".into(), vec!["予防接種法".into()])]);
        assert!(out.iter().map(|e| e.text.chars().count()).sum::<usize>() <= MAX_CHARS_PER_MEETING);
    }

    #[test]
    fn policy_meeting_without_laws_keeps_scope_provenance_and_pending_work() {
        let root = temp_dir("policy_intake");
        let public = root.join("public");
        let wiki = root.join("wiki");
        let work = root.join("work");
        let write = |rel: &str, v: Value| {
            let path = public.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, serde_json::to_vec(&v).unwrap()).unwrap();
        };
        write("proceedings/index.json", json!({"meetings": [
            {"meeting_id": "M2", "date": "2026-10-04"},
            {"meeting_id": "M3", "date": "2026-10-03"},
            {"meeting_id": "M1", "date": "2026-10-02"},
            {"meeting_id": "M4", "date": "2026-10-01"}
        ]}));
        write("proceedings/M1.json", json!({"meeting_id": "M1", "house": "shugiin", "committee": "政策委員会", "issue": "第1号", "speeches": [
            {"speech_id": "M1_001", "order": 1, "speaker": "山田", "speech": "○山田委員　新しい政策の実施時期について伺います。"},
            {"speech_id": "M1_002", "order": 2, "speaker": "佐藤", "speaker_position": "国務大臣", "speech": format!("○佐藤大臣　検討しています。{}ただし財源の確保が条件です。{}実施時期は未定です。", "背景説明。".repeat(200), "検討過程。".repeat(200))},
            {"speech_id": "M1_003", "order": 3, "speaker": "田中", "speech": "○田中委員長　本日はこれにて散会いたします。"}
        ]}));
        write("proceedings/M3.json", json!({"meeting_id": "M3", "speeches": [
            {"speech_id": "M3_001", "speaker": "田中", "speech": "○田中委員長　本日はこれにて散会いたします。"}
        ]}));
        let mut state = State::default();
        state.mark("kokkai:M1", "no_link", "2026-10-02");
        state.mark("kokkai:M3", "formal_only", "2026-10-03");
        state.save(&wiki).unwrap();
        let args = PlanArgs { base_url: public.display().to_string(), wiki: wiki.clone(), work: work.clone(),
            max_items: 1, max_probes: 3, lookback_days: 30, today: Some("2026-10-08".into()),
            max_bills: 0, max_enforcements: 0, max_hanrei: 0, max_hanrei_backfill: 0, max_pubcomments: 0, max_kanpo: 0,
            max_people: 0, max_roster: 0, reiki_base_url: String::new(), chunk_size: 4 };
        run_plan(&args).unwrap();
        let intake = || -> Value { serde_json::from_slice(&std::fs::read(wiki.join(".lawpub/intake.json")).unwrap()).unwrap() };
        assert_eq!(intake()["entries"], json!({"kokkai:M1": "planned", "kokkai:M2": "source_unavailable", "kokkai:M3": "no_substantive_text", "kokkai:M4": "budget_pending"}));
        let plan = Plan::load(&work).unwrap().unwrap();
        assert_eq!(plan.tasks.len(), 1);
        assert!(plan.tasks[0].laws.is_empty(), "法令を捏造しない");
        let bundle: Value = serde_json::from_slice(&std::fs::read(work.join(&plan.tasks[0].source)).unwrap()).unwrap();
        assert_eq!(bundle["scope"]["total_units"], 3);
        assert_eq!(bundle["scope"]["included_units"], 2);
        assert_eq!(bundle["scope"]["partial_refs"], json!(["kokkai:M1_002"]));
        assert_eq!(bundle["scope"]["omitted_refs"], json!(["kokkai:M1_003"]));
        let answer = bundle["excerpts"][1]["text"].as_str().unwrap();
        assert!(answer.contains("ただし財源の確保が条件です"));
        assert!(answer.contains("実施時期は未定です"));
        assert!(answer.chars().count() <= CONTEXT_CHARS);
        assert_eq!(bundle["attributions"][1]["role"], "government");

        // 引用が存在しても、主張との対応が無ければ処理済みにしない。
        let path = wiki.join(&plan.tasks[0].page);
        let summarize = |claims: bool| {
            let mut page = Page::read(&path).unwrap();
            page.set("description", json!("政府は財源確保を条件に検討し、実施時期は未定とした。"));
            page.body = page.body.replace(&format!("{LLM_BEGIN}\n{LLM_END}"), &format!("{LLM_BEGIN}\n政府は財源確保を条件に検討し、実施時期は未定とした[^1]。\n\n[^1]: [kokkai:M1_002](https://kokkai.ndl.go.jp/txt/M1/2) 「ただし財源の確保が条件です」\n{LLM_END}"));
            if claims { page.set("claims", json!([{"text": "政府は財源確保を条件に検討し、実施時期は未定とした", "kind": "government_view", "attribution": "kokkai:M1_002", "evidence": ["1"], "review_status": "needs_review", "review_note": "条件の引用はあるが時期未定の根拠も追加確認する。"}])); }
            page.write(&path).unwrap();
        };
        summarize(false);
        let finalize = super::super::finalize::FinalizeArgs { wiki: wiki.clone(), work: work.clone() };
        super::super::finalize::run_finalize(&finalize).unwrap();
        assert!(!State::load(&wiki).unwrap().is_processed("kokkai:M1"));
        assert_eq!(intake()["entries"]["kokkai:M1"], "summary_pending");
        assert!(!path.exists());
        run_plan(&args).unwrap();
        summarize(true);
        super::super::finalize::run_finalize(&finalize).unwrap();
        assert!(State::load(&wiki).unwrap().is_processed("kokkai:M1"));
        assert_eq!(intake()["entries"]["kokkai:M1"], "completed");
        assert!(!State::load(&wiki).unwrap().is_processed("kokkai:M3"));
        let check = super::super::check::CheckArgs { wiki: wiki.clone(), work: work.clone(), base_url: public.display().to_string(), changed_only: false };
        super::super::check::run_check(&check).unwrap();
        let good_page = Page::read(&path).unwrap();
        for field in ["date", "source_scope", "evidence_sources"] {
            let mut changed = good_page.clone();
            changed.set(field, json!("改変"));
            changed.write(&path).unwrap();
            assert!(super::super::check::run_check(&check).is_err());
            let report = std::fs::read_to_string(work.join("check-report.md")).unwrap();
            assert!(report.contains(&format!("機械生成の {field} がソースバンドルと一致しません")));
        }
        good_page.write(&path).unwrap();
        super::super::check::run_check(&check).unwrap();
        let out = root.join("export");
        super::super::export::run_export(&super::super::export::ExportArgs { wiki: wiki.clone(), out: out.clone() }).unwrap();
        let exported: Value = serde_json::from_slice(&std::fs::read(out.join("page/meetings/kokkai/M1.json")).unwrap()).unwrap();
        assert_eq!(exported["claim_assessments"][0]["review_status"], "needs_review");
        assert_eq!(exported["claim_assessments"][0]["sources"][0]["date"], "2026-10-02");
        let index: Value = serde_json::from_slice(&std::fs::read(out.join("index.json")).unwrap()).unwrap();
        assert_eq!(index["intake"]["entries"]["kokkai:M4"], "budget_pending");

        // 処理済みでも日付窓内は再取得する。取得時刻だけの変更ではページの版を動かさない。
        write("proceedings/index.json", json!({"meetings": [{"meeting_id": "M1", "date": "2026-10-02"}]}));
        let mut doc: Value = serde_json::from_slice(&std::fs::read(public.join("proceedings/M1.json")).unwrap()).unwrap();
        doc["fetched_at"] = json!("2026-10-09T00:00:00Z");
        write("proceedings/M1.json", doc.clone());
        let original = std::fs::read(&path).unwrap();
        run_plan(&args).unwrap();
        assert!(Plan::load(&work).unwrap().unwrap().tasks.is_empty());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert_eq!(intake()["entries"]["kokkai:M1"], "source_unchanged");

        // 以前は未収録だった発言の訂正も検出し、旧判定と新しい話者情報を混ぜない。
        doc["speeches"][2]["speech"] = json!("○田中委員長　実施に当たっては慎重に条件を検討し、次回に改めて議論を続けます。");
        write("proceedings/M1.json", doc.clone());
        run_plan(&args).unwrap();
        let refreshed = Plan::load(&work).unwrap().unwrap();
        assert_eq!(refreshed.tasks.len(), 1);
        assert!(refreshed.tasks[0].created.is_empty());
        let pending = Page::read(&path).unwrap();
        assert_ne!(pending.get("source_fingerprint"), pending.get("llm_source_fingerprint"));
        let assessment = super::super::evidence::assessments(&pending);
        assert_eq!(assessment[0]["review_status"], "indeterminate");
        assert_eq!(assessment[0]["sources"], json!([]));
        super::super::finalize::run_finalize(&finalize).unwrap();
        assert!(path.exists(), "未更新の既存ページは残す");
        assert!(!State::load(&wiki).unwrap().is_processed("kokkai:M1"));
        assert_eq!(intake()["entries"]["kokkai:M1"], "summary_pending");
        assert!(std::fs::read_to_string(wiki.join(refreshed.tasks[0].committee.as_ref().unwrap())).unwrap().contains("原資料更新・要再確認"));

        // 完了マーカーを LLM が先に進めても受理しない。正しい再要約後は繰り返し finalize できる。
        let mut updated = pending.clone();
        updated.body = updated.body.replace("政府は財源確保を条件に検討し、実施時期は未定とした", "政府は実施時期は未定と説明した");
        updated.set("claims", json!([{"text": "政府は実施時期は未定と説明した", "kind": "government_view", "attribution": "kokkai:M1_002", "evidence": ["1"], "review_status": "needs_review", "review_note": "条件と時期の関係は追加確認が必要。"}]));
        let mut forged = updated.clone();
        forged.set("llm_source_fingerprint", forged.get("source_fingerprint").cloned().unwrap());
        forged.write(&path).unwrap();
        assert!(super::super::check::run_check(&check).is_err());
        super::super::finalize::run_finalize(&finalize).unwrap();
        assert!(!State::load(&wiki).unwrap().is_processed("kokkai:M1"));
        updated.write(&path).unwrap();
        super::super::finalize::run_finalize(&finalize).unwrap();
        super::super::finalize::run_finalize(&finalize).unwrap();
        let current = Page::read(&path).unwrap();
        assert_eq!(super::super::evidence::summary_source_status(&current), "current");
        assert!(State::load(&wiki).unwrap().is_processed("kokkai:M1"));
        super::super::check::run_check(&check).unwrap();

        // 原資料の本文が無くなった場合も、旧要約を処理済みのままにしない。
        doc["speeches"] = json!([]);
        write("proceedings/M1.json", doc);
        run_plan(&args).unwrap();
        assert!(Plan::load(&work).unwrap().unwrap().tasks.is_empty());
        assert_eq!(intake()["entries"]["kokkai:M1"], "no_substantive_text");
        assert!(!State::load(&wiki).unwrap().is_processed("kokkai:M1"));
        assert_eq!(super::super::evidence::summary_source_status(&Page::read(&path).unwrap()), "stale");
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn processed_meeting_probes_rotate_within_budget_and_ignore_outside_window() {
        let root = temp_dir("meeting_probe_rotation");
        let public = root.join("public");
        let wiki = root.join("wiki");
        let work = root.join("work");
        std::fs::create_dir_all(public.join("proceedings")).unwrap();
        std::fs::write(public.join("proceedings/index.json"), serde_json::to_vec(&json!({"meetings": [
            {"meeting_id": "M1", "date": "2026-10-03"},
            {"meeting_id": "M2", "date": "2026-10-02"},
            {"meeting_id": "M0", "date": "2025-10-01"}
        ]})).unwrap()).unwrap();
        let mut state = State::default();
        for id in ["M1", "M2", "M0"] { state.mark(&format!("kokkai:{id}"), "linked", "2026-10-03"); }
        state.save(&wiki).unwrap();
        let args = PlanArgs { base_url: public.display().to_string(), wiki: wiki.clone(), work: work.clone(),
            max_items: 1, max_probes: 1, lookback_days: 30, today: Some("2026-10-08".into()),
            max_bills: 0, max_enforcements: 0, max_hanrei: 0, max_hanrei_backfill: 0, max_pubcomments: 0, max_kanpo: 0,
            max_people: 0, max_roster: 0, reiki_base_url: String::new(), chunk_size: 4 };
        // 取得失敗も巡回順を進める。欠けた資料を毎回先頭で試すことを避ける。
        run_plan(&args).unwrap();
        let first = State::load(&wiki).unwrap();
        assert_eq!(first.meeting_probes.keys().collect::<Vec<_>>(), vec!["kokkai:M1"]);
        run_plan(&args).unwrap();
        let second = State::load(&wiki).unwrap();
        assert_eq!(second.meeting_probes["kokkai:M2"], 2);
        assert!(!second.meeting_probes.contains_key("kokkai:M0"));
        let intake: Value = serde_json::from_slice(&std::fs::read(wiki.join(".lawpub/intake.json")).unwrap()).unwrap();
        assert_eq!(intake["entries"]["kokkai:M1"], "budget_pending");
        assert_eq!(intake["entries"]["kokkai:M2"], "source_unavailable");
        let mut disabled = args;
        disabled.max_probes = 0;
        run_plan(&disabled).unwrap();
        assert_eq!(State::load(&wiki).unwrap().meeting_probes, second.meeting_probes);
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn meeting_fingerprint_tracks_attribution_agenda_and_law_mapping_in_both_corpora() {
        for kind in [KIND_KOKKAI, KIND_SHINGIKAI] {
            let c = Candidate { kind, id: "M1".into(), date: "2026-10-02".into(), ministry: None };
            let doc = json!({"agenda": ["制度の見直し"]});
            let units = vec![unit("ref:1", "政策を検討します。ただし実施は未定です。")];
            let original = meeting_fingerprint(&c, &doc, &[], &units);
            let mut changed = units.clone();
            changed[0].speaker = Some("別の発言者".into());
            assert_ne!(original, meeting_fingerprint(&c, &doc, &[], &changed));
            changed = units.clone();
            changed[0].position = Some("参考人".into());
            assert_ne!(original, meeting_fingerprint(&c, &doc, &[], &changed));
            changed = units.clone();
            changed[0].text.push_str("反対意見もあります。");
            assert_ne!(original, meeting_fingerprint(&c, &doc, &[], &changed));
            assert_ne!(original, meeting_fingerprint(&c, &json!({"agenda": ["決定事項"]}), &[], &units));
            let laws = linked_laws(&json!({"linked_laws": [{"law_id": "L1", "title": "予防接種法"}]}));
            assert_ne!(original, meeting_fingerprint(&c, &doc, &laws, &units));
        }
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
            max_enforcements: 0, max_hanrei: 0, max_hanrei_backfill: 0,
            max_pubcomments: 0,
            max_kanpo: 0,
            max_people: 0,
            max_roster: 100,
            reiki_base_url: String::new(),
            chunk_size: 4,
        })
        .unwrap();
        let plan = Plan::load(&work).unwrap().unwrap();
        assert_eq!(plan.tasks.len(), 1, "法令リンクが無くても審議会は要点化する");
        let bundle: Value = serde_json::from_slice(&std::fs::read(work.join(&plan.tasks[0].source)).unwrap()).unwrap();
        assert_eq!(bundle["agenda"], "新規の疾病追加について");
        let ex = bundle["excerpts"].as_array().unwrap();
        let speakers: Vec<&str> = ex.iter().filter_map(|e| e["speaker"].as_str()).collect();
        assert_eq!(speakers, vec!["持田委員長", "西垣補佐", "山田委員", "持田委員長"]);
        // 長い説明は予算内、締めは全文 (短い) が入る。
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
            context_for: Vec::new(),
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
            max_enforcements: 0, max_hanrei: 0, max_hanrei_backfill: 0,
            max_pubcomments: 0,
            max_kanpo: 0,
            max_people: 0,
            max_roster: 100,
            reiki_base_url: String::new(),
            chunk_size: 4,
        })
        .unwrap();

        let plan = Plan::load(&work).unwrap().unwrap();
        assert_eq!(plan.tasks.len(), 1);
        assert_eq!(plan.tasks[0].page, "meetings/kokkai/M1.md");
        // LLM に渡す単位 (チャンク) にも同じタスクが書き出される。
        let chunk = std::fs::read_to_string(work.join("chunks/001.md")).unwrap();
        assert!(chunk.contains("meetings/kokkai/M1.md"));
        let meeting = Page::read(&wiki.join("meetings/kokkai/M1.md")).unwrap();
        assert_eq!(meeting.get_str("title"), "参議院 厚生労働委員会 第1号");
        assert_eq!(meeting.get("speakers").unwrap()[0]["name"], "山田太郎");
        assert!(meeting
            .body
            .contains("[予防接種法](../../laws/323AC0000000068.md)"));
        assert!(wiki.join("laws/323AC0000000068.md").exists());
        let state = State::load(&wiki).unwrap();
        assert!(!state.is_processed("kokkai:M2"));
        assert!(
            !state.is_processed("kokkai:M1"),
            "linked はfinalize まで未処理のまま"
        );

        // 人物 (議事進行・会議録情報を除く) と会議体のページも作業対象に入る。
        let t = &plan.tasks[0];
        assert_eq!(plan.schema_version, 2);
        assert_eq!(plan.before.get(&t.page), Some(&llm_hash(&meeting.body)));
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
            max_enforcements: 0, max_hanrei: 0, max_hanrei_backfill: 0,
            max_pubcomments: 0,
            max_kanpo: 0,
            max_people: 0,
            max_roster: 100,
            reiki_base_url: String::new(),
            chunk_size: 4,
        })
        .unwrap();
        let again = Plan::load(&work).unwrap().unwrap();
        let ids: Vec<&str> = again.tasks.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["M1"]);
        std::fs::remove_dir_all(root).ok();
    }
}

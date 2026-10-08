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
use std::collections::BTreeSet;

pub struct PlanArgs {
    pub base_url: String,
    pub wiki: PathBuf,
    pub work: PathBuf,
    pub max_items: usize,
    pub max_probes: usize,
    pub lookback_days: i64,
    pub today: Option<String>,
}

/// 1 発言から抜き出す文脈幅 (文字数)。
const WINDOW_BEFORE: usize = 200;
const WINDOW_AFTER: usize = 500;
/// 1 会議あたりのソースバンドルの上限。
const MAX_EXCERPTS_PER_MEETING: usize = 12;
const MAX_CHARS_PER_MEETING: usize = 8_000;

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
    let candidates = collect_candidates(&source, &state, &since)?;
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
        let links = match source.get_json(&link_path) {
            Ok(Some(v)) => v,
            Ok(None) => {
                state.mark(&key, "no_link", &today_s);
                continue;
            }
            Err(e) => {
                tracing::warn!("wiki-plan: {key}: {e:#} — 次回に再試行");
                continue;
            }
        };
        let laws = linked_laws(&links);
        if laws.is_empty() {
            state.mark(&key, "no_link", &today_s);
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
        let excerpts = build_excerpts(&units, &laws);
        // 会議録情報 (付議案件の一覧) と委員長・議長の議事進行にしか法令名が出ない会議は、
        // LLM に渡しても「形式的言及」にしかならないので渡さない (トークン節約)。
        if c.kind == KIND_KOKKAI && !excerpts.is_empty() && excerpts.iter().all(is_procedural) {
            state.mark(&key, "formal_only", &today_s);
            continue;
        }
        if excerpts.is_empty() {
            // 法令名が添付資料にだけ現れる等、発言として引用できる言及が無い。
            state.mark(&key, "no_excerpt", &today_s);
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

    // 議案と改正履歴 (LLM 不要) を取り込む。失敗しても会議のタスクは続ける。
    match structured::sync(&source, &args.wiki, today) {
        Ok(st) => println!(
            "wiki-plan: {} bill(s) ({} fetched), {} new law page(s), {} law(s) with revisions",
            st.bills, st.bills_fetched, st.laws_created, st.laws_with_revisions
        ),
        Err(e) => tracing::warn!("wiki-plan: 議案・改正履歴の取り込みに失敗: {e:#}"),
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

fn collect_candidates(source: &Source, state: &State, since: &str) -> Result<Vec<Candidate>> {
    let mut out = Vec::new();
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
                });
            }
        }
    }
    if let Some(index) = source.get_json("shingikai/index.json")? {
        for m in index["minutes"].as_array().into_iter().flatten() {
            let (Some(id), Some(date)) = (m["minutes_id"].as_str(), m["date"].as_str()) else {
                continue;
            };
            // 議事録が公開されるまでは引用元が無い。公開後の run で拾う。
            if !m["has_minutes"].as_bool().unwrap_or(false) {
                continue;
            }
            if date >= since && !state.is_processed(&format!("{KIND_SHINGIKAI}:{id}")) {
                out.push(Candidate {
                    kind: KIND_SHINGIKAI,
                    id: id.into(),
                    date: date.into(),
                    ministry: m["ministry"].as_str().map(String::from),
                });
            }
        }
    }
    out.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.id.cmp(&b.id)));
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
            text,
        });
        if out.len() >= MAX_EXCERPTS_PER_MEETING {
            break;
        }
    }
    out
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

fn build_excerpts(units: &[Unit], laws: &[LinkedLaw]) -> Vec<Excerpt> {
    let pairs: Vec<(String, Vec<String>)> = laws
        .iter()
        .map(|l| (l.law_id.clone(), l.patterns.clone()))
        .collect();
    build_excerpts_for(units, &pairs)
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
    let mut speakers: BTreeMap<String, (Option<String>, BTreeSet<String>)> = BTreeMap::new();
    if c.kind == KIND_KOKKAI {
        for e in excerpts {
            let Some(name) = e.speaker.as_deref().filter(|n| *n != KOKKAI_HEADER_SPEAKER) else {
                continue;
            };
            let entry = speakers
                .entry(name.to_string())
                .or_insert((e.group.clone(), BTreeSet::new()));
            entry.1.extend(e.laws.iter().cloned());
        }
    }
    let speakers_json: Vec<Value> = speakers
        .iter()
        .map(|(name, (group, laws))| json!({"name": name, "group": group, "laws": laws}))
        .collect();
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
        ("speakers".into(), Value::Array(speakers_json)),
        ("tags".into(), tags),
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
        task_laws.push(TaskLaw {
            law_id: law.law_id.clone(),
            title: law.title.clone(),
            page: lp,
        });
    }

    let source_rel = format!("sources/{}_{}.json", c.kind, file_safe(&c.id));
    let bundle = json!({
        "kind": c.kind,
        "id": c.id,
        "title": title,
        "date": c.date,
        "organization": organization,
        "committee": committee,
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
        created,
    })
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
        out.push_str(&format!(
            "- 会議ページ: `{wiki}/{}`{}\n",
            t.page,
            new(&t.page)
        ));
        for l in &t.laws {
            out.push_str(&format!(
                "- 法令ページ: `{wiki}/{}` {}{}\n",
                l.page,
                l.title,
                new(&l.page)
            ));
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
    fn procedural_speeches_are_detected() {
        let ex = |speaker: &str, text: &str| Excerpt {
            reference: "r".into(),
            url: "u".into(),
            speaker: Some(speaker.into()),
            group: None,
            position: None,
            laws: vec![],
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
        std::fs::remove_dir_all(root).ok();
    }
}

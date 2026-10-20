//! 人物タスク: 人物ページの「全体像」と「発言と経緯」を、その人の発言だけを集めたバンドルから LLM に書かせる。
//!
//! 会議タスクは 1 日数件で、窓 (`lookback_days`) の中の会議しか扱わないので、人物ページの経緯は
//! ほとんど埋まらない (名簿で人物ページは 1,000 件近くあるのに、経緯が付くのは会議タスクに当たった人だけ)。
//! そこで発言者名簿 (`.lawpub/roster`) から人物ごとに未要約の会議を集め、1 日数人ずつ渡す。
//!
//! - 対象: 名簿の発言者のうち審議会の委員 (姓だけで同定できない) を除いた全員。ページの `llm_until`
//!   (要約済みの最後の会議の日付) より後の会議が残っている人を、残りの会議が多い順 → 発言した会議が
//!   多い順に選ぶ (よく発言する人から埋まり、新しい会議があれば書き足される)。
//! - 1 人あたり古い順に最大 `MAX_MEETINGS_PER_PERSON` 会議・`MAX_CHARS_PER_PERSON` 字まで渡し、
//!   残りは翌日以降に回す。同じ日の会議は分けない (`llm_until` を日付で進めるため)。
//! - 抜粋は、その人の発言のうち法令に言及したものと、その直前・直後の発言 (答えた質問・受けた答弁)。
//! - 完了の判定は、LLM 区間が書き足され、引用が 1 つ以上あること (`PersonTask::before`)。
//!   完了したら finalize がページの `llm_until` を進める。

use super::plan::{build_excerpts_for, is_procedural, linked_law_entries, Excerpt};
use super::roster::RosterMeeting;
use super::*;
use serde_json::json;
use std::collections::{BTreeSet, HashMap};

pub const KIND_PERSON: &str = "person";

/// 1 人あたりに渡す会議の上限と、抜粋全体の文字数の上限。
const MAX_MEETINGS_PER_PERSON: usize = 12;
const MAX_CHARS_PER_PERSON: usize = 24_000;
/// 1 会議あたりの、その人の発言の抜粋の上限 (件数・文字数)。
const MAX_OWN_PER_MEETING: usize = 4;
const MAX_OWN_CHARS_PER_MEETING: usize = 2_400;
/// 1 タスクあたりに調べる人数の上限 (候補を順に見て、抜粋が取れた人をタスクにする)。
const MAX_PROBES_PER_TASK: usize = 5;
/// 直前・直後の発言 (質問・答弁) として足す長さ。
const CONTEXT_CHARS: usize = 400;

/// 人物ごとの、名簿に載っている会議 (新しい順ではなく古い順)。
struct Candidate<'a> {
    name: String,
    meetings: Vec<&'a RosterMeeting>,
    /// `llm_until` より後の会議の数。
    pending: usize,
    until: Option<String>,
}

/// 未要約の会議が残っている人物を最大 `max` 人タスクにする。`skip` は今日の会議タスクで
/// 経緯を書き足す人物ページ (同じ会議を二重に書かないよう、その人は翌日以降に回す)。
pub fn plan_tasks(source: &Source, wiki: &Path, work: &Path, max: usize, skip: &BTreeSet<String>) -> Result<Vec<Task>> {
    if max == 0 {
        return Ok(Vec::new());
    }
    let roster = super::roster::load(wiki);
    let mut by_name: BTreeMap<String, Vec<&RosterMeeting>> = BTreeMap::new();
    for m in &roster {
        for sp in m.speakers.iter().filter(|sp| sp.role != "member") {
            by_name.entry(sp.name.clone()).or_default().push(m);
        }
    }
    let mut candidates: Vec<Candidate> = Vec::new();
    for (name, mut meetings) in by_name {
        let rel = person_page(&name);
        if skip.contains(&rel) {
            continue;
        }
        let until = Page::read(&wiki.join(&rel))
            .ok()
            .and_then(|p| p.get("llm_until").and_then(Value::as_str).map(String::from));
        meetings.sort_by(|a, b| a.date.cmp(&b.date).then_with(|| a.id.cmp(&b.id)));
        let pending = meetings.iter().filter(|m| until.as_deref().is_none_or(|u| m.date.as_str() > u)).count();
        if pending > 0 {
            candidates.push(Candidate { name, meetings, pending, until });
        }
    }
    candidates.sort_by(|a, b| {
        b.pending
            .cmp(&a.pending)
            .then_with(|| b.meetings.len().cmp(&a.meetings.len()))
            .then_with(|| a.name.cmp(&b.name))
    });

    let mut ministries: Option<HashMap<String, String>> = None;
    let mut tasks = Vec::new();
    // 抜粋の無い人 (llm_until だけ進めて飛ばす) が続いても、取得が膨らまないようにする。
    for c in candidates.into_iter().take(max * MAX_PROBES_PER_TASK) {
        if tasks.len() >= max {
            break;
        }
        match plan_one(source, wiki, work, &c, &mut ministries) {
            Ok(Some(task)) => tasks.push(task),
            Ok(None) => {}
            Err(e) => tracing::warn!("wiki-plan: 人物 {}: {e:#} — 次回に再試行", c.name),
        }
    }
    Ok(tasks)
}

fn plan_one(
    source: &Source,
    wiki: &Path,
    work: &Path,
    c: &Candidate,
    ministries: &mut Option<HashMap<String, String>>,
) -> Result<Option<Task>> {
    let pending: Vec<&RosterMeeting> = c
        .meetings
        .iter()
        .copied()
        .filter(|m| c.until.as_deref().is_none_or(|u| m.date.as_str() > u))
        .collect();

    let mut bundle_meetings = Vec::new();
    let mut total_chars = 0usize;
    let mut until = String::new();
    let mut included = 0usize;
    let mut i = 0;
    // 同じ日の会議はまとめて入れるか、まとめて翌日に回す。
    while i < pending.len() {
        let date = pending[i].date.clone();
        let day: Vec<&RosterMeeting> = pending[i..].iter().copied().take_while(|m| m.date == date).collect();
        let mut day_out = Vec::new();
        let mut day_chars = 0usize;
        for m in &day {
            let Some(excerpts) = meeting_excerpts(source, work, m, &c.name, ministries)? else {
                // 配信がまだ無い (index にあるが本体が無い) — この日以降は翌日に回す。
                return finish(wiki, work, c, bundle_meetings, until, included, pending.len());
            };
            day_chars += excerpts.iter().map(|e| e.text.chars().count()).sum::<usize>();
            day_out.push((m, excerpts));
        }
        let over = included > 0
            && (included + day.len() > MAX_MEETINGS_PER_PERSON || total_chars + day_chars > MAX_CHARS_PER_PERSON);
        if over {
            break;
        }
        for (m, excerpts) in day_out {
            if excerpts.is_empty() {
                continue;
            }
            let page = meeting_page(&m.corpus, &m.id);
            bundle_meetings.push(json!({
                "date": m.date,
                "title": m.title,
                "url": m.url,
                // wiki に会議ページがあれば見出しからそこへリンクする (無ければ原文 URL)。
                "page": wiki.join(&page).exists().then_some(page),
                "laws": m.laws.iter().map(|l| &l.title).collect::<Vec<_>>(),
                "excerpts": excerpts,
            }));
        }
        total_chars += day_chars;
        included += day.len();
        until = date;
        i += day.len();
    }
    finish(wiki, work, c, bundle_meetings, until, included, pending.len())
}

/// バンドルとタスクを書く。渡せる抜粋が 1 つも無ければ (法令名が議事進行にしか出ない等)、
/// LLM に渡さず `llm_until` だけ進めて毎日選ばれ続けないようにする。
fn finish(
    wiki: &Path,
    work: &Path,
    c: &Candidate,
    meetings: Vec<Value>,
    until: String,
    included: usize,
    pending: usize,
) -> Result<Option<Task>> {
    if until.is_empty() {
        return Ok(None);
    }
    let rel = person_page(&c.name);
    let path = wiki.join(&rel);
    let created = ensure_person_page(wiki, &c.name)?;
    let mut page = Page::read(&path)?;
    if meetings.is_empty() {
        page.set("llm_until", json!(until));
        page.write(&path)?;
        return Ok(None);
    }
    // 肩書き・会派は、渡す会議の中で最後に分かったもの。
    let (mut position, mut group) = (None, None);
    for m in c.meetings.iter().filter(|m| m.date <= until) {
        if let Some(sp) = m.speakers.iter().find(|sp| sp.name == c.name) {
            position = sp.position.clone().or(position);
            group = sp.group.clone().or(group);
        }
    }
    let source_rel = format!("sources/person_{}.json", file_safe(&c.name));
    let bundle = json!({
        "kind": KIND_PERSON,
        "name": c.name,
        "position": position,
        "group": group,
        "page": rel,
        "summarized_until": c.until,
        "meetings_total": c.meetings.len(),
        "remaining_after_this": pending - included,
        "meetings": meetings,
    });
    std::fs::create_dir_all(work.join("sources"))?;
    std::fs::write(work.join(&source_rel), serde_json::to_string_pretty(&bundle)?)?;
    Ok(Some(Task {
        key: format!("{KIND_PERSON}:{}", c.name),
        kind: KIND_PERSON.into(),
        id: c.name.clone(),
        date: until.clone(),
        title: c.name.clone(),
        page: rel.clone(),
        source: source_rel,
        laws: Vec::new(),
        people: Vec::new(),
        committee: None,
        created: if created { vec![rel] } else { Vec::new() },
        person: Some(PersonTask { until, before: llm_hash(&page.body) }),
    }))
}

/// 1 会議分の、その人の発言の抜粋 (と直前・直後の発言)。会議本体が配信に無ければ None。
fn meeting_excerpts(
    source: &Source,
    work: &Path,
    m: &RosterMeeting,
    name: &str,
    ministries: &mut Option<HashMap<String, String>>,
) -> Result<Option<Vec<Excerpt>>> {
    let (link_path, doc_path) = if m.corpus == KIND_KOKKAI {
        (format!("links/meeting-to-laws/{}.json", m.id), format!("proceedings/{}.json", m.id))
    } else {
        if ministries.is_none() {
            let index = source.get_json("shingikai/index.json")?.unwrap_or(Value::Null);
            *ministries = Some(
                index["minutes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|x| Some((x["minutes_id"].as_str()?.to_string(), x["ministry"].as_str()?.to_string())))
                    .collect(),
            );
        }
        let Some(ministry) = ministries.as_ref().and_then(|x| x.get(&m.id)) else { return Ok(None) };
        (format!("links/shingikai-to-laws/{}.json", m.id), format!("shingikai/{ministry}/{}.json", m.id))
    };
    let Some(doc) = source.get_json(&doc_path)? else { return Ok(None) };
    let laws: Vec<(String, Vec<String>)> = match source.get_json(&link_path)? {
        Some(v) => linked_law_entries(&v).into_iter().map(|(id, _, patterns)| (id, patterns)).collect(),
        None => Vec::new(),
    };
    let units = if m.corpus == KIND_KOKKAI { kokkai_units(&doc) } else { shingikai_units(&doc) };
    let excerpts = person_excerpts(&m.corpus, &units, name, &laws);
    if !excerpts.is_empty() {
        // wiki-check がオフラインで引用を照合できるよう会議本体を残す。
        let dir = work.join("docs").join(&m.corpus);
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join(format!("{}.json", m.id)), serde_json::to_vec(&doc)?)?;
    }
    Ok(Some(excerpts))
}

/// その人の発言のうち法令に言及したもの (議事進行を除く) と、それぞれの直前の発言 (`question`:
/// 答えた質問) と直後の発言 (`reply`: 受けた答弁)。会議の発言順に並べる。
pub(crate) fn person_excerpts(corpus: &str, units: &[Unit], name: &str, laws: &[(String, Vec<String>)]) -> Vec<Excerpt> {
    let index: HashMap<&str, usize> = units.iter().enumerate().map(|(i, u)| (u.reference.as_str(), i)).collect();
    let own: Vec<Unit> = units.iter().filter(|u| u.speaker.as_deref() == Some(name)).cloned().collect();
    let mut matched: Vec<Excerpt> = build_excerpts_for(&own, laws)
        .into_iter()
        .filter(|e| corpus != KIND_KOKKAI || !is_procedural(e))
        .collect();
    let mut chars = 0usize;
    matched.retain(|e| {
        chars += e.text.chars().count();
        chars <= MAX_OWN_CHARS_PER_MEETING
    });
    if matched.is_empty() {
        // 1 つ目だけで上限を超える長い発言は、切り詰めて 1 つは渡す。
        if let Some(mut first) = build_excerpts_for(&own, laws).into_iter().find(|e| corpus != KIND_KOKKAI || !is_procedural(e)) {
            first.text = first.text.chars().take(MAX_OWN_CHARS_PER_MEETING).collect::<String>() + "…";
            matched.push(first);
        }
    }
    matched.truncate(MAX_OWN_PER_MEETING);

    let own_idx: BTreeSet<usize> = matched.iter().filter_map(|e| index.get(e.reference.as_str()).copied()).collect();
    let mut out: Vec<(usize, Excerpt)> = matched
        .into_iter()
        .map(|e| (index.get(e.reference.as_str()).copied().unwrap_or(usize::MAX), e))
        .collect();
    let mut extra: BTreeSet<(usize, &'static str)> = BTreeSet::new();
    for &i in &own_idx {
        if i > 0 && !own_idx.contains(&(i - 1)) {
            extra.insert((i - 1, "question"));
        }
        if i + 1 < units.len() && !own_idx.contains(&(i + 1)) {
            extra.insert((i + 1, "reply"));
        }
    }
    for (i, ctx) in extra {
        let u = &units[i];
        if u.speaker.as_deref() == Some(name) || u.speaker.as_deref() == Some(KOKKAI_HEADER_SPEAKER) {
            continue;
        }
        let cs: Vec<char> = u.text.chars().collect();
        // 質問は末尾 (何を問うたか)、答弁は冒頭が肝心。
        let text: String = if cs.len() <= CONTEXT_CHARS {
            u.text.clone()
        } else if ctx == "reply" {
            cs[..CONTEXT_CHARS].iter().collect::<String>() + "…"
        } else {
            "…".to_string() + &cs[cs.len() - CONTEXT_CHARS..].iter().collect::<String>()
        };
        let e = Excerpt {
            reference: u.reference.clone(),
            url: u.url.clone(),
            speaker: u.speaker.clone(),
            group: u.group.clone(),
            position: u.position.clone(),
            laws: vec![],
            context: Some(ctx.to_string()),
            context_for: Vec::new(),
            text,
        };
        // 委員長の「次に〜君」のような議事進行は文脈にならない。
        if corpus == KIND_KOKKAI && is_procedural(&e) {
            continue;
        }
        out.push((i, e));
    }
    out.sort_by_key(|(i, _)| *i);
    out.into_iter().map(|(_, e)| e).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::roster::{RosterLaw, RosterSpeaker, ROSTER_VERSION};

    fn unit(r: &str, speaker: &str, text: &str) -> Unit {
        Unit {
            reference: r.into(),
            url: format!("https://kokkai.ndl.go.jp/txt/{r}"),
            speaker: Some(speaker.into()),
            group: None,
            position: None,
            text: text.into(),
        }
    }

    fn laws() -> Vec<(String, Vec<String>)> {
        vec![("L1".to_string(), vec!["皇室典範".to_string()])]
    }

    #[test]
    fn excerpts_are_own_remarks_with_the_question_and_the_reply() {
        let units = vec![
            unit("k:0", "会議録情報", "皇室典範"),
            unit("k:1", "國場幸之助", "○國場委員長　次に、山田太郎君。"),
            unit("k:2", "山田太郎", "○山田太郎君　皇室典範の改正について総理の見解を伺います。"),
            unit("k:3", "高市早苗", "○内閣総理大臣（高市早苗君）　皇室典範改正法は立法府の総意として取りまとめられました。"),
            unit("k:4", "山田太郎", "○山田太郎君　附帯決議の検討はいつまでに行うのか。"),
            unit("k:5", "高市早苗", "○内閣総理大臣（高市早苗君）　法令に触れない答弁です。"),
        ];
        let out = person_excerpts(KIND_KOKKAI, &units, "高市早苗", &laws());
        let refs: Vec<(&str, Option<&str>)> = out.iter().map(|e| (e.reference.as_str(), e.context.as_deref())).collect();
        assert_eq!(refs, vec![("k:2", Some("question")), ("k:3", None), ("k:4", Some("reply"))]);
        // 法令に言及しない人は何も渡さない。
        assert!(person_excerpts(KIND_KOKKAI, &units, "國場幸之助", &laws()).is_empty());
    }

    fn roster_meeting(id: &str, date: &str, speech: &str) -> RosterMeeting {
        RosterMeeting {
            schema_version: ROSTER_VERSION,
            corpus: KIND_KOKKAI.into(),
            id: id.into(),
            date: date.into(),
            title: format!("衆議院 予算委員会 {id}"),
            organization: "衆議院".into(),
            committee: "予算委員会".into(),
            url: format!("https://kokkai.ndl.go.jp/txt/{id}"),
            laws: vec![RosterLaw { law_id: "L1".into(), title: "皇室典範".into() }],
            speakers: vec![RosterSpeaker {
                name: "高市早苗".into(),
                group: Some("自由民主党・無所属の会".into()),
                position: Some("内閣総理大臣".into()),
                role: "kokkai".into(),
                laws: vec!["L1".into()],
                excerpt: Some(speech.into()),
                excerpt_url: None,
            }],
        }
    }

    #[test]
    fn tasks_cover_unsummarized_meetings_oldest_first_and_advance_after_completion() {
        let root = temp_dir("person_plan");
        let public = root.join("public");
        let wiki = root.join("wiki");
        let work = root.join("work");
        let write = |rel: &str, v: Value| {
            let p = public.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, serde_json::to_vec(&v).unwrap()).unwrap();
        };
        for (id, date) in [("M1", "2026-02-20"), ("M2", "2026-03-02"), ("M3", "2026-03-02")] {
            let speech = format!("○内閣総理大臣（高市早苗君）　{id}で皇室典範について答弁します。");
            write(&format!("links/meeting-to-laws/{id}.json"), json!({"linked_laws": [{"law_id": "L1", "title": "皇室典範", "match_reasons": ["皇室典範"]}]}));
            write(&format!("proceedings/{id}.json"), json!({"meeting_id": id, "speeches": [
                {"speech_id": format!("{id}_001"), "order": 1, "speaker": "高市早苗", "speech": speech}
            ]}));
            let m = roster_meeting(id, date, &speech);
            let p = wiki.join(super::super::roster::roster_path(KIND_KOKKAI, id));
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, serde_json::to_vec(&m).unwrap()).unwrap();
        }
        let source = Source::new(&public.display().to_string()).unwrap();

        let tasks = plan_tasks(&source, &wiki, &work, 3, &BTreeSet::new()).unwrap();
        assert_eq!(tasks.len(), 1);
        let t = &tasks[0];
        assert_eq!(t.page, "people/高市早苗.md");
        assert_eq!(t.person.as_ref().unwrap().until, "2026-03-02", "同じ日の会議はまとめて渡す");
        let bundle: Value = serde_json::from_slice(&std::fs::read(work.join(&t.source)).unwrap()).unwrap();
        let dates: Vec<&str> = bundle["meetings"].as_array().unwrap().iter().map(|m| m["date"].as_str().unwrap()).collect();
        assert_eq!(dates, vec!["2026-02-20", "2026-03-02", "2026-03-02"], "古い順");
        assert_eq!(bundle["meetings"][0]["excerpts"][0]["ref"], "kokkai:M1_001");
        assert!(work.join("docs/kokkai/M1.json").exists(), "引用の照合用に会議本体を残す");
        // 人物ページに全体像と経緯の区間ができている。
        let page = Page::read(&wiki.join(&t.page)).unwrap();
        assert!(page.body.contains(PERSON_SUMMARY_HEADING));

        // 今日の会議タスクで書き足す人は翌日に回す。
        let skip: BTreeSet<String> = ["people/高市早苗.md".to_string()].into();
        assert!(plan_tasks(&source, &wiki, &work, 3, &skip).unwrap().is_empty());

        // 要約済み (llm_until が最後の会議) なら選ばない。
        let mut page = Page::read(&wiki.join(&t.page)).unwrap();
        page.set("llm_until", json!("2026-03-02"));
        page.write(&wiki.join(&t.page)).unwrap();
        assert!(plan_tasks(&source, &wiki, &work, 3, &BTreeSet::new()).unwrap().is_empty());
        std::fs::remove_dir_all(root).ok();
    }
}

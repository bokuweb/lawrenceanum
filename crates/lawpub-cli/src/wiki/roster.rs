//! 発言者名簿 (roster): 配信済みコーパスの**全会議**から、法令に言及した発言者を決定的に集める。
//!
//! 人物・会議体ページは、もともと LLM が要点を書いた会議ページの frontmatter だけから作って
//! いたので、LLM の処理上限 (1 日数件) と `lookback_days` の窓からこぼれた会議の発言者が
//! 永久に載らなかった。名簿は LLM と切り離し、`wiki-plan` が会議ごとに
//! `.lawpub/roster/{corpus}/{id}.json` を作って `wiki-finalize` がそれを読む。
//!
//! - 会議録は公開後に変わらないので、一度作った名簿は作り直さない (`ROSTER_VERSION` を上げたときだけ)。
//! - 国会は法令リンクの無い回の本体を取りに行かない (「法令に言及した発言者」の定義どおり)。
//!   リンクが無いことも名簿に記録して、翌日以降に引き直さない。
//! - 審議会は件数が少なく資料だけでは中身がわからないので、議事録があれば必ず読む。

use super::*;

/// 名簿の置き場所 (wiki からの相対パス)。
pub const ROSTER_DIR: &str = ".lawpub/roster";
/// 名簿の形式。上げると既存の名簿も作り直す。
/// 2: 発言者ごとに、法令に言及した発言の抜粋 (`excerpt`) と原文 URL を持たせた。
///    あわせて両院合同審査会の表記を `both` から「両院」に直した。
pub const ROSTER_VERSION: u64 = 2;

/// 人物ページに載せる抜粋の長さ (法令名の前後)。
const EXCERPT_BEFORE: usize = 40;
const EXCERPT_AFTER: usize = 120;

/// 会議が言及した法令。法令ページがまだ無くても題名を出せるよう、題名も持つ。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RosterLaw {
    pub law_id: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RosterSpeaker {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<String>,
    /// `kokkai` (国会議員・政府側・職員) / `official` (審議会の官職者) / `member` (審議会の委員)。
    pub role: String,
    /// この発言者が言及した法令 ID。
    pub laws: Vec<String>,
    /// その会議で法令に言及した最初の発言の抜粋 (原文のまま)。人物ページに引用として載せる。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<String>,
    /// 抜粋した発言の原文 URL。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub excerpt_url: Option<String>,
}

/// 1 会議分の名簿。会議ページが無い会議も載るので、原文の URL を持つ。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RosterMeeting {
    pub schema_version: u64,
    pub corpus: String,
    pub id: String,
    pub date: String,
    pub title: String,
    pub organization: String,
    pub committee: String,
    pub url: String,
    pub laws: Vec<RosterLaw>,
    pub speakers: Vec<RosterSpeaker>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SyncStats {
    /// コーパスの index にあった会議数。
    pub total: usize,
    /// この run で新しく名簿を作った会議数。
    pub built: usize,
    /// そのうち発言者が 1 人以上いた会議数。
    pub with_speakers: usize,
    /// 上限や通信失敗で持ち越した会議数。
    pub pending: usize,
}

pub fn roster_path(corpus: &str, id: &str) -> String {
    format!("{ROSTER_DIR}/{corpus}/{}.json", file_safe(id))
}

/// コーパスの全会議を走査し、名簿の無い会議の名簿を作る (1 run あたり `max_fetch` 件まで)。
pub fn sync(source: &Source, wiki: &Path, max_fetch: usize) -> Result<SyncStats> {
    let mut stats = SyncStats::default();
    let mut todo: Vec<(&'static str, String, String, Option<String>)> = Vec::new();

    if let Some(index) = source.get_json("proceedings/index.json")? {
        for m in index["meetings"].as_array().into_iter().flatten() {
            let (Some(id), Some(date)) = (m["meeting_id"].as_str(), m["date"].as_str()) else {
                continue;
            };
            // 日付の無い (開催前・枠だけの) 会議は時系列に置けないので採らない。
            if date.is_empty() {
                continue;
            }
            stats.total += 1;
            if !is_fresh(wiki, KIND_KOKKAI, id) {
                todo.push((KIND_KOKKAI, id.to_string(), date.to_string(), None));
            }
        }
    }
    if let Some(index) = source.get_json("shingikai/index.json")? {
        for m in index["minutes"].as_array().into_iter().flatten() {
            let (Some(id), Some(date)) = (m["minutes_id"].as_str(), m["date"].as_str()) else {
                continue;
            };
            if date.is_empty() || !m["has_minutes"].as_bool().unwrap_or(false) {
                continue;
            }
            stats.total += 1;
            if !is_fresh(wiki, KIND_SHINGIKAI, id) {
                let ministry = m["ministry"].as_str().map(String::from);
                todo.push((KIND_SHINGIKAI, id.to_string(), date.to_string(), ministry));
            }
        }
    }

    // 新しい会議から埋める (上限で切れても最近の分は載る)。
    todo.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.1.cmp(&b.1)));
    stats.pending = todo.len();
    for (corpus, id, date, ministry) in todo {
        if stats.built >= max_fetch {
            break;
        }
        match build(source, corpus, &id, &date, ministry.as_deref()) {
            Ok(Some(entry)) => {
                let has = !entry.speakers.is_empty();
                write_entry(wiki, &entry)?;
                stats.built += 1;
                stats.pending -= 1;
                if has {
                    stats.with_speakers += 1;
                }
            }
            // 配信がまだ追いついていない (index にあるが本体が無い) — 翌日の run で拾う。
            Ok(None) => {}
            Err(e) => tracing::warn!("wiki-roster: {corpus}:{id}: {e:#} — 次回に再試行"),
        }
    }
    Ok(stats)
}

/// 名簿が既にあり、形式も最新か。
fn is_fresh(wiki: &Path, corpus: &str, id: &str) -> bool {
    let path = wiki.join(roster_path(corpus, id));
    let Ok(bytes) = std::fs::read(&path) else {
        return false;
    };
    serde_json::from_slice::<RosterMeeting>(&bytes)
        .map(|e| e.schema_version >= ROSTER_VERSION)
        .unwrap_or(false)
}

fn write_entry(wiki: &Path, entry: &RosterMeeting) -> Result<()> {
    let path = wiki.join(roster_path(&entry.corpus, &entry.id));
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut bytes = serde_json::to_vec_pretty(entry)?;
    bytes.push(b'\n');
    std::fs::write(&path, bytes).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn build(
    source: &Source,
    corpus: &'static str,
    id: &str,
    date: &str,
    ministry: Option<&str>,
) -> Result<Option<RosterMeeting>> {
    let link_path = match corpus {
        KIND_KOKKAI => format!("links/meeting-to-laws/{id}.json"),
        _ => format!("links/shingikai-to-laws/{id}.json"),
    };
    let laws = match source.get_json(&link_path)? {
        Some(v) => super::plan::linked_law_entries(&v),
        None => Vec::new(),
    };

    // 国会は会議数が多いので、法令リンクの無い回の本体は取りに行かない。
    // 「リンクが無い」ことを名簿に残して、翌日以降に引き直さないようにする。
    if laws.is_empty() && corpus == KIND_KOKKAI {
        return Ok(Some(RosterMeeting {
            schema_version: ROSTER_VERSION,
            corpus: corpus.to_string(),
            id: id.to_string(),
            date: date.to_string(),
            title: String::new(),
            organization: String::new(),
            committee: String::new(),
            url: format!("https://kokkai.ndl.go.jp/txt/{id}"),
            laws: Vec::new(),
            speakers: Vec::new(),
        }));
    }

    let doc_path = match (corpus, ministry) {
        (KIND_KOKKAI, _) => format!("proceedings/{id}.json"),
        (_, Some(ministry)) => format!("shingikai/{ministry}/{id}.json"),
        (_, None) => return Ok(None),
    };
    let Some(doc) = source.get_json(&doc_path)? else {
        return Ok(None);
    };
    let units = match corpus {
        KIND_KOKKAI => kokkai_units(&doc),
        _ => shingikai_units(&doc),
    };
    let (title, organization, committee, url) = super::plan::meeting_labels(corpus, id, &doc);
    let law_list: Vec<RosterLaw> = laws
        .iter()
        .map(|(law_id, title, _)| RosterLaw { law_id: law_id.clone(), title: title.clone() })
        .collect();

    Ok(Some(RosterMeeting {
        schema_version: ROSTER_VERSION,
        corpus: corpus.to_string(),
        id: id.to_string(),
        date: date.to_string(),
        title,
        organization,
        committee,
        url,
        laws: law_list,
        speakers: speakers_from_units(corpus, &units, &laws),
    }))
}

/// 会議の全発言から発言者を集める。LLM 用の抜粋 (`speakers_from_excerpts`) と違って
/// 件数・文字数の上限が無いので、その会議で法令に言及した発言者が漏れない。
///
/// 国会は議事進行 (会議録情報・委員長/議長) を除いた、法令に言及した発言者全員。
/// 審議会は全発言者を官職者 (`official`) と委員 (`member`) に分ける (姓だけの委員は
/// 同定できないので、会議体ページ側で表記のまま扱う)。
pub(crate) fn speakers_from_units(
    corpus: &str,
    units: &[Unit],
    laws: &[(String, String, Vec<String>)],
) -> Vec<RosterSpeaker> {
    let mut out: BTreeMap<String, RosterSpeaker> = BTreeMap::new();
    for unit in units {
        let Some(name) = unit.speaker.as_deref().filter(|n| !n.is_empty()) else {
            continue;
        };
        let mentioned: Vec<String> = laws
            .iter()
            .filter(|(_, _, patterns)| patterns.iter().any(|p| unit.text.contains(p.as_str())))
            .map(|(law_id, _, _)| law_id.clone())
            .collect();
        if corpus == KIND_KOKKAI {
            // 国会は「法令に言及した発言」だけを採る。議事進行の読み上げは発言者にしない。
            if mentioned.is_empty() || super::plan::is_procedural_text(Some(name), &unit.text) {
                continue;
            }
            let entry = out.entry(name.to_string()).or_insert_with(|| RosterSpeaker {
                name: name.to_string(),
                group: unit.group.clone(),
                position: unit.position.clone(),
                role: "kokkai".to_string(),
                laws: Vec::new(),
                excerpt: None,
                excerpt_url: None,
            });
            entry.group = entry.group.take().or_else(|| unit.group.clone());
            entry.position = entry.position.take().or_else(|| unit.position.clone());
            if entry.excerpt.is_none() {
                entry.excerpt = excerpt_around(&unit.text, laws);
                entry.excerpt_url = Some(unit.url.clone()).filter(|u| !u.is_empty());
            }
            merge_laws(&mut entry.laws, mentioned);
        } else {
            let role = if super::plan::is_official_label(name) { "official" } else { "member" };
            let entry = out.entry(name.to_string()).or_insert_with(|| RosterSpeaker {
                name: name.to_string(),
                group: None,
                position: None,
                role: role.to_string(),
                laws: Vec::new(),
                excerpt: None,
                excerpt_url: None,
            });
            if entry.excerpt.is_none() && !mentioned.is_empty() {
                entry.excerpt = excerpt_around(&unit.text, laws);
                entry.excerpt_url = Some(unit.url.clone()).filter(|u| !u.is_empty());
            }
            merge_laws(&mut entry.laws, mentioned);
        }
    }
    out.into_values().collect()
}

/// 法令名が最初に出てくるところの前後を切り出す。原文のまま (言い換えない) で、
/// 冒頭の話者表記「○田中花子君　」は人物ページでは重複するので落とす。
fn excerpt_around(text: &str, laws: &[(String, String, Vec<String>)]) -> Option<String> {
    let body = strip_speaker_prefix(text);
    let chars: Vec<char> = body.chars().collect();
    let hit = laws
        .iter()
        .flat_map(|(_, _, patterns)| patterns)
        .filter_map(|p| body.match_indices(p.as_str()).next().map(|(i, _)| (i, p.chars().count())))
        .min_by_key(|(i, _)| *i)?;
    let start_char = body[..hit.0].chars().count();
    let from = start_char.saturating_sub(EXCERPT_BEFORE);
    let to = (start_char + hit.1 + EXCERPT_AFTER).min(chars.len());
    let mut out = String::new();
    if from > 0 {
        out.push('…');
    }
    out.extend(&chars[from..to]);
    if to < chars.len() {
        out.push('…');
    }
    // 会議録は全角空白で区切られているので、1 行の引用に畳む。
    let out: String = out.split_whitespace().collect::<Vec<_>>().join(" ");
    (!out.is_empty()).then_some(out)
}

/// 「○田中花子君　本題ですが…」の先頭の話者表記を落とす。
fn strip_speaker_prefix(text: &str) -> &str {
    let trimmed = text.trim_start();
    let Some(rest) = trimmed.strip_prefix('○') else { return trimmed };
    match rest.find(char::is_whitespace) {
        // 話者表記にしては長すぎるものは本文とみなしてそのまま返す。
        Some(i) if rest[..i].chars().count() <= 30 => rest[i..].trim_start(),
        _ => trimmed,
    }
}

fn merge_laws(into: &mut Vec<String>, add: Vec<String>) {
    for law_id in add {
        if !into.contains(&law_id) {
            into.push(law_id);
        }
    }
    into.sort();
}

/// wiki に保存済みの名簿を全部読む。壊れた名簿は無視する (翌日の run で作り直される)。
pub fn load(wiki: &Path) -> Vec<RosterMeeting> {
    let mut out = Vec::new();
    let root = wiki.join(ROSTER_DIR);
    for entry in walkdir::WalkDir::new(&root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("json"))
    {
        let Ok(bytes) = std::fs::read(entry.path()) else { continue };
        match serde_json::from_slice::<RosterMeeting>(&bytes) {
            Ok(m) => out.push(m),
            Err(e) => tracing::warn!("wiki-roster: {}: {e} — 無視", entry.path().display()),
        }
    }
    out.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.id.cmp(&b.id)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(speaker: &str, group: Option<&str>, text: &str) -> Unit {
        Unit {
            reference: format!("kokkai:{speaker}{text}"),
            url: String::new(),
            speaker: Some(speaker.to_string()),
            group: group.map(String::from),
            position: None,
            text: text.to_string(),
        }
    }

    fn laws() -> Vec<(String, String, Vec<String>)> {
        vec![
            ("L1".to_string(), "予防接種法".to_string(), vec!["予防接種法".to_string()]),
            ("L2".to_string(), "感染症法".to_string(), vec!["感染症法".to_string()]),
        ]
    }

    #[test]
    fn kokkai_roster_keeps_every_law_mentioning_speaker() {
        // 抜粋の上限 (12 件) を超えても全員載ることを確かめる。
        let mut units = Vec::new();
        for i in 0..20 {
            units.push(unit(&format!("議員{i}"), Some("会派A"), "予防接種法について伺います。"));
        }
        units.push(unit("田中花子", Some("会派B"), "感染症法と予防接種法の両方です。"));
        units.push(unit("山田太郎", Some("会派C"), "法令に触れない一般質疑です。"));
        units.push(unit("國場委員長", None, "○國場委員長　予防接種法改正案を議題といたします。"));

        let speakers = speakers_from_units(KIND_KOKKAI, &units, &laws());
        let names: Vec<&str> = speakers.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(speakers.len(), 21, "{names:?}");
        assert!(!names.contains(&"山田太郎"), "法令に触れない発言者は載せない");
        assert!(!names.contains(&"國場委員長"), "議事進行は発言者にしない");
        let tanaka = speakers.iter().find(|s| s.name == "田中花子").unwrap();
        assert_eq!(tanaka.laws, vec!["L1".to_string(), "L2".to_string()]);
        assert_eq!(tanaka.group.as_deref(), Some("会派B"));
    }

    #[test]
    fn excerpts_quote_the_original_around_the_law_name() {
        let units = vec![unit(
            "田中花子",
            Some("会派B"),
            &format!(
                "○田中花子君　{}　予防接種法の健康被害救済は現場の実態に追いついていないのではないか、{}",
                "まず大臣に伺います。".repeat(6),
                "申請から認定までの期間についても厚生労働省の見解を伺います。".repeat(6),
            ),
        )];
        let sp = &speakers_from_units(KIND_KOKKAI, &units, &laws())[0];
        let excerpt = sp.excerpt.as_deref().unwrap();
        assert!(excerpt.contains("予防接種法の健康被害救済は現場の実態に追いついていない"), "{excerpt}");
        assert!(!excerpt.contains("○田中花子君"), "話者表記は落とす: {excerpt}");
        assert!(excerpt.starts_with('…'), "前を切ったことを示す: {excerpt}");
        assert!(excerpt.ends_with('…'), "後ろを切ったことを示す: {excerpt}");
        assert!(excerpt.chars().count() <= EXCERPT_BEFORE + EXCERPT_AFTER + 20, "長すぎる: {excerpt}");
    }

    #[test]
    fn shingikai_roster_splits_officials_and_members() {
        let units = vec![
            unit("森光健康・生活衛生局長", None, "予防接種法の施行状況です。"),
            unit("森委員", None, "異議ありません。"),
        ];
        let speakers = speakers_from_units(KIND_SHINGIKAI, &units, &laws());
        assert_eq!(speakers.len(), 2);
        let official = speakers.iter().find(|s| s.name == "森光健康・生活衛生局長").unwrap();
        assert_eq!(official.role, "official");
        assert_eq!(official.laws, vec!["L1".to_string()]);
        // 審議会は法令に触れない委員も (会議体ページ用に) 残す。
        assert_eq!(speakers.iter().find(|s| s.name == "森委員").unwrap().role, "member");
    }

    #[test]
    fn sync_covers_the_whole_corpus_regardless_of_the_llm_lookback() {
        let root = temp_dir("roster_sync");
        let public = root.join("public");
        let wiki = root.join("wiki");
        let write = |rel: &str, v: serde_json::Value| {
            let p = public.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, serde_json::to_vec(&v).unwrap()).unwrap();
        };
        // lookback (45 日) のはるか外の会議と、法令リンクの無い会議。
        write(
            "proceedings/index.json",
            serde_json::json!({"meetings": [
                {"meeting_id": "OLD", "date": "2025-11-12"},
                {"meeting_id": "NOLINK", "date": "2025-11-13"},
                {"meeting_id": "NODATE", "date": ""}
            ]}),
        );
        write(
            "links/meeting-to-laws/OLD.json",
            serde_json::json!({"linked_laws": [{"law_id": "L1", "title": "予防接種法", "match_reasons": ["予防接種法"]}]}),
        );
        write(
            "proceedings/OLD.json",
            serde_json::json!({
                "meeting_id": "OLD", "house": "shugiin", "committee": "厚生労働委員会", "issue": "第13号",
                "speeches": [
                    {"speech_id": "OLD_001", "order": 1, "speaker": "田中花子", "speaker_group": "会派B",
                     "speech": "○田中花子君　予防接種法の救済を拡充すべきでは。"},
                    {"speech_id": "OLD_002", "order": 2, "speaker": "國場委員長",
                     "speech": "○國場委員長　予防接種法改正案を議題といたします。"}
                ]
            }),
        );

        let source = Source::new(&public.display().to_string()).unwrap();
        let stats = sync(&source, &wiki, 100).unwrap();
        assert_eq!(stats.total, 2, "日付の無い会議は数えない");
        assert_eq!(stats.built, 2);
        assert_eq!(stats.with_speakers, 1);
        assert_eq!(stats.pending, 0);

        let entries = load(&wiki);
        let old = entries.iter().find(|e| e.id == "OLD").unwrap();
        assert_eq!(old.title, "衆議院 厚生労働委員会 第13号");
        assert_eq!(old.url, "https://kokkai.ndl.go.jp/txt/OLD");
        let names: Vec<&str> = old.speakers.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["田中花子"], "議事進行は載せない");
        // リンクの無い会議は本体を取らず、空の名簿で「引き直さない」印にする。
        let nolink = entries.iter().find(|e| e.id == "NOLINK").unwrap();
        assert!(nolink.speakers.is_empty());

        // 2 回目は作り直さない。
        assert_eq!(sync(&source, &wiki, 100).unwrap().built, 0);
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn roster_round_trips_through_the_cache() {
        let wiki = &temp_dir("roster");
        let entry = RosterMeeting {
            schema_version: ROSTER_VERSION,
            corpus: KIND_KOKKAI.to_string(),
            id: "M1".to_string(),
            date: "2026-06-16".to_string(),
            title: "衆議院 厚生労働委員会 第13号".to_string(),
            organization: "衆議院".to_string(),
            committee: "厚生労働委員会".to_string(),
            url: "https://kokkai.ndl.go.jp/txt/M1".to_string(),
            laws: vec![RosterLaw { law_id: "L1".to_string(), title: "予防接種法".to_string() }],
            speakers: vec![RosterSpeaker {
                name: "田中花子".to_string(),
                group: Some("会派B".to_string()),
                position: None,
                role: "kokkai".to_string(),
                laws: vec!["L1".to_string()],
                excerpt: None,
                excerpt_url: None,
            }],
        };
        assert!(!is_fresh(wiki, KIND_KOKKAI, "M1"));
        write_entry(wiki, &entry).unwrap();
        assert!(is_fresh(wiki, KIND_KOKKAI, "M1"));
        let loaded = load(wiki);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].speakers[0].name, "田中花子");
    }
}

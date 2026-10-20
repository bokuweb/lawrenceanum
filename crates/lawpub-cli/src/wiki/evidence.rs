//! 主張と引用の対応。原文一致は支持関係の証明ではない。
use super::check::citations_in;
use super::*;
use serde_json::json;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Claim {
    pub text: String,
    pub kind: String,
    /// 主張を帰属させる発言。前後関係だけで答弁者を推測しない。
    pub attribution: String,
    /// 同じ LLM 区間内の脚注ラベル。
    pub evidence: Vec<String>,
    pub review_status: String,
    pub review_note: String,
}

/// 明示された官職等だけで分類する。「委員」「参考人」から有識者・業界を推定しない。
pub fn speaker_role(u: &Unit) -> &'static str {
    let position = u.position.as_deref().unwrap_or("");
    let label = u.speaker.as_deref().unwrap_or("");
    let metadata = format!(
        "{} {} {}",
        position,
        u.group.as_deref().unwrap_or(""),
        label
    );
    // 姓＋「部長」「局長」だけでは民間の役職と区別できない。
    let explicit_office =
        ["省", "庁"].iter().any(|s| label.contains(s)) && super::plan::is_official_label(label);
    if [
        "国務大臣",
        "内閣総理大臣",
        "副大臣",
        "大臣政務官",
        "政府参考人",
        "政府委員",
    ]
    .iter()
    .any(|s| position.contains(s))
        || (u.reference.starts_with("shingikai:") && explicit_office)
    {
        "government"
    } else if ["協会", "連合会", "株式会社", "商工会", "経済団体"]
        .iter()
        .any(|s| metadata.contains(s))
    {
        "industry"
    } else if ["教授", "准教授", "研究員"]
        .iter()
        .any(|s| metadata.contains(s))
    {
        "expert"
    } else if position.contains("参考人") {
        "witness"
    } else if label.ends_with("委員") || label.ends_with("部会長") {
        "committee_member"
    } else {
        "unknown"
    }
}

pub fn attribution(u: &Unit, date: &str) -> Value {
    json!({"ref": u.reference, "url": u.url, "speaker": u.speaker,
        "group": u.group, "position": u.position, "date": date, "role": speaker_role(u)})
}

pub fn source_scope(units: &[Unit], excerpts: &[super::plan::Excerpt]) -> Value {
    let omitted: Vec<&str> = units
        .iter()
        .filter(|u| !excerpts.iter().any(|e| e.reference == u.reference))
        .map(|u| u.reference.as_str())
        .collect();
    let partial: Vec<&str> = units
        .iter()
        .filter(|u| {
            excerpts
                .iter()
                .any(|e| e.reference == u.reference && e.text != u.text)
        })
        .map(|u| u.reference.as_str())
        .collect();
    json!({"total_units": units.len(), "included_units": excerpts.len(),
        "source_chars": units.iter().map(|u| u.text.chars().count()).sum::<usize>(),
        "excerpt_chars": excerpts.iter().map(|e| e.text.chars().count()).sum::<usize>(),
        "omitted_refs": omitted, "partial_refs": partial,
        "full_text_reviewed": false,
        "limitations": "上限付きの抜粋。冒頭・留保候補・末尾を優先するが、抜粋外の条件・反対意見や未収集資料は未確認。前後の発言は応答関係の確定ではない。"})
}

pub fn claims(page: &Page) -> Result<Vec<Claim>> {
    serde_json::from_value(page.get("claims").cloned().unwrap_or(json!([])))
        .context("claims の形式が不正")
}

/// 発言の引用 ID から会議ページ ID へ。名前・URL の類似や文字の除去では結び付けない。
/// コーパスを読まないので、該当会議・発言の存在や引用内容を検証したことにはならない。
pub(super) fn citation_meeting_path(reference: &str) -> Option<String> {
    let (kind, rest) = reference.split_once(':')?;
    let (id, turn) = match kind {
        KIND_KOKKAI => rest.rsplit_once('_')?,
        KIND_SHINGIKAI => rest.split_once('#')?,
        _ => return None,
    };
    if id.is_empty()
        || matches!(id, "." | "..")
        || id != file_safe(id)
        || id.chars().any(char::is_control)
        || turn.is_empty()
        || !turn.bytes().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    Some(meeting_page(kind, id).strip_suffix(".md")?.to_owned())
}

/// 構造だけの検査。意味的支持を検証済みにはしない。finalize と check の両方で使用する。
pub fn claim_errors(page: &Page) -> Vec<String> {
    let claims = match claims(page) {
        Ok(c) => c,
        Err(e) => return vec![e.to_string()],
    };
    if claims.is_empty() {
        return vec!["会議の要約には claims（主張と脚注の対応）が必要です".into()];
    }
    let body = llm_blocks(&page.body).join("\n");
    let citations = citations_in(&body);
    let mut narrative = body
        .lines()
        .filter(|line| !line.trim_start().starts_with("[^"))
        .collect::<Vec<_>>()
        .join("\n");
    for q in &citations {
        narrative = narrative.replace(&format!("[^{}]", q.label), "");
    }
    let sources = page.get("evidence_sources").and_then(Value::as_array);
    let mut errors = Vec::new();
    for c in claims {
        let mut err = |s: &str| errors.push(format!("claim {:?}: {s}", c.text));
        if c.text.trim().is_empty() || !narrative.contains(&c.text) {
            err("text は LLM 本文中の主張と一致させてください");
        }
        if ![
            "government_view",
            "opinion",
            "request",
            "question",
            "decision",
            "unknown",
        ]
        .contains(&c.kind.as_str())
        {
            err("kind が不正です");
        }
        if ![
            "unreviewed",
            "needs_review",
            "unsupported",
            "indeterminate",
            "llm_supported",
        ]
        .contains(&c.review_status.as_str())
        {
            err("review_status が不正です。検証済みという状態はありません");
        }
        if c.review_status != "unreviewed" && c.review_note.trim().is_empty() {
            err("判定理由が必要です");
        }
        if c.evidence.is_empty() {
            err("根拠の脚注が必要です");
        }
        let cited: Vec<_> = c
            .evidence
            .iter()
            .filter_map(|label| citations.iter().find(|q| q.label == *label))
            .collect();
        if cited.len() != c.evidence.len() {
            err("evidence は同じ LLM 区間の引用脚注を参照してください");
        }
        if !cited.iter().any(|q| q.reference == c.attribution) {
            err("attribution は引用した発言から指定してください");
        }
        for q in &cited {
            if !sources.is_some_and(|s| s.iter().any(|s| s["ref"] == q.reference)) {
                err("この会議で取得した発言の根拠情報がありません");
            }
        }
        if c.kind == "government_view"
            && !sources.is_some_and(|s| {
                s.iter()
                    .any(|s| s["ref"] == c.attribution && s["role"] == "government")
            })
        {
            err("政府見解の帰属先に政府側の役職を確認できません。unknown / opinion 等で要確認としてください");
        }
    }
    errors
}

/// 比較対象は最後に取得したコーパス。原サイトの現在の版を保証するものではない。
pub fn summary_source_status(page: &Page) -> &'static str {
    let source = page.get_str("source_fingerprint");
    if source.is_empty() { return "untracked"; }
    if source == page.get_str("llm_source_fingerprint") { return "current"; }
    if llm_blocks(&page.body).iter().all(|b| b.trim().is_empty()) { "pending" } else { "stale" }
}

pub fn source_fields_match(page: &Page, bundle: &Value, completed: bool) -> bool {
    page.get("evidence_version").and_then(Value::as_u64) == Some(1)
        && !page.get_str("source_fingerprint").is_empty()
        && page.get("source_fingerprint") == bundle.get("source_fingerprint")
        && (page.get("llm_source_fingerprint") == bundle.get("previous_summary_fingerprint")
            || (completed && page.get("llm_source_fingerprint") == bundle.get("source_fingerprint")))
}

/// 機械生成の出典情報に引用本文をつなぐ。原資料更新後の旧判定・帰属を引き継がない。
pub fn assessments(page: &Page) -> Value {
    let citations = citations_in(&llm_blocks(&page.body).join("\n"));
    let sources = page.get("evidence_sources").and_then(Value::as_array);
    let valid = claim_errors(page).is_empty();
    let stale = matches!(summary_source_status(page), "stale" | "pending");
    json!(claims(page).unwrap_or_default().iter().map(|c| {
        let evidence: Vec<Value> = c.evidence.iter().filter_map(|label| {
            if stale { return None; }
            let q = citations.iter().find(|q| q.label == *label)?;
            let mut s = sources?.iter().find(|s| s["ref"] == q.reference)?.clone();
            s["quote"] = json!(q.quote);
            Some(s)
        }).collect();
        json!({"text": c.text, "kind": c.kind, "attribution": c.attribution,
            "review_status": if valid && !stale { c.review_status.as_str() } else { "indeterminate" },
            "review_note": if stale { "原資料の版に対応する要約の更新が未完了です。引用・帰属・支持関係を再確認してください" }
                else if valid { c.review_note.as_str() } else { "根拠情報の形式が不完全です" }, "sources": evidence})
    }).collect::<Vec<_>>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meeting_paths_use_exact_supported_citation_ids_only() {
        for (reference, path) in [
            ("kokkai:M1_001", "meetings/kokkai/M1"),
            ("kokkai:meeting_with_underscores_12", "meetings/kokkai/meeting_with_underscores"),
            ("shingikai:mhlw_example_20261001#0", "meetings/shingikai/mhlw_example_20261001"),
            ("shingikai:日本語#12", "meetings/shingikai/日本語"),
        ] {
            assert_eq!(citation_meeting_path(reference).as_deref(), Some(path));
        }
        for reference in ["kokkai:M1", "kokkai:M1_", "kokkai:M1_bad", "kokkai:_1",
            "shingikai:S1", "shingikai:S1#", "shingikai:S1#1#2", "shingikai:../S1#1",
            "shingikai:..#1", "shingikai:S 1#1", "shingikai:S:1#1", "shingikai:S\u{0}1#1",
            "gian:S1#1", "pubcomment:S1#1", "kanpo:S1#1"] {
            assert_eq!(citation_meeting_path(reference), None, "{reference:?}");
        }
    }

    fn unit(speaker: &str, position: &str, group: &str) -> Unit {
        Unit {
            reference: "kokkai:M1_001".into(),
            url: "https://kokkai.ndl.go.jp/txt/M1/1".into(),
            speaker: Some(speaker.into()),
            position: Some(position.into()),
            group: Some(group.into()),
            text: "現時点では実施を決めていません。".into(),
        }
    }

    fn page() -> Page {
        let mut page = Page::default();
        page.body = format!("{LLM_BEGIN}\n政府は実施を決めた[^1]。\n\n[^1]: [kokkai:M1_001](https://kokkai.ndl.go.jp/txt/M1/1) 「現時点では実施を決めていません」\n{LLM_END}");
        page.set(
            "evidence_sources",
            json!([attribution(&unit("大臣", "国務大臣", ""), "2026-10-01")]),
        );
        page.set("claims", json!([{"text": "政府は実施を決めた", "kind": "government_view", "attribution": "kokkai:M1_001", "evidence": ["1"], "review_status": "unsupported", "review_note": "引用は未決定と述べており、主張を支持しない。"}]));
        page
    }

    #[test]
    fn updated_sources_invalidate_prior_reviews_until_machine_finalization() {
        let mut p = page();
        p.set("evidence_version", json!(1));
        assert_eq!(summary_source_status(&p), "untracked");
        p.set("source_fingerprint", json!("new"));
        p.set("llm_source_fingerprint", json!("old"));
        assert_eq!(summary_source_status(&p), "stale");
        let assessment = assessments(&p);
        assert_eq!(assessment[0]["review_status"], "indeterminate");
        assert!(assessment[0]["sources"].as_array().unwrap().is_empty());
        let bundle = json!({"source_fingerprint": "new", "previous_summary_fingerprint": "old"});
        assert!(source_fields_match(&p, &bundle, false));
        p.set("llm_source_fingerprint", json!("new"));
        assert!(!source_fields_match(&p, &bundle, false));
        assert!(source_fields_match(&p, &bundle, true));
        assert_eq!(summary_source_status(&p), "current");
        assert_eq!(assessments(&p)[0]["review_status"], "unsupported");
        p.set("source_fingerprint", json!("forged"));
        assert!(!source_fields_match(&p, &bundle, true));
        p.body = format!("{LLM_BEGIN}\n{LLM_END}");
        assert_eq!(summary_source_status(&p), "pending");
    }

    #[test]
    fn roles_require_explicit_metadata_and_do_not_promote_a_witness_to_government() {
        for (position, group, expected) in [
            ("政府参考人", "", "government"),
            ("参考人", "業界協会", "industry"),
            ("教授", "", "expert"),
            ("参考人", "", "witness"),
            ("", "", "unknown"),
        ] {
            assert_eq!(speaker_role(&unit("山田", position, group)), expected);
        }
        let mut u = unit("山田部長", "", "");
        u.reference = "shingikai:S1#1".into();
        assert_eq!(speaker_role(&u), "unknown", "民間の部長もあり得る");
        u.speaker = Some("山田委員".into());
        assert_eq!(speaker_role(&u), "committee_member");
        u.speaker = Some("厚生労働省医政局長".into());
        assert_eq!(speaker_role(&u), "government");
    }

    #[test]
    fn uncertainty_survives_export_and_verified_is_not_a_valid_status() {
        let mut p = page();
        for status in [
            "unreviewed",
            "needs_review",
            "unsupported",
            "indeterminate",
            "llm_supported",
        ] {
            let mut c = p.get("claims").unwrap().clone();
            c[0]["review_status"] = json!(status);
            p.set("claims", c);
            assert!(claim_errors(&p).is_empty());
            let out = assessments(&p);
            assert_eq!(out[0]["review_status"], status);
            assert_eq!(out[0]["sources"][0]["speaker"], "大臣");
            assert_eq!(
                out[0]["sources"][0]["quote"],
                "現時点では実施を決めていません"
            );
        }
        let mut c = p.get("claims").unwrap().clone();
        c[0]["review_status"] = json!("verified");
        p.set("claims", c);
        assert!(!claim_errors(&p).is_empty());
        assert_eq!(assessments(&p)[0]["review_status"], "indeterminate");
    }

    #[test]
    fn claims_need_a_real_body_assertion_footnote_and_attribution() {
        let p = page();
        for (key, value) in [
            ("text", json!("現時点では実施を決めていません")),
            ("evidence", json!(["missing"])),
            ("attribution", json!("kokkai:M2_001")),
            ("review_note", json!("")),
        ] {
            let mut p = Page {
                frontmatter: p.frontmatter.clone(),
                body: p.body.clone(),
            };
            let mut c = p.get("claims").unwrap().clone();
            c[0][key] = value;
            p.set("claims", c);
            assert!(!claim_errors(&p).is_empty(), "{key}");
        }
        let mut p = p;
        p.set(
            "evidence_sources",
            json!([attribution(
                &unit("山田", "参考人", "業界協会"),
                "2026-10-01"
            )]),
        );
        assert!(claim_errors(&p).iter().any(|s| s.contains("政府側の役職")));
        p.set("claims", json!([]));
        assert!(!claim_errors(&p).is_empty());
    }
}

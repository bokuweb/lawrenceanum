//! 資料で利用した Wiki の版を保存し、根拠を含む変更とリンク先の再確認候補を出す。
//! リンクは参照関係であり、主張の支持関係や政策への影響を意味しない。

use super::{check, evidence, export, *};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, VecDeque};

const SCHEMA_VERSION: u32 = 1;
const SCOPE: &str = "published_wiki_snapshot";

#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    schema_version: u32,
    scope: String,
    generated_at: String,
    limitations: String,
    pages: BTreeMap<String, ReportPage>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ReportPage {
    revision: String,
    content: PageContent,
}

#[derive(Debug, Serialize, Deserialize)]
struct PageContent {
    frontmatter: BTreeMap<String, Value>,
    body: String,
    stub: bool,
    /// Wiki ルート相対・拡張子なし。未公開・欠落のリンク先も残す。
    references: BTreeSet<String>,
    citations: Vec<Value>,
    claim_structure: String,
    claim_errors: Vec<String>,
    claims: BTreeMap<String, ReportClaim>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ReportClaim {
    revision: String,
    /// 同じ文・種別・帰属先が複数回現れる場合も情報を捨てない。
    records: Vec<Value>,
}

// オブジェクトのキー順だけの違いは版の変更にしない。配列の順は意味を持ち得るので保つ。
fn canonical(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let sorted: BTreeMap<_, _> = map.into_iter().collect();
            Value::Object(sorted.into_iter().map(|(k, v)| (k, canonical(v))).collect())
        }
        Value::Array(items) => Value::Array(items.into_iter().map(canonical).collect()),
        v => v,
    }
}

pub(super) fn revision(value: &impl Serialize) -> String {
    let bytes = serde_json::to_vec(&canonical(
        serde_json::to_value(value).expect("serializable"),
    ))
    .expect("serializable");
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

fn manifest(pages: &[(String, Page)]) -> Manifest {
    let pages = pages
        .iter()
        .filter(|(path, _)| !export::HUB_PAGES.contains(&path.as_str()))
        .map(|(path, page)| {
            let id = path.trim_end_matches(".md");
            let mut grouped: BTreeMap<String, Vec<Value>> = BTreeMap::new();
            let assessments = evidence::assessments(page);
            let citations = check::citations_in(&llm_blocks(&page.body).join("\n"));
            // 壊れた claims は frontmatter と claim_errors に残し、正常な主張としては扱わない。
            for (claim, assessment) in evidence::claims(page)
                .unwrap_or_default()
                .iter()
                .zip(assessments.as_array().into_iter().flatten())
            {
                let claim_id = revision(&json!([id, claim.text, claim.kind, claim.attribution]));
                let quotes: Vec<_> = citations
                    .iter()
                    .filter(|c| claim.evidence.contains(&c.label))
                    .map(citation_record)
                    .collect();
                grouped
                    .entry(claim_id)
                    .or_default()
                    .push(json!({"claim": claim, "assessment": assessment, "citations": quotes}));
            }
            let claims = grouped
                .into_iter()
                .map(|(id, records)| {
                    (
                        id,
                        ReportClaim {
                            revision: revision(&records),
                            records,
                        },
                    )
                })
                .collect();
            let references = check::relative_links(&page.body)
                .into_iter()
                .filter_map(|target| {
                    if !target.ends_with(".md") {
                        return None;
                    }
                    let target = export::resolve_link(path, &target)?;
                    (target != *path && !export::HUB_PAGES.contains(&target.as_str()))
                        .then(|| target.trim_end_matches(".md").to_string())
                })
                .collect();
            let citations = citations.iter().map(citation_record).collect();
            let claim_errors = if page.get("claims").is_some() {
                evidence::claim_errors(page)
            } else {
                Vec::new()
            };
            let claim_structure = if page.get("claims").is_none() {
                "unstructured"
            } else if claim_errors.is_empty() {
                "structured"
            } else {
                "invalid"
            };
            let content = PageContent {
                frontmatter: page.frontmatter.iter().cloned().collect(),
                body: export::display_body(&page.body),
                stub: export::is_stub(page),
                references,
                citations,
                claim_structure: claim_structure.into(),
                claim_errors,
                claims,
            };
            (
                id.to_string(),
                ReportPage {
                    revision: revision(&content),
                    content,
                },
            )
        })
        .collect();
    Manifest {
        schema_version: SCHEMA_VERSION,
        scope: SCOPE.into(), generated_at: now_rfc3339(),
        limitations: "公開対象の Wiki の保存版です。コーパス全体の網羅性、原文の最新性、引用が主張を支持することは保証しません。claims のないページも未評価のまま含みます。references は本文のリンクであり、支持・因果関係ではありません。".into(),
        pages,
    }
}

fn citation_record(c: &check::Citation) -> Value {
    json!({"label": c.label, "reference": c.reference, "url": c.url, "quote": c.quote})
}

pub fn write_manifest(pages: &[(String, Page)], out: &Path) -> Result<()> {
    std::fs::write(out, serde_json::to_vec(&manifest(pages))?)?;
    Ok(())
}

fn read_manifest(path: &Path) -> Result<Manifest> {
    let value: Value = serde_json::from_slice(
        &std::fs::read(path).with_context(|| format!("read {}", path.display()))?,
    )?;
    if value["schema_version"] != json!(SCHEMA_VERSION) || value["scope"] != SCOPE {
        bail!(
            "{}: 未対応の report-manifest schema / scope",
            path.display()
        );
    }
    let manifest: Manifest = serde_json::from_value(value)?;
    // 保存版の一部だけを書き換えた場合、古いハッシュによる「変更なし」を防ぐ。
    for (id, page) in &manifest.pages {
        if revision(&page.content) != page.revision {
            bail!(
                "{}: ページ {id} の revision が内容に一致しません",
                path.display()
            );
        }
        for (claim_id, claim) in &page.content.claims {
            if revision(&claim.records) != claim.revision {
                bail!(
                    "{}: 主張 {claim_id} の revision が内容に一致しません",
                    path.display()
                );
            }
        }
    }
    Ok(manifest)
}

#[derive(Debug, Serialize)]
struct Change {
    kind: &'static str,
    previous_revision: Option<String>,
    current_revision: Option<String>,
    claims: BTreeMap<String, &'static str>,
}

#[derive(Debug, Serialize)]
struct Diff {
    schema_version: u32,
    previous_generated_at: String,
    current_generated_at: String,
    limitations: String,
    changes: BTreeMap<String, Change>,
    /// 値は到達可能な変更ページの ID。結論への影響の判定ではない。
    review_candidates: BTreeMap<String, BTreeSet<String>>,
}

fn diff(previous: &Manifest, current: &Manifest) -> Diff {
    let mut changes = BTreeMap::new();
    for id in previous
        .pages
        .keys()
        .chain(current.pages.keys())
        .collect::<BTreeSet<_>>()
    {
        let before = previous.pages.get(id);
        let after = current.pages.get(id);
        if before.map(|p| &p.revision) == after.map(|p| &p.revision) {
            continue;
        }
        let mut claims = BTreeMap::new();
        let old = before.map(|p| &p.content.claims);
        let new = after.map(|p| &p.content.claims);
        for claim_id in old
            .into_iter()
            .flat_map(|m| m.keys())
            .chain(new.into_iter().flat_map(|m| m.keys()))
            .collect::<BTreeSet<_>>()
        {
            let a = old.and_then(|m| m.get(claim_id));
            let b = new.and_then(|m| m.get(claim_id));
            if a.map(|c| &c.revision) != b.map(|c| &c.revision) {
                claims.insert(
                    claim_id.clone(),
                    match (a, b) {
                        (None, _) => "added",
                        (_, None) => "removed",
                        _ => "updated",
                    },
                );
            }
        }
        changes.insert(
            id.clone(),
            Change {
                kind: match (before, after) {
                    (None, _) => "added",
                    (_, None) => "removed",
                    _ => "updated",
                },
                previous_revision: before.map(|p| p.revision.clone()),
                current_revision: after.map(|p| p.revision.clone()),
                claims,
            },
        );
    }
    let mut reverse: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
    // 削除された参照先・外されたリンクも再確認できるよう、両版の参照を使う。
    for m in [previous, current] {
        for (id, page) in &m.pages {
            for target in &page.content.references {
                reverse.entry(target.clone()).or_default().insert(id);
            }
            // 既存の保存版にある citations から導出する。保存形式・内容ハッシュは変えない。
            for citation in &page.content.citations {
                if let Some(target) = citation["reference"]
                    .as_str()
                    .and_then(evidence::citation_meeting_path)
                    .filter(|target| target != id)
                {
                    reverse.entry(target).or_default().insert(id);
                }
            }
        }
    }
    let mut review_candidates: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for changed in changes.keys() {
        let mut visited = BTreeSet::from([changed.as_str()]);
        let mut queue = VecDeque::from([changed.as_str()]);
        while let Some(target) = queue.pop_front() {
            for &referrer in reverse.get(target).into_iter().flatten() {
                if !visited.insert(referrer) {
                    continue;
                }
                queue.push_back(referrer);
                if current.pages.contains_key(referrer) {
                    review_candidates
                        .entry(referrer.into())
                        .or_default()
                        .insert(changed.clone());
                }
            }
        }
    }
    Diff {
        schema_version: SCHEMA_VERSION,
        previous_generated_at: previous.generated_at.clone(),
        current_generated_at: current.generated_at.clone(),
        limitations: "Wiki 保存版間の差分です。review_candidates は新旧の本文リンクと、LLM 区間の国会・審議会の脚注から識別した引用元会議を逆にたどった再確認候補です。引用先の存在・主張の支持関係・政策の変化・資料の書換えの必要性を判定しません。本文を書き換えた主張は removed / added として扱います。".into(),
        changes, review_candidates,
    }
}

pub fn run_diff(previous: &Path, current: &Path, out: &Path) -> Result<()> {
    // 入力の上書きを避ける。symlink・相対パスの別名も canonicalize で照合する。
    if out.exists() {
        let dest = out.canonicalize()?;
        if dest == previous.canonicalize()? || dest == current.canonicalize()? {
            bail!("--out は入力の report-manifest と別のファイルにしてください");
        }
    }
    let previous = read_manifest(previous)?;
    let current = read_manifest(current)?;
    let report = diff(&previous, &current);
    if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(out, serde_json::to_vec_pretty(&report)?)?;
    println!(
        "wiki-report-diff: {} changes, {} review candidates",
        report.changes.len(),
        report.review_candidates.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(text: &str) -> Page {
        let mut p = Page { body: format!("{LLM_BEGIN}\n{text}[^1]。\n\n[^1]: [kokkai:M1_001](https://example.com/M1) 「検討中であり実施は未定」\n{LLM_END}\n"), ..Page::default() };
        p.set("type", json!("meeting"));
        p.set("title", json!("検討会"));
        p.set(
            "source_scope",
            json!({"full_text_reviewed":false,"omitted_refs":["kokkai:M1_002"]}),
        );
        p.set("evidence_sources", json!([{"ref":"kokkai:M1_001","url":"https://example.com/M1","speaker":"大臣","role":"government","date":"2026-10-01"}]));
        p.set("claims", json!([{"text":text,"kind":"government_view","attribution":"kokkai:M1_001","evidence":["1"],"review_status":"needs_review","review_note":"前後の留保を確認する"}]));
        p
    }

    fn snapshot(p: Page) -> Manifest {
        manifest(&[("meetings/M1.md".into(), p)])
    }

    #[test]
    fn revisions_ignore_export_time_page_order_and_object_key_order() {
        let p = page("政府は検討している");
        let mut reordered = p.clone();
        reordered.frontmatter.reverse();
        let mut a = manifest(&[
            ("meetings/M1.md".into(), p),
            ("laws/L1.md".into(), Page::default()),
        ]);
        let mut b = manifest(&[
            ("laws/L1.md".into(), Page::default()),
            ("meetings/M1.md".into(), reordered),
        ]);
        a.generated_at = "before".into();
        b.generated_at = "after".into();
        assert!(diff(&a, &b).changes.is_empty());
        assert!(diff(&a, &b).review_candidates.is_empty());
    }

    #[test]
    fn evidence_and_review_updates_keep_claim_identity_but_change_revision() {
        let original = page("政府は検討している");
        let before = snapshot(original.clone());
        for updated in [
            {
                let mut p = original.clone();
                p.body = p.body.replace("実施は未定", "結論は未定");
                p
            },
            {
                let mut p = original.clone();
                p.body = p
                    .body
                    .replace("https://example.com/M1", "https://example.com/corrected/M1");
                p
            },
            {
                let mut p = original.clone();
                let mut c = p.get("claims").unwrap().clone();
                c[0]["review_status"] = json!("unsupported");
                p.set("claims", c);
                p
            },
            {
                let mut p = original.clone();
                let mut s = p.get("evidence_sources").unwrap().clone();
                s[0]["role"] = json!("unknown");
                p.set("evidence_sources", s);
                p
            },
        ] {
            let after = snapshot(updated);
            let d = diff(&before, &after);
            let change = &d.changes["meetings/M1"];
            assert_eq!(change.kind, "updated");
            assert_eq!(
                change.claims.values().copied().collect::<Vec<_>>(),
                ["updated"]
            );
            assert_eq!(
                before.pages["meetings/M1"].content.claims.keys().next(),
                after.pages["meetings/M1"].content.claims.keys().next()
            );
        }
        let content = &before.pages["meetings/M1"].content;
        assert_eq!(content.claim_structure, "structured");
        assert_eq!(
            content.frontmatter["source_scope"]["full_text_reviewed"],
            false
        );
        assert_eq!(content.citations[0]["quote"], "検討中であり実施は未定");
    }

    #[test]
    fn text_changes_are_added_and_removed_and_duplicate_claims_are_preserved() {
        let before = snapshot(page("政府は検討している"));
        let after = snapshot(page("政府は実施を決定した"));
        let d = diff(&before, &after);
        let kinds: BTreeSet<_> = d.changes["meetings/M1"].claims.values().copied().collect();
        assert_eq!(kinds, BTreeSet::from(["added", "removed"]));
        let mut duplicate = page("政府は検討している");
        let claim = duplicate.get("claims").unwrap()[0].clone();
        duplicate.set("claims", json!([claim, claim]));
        let m = snapshot(duplicate);
        let claims = &m.pages["meetings/M1"].content.claims;
        assert_eq!(claims.len(), 1);
        assert_eq!(claims.values().next().unwrap().records.len(), 2);
    }

    fn linked(body: &str) -> Page {
        Page {
            body: body.into(),
            ..Page::default()
        }
    }

    #[test]
    fn review_candidates_follow_old_and_new_links_transitively_and_handle_cycles() {
        let before = manifest(&[
            ("topics/T.md".into(), linked("[law](../laws/L.md)")),
            (
                "laws/L.md".into(),
                linked("[source](../meetings/M.md) [topic](../topics/T.md)"),
            ),
            ("meetings/M.md".into(), page("検討中")),
        ]);
        let after = manifest(&[
            ("topics/T.md".into(), linked("[law](../laws/L.md)")),
            (
                "laws/L.md".into(),
                linked("[source](../meetings/M.md) [topic](../topics/T.md)"),
            ),
        ]);
        let d = diff(&before, &after);
        assert_eq!(d.changes["meetings/M"].kind, "removed");
        assert_eq!(
            d.review_candidates
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["laws/L", "topics/T"]
        );
        assert!(d.review_candidates["topics/T"].contains("meetings/M"));
        assert!(d.review_candidates["laws/L"].contains("meetings/M"));
        // 未解決の参照先が新たに公開された場合も再確認候補になる。
        let added = diff(&after, &before);
        assert_eq!(added.changes["meetings/M"].kind, "added");
        assert!(added.review_candidates["topics/T"].contains("meetings/M"));
    }

    #[test]
    fn legacy_invalid_and_stub_pages_are_explicit_and_hubs_are_excluded() {
        let mut broken = page("検討中");
        broken.set("claims", json!({"invalid":true}));
        let m = manifest(&[
            ("index.md".into(), linked("[law](laws/L.md)")),
            ("laws/L.md".into(), linked("旧要約")),
            ("meetings/M.md".into(), broken),
            ("meetings/stub.md".into(), {
                let mut p = Page::default();
                p.set("type", json!("meeting"));
                p
            }),
        ]);
        assert!(!m.pages.contains_key("index"));
        assert_eq!(m.pages["laws/L"].content.claim_structure, "unstructured");
        assert!(m.pages["laws/L"].content.claims.is_empty());
        assert_eq!(m.pages["meetings/M"].content.claim_structure, "invalid");
        assert!(!m.pages["meetings/M"].content.claim_errors.is_empty());
        assert!(m.pages["meetings/M"].content.frontmatter["claims"].is_object());
        assert!(m.pages["meetings/stub"].content.stub);
    }

    #[test]
    fn direct_citations_trigger_transitive_review_without_wiki_links() {
        for (reference, meeting) in [
            ("kokkai:M1_001", "meetings/kokkai/M1"),
            (
                "shingikai:committee_20261001#0",
                "meetings/shingikai/committee_20261001",
            ),
        ] {
            let mut law = linked(&format!("{LLM_BEGIN}\n検討中[^1]。\n\n[^1]: [{reference}](https://example.com) 「検討中」\n[^2]: [{reference}](https://example.com) 「検討中」\n{LLM_END}"));
            law.set("type", json!("law"));
            let topic = linked("[law](../laws/L.md)");
            // 引用と本文リンクを混ぜた循環。会議自身を自分の再確認候補にはしない。
            let mut source = page("検討中");
            source.body.push_str("\n[topic](../../topics/T.md)");
            let make = |p| {
                manifest(&[
                    (format!("{meeting}.md"), p),
                    ("laws/L.md".into(), law.clone()),
                    ("topics/T.md".into(), topic.clone()),
                ])
            };
            let before = make(source.clone());
            let original_revision = before.pages["laws/L"].revision.clone();
            assert!(before.pages["laws/L"].content.references.is_empty());
            source.set("source_fingerprint", json!("corrected"));
            let after = make(source);
            assert_eq!(after.pages["laws/L"].revision, original_revision);
            let d = diff(&before, &after);
            assert_eq!(
                d.review_candidates["laws/L"],
                BTreeSet::from([meeting.to_owned()])
            );
            assert_eq!(
                d.review_candidates["topics/T"],
                BTreeSet::from([meeting.to_owned()])
            );
            assert!(!d.review_candidates.contains_key(meeting));

            // 未収録の会議の追加と、引用元会議の削除も拾う。
            let absent = manifest(&[
                ("laws/L.md".into(), law.clone()),
                ("topics/T.md".into(), topic.clone()),
            ]);
            for d in [diff(&absent, &before), diff(&before, &absent)] {
                assert!(d.review_candidates["laws/L"].contains(meeting));
                assert!(d.review_candidates["topics/T"].contains(meeting));
            }
            // 新版で引用を外していても、旧版にあった依存は再確認する。
            let removed = manifest(&[
                ("laws/L.md".into(), linked("引用を削除")),
                ("topics/T.md".into(), topic.clone()),
            ]);
            assert!(diff(&before, &removed).review_candidates["laws/L"].contains(meeting));
        }
    }

    #[test]
    fn existing_snapshot_citations_are_read_without_schema_or_revision_changes() {
        let root = temp_dir("report-citation-compat");
        let path = root.join("saved.json");
        let before = manifest(&[
            ("laws/L.md".into(), page("検討中")),
            ("meetings/kokkai/M1.md".into(), page("検討中")),
        ]);
        std::fs::write(&path, serde_json::to_vec(&before).unwrap()).unwrap();
        let loaded = read_manifest(&path).unwrap();
        assert_eq!(loaded.schema_version, 1);
        let after = manifest(&[
            ("laws/L.md".into(), page("検討中")),
            ("meetings/kokkai/M1.md".into(), page("変更")),
        ]);
        assert!(diff(&loaded, &after).review_candidates["laws/L"].contains("meetings/kokkai/M1"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cli_validates_snapshots_and_protects_inputs() {
        let root = temp_dir("report-diff");
        let previous = root.join("previous.json");
        let current = root.join("current.json");
        let out = root.join("diff.json");
        let pages = [("meetings/M1.md".into(), page("検討中"))];
        write_manifest(&pages, &previous).unwrap();
        write_manifest(&pages, &current).unwrap();
        run_diff(&previous, &current, &out).unwrap();
        let d: Value = serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
        assert_eq!(d["changes"], json!({}));
        assert!(run_diff(&previous, &current, &previous).is_err());
        let original = std::fs::read(&current).unwrap();
        let mut corrupted: Value = serde_json::from_slice(&original).unwrap();
        corrupted["pages"]["meetings/M1"]["content"]["body"] = json!("変更");
        std::fs::write(&current, serde_json::to_vec(&corrupted).unwrap()).unwrap();
        assert!(read_manifest(&current)
            .unwrap_err()
            .to_string()
            .contains("revision"));
        corrupted = serde_json::from_slice(&original).unwrap();
        corrupted["schema_version"] = json!(999);
        std::fs::write(&current, serde_json::to_vec(&corrupted).unwrap()).unwrap();
        assert!(read_manifest(&current)
            .unwrap_err()
            .to_string()
            .contains("schema"));
        std::fs::remove_dir_all(root).unwrap();
    }
}

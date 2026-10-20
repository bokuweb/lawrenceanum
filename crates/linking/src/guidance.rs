//! 政府解釈 (ノーアクションレター・Q&A・ガイドライン等) → 法令 クロスリンク。
//!
//! `public/guidance/{id}.json` を読み、`public/links/law-to-guidance/{law_id}.json` を書き出す。
//! 法令名の解決は [`crate::law_refs`] (判例・質問主意書と共通)。関係の強さは 3 段階:
//!
//! - `subject`: 一覧に「該当条文」として載っている法令 (ノーアクションレターの照会対象など)、
//!   または一覧の分野見出しが示す法令。
//! - `title`: 文書名に法令名が出てくる (「金融商品取引法第２条に規定する定義に関するQ&A」)。
//! - `mention`: 本文で条番号付きで参照している (「銀行法第10条第２項」)。条を挙げない言及は拾わない
//!   (Q&A は他法令の名前に広く触れるため)。

use crate::law_refs::LawMatcher;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::path::Path;

/// 文書名で条番号無しの法令名を拾う最小文字数 (「銀行法」「信託業法」も拾う)。
const TITLE_BARE_MIN_CHARS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    Subject,
    Title,
    Mention,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkedGuidance {
    /// 例: `fsa-nal-ginkou-10`。
    pub id: String,
    pub ministry: String,
    pub ministry_name: String,
    /// `no_action_letter` / `written_inquiry` / `consultation_cases` / `qa` / `guideline` / …
    pub kind: String,
    pub title: String,
    pub issued_on: Option<String>,
    pub issuer: Option<String>,
    pub summary: Option<String>,
    pub relation: Relation,
    /// 参照された条番号 (例: "10", "52_36")。条を挙げない関係だけなら空。
    pub articles: Vec<String>,
    /// 一覧ページ (出典)。
    pub source_url: String,
    /// 文書本体 (回答書・PDF) の URL。
    pub document_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LawToGuidance {
    pub schema_version: u32,
    pub law_id: String,
    pub linked_guidance: Vec<LinkedGuidance>,
}

pub fn run_link_guidance(public: &Path) -> Result<()> {
    let matcher = LawMatcher::from_index(public)?;
    let dir = public.join("guidance");
    if !dir.exists() {
        tracing::warn!("no guidance dir at {}; skipping", dir.display());
        return Ok(());
    }
    let mut by_law: HashMap<String, Vec<LinkedGuidance>> = HashMap::new();
    for entry in std::fs::read_dir(&dir)? {
        let path = entry?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.extension().and_then(|e| e.to_str()) != Some("json")
            || name == "index.json"
            || name.starts_with("._")
        {
            continue;
        }
        let d: serde_json::Value = match serde_json::from_slice(&std::fs::read(&path)?) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("link-guidance: skip {}: {e}", path.display());
                continue;
            }
        };
        let s = |k: &str| d[k].as_str().unwrap_or("").to_string();
        let opt = |k: &str| d[k].as_str().map(str::to_string);
        let title = s("title");

        let mut rel: HashMap<String, (Relation, BTreeSet<String>)> = HashMap::new();
        let mut add = |law: String, r: Relation, arts: BTreeSet<String>| {
            let e = rel.entry(law).or_insert((r, BTreeSet::new()));
            e.0 = e.0.min(r);
            e.1.extend(arts);
        };
        for law in d["related_laws"].as_array().into_iter().flatten() {
            let name = law["name"].as_str().unwrap_or("");
            let provisions = law["provisions"].as_str().unwrap_or("");
            let found = matcher.scan(&format!("{name}{provisions}"), Some(0));
            // 名前そのものが当たった法令を主題とし、条文欄に出た別法令は mention 扱い。
            let subject: Option<String> = law["law_id"]
                .as_str()
                .map(str::to_string)
                .or_else(|| matcher.scan(name, Some(0)).into_keys().next());
            for (law_id, arts) in found {
                let r = if Some(&law_id) == subject.as_ref() {
                    Relation::Subject
                } else {
                    Relation::Mention
                };
                add(law_id, r, arts);
            }
            if let Some(id) = subject {
                add(id, Relation::Subject, BTreeSet::new());
            }
        }
        for (law_id, arts) in matcher.scan(&title, Some(TITLE_BARE_MIN_CHARS)) {
            add(law_id, Relation::Title, arts);
        }
        if let Some(body) = d["body_text"].as_str() {
            for (law_id, arts) in matcher.scan(body, None) {
                add(law_id, Relation::Mention, arts);
            }
        }

        let document_url = d["attachments"]
            .as_array()
            .and_then(|a| {
                a.iter()
                    .find(|x| x["role"] == "answer")
                    .or_else(|| a.iter().find(|x| x["role"] == "document"))
                    .or_else(|| a.first())
            })
            .and_then(|x| x["url"].as_str())
            .map(str::to_string);
        for (law_id, (relation, arts)) in rel {
            by_law.entry(law_id).or_default().push(LinkedGuidance {
                id: s("id"),
                ministry: s("ministry"),
                ministry_name: s("ministry_name"),
                kind: s("kind"),
                title: title.clone(),
                issued_on: opt("issued_on"),
                issuer: opt("issuer"),
                summary: opt("summary"),
                relation,
                articles: arts.into_iter().collect(),
                source_url: s("source_url"),
                document_url: document_url.clone(),
            });
        }
    }

    let out_dir = public.join("links").join("law-to-guidance");
    std::fs::create_dir_all(&out_dir)?;
    let mut written = 0usize;
    for (law_id, mut docs) in by_law {
        // 主題 → 文書名 → 本文の順、同じ関係なら新しい順。
        docs.sort_by(|a, b| {
            a.relation
                .cmp(&b.relation)
                .then_with(|| b.issued_on.cmp(&a.issued_on))
                .then_with(|| a.id.cmp(&b.id))
        });
        let doc = LawToGuidance {
            schema_version: 1,
            law_id: law_id.clone(),
            linked_guidance: docs,
        };
        std::fs::write(
            out_dir.join(format!("{law_id}.json")),
            serde_json::to_string_pretty(&doc)?,
        )?;
        written += 1;
    }
    tracing::info!("link-guidance: {written} files written");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_guidance_end_to_end() {
        let dir = std::env::temp_dir().join(format!("lawpub_guidance_link_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("laws")).unwrap();
        std::fs::create_dir_all(dir.join("guidance")).unwrap();
        std::fs::write(
            dir.join("laws").join("index.json"),
            serde_json::to_vec(&serde_json::json!({"laws": [
                {"law_id": "GINKO", "title": "銀行法"},
                {"law_id": "KINSHO", "title": "金融商品取引法"},
                {"law_id": "KASHIKIN", "title": "貸金業法"},
            ]}))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            dir.join("guidance").join("fsa-nal-ginkou-10.json"),
            serde_json::to_vec(&serde_json::json!({
                "id": "fsa-nal-ginkou-10", "ministry": "fsa", "ministry_name": "金融庁",
                "kind": "no_action_letter",
                "title": "銀行法 第10条第２項、第12条 に関するノーアクションレター（整理番号10）",
                "issued_on": "2010-03-25", "issuer": "金融庁監督局 銀行第一課長",
                "summary": "照会書にある事実を前提とすると…",
                "related_laws": [{"name": "銀行法", "provisions": "第10条第２項、第12条"}],
                "body_text": "【回答書】\n金融商品取引法第二条第八項の規定にも留意。貸金業法の趣旨。",
                "attachments": [
                    {"role": "inquiry", "url": "https://x/a.pdf"},
                    {"role": "answer", "url": "https://x/b.pdf"}
                ],
                "source_url": "https://www.fsa.go.jp/common/noact/kaitou_2/ginkou/index.html"
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            dir.join("guidance").join("fsa-qa-abc.json"),
            serde_json::to_vec(&serde_json::json!({
                "id": "fsa-qa-abc", "ministry": "fsa", "ministry_name": "金融庁", "kind": "qa",
                "title": "貸金業法に関するQ&A", "issued_on": null,
                "related_laws": [], "body_text": null,
                "attachments": [{"role": "document", "url": "https://x/qa.html"}],
                "source_url": "https://www.fsa.go.jp/common/law/kokuji.html"
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(dir.join("guidance").join("index.json"), b"{}").unwrap();

        run_link_guidance(&dir).unwrap();
        let read = |id: &str| -> LawToGuidance {
            let p = dir
                .join("links")
                .join("law-to-guidance")
                .join(format!("{id}.json"));
            serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap()
        };
        let ginko = read("GINKO");
        assert_eq!(ginko.linked_guidance.len(), 1);
        let g = &ginko.linked_guidance[0];
        assert_eq!(g.relation, Relation::Subject);
        assert_eq!(g.articles, vec!["10", "12"]);
        assert_eq!(g.document_url.as_deref(), Some("https://x/b.pdf"));
        // 本文で条番号付きで触れた法令は mention。
        let kinsho = read("KINSHO");
        assert_eq!(kinsho.linked_guidance[0].relation, Relation::Mention);
        assert_eq!(kinsho.linked_guidance[0].articles, vec!["2"]);
        // 文書名に出る法令は title。本文の「貸金業法の趣旨」(条番号なし) は拾わない。
        let kashikin = read("KASHIKIN");
        assert_eq!(kashikin.linked_guidance.len(), 1);
        assert_eq!(kashikin.linked_guidance[0].id, "fsa-qa-abc");
        assert_eq!(kashikin.linked_guidance[0].relation, Relation::Title);
        std::fs::remove_dir_all(&dir).ok();
    }
}

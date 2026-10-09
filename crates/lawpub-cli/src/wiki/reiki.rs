//! 自治体例規 (条例・規則) を、法令ページの「自治体の対応」として結びつける (LLM 不使用)。
//!
//! 例規の題名には根拠法令の名前が入っていることが多い (「留萌市建築基準法施行細則」
//! 「〜法に基づく〜条例」)。wiki に法令ページがある法令の名前を、全自治体の例規の題名と最長一致で
//! 照合し、法令ページの frontmatter `reiki` に件数・自治体数・最近の制定例を書く。
//! finalize がこれを法令ページの「自治体の対応（例規）」節に描く。
//!
//! 例規は R2 から配信されている (`--reiki-base-url`)。全自治体の一覧を読むので週 1 回だけ更新する。

use super::*;
use aho_corasick::{AhoCorasick, MatchKind};
use serde_json::json;
use std::collections::{BTreeSet, HashMap};

const CACHE_PATH: &str = ".lawpub/reiki.json";
const REFRESH_DAYS: i64 = 7;
const RECENT: usize = 8;
/// 短すぎる法令名 (「民法」等) は他の語の一部に誤一致しやすいので照合に使わない。
const MIN_TITLE_CHARS: usize = 4;

#[derive(Debug, Clone, Serialize)]
struct Hit {
    date: String,
    municipality: String,
    title: String,
    kind: String,
    url: String,
}

/// 例規を照合して法令ページの `reiki` を更新する。更新した法令ページ数を返す (更新不要なら None)。
pub fn sync(reiki_base: &str, app_base: &str, wiki: &Path, today: chrono::NaiveDate) -> Result<Option<usize>> {
    if reiki_base.is_empty() {
        return Ok(None);
    }
    let cache = wiki.join(CACHE_PATH);
    if let Ok(bytes) = std::fs::read(&cache) {
        let v: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        let fresh = v["generated"]
            .as_str()
            .and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
            .is_some_and(|d| (today - d).num_days() < REFRESH_DAYS);
        if fresh {
            return Ok(None);
        }
    }

    // wiki に法令ページがある法令だけを照合する (例規のために法令ページを増やさない)。
    let mut laws: Vec<(String, String)> = Vec::new();
    for path in walk_md(wiki, "laws") {
        let page = Page::read(&path)?;
        let (id, title) = (page.get_str("law_id"), page.get_str("title"));
        if !id.is_empty() && title.chars().count() >= MIN_TITLE_CHARS {
            laws.push((id.to_string(), title.to_string()));
        }
    }
    if laws.is_empty() {
        return Ok(None);
    }
    let ac = AhoCorasick::builder()
        .match_kind(MatchKind::LeftmostLongest)
        .build(laws.iter().map(|(_, t)| t.as_str()))?;

    let source = Source::new(reiki_base)?;
    let Some(index) = source.get_json("index.json")? else {
        return Ok(None);
    };
    let mut hits: HashMap<String, Vec<Hit>> = HashMap::new();
    for m in index["municipalities"].as_array().into_iter().flatten() {
        let Some(code) = m["municipality_code"].as_str() else { continue };
        if m["count"].as_u64().unwrap_or(0) == 0 {
            continue;
        }
        let list = match source.get_json(&format!("{code}/index.json")) {
            Ok(Some(l)) => l,
            Ok(None) => continue,
            Err(e) => {
                tracing::warn!("wiki-plan: 例規 {code}: {e:#}");
                continue;
            }
        };
        // 都道府県自身の例規は「滋賀県滋賀県」とならないよう、名前が都道府県で始まるときはそのまま使う。
        let (pref, mname) = (m["prefecture"].as_str().unwrap_or(""), m["name"].as_str().unwrap_or(""));
        let name = if mname.starts_with(pref) { mname.to_string() } else { format!("{pref}{mname}") };
        for r in list["reiki"].as_array().into_iter().flatten() {
            let (Some(id), Some(title)) = (r["reiki_id"].as_str(), r["title"].as_str()) else { continue };
            let matched: BTreeSet<usize> = ac.find_iter(title).map(|m| m.pattern().as_usize()).collect();
            for i in matched {
                hits.entry(laws[i].0.clone()).or_default().push(Hit {
                    date: r["promulgated_date"].as_str().unwrap_or("").to_string(),
                    municipality: name.clone(),
                    title: title.to_string(),
                    kind: r["kind"].as_str().unwrap_or("").to_string(),
                    url: format!("{app_base}/#/reiki/{code}/{id}"),
                });
            }
        }
    }

    let mut updated = 0;
    for (law_id, _) in &laws {
        let path = wiki.join(law_page(law_id));
        let mut page = Page::read(&path)?;
        let value = match hits.get_mut(law_id) {
            Some(list) => {
                list.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.municipality.cmp(&b.municipality)));
                let municipalities: BTreeSet<&str> = list.iter().map(|h| h.municipality.as_str()).collect();
                json!({
                    "count": list.len(),
                    "municipalities": municipalities.len(),
                    "recent": list.iter().take(RECENT).collect::<Vec<_>>(),
                    "as_of": today.format("%Y-%m-%d").to_string(),
                })
            }
            None => Value::Null,
        };
        let current = page.get("reiki").cloned().unwrap_or(Value::Null);
        // 件数・最近の例が変わったときだけ書く (as_of だけの変化で毎週差分を出さない)。
        let same = current["count"] == value["count"] && current["recent"] == value["recent"];
        if same {
            continue;
        }
        if value.is_null() {
            page.frontmatter.retain(|(k, _)| k != "reiki");
        } else {
            page.set("reiki", value);
        }
        page.write(&path)?;
        updated += 1;
    }
    std::fs::create_dir_all(cache.parent().expect("has parent"))?;
    std::fs::write(&cache, serde_json::to_vec_pretty(&json!({"generated": today.format("%Y-%m-%d").to_string()}))?)?;
    Ok(Some(updated))
}

/// 法令ページの「自治体の対応（例規）」節の中身 (frontmatter `reiki` から)。無ければ空。
pub fn render_section(reiki: Option<&Value>) -> String {
    let Some(r) = reiki.filter(|r| r["count"].as_u64().unwrap_or(0) > 0) else {
        return String::new();
    };
    let mut out = format!(
        "## 自治体の対応（例規）\n\n{} 自治体の {} 件の例規が、題名にこの法令名を含みます（施行細則・条例など。{} 時点）。\n\n| 制定日 | 自治体 | 例規 |\n|---|---|---|\n",
        r["municipalities"], r["count"], r["as_of"].as_str().unwrap_or("")
    );
    for h in r["recent"].as_array().into_iter().flatten() {
        out.push_str(&format!(
            "| {} | {} | [{}]({}) |\n",
            h["date"].as_str().unwrap_or(""),
            cell(h["municipality"].as_str().unwrap_or("")),
            cell(h["title"].as_str().unwrap_or("")),
            h["url"].as_str().unwrap_or("")
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reiki_titles_are_matched_to_law_pages_weekly() {
        let root = temp_dir("reiki");
        let r2 = root.join("r2");
        let wiki = root.join("wiki");
        let write = |rel: &str, v: Value| {
            let p = r2.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, serde_json::to_vec(&v).unwrap()).unwrap();
        };
        write("index.json", json!({"municipalities": [
            {"municipality_code": "012122", "name": "留萌市", "prefecture": "北海道", "count": 3}
        ]}));
        write("012122/index.json", json!({"reiki": [
            {"reiki_id": "R1", "title": "留萌市建築基準法施行細則", "kind": "規則", "promulgated_date": "2001-04-01"},
            {"reiki_id": "R2", "title": "留萌市児童福祉法に基づく基準該当障害児通所支援事業者の登録等に関する規則", "kind": "規則", "promulgated_date": "2013-04-01"},
            {"reiki_id": "R3", "title": "留萌市応援寄附条例", "kind": "条例", "promulgated_date": "2008-09-25"}
        ]}));
        for (id, title) in [("L1", "建築基準法"), ("L2", "児童福祉法"), ("L3", "民法")] {
            ensure_law_page(&wiki, "https://app", id, title).unwrap();
        }
        let today = chrono::NaiveDate::from_ymd_opt(2026, 10, 9).unwrap();
        let n = sync(&r2.display().to_string(), "https://app", &wiki, today).unwrap();
        assert_eq!(n, Some(2));
        let law = Page::read(&wiki.join("laws/L1.md")).unwrap();
        let r = law.get("reiki").unwrap();
        assert_eq!(r["count"], 1);
        assert_eq!(r["recent"][0]["url"], "https://app/#/reiki/012122/R1");
        assert!(Page::read(&wiki.join("laws/L3.md")).unwrap().get("reiki").is_none(), "短い法令名は照合しない");
        let section = render_section(law.get("reiki"));
        assert!(section.contains("1 自治体の 1 件の例規"));
        assert!(section.contains("[留萌市建築基準法施行細則](https://app/#/reiki/012122/R1)"));

        // 1 週間以内は取り直さない。
        assert_eq!(sync(&r2.display().to_string(), "https://app", &wiki, today + chrono::Duration::days(3)).unwrap(), None);
        std::fs::remove_dir_all(root).ok();
    }
}

//! 蓄積した官報（`lawpub kanpo-archive` の出力）から、スナップショットテスト
//! （`tests/archive_snapshots.rs`）に使うページと項目を層別に選んで `tests/archive/` に置く。
//!
//! ```text
//! cargo run --release -p kanpo-parser --example snapshot_sample -- <archive_dir> crates/kanpo-parser/tests/archive [ページ数]
//! ```
//!
//! - ページ: 号の種類（本紙・号外・特別号外）× 組み方（段・継続ページ・横組み）× 新旧対照表の
//!   帯 × 罫線の表 × 割注 × 私用 CID の層ごとに、層の大きさに比例して（少なくとも 2 ページ）選ぶ。
//!   選び方は語の並びに依らないページ名のハッシュ順なので、同じアーカイブからは同じページが選ばれる。
//! - 項目: 目次の改正・廃止・制定の項目のうち 2〜6 ページのものから、号の種類 × 新旧対照表の
//!   有無の層ごとに選ぶ（ページまたぎの表のつなぎ方を見るため）。項目のページもページの集合に入れる。
//! - 置くもの: `pages/<号>/<ページ>.bbox.html.gz`（pdftotext -bbox-layout）と `.rules.json`（罫線）、
//!   `pages.tsv`（ページと層）、`items/<項目>.json`（項目の標題とページ）。PDF は置かない。

use flate2::write::GzEncoder;
use flate2::Compression;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};

/// ページ名の FNV-1a ハッシュ（選ぶ順を決めるだけ）。
fn hash(s: &str) -> u64 {
    s.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}

struct Page {
    /// `<日付>/<号>/<ページ>`（拡張子なし）。
    stem: String,
    stratum: String,
}

fn features(root: &Path, stem: &str) -> Option<Page> {
    let xhtml = std::fs::read_to_string(root.join(format!("{stem}.bbox.html"))).ok()?;
    let rules: Option<kanpo_parser::lines::PageRules> =
        std::fs::read_to_string(root.join(format!("{stem}.rules.json"))).ok().and_then(|s| serde_json::from_str(&s).ok());
    let a = kanpo_parser::analyze_with_rules(&xhtml, &[rules]);
    let p = a.pages.first()?;
    let shinkyu = p.regions.iter().any(|r| matches!(r.role, kanpo_parser::trace::Role::After | kanpo_parser::trace::Role::Before));
    let tables = p.regions.iter().flat_map(|r| &r.lines).any(|l| l.table.is_some());
    let warichu = p.text.lines().any(|l| l.contains('・') && (l.starts_with('〇') || l.contains("告示第") || l.contains("令第")));
    let cid = p.words.iter().any(|w| w.raw != w.text && w.text.starts_with('（'));
    let dir = stem.split('/').nth(1)?;
    let kind = match dir.chars().nth(8)? {
        'h' => "honshi",
        'g' => "gougai",
        't' => "tokubetsu",
        _ => "other",
    };
    let mut tags = vec![kind.to_string(), p.layout.to_string()];
    for (on, tag) in [(shinkyu, "shinkyu"), (tables, "table"), (warichu, "warichu"), (cid, "private-cid")] {
        if on {
            tags.push(tag.to_string());
        }
    }
    Some(Page { stem: stem.to_string(), stratum: tags.join(",") })
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: snapshot_sample <archive_dir> <out_dir> [pages]");
        std::process::exit(2);
    }
    let (root, out) = (PathBuf::from(&args[1]), PathBuf::from(&args[2]));
    let target: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(150);

    // 日付のディレクトリと manifest。
    let mut days: Vec<PathBuf> = std::fs::read_dir(&root).unwrap().flatten().map(|e| e.path()).filter(|p| p.join("manifest.json").exists()).collect();
    days.sort();
    let mut stems: Vec<String> = Vec::new();
    let mut items: Vec<Value> = Vec::new();
    for day in &days {
        let date = day.file_name().unwrap().to_string_lossy().to_string();
        let manifest: Value = serde_json::from_str(&std::fs::read_to_string(day.join("manifest.json")).unwrap()).unwrap();
        for issue in manifest["issues"].as_array().into_iter().flatten() {
            let dir = issue["dir"].as_str().unwrap_or_default();
            let page_count = issue["page_count"].as_u64().unwrap_or(0) as u32;
            for n in 1..=page_count {
                let stem = format!("{date}/{dir}/{dir}{n:04}");
                if root.join(format!("{stem}.bbox.html")).exists() {
                    stems.push(stem);
                }
            }
            let starts: Vec<u32> = issue["items"].as_array().into_iter().flatten().filter_map(|i| i["page"].as_u64()).map(|p| p as u32).collect();
            for item in issue["items"].as_array().into_iter().flatten() {
                let title = item["title"].as_str().unwrap_or_default();
                if !(title.contains("改正") || title.contains("廃止") || title.contains("制定")) {
                    continue;
                }
                let start = item["page"].as_u64().unwrap_or(0) as u32;
                let end = starts.iter().copied().find(|&p| p > start).unwrap_or(page_count).max(start);
                if end - start + 1 < 2 || end - start + 1 > 6 {
                    continue;
                }
                let pages: Vec<String> = (start..=end).map(|n| format!("{date}/{dir}/{dir}{n:04}")).collect();
                if pages.iter().all(|s| root.join(format!("{s}.bbox.html")).exists()) {
                    items.push(json!({ "date": date, "issue": dir, "page": start, "title": title, "pages": pages }));
                }
            }
        }
    }
    eprintln!("pages {} items {}", stems.len(), items.len());

    // ページを層に分けて、層の大きさに比例して選ぶ。
    let pages: Vec<Page> = stems.iter().filter_map(|s| features(&root, s)).collect();
    let mut strata: BTreeMap<String, Vec<&Page>> = BTreeMap::new();
    for p in &pages {
        strata.entry(p.stratum.clone()).or_default().push(p);
    }
    let total = pages.len().max(1);
    let mut chosen: BTreeMap<String, String> = BTreeMap::new();
    for (stratum, mut ps) in strata {
        ps.sort_by_key(|p| hash(&p.stem));
        let quota = ((ps.len() * target).div_ceil(total)).max(2).min(ps.len());
        for p in ps.into_iter().take(quota) {
            chosen.insert(p.stem.clone(), stratum.clone());
        }
    }

    // 項目: 号の種類 × 新旧対照表の有無の層ごとに選ぶ。
    let stratum_of: BTreeMap<&str, &str> = pages.iter().map(|p| (p.stem.as_str(), p.stratum.as_str())).collect();
    let mut item_strata: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
    for it in &items {
        let ps: Vec<&str> = it["pages"].as_array().unwrap().iter().filter_map(|v| v.as_str()).collect();
        let kind = stratum_of.get(ps[0]).and_then(|s| s.split(',').next()).unwrap_or("other");
        let shinkyu = ps.iter().any(|p| stratum_of.get(p).is_some_and(|s| s.contains("shinkyu")));
        let tables = ps.iter().any(|p| stratum_of.get(p).is_some_and(|s| s.contains("table")));
        item_strata.entry(format!("{kind},shinkyu={shinkyu},table={tables}")).or_default().push(it);
    }
    let mut picked_items: Vec<Value> = Vec::new();
    for (_, mut its) in item_strata {
        its.sort_by_key(|it| hash(&format!("{}:{}", it["issue"], it["page"])));
        for it in its.into_iter().take(5) {
            for p in it["pages"].as_array().unwrap() {
                let s = p.as_str().unwrap().to_string();
                let stratum = stratum_of.get(s.as_str()).copied().unwrap_or("item").to_string();
                chosen.entry(s).or_insert(format!("{stratum},item"));
            }
            picked_items.push(it.clone());
        }
    }

    // 書き出し。
    let pages_dir = out.join("pages");
    let _ = std::fs::remove_dir_all(&pages_dir);
    let mut tsv = String::from("page\tstratum\n");
    let mut seen_dirs: BTreeSet<PathBuf> = BTreeSet::new();
    for (stem, stratum) in &chosen {
        let rel = stem.split_once('/').unwrap().1; // <号>/<ページ>
        let dst = pages_dir.join(rel);
        let dir = dst.parent().unwrap().to_path_buf();
        if seen_dirs.insert(dir.clone()) {
            std::fs::create_dir_all(&dir).unwrap();
        }
        let xhtml = std::fs::read(root.join(format!("{stem}.bbox.html"))).unwrap();
        let mut gz = GzEncoder::new(Vec::new(), Compression::best());
        gz.write_all(&xhtml).unwrap();
        std::fs::write(dst.with_extension("bbox.html.gz"), gz.finish().unwrap()).unwrap();
        if let Ok(rules) = std::fs::read(root.join(format!("{stem}.rules.json"))) {
            std::fs::write(dst.with_extension("rules.json"), rules).unwrap();
        }
        tsv.push_str(&format!("{rel}\t{stratum}\n"));
    }
    std::fs::write(out.join("pages.tsv"), tsv).unwrap();
    // 項目は 1 つずつのファイルにする（テストで項目ごとにスナップショットを取るため）。同じ
    // ページから始まる項目には連番を付ける。
    let items_dir = out.join("items");
    let _ = std::fs::remove_dir_all(&items_dir);
    std::fs::create_dir_all(&items_dir).unwrap();
    let mut used: BTreeMap<String, usize> = BTreeMap::new();
    for it in &picked_items {
        let pages: Vec<String> = it["pages"].as_array().unwrap().iter().map(|p| p.as_str().unwrap().split_once('/').unwrap().1.to_string()).collect();
        let base = format!("{}-p{}", it["issue"].as_str().unwrap(), it["page"]);
        let n = used.entry(base.clone()).or_default();
        *n += 1;
        let id = if *n == 1 { base } else { format!("{base}-{n}") };
        let v = json!({ "id": id, "date": it["date"], "title": it["title"], "pages": pages });
        std::fs::write(items_dir.join(format!("{id}.json")), serde_json::to_string_pretty(&v).unwrap() + "\n").unwrap();
    }
    eprintln!("chosen pages {} items {}", chosen.len(), picked_items.len());
}

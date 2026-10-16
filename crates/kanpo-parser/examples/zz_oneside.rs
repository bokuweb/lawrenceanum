use kanpo_parser::model::{Block, Run};
use kanpo_parser::PageInput;
use serde_json::Value;
fn t(r: &[Run]) -> String { r.iter().map(|x| x.text.as_str()).collect::<String>().replace('\n', "⏎") }
fn main() {
    let mut args = std::env::args().skip(1);
    let root = std::path::PathBuf::from(args.next().unwrap());
    let only = args.next();
    let rows_mode = std::env::var("ROWS").is_ok();
    let mut days: Vec<_> = std::fs::read_dir(&root).unwrap().flatten().map(|e| e.path()).filter(|p| p.join("manifest.json").exists()).collect();
    days.sort();
    for day in days {
        let m: Value = serde_json::from_str(&std::fs::read_to_string(day.join("manifest.json")).unwrap()).unwrap();
        for issue in m["issues"].as_array().into_iter().flatten() {
            let dir = issue["dir"].as_str().unwrap_or("");
            if only.as_deref().is_some_and(|o| !o.starts_with(dir)) { continue; }
            let pc = issue["page_count"].as_u64().unwrap_or(0) as u32;
            let mut starts: Vec<u32> = issue["items"].as_array().into_iter().flatten().filter_map(|i| i["page"].as_u64()).map(|p| p as u32).collect();
            starts.sort_unstable(); starts.dedup();
            for item in issue["items"].as_array().into_iter().flatten() {
                let title = item["title"].as_str().unwrap_or("");
                if !(title.contains("改正") || title.contains("廃止") || title.contains("制定")) { continue; }
                let start = item["page"].as_u64().unwrap_or(0) as u32;
                if only.as_deref().is_some_and(|o| o != format!("{dir}:{start}")) { continue; }
                let end = starts.iter().copied().find(|&p| p > start).unwrap_or(pc).max(start).min(start + 24);
                let pages: Vec<PageInput> = (start..=end).filter_map(|n| {
                    let pdf = std::fs::read(day.join(dir).join(format!("{dir}{n:04}.pdf"))).ok()?;
                    Some(PageInput { xhtml: kanpo_parser::bbox_xhtml(&pdf).ok()?, rules: kanpo_parser::page_rules(&pdf).ok() })
                }).collect();
                if pages.is_empty() { continue; }
                let a = kanpo_parser::analyze_item(&pages);
                let Some(i) = kanpo_parser::pick_article(title, &a.articles) else { continue };
                let mut ti = 0;
                for b in &a.articles[i].document.blocks {
                    if let Block::Shinkyu { rows } = b {
                        ti += 1;
                        for (k, r) in rows.iter().enumerate() {
                            if rows_mode { println!("T{ti}:{k}\n  後 {}\n  前 {}", t(&r.after).chars().take(80).collect::<String>(), t(&r.before).chars().take(80).collect::<String>()); continue; }
                            if r.after.is_empty() != r.before.is_empty() {
                                let other = if r.after.is_empty() { t(&r.before) } else { t(&r.after) };
                                println!("{dir}\t{start}\tT{ti}:{k}/{}\t{}\t{}", rows.len(), if r.after.is_empty() {"前"} else {"後"}, other.chars().take(60).collect::<String>());
                            }
                        }
                    }
                }
            }
        }
    }
}

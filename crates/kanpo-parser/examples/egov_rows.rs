//! 改め文の項目ごとに、新旧対照表の欄の文と行の組みを e-Gov（法令 API v2）の改正後・改正前の法令本文と突き合わせる
//! （パーサを直す前後で比べる用）。
//!
//! ```text
//! cargo run --release -p kanpo-parser --example egov_rows -- <archive_dir> [<cache_dir>] [<issue>:<page>]
//! ```
//!
//! `<archive_dir>` は `lawpub kanpo-archive` の出力（`{date}/manifest.json` と `{date}/{号}/{号}{頁}.pdf`）。目次の項目のうち
//! 標題に「改正」を含むものを、本番と同じく PDF から項目単位で解析し（[`kanpo_parser::analyze_item`]）、標題の記事の
//! 新旧対照表を見る。改める省令（「〇財務省令第五十六号」）と、表の前の「X（…号）の一部を次のように改正する」の
//! 改正される法令から、e-Gov の改正後の版（その省令による改正）と改正前の版（その直前の版）を取る。告示など e-Gov に
//! 無いもの、まだ載っていない改正は数えない。
//!
//! - **欄の文**: 欄の各行（注記・「略」を含む行・6 字未満の行を除く）が、その版の本文に含まれるか（[`egov_check`] と同じ）。
//! - **行の組み**: e-Gov の両方の版を条・項・号などの単位に分け、同じ文の単位を組み、残りを文の似方で順序を保って組む
//!   （版の対応）。表の行の改正後欄の文が当たる改正後の版の単位と、改正前欄の文が当たる改正前の版の単位が、版の
//!   対応で組になっていれば「合う」。改正後欄の単位がどれも改正前の別の単位と組になり、改正前欄の単位もどれも改正後の
//!   別の単位と組になっていれば「組み違い」（隣の行と取り違えている）。それ以外（大きく書き換えた規定など）は
//!   「決まらない」。
//!
//! e-Gov の応答は `<cache_dir>`（既定は `.cache/egov-check`）に保存して使い回す。

use kanpo_parser::model::{Block, Run};
use kanpo_parser::PageInput;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const API: &str = "https://laws.e-gov.go.jp/api/2";

fn get(cache: &Path, path: &str) -> anyhow::Result<Value> {
    let name: String = path.chars().map(|c| if c.is_ascii_alphanumeric() || "_.-".contains(c) { c } else { '_' }).take(200).collect();
    let file = cache.join(format!("{name}.json"));
    if let Ok(bytes) = std::fs::read(&file) {
        return Ok(serde_json::from_slice(&bytes)?);
    }
    std::thread::sleep(std::time::Duration::from_millis(300));
    let bytes = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()?
        .get(format!("{API}{path}"))
        .send()?
        .error_for_status()?
        .bytes()?;
    std::fs::create_dir_all(cache)?;
    std::fs::write(&file, &bytes)?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn encode(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// 空白を除き、横線の字と全角の英数字（e-Gov は「ＭＨｚ」、官報は「MHz」のことがある）をそろえる。
fn compact(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| match c {
            _ if "－―‐—−-".contains(c) => '-',
            '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
            // 化学名の角括弧は、官報は「〔〕」、e-Gov は「［］」のことがある。
            '〔' => '[',
            '〕' => ']',
            _ => c,
        })
        .collect::<String>()
        // 単位の合字（官報は「㎎」、e-Gov は「mg」）。
        .replace('㎎', "mg")
        .replace('㎏', "kg")
        .replace('㎖', "ml")
        .replace('㎝', "cm")
        .replace('㎜', "mm")
}

/// 〔〕・［］で囲んだ注記（「〔同上〕」「〔条を加える。〕」「［略］」）を除く。化学名の「〔三－（アミノメチル）フエニル〕」の
/// ような本文の括弧は残す。
fn strip_notes(s: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '〔' || c == '［' {
            let close = if c == '〔' { '〕' } else { '］' };
            if let Some(len) = chars[i + 1..].iter().position(|&d| d == close) {
                let inner: String = chars[i + 1..i + 1 + len].iter().collect();
                if ["略", "同上", "同左", "新設", "削る", "加える", "追加", "注"].iter().any(|m| inner.contains(m)) || inner.trim().is_empty() {
                    i += len + 2;
                    continue;
                }
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// 比べる行か（注記を除いたもの）。省略の行（「略」「同上」「同左」を含む行）、6 字未満の行、表の注記（「備考　表中の…」）、
/// 様式の中の行（e-Gov では図や別のファイル）は見ない。
fn comparable(line: &str) -> Option<String> {
    if ["略", "同上", "同左", "（新設）", "（削る", "備考"].iter().any(|m| line.contains(m)) {
        return None;
    }
    let s = compact(&strip_notes(line));
    (s.chars().count() >= 6).then_some(s)
}

/// 様式の欄か（「別紙様式第１号」「…の様式」「（日本産業規格Ａ４）」「第24号書式」「記載要領」「（第一面）」）。
fn form_cell(cell: &str) -> bool {
    ["様式", "日本産業規格", "号書式", "記載要領", "記載上の注意", "（第一面）"].iter().any(|m| cell.contains(m))
}

/// 「〇内閣府令第八十二号」の「内閣府令第八十二号」（政令・省令・府令・規則・命令の見出し。最後のもの）。
fn heading(text: &str) -> Option<String> {
    let numerals = "〇一二三四五六七八九十百千";
    let mut found = None;
    for (i, _) in text.match_indices('〇') {
        let rest = &text[i + '〇'.len_utf8()..];
        let Some(dai) = rest.char_indices().take(25).find(|&(_, c)| c == '第').map(|(j, _)| j) else { continue };
        let kind = &rest[..dai];
        let num: String = rest[dai + '第'.len_utf8()..].chars().take_while(|c| numerals.contains(*c)).collect();
        let after = &rest[dai + '第'.len_utf8() + num.len()..];
        if !num.is_empty() && after.starts_with('号') && ["政令", "省令", "府令", "規則", "命令"].iter().any(|k| kind.ends_with(k)) {
            found = Some(format!("{kind}第{num}号"));
        }
    }
    found
}

/// 「…（平成十二年総理府令第百二十九号）の一部を次の（表の）ように改正する」の法令番号（最後のもの）。
fn amended_law(text: &str) -> Option<String> {
    // 「次のように改正する」と「次の表のように改正する」。
    let at = ["）の一部を次のように改正する", "）の一部を次の表のように改正する"].iter().filter_map(|m| text.rfind(m)).max()?;
    let open = text[..at].rfind('（')?;
    let num = &text[open + '（'.len_utf8()..at];
    (["明治", "大正", "昭和", "平成", "令和"].iter().any(|e| num.starts_with(e)) && num.ends_with('号')).then(|| num.to_string())
}

fn era_year(date: &str) -> String {
    let year: i32 = date.get(..4).and_then(|y| y.parse().ok()).unwrap_or(2026);
    let n = (year - 2018) as usize;
    let kanji = ["", "元", "二", "三", "四", "五", "六", "七", "八", "九", "十", "十一", "十二", "十三", "十四", "十五"];
    format!("令和{}年", kanji.get(n).copied().unwrap_or("八"))
}

fn runs_text(runs: &[Run]) -> String {
    runs.iter().map(|r| r.text.as_str()).collect()
}

/// 罫線の表を除いたセルの文（表のセルは e-Gov では行ごとの構造なので比べない）。
fn plain_text(runs: &[Run]) -> String {
    runs.iter().filter(|r| r.table.is_none()).map(|r| r.text.as_str()).collect()
}

fn flatten(v: &Value, out: &mut String) {
    match v {
        Value::String(s) => out.push_str(s),
        // 振り仮名（Ruby の Rt）は本文に入れない（官報の本文も振り仮名を除いている）。
        Value::Object(o) if o.get("tag").and_then(Value::as_str) == Some("Rt") => {}
        Value::Object(o) => o.get("children").into_iter().for_each(|c| flatten(c, out)),
        Value::Array(a) => a.iter().for_each(|c| flatten(c, out)),
        _ => {}
    }
}

/// e-Gov の本文を、条・項・号などの単位（入れ子の単位を除いた、その単位自身の文）に分ける。
fn units(v: &Value, out: &mut Vec<String>) {
    const UNIT: [&str; 14] = [
        "Article", "Paragraph", "Item", "Subitem1", "Subitem2", "Subitem3", "Subitem4", "Subitem5", "Chapter", "Section", "Subsection",
        "Division", "SupplProvision", "TableStruct",
    ];
    fn own_text(v: &Value, out: &mut String) {
        match v {
            Value::String(s) => out.push_str(s),
            Value::Object(o) => {
                let tag = o.get("tag").and_then(Value::as_str).unwrap_or("");
                if UNIT.contains(&tag) || tag == "Rt" {
                    return;
                }
                o.get("children").into_iter().for_each(|c| own_text(c, out));
            }
            Value::Array(a) => a.iter().for_each(|c| own_text(c, out)),
            _ => {}
        }
    }
    match v {
        Value::Object(o) => {
            let tag = o.get("tag").and_then(Value::as_str).unwrap_or("");
            if UNIT.contains(&tag) {
                let mut s = String::new();
                o.get("children").into_iter().for_each(|c| own_text(c, &mut s));
                out.push(compact(&s));
            }
            o.get("children").into_iter().for_each(|c| units(c, out));
        }
        Value::Array(a) => a.iter().for_each(|c| units(c, out)),
        _ => {}
    }
}

/// 2 字の組の Jaccard 係数。
fn similarity(a: &str, b: &str) -> f32 {
    use std::collections::HashSet;
    let grams = |s: &str| -> HashSet<(char, char)> {
        let cs: Vec<char> = s.chars().collect();
        cs.windows(2).map(|w| (w[0], w[1])).collect()
    };
    let (x, y) = (grams(a), grams(b));
    if x.is_empty() || y.is_empty() {
        return 0.0;
    }
    x.intersection(&y).count() as f32 / x.union(&y).count() as f32
}

/// 2 つの版の単位を、順序を保って組む（同じ文は 2、似た文（0.5 以上）は似方の重みで最大にする）。`after[i]` の組は `map[i]`。
fn align_versions(after: &[String], before: &[String]) -> Vec<Option<usize>> {
    let (n, m) = (after.len(), before.len());
    let mut map = vec![None; n];
    let score = |i: usize, j: usize| -> f32 {
        if after[i] == before[j] {
            2.0
        } else if after[i].len() > 6 && before[j].len() > 6 {
            let s = similarity(&after[i], &before[j]);
            if s >= 0.5 { s } else { 0.0 }
        } else {
            0.0
        }
    };
    // 長い法令（数千単位）で n×m の似方を全部は計れないので、同じ文の単位を手がかりに区切って組む。
    // 目印: 両方の版で 1 回だけ出る同じ文の組のうち、順序の保たれる最も長い並び（最長増加部分列）。
    let anchors: Vec<(usize, usize)> = {
        let mut count_a: BTreeMap<&str, usize> = BTreeMap::new();
        for a in after {
            *count_a.entry(a.as_str()).or_default() += 1;
        }
        let mut index: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
        for (j, b) in before.iter().enumerate() {
            index.entry(b.as_str()).or_default().push(j);
        }
        let pairs: Vec<(usize, usize)> = after
            .iter()
            .enumerate()
            .filter(|(_, a)| a.chars().count() >= 12 && count_a[a.as_str()] == 1)
            .filter_map(|(i, a)| index.get(a.as_str()).filter(|js| js.len() == 1).map(|js| (i, js[0])))
            .collect();
        // j の最長増加部分列（O(n log n)）。
        let mut tails: Vec<usize> = Vec::new(); // tails[k] = 長さ k+1 の列の末尾の pairs の添字
        let mut parent: Vec<Option<usize>> = vec![None; pairs.len()];
        for (p, &(_, j)) in pairs.iter().enumerate() {
            let k = tails.partition_point(|&t| pairs[t].1 < j);
            parent[p] = k.checked_sub(1).map(|k| tails[k]);
            if k == tails.len() {
                tails.push(p);
            } else {
                tails[k] = p;
            }
        }
        let mut out = Vec::new();
        let mut cur = tails.last().copied();
        while let Some(p) = cur {
            out.push(pairs[p]);
            cur = parent[p];
        }
        out.reverse();
        out
    };
    let mut prev = (0usize, 0usize);
    for &(ai, bj) in anchors.iter().chain(std::iter::once(&(n, m))) {
        // 区間 [prev.0, ai) × [prev.1, bj) を組む。
        let (r0, r1, c0, c1) = (prev.0, ai, prev.1, bj);
        if r1 > r0 && c1 > c0 && (r1 - r0) * (c1 - c0) <= 400_000 {
            let (h, w) = (r1 - r0, c1 - c0);
            let mut t = vec![vec![0f32; w + 1]; h + 1];
            for i in (0..h).rev() {
                for j in (0..w).rev() {
                    let s = score(r0 + i, c0 + j);
                    let take = if s > 0.0 { t[i + 1][j + 1] + s } else { f32::MIN };
                    t[i][j] = take.max(t[i + 1][j]).max(t[i][j + 1]);
                }
            }
            let (mut i, mut j) = (0, 0);
            while i < h && j < w {
                let s = score(r0 + i, c0 + j);
                if s > 0.0 && (t[i][j] - (t[i + 1][j + 1] + s)).abs() < 1e-4 {
                    map[r0 + i] = Some(c0 + j);
                    i += 1;
                    j += 1;
                } else if t[i + 1][j] >= t[i][j + 1] {
                    i += 1;
                } else {
                    j += 1;
                }
            }
        }
        if ai < n {
            map[ai] = Some(bj);
        }
        prev = (ai + 1, bj + 1);
    }
    map
}

/// セルの文が当たる単位（行を文に含む単位。同じ文の規定がいくつもある法令では、そのすべて（1 行に 5 つまで））。
fn hit_units(cell: &str, units: &[String]) -> Vec<usize> {
    let mut hits = Vec::new();
    for line in cell.lines() {
        let Some(s) = comparable(line) else { continue };
        let found: Vec<usize> = (0..units.len()).filter(|&u| units[u].contains(&s)).collect();
        if found.len() <= 5 {
            hits.extend(found);
        }
    }
    hits.sort_unstable();
    hits.dedup();
    hits
}

struct Version {
    full: String,
    units: Vec<String>,
}

fn load_version(cache: &Path, rev: &str) -> anyhow::Result<Version> {
    let data = get(cache, &format!("/law_data/{rev}?law_full_text_format=json"))?;
    let mut list = Vec::new();
    units(&data["law_full_text"], &mut list);
    let mut full = String::new();
    flatten(&data["law_full_text"], &mut full);
    Ok(Version { full: compact(&full), units: list })
}

/// 改める省令による改正後の版と、その直前の版の組。段階的に施行される改正（同じ省令の版がいくつもある）では、
/// その全部を候補にする。
fn versions(cache: &Path, amend: &str, law: &str) -> anyhow::Result<Vec<(Version, Version)>> {
    let laws = get(cache, &format!("/laws?law_num={}", encode(law)))?;
    let Some(law_id) = laws["laws"][0]["law_info"]["law_id"].as_str() else { return Ok(Vec::new()) };
    let revisions = get(cache, &format!("/law_revisions/{law_id}"))?;
    let revisions: Vec<&Value> = revisions["revisions"].as_array().into_iter().flatten().collect();
    let date = |r: &Value| r["amendment_enforcement_date"].as_str().unwrap_or("").to_string();
    let id = |r: &Value| r["law_revision_id"].as_str().map(str::to_string);
    let mut out = Vec::new();
    for target in revisions.iter().filter(|r| r["amendment_law_num"].as_str() == Some(amend)) {
        let Some(after) = id(target) else { continue };
        let Some(before) = revisions.iter().filter(|r| date(r) < date(target)).max_by_key(|r| date(r)).and_then(|r| id(r)) else { continue };
        out.push((load_version(cache, &after)?, load_version(cache, &before)?));
    }
    Ok(out)
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let root = PathBuf::from(args.next().expect("usage: egov_rows <archive_dir> [<cache_dir>] [<issue>:<page>]"));
    let cache = args.next().map(PathBuf::from).unwrap_or_else(|| PathBuf::from(".cache/egov-check"));
    let only = args.next();
    let mut days: Vec<PathBuf> = std::fs::read_dir(&root)?.flatten().map(|e| e.path()).filter(|p| p.join("manifest.json").exists()).collect();
    days.sort();
    let mut cached: BTreeMap<(String, String), Vec<(Version, Version)>> = BTreeMap::new();
    let (mut lines_total, mut lines_bad, mut rows_total, mut rows_ok, mut rows_bad, mut rows_unknown) = (0, 0, 0, 0, 0, 0);
    let mut items_checked = 0;
    for day in days {
        let date = day.file_name().unwrap().to_string_lossy().to_string();
        let manifest: Value = serde_json::from_str(&std::fs::read_to_string(day.join("manifest.json"))?)?;
        for issue in manifest["issues"].as_array().into_iter().flatten() {
            let dir = issue["dir"].as_str().unwrap_or("");
            if only.as_deref().is_some_and(|o| !o.starts_with(dir)) {
                continue;
            }
            let page_count = issue["page_count"].as_u64().unwrap_or(0) as u32;
            let mut starts: Vec<u32> = issue["items"].as_array().into_iter().flatten().filter_map(|i| i["page"].as_u64()).map(|p| p as u32).collect();
            starts.sort_unstable();
            starts.dedup();
            for item in issue["items"].as_array().into_iter().flatten() {
                let title = item["title"].as_str().unwrap_or("");
                if !title.contains("改正") {
                    continue;
                }
                let start = item["page"].as_u64().unwrap_or(0) as u32;
                if only.as_deref().is_some_and(|o| o != format!("{dir}:{start}")) {
                    continue;
                }
                let end = starts.iter().copied().find(|&p| p > start).unwrap_or(page_count).max(start).min(start + 24);
                let pages: Vec<PageInput> = (start..=end)
                    .filter_map(|n| {
                        let pdf = std::fs::read(day.join(dir).join(format!("{dir}{n:04}.pdf"))).ok()?;
                        Some(PageInput { xhtml: kanpo_parser::bbox_xhtml(&pdf).ok()?, rules: kanpo_parser::page_rules(&pdf).ok() })
                    })
                    .collect();
                if pages.is_empty() {
                    continue;
                }
                let analysis = kanpo_parser::analyze_item(&pages);
                let Some(i) = kanpo_parser::pick_article(title, &analysis.articles) else { continue };
                let article = &analysis.articles[i];
                // 改める省令は記事の頭の見出し、改正される法令は表の前の改め文から。
                let mut amend: Option<String> = None;
                let mut law: Option<String> = None;
                let mut table_index = 0;
                for block in &article.document.blocks {
                    match block {
                        Block::Paragraph { runs } => {
                            let text = runs_text(runs);
                            if amend.is_none() {
                                amend = heading(&text).map(|h| format!("{}{h}", era_year(&date)));
                            }
                            if let Some(l) = amended_law(&text) {
                                law = Some(l);
                            }
                        }
                        Block::Shinkyu { rows, .. } => {
                            table_index += 1;
                            let (Some(amend), Some(law)) = (amend.as_ref(), law.as_ref()) else { continue };
                            let candidates = cached.entry((amend.clone(), law.clone())).or_insert_with(|| {
                                versions(&cache, amend, law).unwrap_or_else(|e| {
                                    eprintln!("{dir}:{start} {amend} → {law}: {e}");
                                    Vec::new()
                                })
                            });
                            // 段階施行の版のうち、欄の文がいちばん多く含まれるもの。
                            let contained = |(a, b): &(Version, Version)| -> usize {
                                rows.iter()
                                    .map(|r| {
                                        let count = |cell: &str, v: &Version| cell.lines().filter_map(comparable).filter(|s| v.full.contains(s.as_str())).count();
                                        count(&plain_text(&r.after), a) + count(&plain_text(&r.before), b)
                                    })
                                    .sum()
                            };
                            let Some((after_v, before_v)) = candidates.iter().max_by_key(|c| contained(c)) else { continue };
                            items_checked += 1;
                            let map = align_versions(&after_v.units, &before_v.units);
                            let mut inverse = vec![None; before_v.units.len()];
                            for (a, b) in map.iter().enumerate() {
                                if let Some(b) = b {
                                    inverse[*b] = Some(a);
                                }
                            }
                            for (k, row) in rows.iter().enumerate() {
                                let (after, before) = (plain_text(&row.after), plain_text(&row.before));
                                // 欄の文（様式の欄は除く）。
                                for (cell, v, side) in [(&after, after_v, "後"), (&before, before_v, "前")] {
                                    if form_cell(cell) {
                                        continue;
                                    }
                                    for line in cell.lines() {
                                        let Some(s) = comparable(line) else { continue };
                                        lines_total += 1;
                                        if !v.full.contains(&s) {
                                            lines_bad += 1;
                                            // e-Gov の本文で、行の頭から最も長く一致する所の続き（どこから食い違うか）。
                                            let chars: Vec<char> = s.chars().collect();
                                            let mut n = chars.len();
                                            let mut near = String::new();
                                            while n >= 6 {
                                                let head: String = chars[..n].iter().collect();
                                                if let Some(at) = v.full.find(&head) {
                                                    near = v.full[at..].chars().take(chars.len() + 6).collect();
                                                    break;
                                                }
                                                n -= 1;
                                            }
                                            println!(
                                                "LINE\t{dir}:{start}\tT{table_index}:{k}\t{side}\t{}\t{n}/{}\te-Gov: {near}",
                                                s.chars().take(70).collect::<String>(),
                                                chars.len()
                                            );
                                        }
                                    }
                                }
                                // 行の組み。
                                let a_units = hit_units(&after, &after_v.units);
                                let b_units = hit_units(&before, &before_v.units);
                                if a_units.is_empty() || b_units.is_empty() {
                                    continue;
                                }
                                rows_total += 1;
                                let matched = a_units.iter().any(|&a| map[a].is_some_and(|b| b_units.contains(&b)));
                                let a_elsewhere = a_units.iter().all(|&a| map[a].is_some_and(|b| !b_units.contains(&b)));
                                let b_elsewhere = b_units.iter().all(|&b| inverse[b].is_some_and(|a| !a_units.contains(&a)));
                                if matched {
                                    rows_ok += 1;
                                } else if a_elsewhere && b_elsewhere {
                                    rows_bad += 1;
                                    println!(
                                        "ROW\t{dir}:{start}\tT{table_index}:{k}\t後 {}\t前 {}",
                                        after.replace('\n', "⏎").chars().take(50).collect::<String>(),
                                        before.replace('\n', "⏎").chars().take(50).collect::<String>()
                                    );
                                } else {
                                    rows_unknown += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    println!(
        "計: 表 {items_checked}・欄の行 {lines_total}（e-Gov に含まれない {lines_bad}）・行の組み {rows_total}（合う {rows_ok}・組み違い {rows_bad}・決まらない {rows_unknown}）"
    );
    Ok(())
}

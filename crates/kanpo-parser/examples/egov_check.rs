//! 改め文の新旧対照表の改正後欄・改正前欄を、e-Gov（法令 API v2）の改正後・改正前の法令本文と突き合わせ、
//! 読み順・字の誤りの候補を出す（パーサを直す前後で比べる用）。
//!
//! ```text
//! cargo run --release -p kanpo-parser --example egov_check -- <dir> [<cache_dir>]
//! ```
//!
//! `<dir>` の `.bbox.html`（と `.rules.json`。examples/prepare が作る）をファイル名の順に解析する。
//! 「〇〇〇省令第N号」の見出しの後の「X（令和…号）の一部を次のように改正する」で改正される法令と改める
//! 省令を決め、その後に続く改正後欄・改正前欄の行を集める（同じ号のページの間で引き継ぐ）。e-Gov から、
//! 改正される法令の、その省令による改正後の版と直前の版の本文を取り、欄の各行と、続く 2 行をつないだものが
//! 本文（空白を除く）に含まれるかを見る（次の行が条の見出し・条・章・節の頭なら、間に省いた規定が
//! あり得るので 2 行は見ない）。
//!
//! 含まれない行は、パーサの誤り（字の取り違え・別の欄の字の混入）か、e-Gov との表記の違い（様式の図、
//! 目次の「－」と「―」など）。続く 2 行が含まれないのは、読み順の誤りか、間に略した規定がある所。
//! 「略」を含む行は、e-Gov の本文には略さずに入っているので見ない。〔〕・［］の注記（「〔同上〕」
//! 「〔条を加える。〕」）は除いて比べる。e-Gov の応答は `<cache_dir>`
//! （既定は `.cache/egov-check`）に保存して使い回す。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const API: &str = "https://laws.e-gov.go.jp/api/2";

fn get(cache: &Path, path: &str) -> anyhow::Result<serde_json::Value> {
    let name: String = path.chars().map(|c| if c.is_ascii_alphanumeric() || "_.-".contains(c) { c } else { '_' }).take(200).collect();
    let file = cache.join(format!("{name}.json"));
    if let Ok(bytes) = std::fs::read(&file) {
        return Ok(serde_json::from_slice(&bytes)?);
    }
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

fn flatten(v: &serde_json::Value, out: &mut String) {
    match v {
        serde_json::Value::String(s) => out.push_str(s),
        // 振り仮名（Ruby の Rt）は本文に入れない（官報の本文も振り仮名を除いている）。
        serde_json::Value::Object(o) if o.get("tag").and_then(serde_json::Value::as_str) == Some("Rt") => {}
        serde_json::Value::Object(o) => o.get("children").into_iter().for_each(|c| flatten(c, out)),
        serde_json::Value::Array(a) => a.iter().for_each(|c| flatten(c, out)),
        _ => {}
    }
}

/// 空白を除き、横線の字（「－」「―」「‐」）を 1 つにする（目次の「第一条－第五条」は e-Gov では「―」）。
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).map(|c| if "－―‐—−-".contains(c) { '-' } else { c }).collect()
}

/// 〔〕・［］で囲んだ注記を除く。
fn strip_notes(s: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in s.chars() {
        match c {
            '〔' | '［' => depth += 1,
            '〕' | '］' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

fn encode(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// 「〇内閣府令第八十二号」の「内閣府令第八十二号」（政令・省令・府令・規則・命令の見出しだけ）。
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

/// 令和の年（官報の号の日付から）。
fn era_year(file: &str) -> String {
    let year: i32 = file.get(..4).and_then(|y| y.parse().ok()).unwrap_or(2026);
    let n = (year - 2018) as usize;
    let kanji = ["", "元", "二", "三", "四", "五", "六", "七", "八", "九", "十", "十一", "十二", "十三", "十四", "十五"];
    format!("令和{}年", kanji.get(n).copied().unwrap_or("八"))
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().expect("usage: egov_check <dir> [<cache_dir>]"));
    let cache = args.next().map(PathBuf::from).unwrap_or_else(|| PathBuf::from(".cache/egov-check"));
    let mut pages: Vec<PathBuf> = std::fs::read_dir(&dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().ends_with(".bbox.html"))
        .collect();
    pages.sort();

    // (改める省令の番号, 改正される法令の番号, 改正後欄か) → 欄の (ページ, 行)。
    let mut columns: BTreeMap<(String, String, bool), Vec<(String, String)>> = BTreeMap::new();
    let mut ctx: Option<(String, String, Option<String>)> = None; // (号, 改める省令, 改正される法令)
    for path in &pages {
        let file = path.file_name().unwrap().to_string_lossy().replace(".bbox.html", "");
        let issue: String = file.chars().take(14).collect();
        if ctx.as_ref().is_some_and(|c| c.0 != issue) {
            ctx = None;
        }
        let xhtml = std::fs::read_to_string(path)?;
        let rules = std::fs::read_to_string(path.to_string_lossy().replace(".bbox.html", ".rules.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok());
        let analysis = kanpo_parser::analyze_with_rules(&xhtml, &[rules]);
        for page in &analysis.pages {
            for region in &page.regions {
                let text: String = region.lines.iter().map(|l| l.text.as_str()).collect();
                let body = matches!(region.role, kanpo_parser::trace::Role::Preamble | kanpo_parser::trace::Role::Body);
                if body && text.contains('〇') && text.contains('号') {
                    // 新しい記事の見出し。政令・省令の見出しでなければ、前の省令の続きではない。
                    if let Some(h) = heading(&text) {
                        ctx = Some((issue.clone(), format!("{}{h}", era_year(&file)), None));
                    } else if text.chars().filter(|&c| c == '〇').count() > 0 && text.contains("告示") {
                        ctx = None;
                    }
                }
                if let (Some(c), Some(law)) = (ctx.as_mut(), amended_law(&text)) {
                    c.2 = Some(law);
                }
                let side = match region.role {
                    kanpo_parser::trace::Role::After => Some(true),
                    kanpo_parser::trace::Role::Before => Some(false),
                    _ => None,
                };
                if let (Some(side), Some((_, amend, Some(law)))) = (side, &ctx) {
                    let lines = columns.entry((amend.clone(), law.clone(), side)).or_default();
                    lines.extend(region.lines.iter().filter(|l| l.table.is_none()).map(|l| (file.clone(), l.text.clone())));
                }
            }
        }
    }

    let (mut total, mut bad_total, mut pairs_total, mut bad_pairs_total) = (0, 0, 0, 0);
    for ((amend, law, side), lines) in &columns {
        let label = if *side { "改正後" } else { "改正前" };
        // 改正後欄は、その省令による改正後の版。改正前欄は、その直前の版（施行日がそれより前で最も新しい版）。
        let found = (|| -> anyhow::Result<Option<String>> {
            let laws = get(&cache, &format!("/laws?law_num={}", encode(law)))?;
            let Some(law_id) = laws["laws"][0]["law_info"]["law_id"].as_str() else { return Ok(None) };
            let revisions = get(&cache, &format!("/law_revisions/{law_id}"))?;
            let revisions: Vec<&serde_json::Value> = revisions["revisions"].as_array().into_iter().flatten().collect();
            let date = |r: &serde_json::Value| r["amendment_enforcement_date"].as_str().unwrap_or("").to_string();
            let Some(target) = revisions.iter().find(|r| r["amendment_law_num"].as_str() == Some(amend.as_str())) else {
                return Ok(None);
            };
            let rev = if *side {
                Some(*target)
            } else {
                revisions.iter().filter(|r| date(r) < date(target)).max_by_key(|r| date(r)).copied()
            };
            let Some(rev) = rev.and_then(|r| r["law_revision_id"].as_str()) else { return Ok(None) };
            let data = get(&cache, &format!("/law_data/{rev}?law_full_text_format=json"))?;
            let mut text = String::new();
            flatten(&data["law_full_text"], &mut text);
            Ok(Some(compact(&text)))
        })();
        let full = match found {
            Ok(Some(full)) => full,
            Ok(None) => {
                println!("== {amend} → {law}（{label}）: e-Gov に本文が無い");
                continue;
            }
            Err(e) => {
                println!("== {amend} → {law}（{label}）: {e}");
                continue;
            }
        };
        // 注記（「〔同上〕」「〔条を加える。〕」「［略］」）は法令の本文ではないので除き、残りが短い行は見ない。
        let lines: Vec<(String, String)> = lines.iter().map(|(p, l)| (p.clone(), strip_notes(l))).collect();
        let lines = &lines;
        let checked = |s: &str| {
            let s = compact(s);
            !s.contains('略') && s.chars().count() >= 6
        };
        let bad: Vec<&(String, String)> = lines.iter().filter(|(_, l)| checked(l) && !full.contains(&compact(l))).collect();
        let pairs: Vec<_> = lines
            .iter()
            .zip(lines.iter().skip(1))
            .filter(|(a, b)| a.0 == b.0 && checked(&a.1) && checked(&b.1) && full.contains(&compact(&a.1)) && full.contains(&compact(&b.1)))
            .collect();
        // 次の行が条の見出し（「（…）」）・条・章・節の頭なら、間に略さずに省いた規定があることがある。
        let starts_provision = |s: &str| {
            let s = compact(s);
            s.starts_with('（') || (s.starts_with('第') && s.chars().take(12).any(|c| "条章節款".contains(c)))
        };
        let bad_pairs: Vec<_> =
            pairs.iter().filter(|(a, b)| !starts_provision(&b.1) && !full.contains(&(compact(&a.1) + &compact(&b.1)))).collect();
        let n = lines.iter().filter(|(_, l)| checked(l)).count();
        total += n;
        bad_total += bad.len();
        pairs_total += pairs.len();
        bad_pairs_total += bad_pairs.len();
        println!("== {amend} → {law}（{label}）: 行 {n}（含まれない {}）・続く 2 行 {}（含まれない {}）", bad.len(), pairs.len(), bad_pairs.len());
        for (page, line) in bad {
            println!("   行  {page}  {line}");
        }
        for ((page, a), (_, b)) in bad_pairs {
            let tail: String = a.chars().rev().take(24).collect::<Vec<_>>().into_iter().rev().collect();
            let head: String = b.chars().take(24).collect();
            println!("   2行 {page}  …{tail} ｜ {head}…");
        }
    }
    println!("計: 行 {total}（含まれない {bad_total}）・続く 2 行 {pairs_total}（含まれない {bad_pairs_total}）");
    Ok(())
}

//! 官報の項目ごとの本文（`kanpo-reextract` の出力の `amend_text`・`amend_document`）が字の単位で正しいかを測る。
//!
//! ```text
//! cargo run --release -p kanpo-parser --example text_audit -- <corpus_dir> <archive_dir> <out_dir> [<egov_cache>]
//! ```
//!
//! `<corpus_dir>` は日ごとの JSON（`date`・`issues[].items[]`）、`<archive_dir>` は `lawpub kanpo-archive` の出力
//! （日ごとの `manifest.json` と PDF）。次の 2 つを `<out_dir>/egov.jsonl`・`<out_dir>/glyph.jsonl` に書き、合計を出す。
//!
//! 1. e-Gov との突き合わせ。本文の頭の「〇環境省令第二十号」から法令番号を作り、e-Gov（法令 API v2）に
//!    その法令があれば（新しく定めた法令）、制定時の版の本文と全文を並べる。無ければ（改正する法令）、
//!    新旧対照表の改正後欄・改正前欄の各欄を、改正される法令の、その法令による改正後の版・直前の版の本文の中に置く
//!    （欄は法令の一部分なので、欄ごとに本文の中の場所を k 字の一致で決め、その前後だけと並べる）。
//!    どちらも空白を除き、全角の英数字を半角に、横線の字を 1 つにしてから字の単位で並べ、合う字の割合と、
//!    落ちた字（e-Gov にあり官報の出力に無い）・入った字（その逆）・並びの違う字（入った字が e-Gov の別の
//!    場所にある・落ちた字が出力の別の場所にある）の区間を出す。区間のページは、前後の字で PDF のページの字から探す。
//! 2. PDF の字との突き合わせ（すべての項目）。各ページのテキスト層の字（座標付きテキストの語）が、その号の
//!    どれかの項目の出力に現れるか（前後と合わせた k 字が出力にあるか）を見て、どこにも無い字の並びを
//!    落ちとして出す。項目の出力の字の数がその項目のページの字の数より多い字は、重なり（または作った字）として出す。
//!
//! e-Gov の応答は `<egov_cache>`（既定は `<out_dir>/egov-cache`）に保存して使い回し、取りに行くたびに 1 秒待つ。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use serde_json::{json, Value};
use similar::{Algorithm, DiffTag};

const API: &str = "https://laws.e-gov.go.jp/api/2";
/// 場所を決める・字が出力にあるかを見る一致の長さ。
const K: usize = 6;
const GLYPH_K: usize = 4;

fn get(cache: &Path, path: &str) -> anyhow::Result<Value> {
    let name: String = path.chars().map(|c| if c.is_ascii_alphanumeric() || "_.-".contains(c) { c } else { '_' }).take(200).collect();
    let file = cache.join(format!("{name}.json"));
    if let Ok(bytes) = std::fs::read(&file) {
        return Ok(serde_json::from_slice(&bytes)?);
    }
    std::thread::sleep(std::time::Duration::from_secs(1));
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

/// 縦組みの約物（U+FE10–FE4F）と、その互換分解（Unicode の NFKC）。PDF のテキスト層は縦組みの形のことがある。
const VERTICAL: &str = "︐,︑、︒。︓:︔;︕!︖?︗〖︘〗︱—︲–︳_︴_︵(︶)︷{︸}︹〔︺〕︻【︼】︽《︾》︿〈﹀〉﹁「﹂」﹃『﹄』﹇[﹈]﹍_﹎_﹏_";

/// 空白を除き、縦組みの約物を横組みの字に、全角の英数字・記号を半角に、横線の字を 1 つにする。
fn norm(s: &str) -> Vec<char> {
    let vertical: HashMap<char, char> = {
        let v: Vec<char> = VERTICAL.chars().collect();
        v.chunks(2).map(|p| (p[0], p[1])).collect()
    };
    s.chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| vertical.get(&c).copied().unwrap_or(c))
        .map(|c| match c {
            '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
            _ => c,
        })
        .map(|c| if "－―‐—−-–".contains(c) { '-' } else { c })
        .collect()
}

fn hash(w: &[char]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    w.hash(&mut h);
    h.finish()
}

/// e-Gov の本文と、その k 字の一致の場所。
struct Text {
    chars: Vec<char>,
    index: HashMap<u64, Vec<u32>>,
}

impl Text {
    fn new(s: &str) -> Self {
        let chars = norm(s);
        let mut index: HashMap<u64, Vec<u32>> = HashMap::new();
        for i in 0..chars.len().saturating_sub(K - 1) {
            index.entry(hash(&chars[i..i + K])).or_default().push(i as u32);
        }
        Text { chars, index }
    }

    fn contains(&self, s: &[char]) -> bool {
        s.len() >= K && s.windows(K).all(|w| self.index.contains_key(&hash(w)))
    }
}

#[derive(Default)]
struct Aligned {
    matched: usize,
    /// 並べた e-Gov の字の数（欄なら置いた場所の、最初に合う字から最後に合う字まで）。
    truth: usize,
    /// (出力の中の位置, 字)。
    dropped: Vec<(usize, String)>,
    inserted: Vec<(usize, String)>,
}

/// `s`（官報の出力）を `t` の中に置いて並べる。`whole` なら `t` の全体と並べる。
fn align(s: &[char], t: &Text, whole: bool) -> Option<Aligned> {
    let (lo, hi) = if whole {
        (0, t.chars.len())
    } else {
        let mut votes: HashMap<i64, usize> = HashMap::new();
        for (i, w) in s.windows(K).enumerate() {
            let Some(ps) = t.index.get(&hash(w)) else { continue };
            if ps.len() > 40 {
                continue;
            }
            for &p in ps {
                *votes.entry(p as i64 - i as i64).or_default() += 1;
            }
        }
        let (&d, _) = votes.iter().max_by_key(|&(d, v)| (*v, -d))?;
        let pad = s.len() as i64 / 4 + 20;
        ((d - pad).max(0) as usize, ((d + s.len() as i64 + pad).max(0) as usize).min(t.chars.len()))
    };
    let window = &t.chars[lo..hi.max(lo)];
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let ops = similar::capture_diff_slices_deadline(Algorithm::Myers, window, s, Some(deadline));
    let tags: Vec<_> = ops.iter().map(|op| op.as_tag_tuple()).collect();
    // 欄の端の合う字が k 字に満たない（略した規定の番号「一・二」が残った）なら、そこまでを置いた場所に入れない。
    let anchor = |(tag, old, _): &(DiffTag, std::ops::Range<usize>, std::ops::Range<usize>)| *tag == DiffTag::Equal && (whole || old.len() >= K);
    let first = tags.iter().position(anchor)?;
    let last = tags.iter().rposition(anchor)?;
    let mut a = Aligned::default();
    a.truth = tags[last].1.end - tags[first].1.start;
    if whole {
        a.truth = t.chars.len();
    }
    for (i, (tag, old, new)) in tags.iter().enumerate() {
        match tag {
            DiffTag::Equal => a.matched += old.len(),
            _ => {
                if !new.is_empty() {
                    a.inserted.push((new.start, s[new.clone()].iter().collect()));
                }
                if !old.is_empty() && (whole || (first..=last).contains(&i)) {
                    a.dropped.push((new.start, window[old.clone()].iter().collect()));
                }
            }
        }
    }
    Some(a)
}

/// 欄を注記で区切り、注記を除いた比べられる部分（正規化済み、短い残り（「二〜八」）は除く）を返す。注記は
/// 〔〕で囲んだもの（「〔略〕」）と、（）・［］で囲んだもののうち囲みごと e-Gov の本文に無いもの（「（略）」
/// 「［（１）〜（４）略］」。法令の本文の「（１）」や化学名の「［」は本文にある）。
fn segments(cell: &str, truth: &Text) -> Vec<Vec<char>> {
    let c = norm(cell);
    let mut out = vec![Vec::new()];
    let mut i = 0;
    while i < c.len() {
        let close = match c[i] {
            '〔' => Some('〕'),
            '[' => Some(']'),
            '(' => Some(')'),
            _ => None,
        };
        // 囲みの中の囲み（「［（１）〜（４）略］」）を数えて閉じを探す。
        let end = close.and_then(|close| {
            let mut depth = 0;
            (i..c.len()).find(|&j| {
                if c[j] == c[i] {
                    depth += 1;
                } else if c[j] == close {
                    depth -= 1;
                }
                depth == 0
            })
        });
        let note = end.filter(|&e| c[i] == '〔' || !truth.chars.windows(e + 1 - i).any(|w| w == &c[i..=e]));
        match note {
            Some(e) => {
                out.push(Vec::new());
                i = e + 1;
            }
            None => {
                out.last_mut().unwrap().push(c[i]);
                i += 1;
            }
        }
    }
    out.retain(|s| s.len() >= K * 2);
    out
}

/// 「〇環境省令第二十号」の「環境省令第二十号」。
fn heading(text: &str) -> Option<String> {
    let numerals = "〇一二三四五六七八九十百千";
    let line = text.lines().next()?.trim();
    let rest = line.strip_prefix('〇')?;
    let dai = rest.char_indices().take(25).find(|&(_, c)| c == '第').map(|(j, _)| j)?;
    let num: String = rest[dai + '第'.len_utf8()..].chars().take_while(|c| numerals.contains(*c)).collect();
    (!num.is_empty() && rest[dai + '第'.len_utf8() + num.len()..].starts_with('号')).then(|| format!("{}第{num}号", &rest[..dai]))
}

/// 「…（平成十二年総理府令第百二十九号）の一部を次の（表の）ように改正する」の法令番号（段落の最後のもの）。
fn amended_law(text: &str) -> Option<String> {
    let at = ["）の一部を次のように改正する", "）の一部を次の表のように改正する"].iter().filter_map(|m| text.rfind(m)).max()?;
    let open = text[..at].rfind('（')?;
    let num = &text[open + '（'.len_utf8()..at];
    (["明治", "大正", "昭和", "平成", "令和"].iter().any(|e| num.starts_with(e)) && num.ends_with('号')).then(|| num.to_string())
}

fn era_year(date: &str) -> String {
    let year: i32 = date.get(..4).and_then(|y| y.parse().ok()).unwrap_or(2026);
    let kanji = ["", "元", "二", "三", "四", "五", "六", "七", "八", "九", "十", "十一", "十二", "十三", "十四", "十五"];
    format!("令和{}年", kanji.get((year - 2018) as usize).copied().unwrap_or("八"))
}

struct Egov {
    cache: PathBuf,
    texts: HashMap<String, Option<std::rc::Rc<Text>>>,
}

impl Egov {
    fn law(&self, num: &str) -> Option<Value> {
        let laws = get(&self.cache, &format!("/laws?law_num={}", encode(num))).ok()?;
        laws["laws"].as_array()?.iter().find(|l| l["law_info"]["law_num"].as_str() == Some(num)).cloned()
    }

    fn revisions(&self, law_id: &str) -> Vec<Value> {
        get(&self.cache, &format!("/law_revisions/{law_id}"))
            .ok()
            .and_then(|r| r["revisions"].as_array().cloned())
            .unwrap_or_default()
    }

    fn text(&mut self, revision: &str) -> Option<std::rc::Rc<Text>> {
        if let Some(t) = self.texts.get(revision) {
            return t.clone();
        }
        let t = get(&self.cache, &format!("/law_data/{revision}?law_full_text_format=json")).ok().map(|d| {
            let mut s = String::new();
            flatten(&d["law_full_text"], &mut s);
            std::rc::Rc::new(Text::new(&s))
        });
        self.texts.insert(revision.to_string(), t.clone());
        t
    }

    /// `base` の、`amend` による改正後の版と直前の版。
    fn amended(&mut self, base: &str, amend: &str) -> Option<(std::rc::Rc<Text>, Option<std::rc::Rc<Text>>)> {
        let law_id = self.law(base)?["law_info"]["law_id"].as_str()?.to_string();
        let revisions = self.revisions(&law_id);
        let date = |r: &Value| r["amendment_enforcement_date"].as_str().unwrap_or("").to_string();
        let target = revisions.iter().find(|r| r["amendment_law_num"].as_str() == Some(amend))?.clone();
        let before = revisions.iter().filter(|r| date(r) < date(&target)).max_by_key(|r| date(r)).cloned();
        let after = self.text(target["law_revision_id"].as_str()?)?;
        let before = before.and_then(|b| b["law_revision_id"].as_str().map(str::to_string)).and_then(|b| self.text(&b));
        Some((after, before))
    }
}

/// 号の各ページの字（座標付きテキストの語の中身、正規化済み）。
fn page_glyphs(pdf: &Path) -> Vec<char> {
    let Ok(bytes) = std::fs::read(pdf) else { return Vec::new() };
    let Ok(xhtml) = kanpo_parser::bbox_xhtml(&bytes) else { return Vec::new() };
    let mut s = String::new();
    for part in xhtml.split("<word ").skip(1) {
        let Some(open) = part.find('>') else { continue };
        let Some(close) = part[open..].find("</word>") else { continue };
        s.push_str(&part[open + 1..open + close].replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&amp;", "&"));
    }
    norm(&s)
}

struct Item {
    date: String,
    issue: String,
    title: String,
    page: usize,
    end: usize,
    text: String,
    doc: Value,
}

fn snippet(s: &[char], at: usize, len: usize) -> String {
    s[at.saturating_sub(8)..(at + len + 8).min(s.len())].iter().collect()
}

/// 区間の前後の字を含む、項目のページ（見つからなければ 0）。
fn locate(pages: &[(usize, Vec<char>)], s: &[char], at: usize, len: usize) -> usize {
    for (before, after) in [(8, 0), (0, 8), (4, 4)] {
        let a = at.saturating_sub(before);
        let b = (at + len + after).min(s.len());
        let probe: Vec<char> = s[a..b].to_vec();
        if probe.len() < 4 {
            continue;
        }
        if let Some((p, _)) = pages.iter().find(|(_, g)| g.windows(probe.len()).any(|w| w == probe.as_slice())) {
            return *p;
        }
    }
    0
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let usage = "usage: text_audit <corpus_dir> <archive_dir> <out_dir> [<egov_cache>]";
    let corpus = PathBuf::from(args.next().expect(usage));
    let archive = PathBuf::from(args.next().expect(usage));
    let out = PathBuf::from(args.next().expect(usage));
    let cache = args.next().map(PathBuf::from).unwrap_or_else(|| out.join("egov-cache"));
    std::fs::create_dir_all(&out)?;

    // 号ごとの項目とページの PDF。
    let mut files: Vec<PathBuf> = std::fs::read_dir(&corpus)?.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "json")).collect();
    files.sort();
    let mut issues: Vec<(String, Vec<(usize, PathBuf)>, Vec<Item>)> = Vec::new();
    for file in &files {
        let day: Value = serde_json::from_slice(&std::fs::read(file)?)?;
        let date = day["date"].as_str().unwrap_or("").to_string();
        let manifest: Value = std::fs::read(archive.join(&date).join("manifest.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null);
        for issue in day["issues"].as_array().into_iter().flatten() {
            let dir = issue["pdf_url"].as_str().unwrap_or("").trim_end_matches('/').rsplit('/').next().unwrap_or("").to_string();
            let pages: Vec<(usize, PathBuf)> = manifest["issues"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|i| i["dir"].as_str() == Some(dir.as_str()))
                .flat_map(|i| i["pages"].as_array().cloned().unwrap_or_default())
                .filter_map(|p| Some((p["page"].as_u64()? as usize, archive.join(&date).join(&dir).join(p["file"].as_str()?))))
                .collect();
            let last = pages.iter().map(|p| p.0).max().unwrap_or(0);
            let raw: Vec<&Value> = issue["items"].as_array().into_iter().flatten().collect();
            let starts: Vec<usize> = raw.iter().map(|i| i["page"].as_u64().unwrap_or(0) as usize).collect();
            let items = raw
                .iter()
                .enumerate()
                .map(|(n, i)| Item {
                    date: date.clone(),
                    issue: dir.clone(),
                    title: i["title"].as_str().unwrap_or("").to_string(),
                    page: starts[n],
                    // 次の（ページの大きい）項目が始まるページまで。
                    end: starts.iter().copied().filter(|&p| p > starts[n]).min().unwrap_or(last),
                    text: i["amend_text"].as_str().unwrap_or("").to_string(),
                    doc: i["amend_document"].clone(),
                })
                .collect();
            issues.push((dir, pages, items));
        }
    }

    // ページの字（並べて PDF を読む）。
    let next = AtomicUsize::new(0);
    let glyphs: Mutex<HashMap<PathBuf, Vec<char>>> = Mutex::new(HashMap::new());
    let all_pages: Vec<PathBuf> = issues.iter().flat_map(|(_, p, _)| p.iter().map(|(_, f)| f.clone())).collect();
    std::thread::scope(|scope| {
        for _ in 0..std::thread::available_parallelism().map_or(4, |n| n.get()) {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(f) = all_pages.get(i) else { break };
                let g = page_glyphs(f);
                glyphs.lock().unwrap().insert(f.clone(), g);
            });
        }
    });
    let glyphs = glyphs.into_inner().unwrap();
    let pages_of = |pages: &[(usize, PathBuf)], from: usize, to: usize| -> Vec<(usize, Vec<char>)> {
        pages.iter().filter(|(p, _)| (from..=to).contains(p)).map(|(p, f)| (*p, glyphs.get(f).cloned().unwrap_or_default())).collect()
    };

    // 1. e-Gov。
    let mut egov = Egov { cache, texts: HashMap::new() };
    let mut w = std::io::BufWriter::new(std::fs::File::create(out.join("egov.jsonl"))?);
    let (mut matched_total, mut truth_total, mut out_total, mut audited) = (0usize, 0usize, 0usize, 0usize);
    for (_, pages, items) in &issues {
        for item in items {
            let Some(head) = heading(&item.text) else { continue };
            let num = format!("{}{head}", era_year(&item.date));
            let item_pages = pages_of(pages, item.page, item.end);
            // (欄, 出力の字, 並べた結果)。
            let mut parts: Vec<(String, Vec<char>, Aligned, bool)> = Vec::new();
            let mut status = "no_truth";
            if let Some(law) = egov.law(&num) {
                let law_id = law["law_info"]["law_id"].as_str().unwrap_or("").to_string();
                let revisions = egov.revisions(&law_id);
                let first = revisions.iter().min_by_key(|r| r["amendment_enforcement_date"].as_str().unwrap_or("").to_string()).cloned();
                if let Some(t) = first.and_then(|r| r["law_revision_id"].as_str().map(str::to_string)).and_then(|r| egov.text(&r)) {
                    let s = norm(&item.text);
                    if let Some(a) = align(&s, &t, true) {
                        status = "new_law";
                        parts.push(("全文".into(), s, a, true));
                    }
                }
            } else if item.doc["format"].as_str() == Some("shinkyu") {
                let mut base: Option<String> = amended_law(&item.text.lines().take(12).collect::<String>());
                for block in item.doc["blocks"].as_array().into_iter().flatten() {
                    if block["kind"] == "paragraph" {
                        let text: String = block["runs"].as_array().into_iter().flatten().filter_map(|r| r["text"].as_str()).collect();
                        if let Some(b) = amended_law(&text) {
                            base = Some(b);
                        }
                        continue;
                    }
                    let Some(b) = base.clone() else { continue };
                    let Some((after, before)) = egov.amended(&b, &num) else { continue };
                    status = "shinkyu";
                    for (r, row) in block["rows"].as_array().into_iter().flatten().enumerate() {
                        for (side, truth) in [("after", Some(&after)), ("before", before.as_ref())] {
                            let Some(truth) = truth else { continue };
                            let cell: String = row[side].as_array().into_iter().flatten().filter_map(|c| c["text"].as_str()).collect::<Vec<_>>().join("\n");
                            for s in segments(&cell, truth) {
                                match align(&s, truth, false) {
                                    Some(a) => parts.push((format!("{side}#{r}"), s, a, false)),
                                    // e-Gov の本文に置く場所が無い（欄の全体が入った字）。
                                    None => parts.push((format!("{side}#{r}"), s.clone(), Aligned { inserted: vec![(0, s.iter().collect())], ..Default::default() }, false)),
                                }
                            }
                        }
                    }
                }
            }
            if parts.is_empty() {
                writeln!(w, "{}", json!({"date": item.date, "issue": item.issue, "page": item.page, "title": item.title, "law_num": num, "status": status}))?;
                continue;
            }
            audited += 1;
            let (mut m, mut t, mut o) = (0, 0, 0);
            let mut spans = Vec::new();
            let mut unanchored = 0;
            for (part, s, a, whole) in &parts {
                // 半分も合わない欄は、e-Gov の本文に字の無い部分（様式・図。e-Gov は画像で持つ）として率に入れない。
                if !whole && a.matched * 2 < s.len() {
                    unanchored += s.len();
                    spans.push(json!({"kind": "no_truth", "part": part, "len": s.len(), "context": s.iter().take(40).collect::<String>(), "page": locate(&item_pages, s, 0, 12.min(s.len())), "at": -1}));
                    continue;
                }
                m += a.matched;
                t += a.truth;
                o += s.len();
                let joined_out: String = s.iter().collect();
                for (at, text) in &a.inserted {
                    let c: Vec<char> = text.chars().collect();
                    let elsewhere = c.len() >= K && parts.iter().any(|(_, _, b, _)| b.dropped.iter().any(|(_, d)| d.contains(text.as_str())));
                    let kind = if elsewhere { "reordered" } else { "inserted" };
                    spans.push(json!({"kind": kind, "part": part, "text": text, "len": c.len(), "context": snippet(s, *at, c.len()), "page": locate(&item_pages, s, *at, c.len()), "at": if *whole { *at as i64 } else { -1 }}));
                }
                for (at, text) in &a.dropped {
                    let c = text.chars().count();
                    let elsewhere = c >= K && joined_out.contains(text.as_str());
                    let kind = if elsewhere { "reordered_drop" } else { "dropped" };
                    spans.push(json!({"kind": kind, "part": part, "text": text, "len": c, "context": snippet(s, *at, 0), "page": locate(&item_pages, s, *at, 0), "at": if *whole { *at as i64 } else { -1 }}));
                }
            }
            matched_total += m;
            truth_total += t;
            out_total += o;
            writeln!(w, "{}", json!({"date": item.date, "issue": item.issue, "page": item.page, "title": item.title, "law_num": num, "status": status,
                "parts": parts.len(), "unanchored": unanchored, "matched": m, "truth": t, "output": o, "spans": spans}))?;
        }
    }
    drop(w);
    println!("e-Gov: {audited} 項目、合う字 {matched_total} / e-Gov {truth_total}（{:.3}%）・出力 {out_total}（{:.3}%）",
        100.0 * matched_total as f64 / truth_total.max(1) as f64, 100.0 * matched_total as f64 / out_total.max(1) as f64);

    // 2. PDF の字。私用領域（PUA）の字はパーサが後で字形から読むので、どの字にもなり得る字として数えるだけにする。
    let pua = |c: char| matches!(c as u32, 0xE000..=0xF8FF | 0xF0000..=0x10FFFF);
    let mut w = std::io::BufWriter::new(std::fs::File::create(out.join("glyph.jsonl"))?);
    let (mut glyph_total, mut pua_total, mut dropped_total, mut repeated_total, mut excess_total) = (0usize, 0usize, 0usize, 0usize, 0usize);
    for (dir, pages, items) in &issues {
        let date = items.first().map(|i| i.date.clone()).unwrap_or_default();
        let mut grams: HashSet<u64> = HashSet::new();
        let mut output: HashMap<char, isize> = HashMap::new();
        for item in items {
            let s = norm(&item.text);
            grams.extend(s.windows(GLYPH_K).map(hash));
            s.iter().for_each(|c| *output.entry(*c).or_default() += 1);
        }
        // 号のページの字（PUA を除く）と PUA の字の数。
        let mut have: HashMap<char, isize> = HashMap::new();
        let mut unknown = 0isize;
        let mut runs: Vec<(usize, Vec<char>, String)> = Vec::new();
        for (p, f) in pages {
            let Some(g) = glyphs.get(f) else { continue };
            let g: Vec<char> = g.iter().copied().filter(|&c| !pua(c)).collect();
            unknown += (glyphs[f].len() - g.len()) as isize;
            glyph_total += glyphs[f].len();
            g.iter().for_each(|c| *have.entry(*c).or_default() += 1);
            let mut hit = vec![false; g.len()];
            for i in 0..g.len().saturating_sub(GLYPH_K - 1) {
                if grams.contains(&hash(&g[i..i + GLYPH_K])) {
                    hit[i..i + GLYPH_K].iter_mut().for_each(|h| *h = true);
                }
            }
            let mut i = 0;
            while i < g.len() {
                if hit[i] {
                    i += 1;
                    continue;
                }
                let j = (i..g.len()).find(|&j| hit[j]).unwrap_or(g.len());
                runs.push((*p, g[i..j].to_vec(), g[i.saturating_sub(10)..i].iter().collect()));
                i = j;
            }
        }
        pua_total += unknown as usize;
        // 3 ページ以上に同じ並びで出る落ち（柱・欄外の繰り返し）は分けて数える。
        let mut seen: HashMap<&[char], HashSet<usize>> = HashMap::new();
        for (p, r, _) in &runs {
            seen.entry(r.as_slice()).or_default().insert(*p);
        }
        for (p, r, before) in &runs {
            let repeated = seen[r.as_slice()].len() >= 3;
            if repeated {
                repeated_total += r.len();
            } else {
                dropped_total += r.len();
            }
            let in_item = items.iter().any(|i| (i.page..=i.end).contains(p));
            writeln!(w, "{}", json!({"kind": if repeated { "repeated" } else { "dropped" }, "date": date, "issue": dir, "page": p, "in_item": in_item,
                "len": r.len(), "text": r.iter().take(200).collect::<String>(), "before": before}))?;
        }
        // 号のページの字（PUA はどの字にもなり得る）より多い出力の字（重なり・作った字）。
        let mut extra: Vec<(char, isize)> = output.iter().map(|(c, n)| (*c, n - have.get(c).copied().unwrap_or(0))).filter(|(_, n)| *n > 0).collect();
        extra.sort_by_key(|(c, n)| (-n, *c));
        let n = (extra.iter().map(|e| e.1).sum::<isize>() - unknown).max(0);
        excess_total += n as usize;
        if n > 0 {
            writeln!(w, "{}", json!({"kind": "excess", "date": date, "issue": dir, "count": n, "pua": unknown,
                "chars": extra.iter().take(30).map(|(c, n)| format!("{c}×{n}")).collect::<Vec<_>>().join(" ")}))?;
        }
    }
    println!("PDF の字: {glyph_total} 字（PUA {pua_total}）、どの出力にも無い字 {dropped_total}（{:.3}%）・繰り返しの欄外 {repeated_total}、号の字より多い出力の字 {excess_total}",
        100.0 * dropped_total as f64 / glyph_total.max(1) as f64);
    Ok(())
}

use std::io::Write as _;

//! 実官報ページのコーパス（`tests/corpus/<case>/`）で、復元結果をスナップショットと照合する。
//!
//! 各ケースは次のファイルを持つ:
//! - `page.pdf` — 原本（デジタル官報の項目別 PDF 1 ページ）。複数ページにまたがるケースは
//!   2 ページ目以降を `page-2.pdf`・`page-3.pdf` …（bbox・rules も同じ名前）に置き、
//!   項目としてまとめて解析する
//! - `page.bbox.html` — 座標付きテキスト（`pdftotext -bbox-layout` と同じ形。kanpo-pdf で作る）
//! - `page.rules.json` — 罫線
//!
//!   どちらも `page.pdf` から作れる（playground が読むので置いている）。テストは PDF から作り直して
//!   保存済みのものと同じかも確かめる（`cargo run -p kanpo-parser --example prepare -- page.pdf`
//!   か `UPDATE_EXPECT=1` で作り直す）
//! - `case.json` — 出典・タグ・何を確かめるケースか（notes）
//! - `expected.txt` — 復元した本文（目視で正しいと確認したもの）
//! - `expected.json` — 記事ごとの構造化結果
//!
//! パーサ・kanpo-pdf を直して出力が変わったら、差分を目視で確かめてから
//! `UPDATE_EXPECT=1 cargo test -p kanpo-parser --test corpus` で更新する。

use std::path::{Path, PathBuf};

// The original HTML fixtures predate the additive cell-origin/advance-band contracts.
// Keep their complete existing payload check. A regenerated fixture containing
// the contract compares every byte, including the new metadata, as before.
// Original glyph/envelope/font-size contracts are tested in kanpo-pdf.
fn source_payload(actual: &str, expected: &str) -> String {
    let mut out = actual.to_owned();
    for needle in [" data-native-cell-origin=\"", " data-native-advance-band=\"", " data-native-terminal-glyph=\"", " data-native-font-aliases=\"", " data-native-lineage-coverage=\"", " data-native-lineage-obstacles=\"", " data-native-lineage-ink=\"", " data-native-line-moves=\"", " data-native-scene=\""] {
        if expected.contains(needle) {
            continue;
        }
        while let Some(start) = out.find(needle) {
            let value = start + needle.len();
            let end = value + out[value..].find('"').expect("closed source attribute");
            out.replace_range(start..=end, "");
        }
    }
    out
}

#[test]
fn source_metadata_versions_keep_all_existing_payload_checks() {
    let historical = "<word xmin='10' data-native-axis='right'>a b</word>";
    let current = historical.replace(
        "data-native-axis",
        "data-native-cell-origin=\"2 original envelope and sizes\" data-native-axis",
    );
    assert_eq!(source_payload(&current, historical), historical);
    assert_eq!(
        source_payload(&current, &current),
        current,
        "UPDATE_EXPECT output stays self-consistent"
    );
    for changed in [
        current.replace("a b", "ab"),
        current.replace("xmin='10'", "xmin='11'"),
        current.replace("'right'", "'left'"),
    ] {
        assert_ne!(
            source_payload(&changed, historical),
            historical,
            "existing text, geometry and source evidence remain checked"
        );
    }
}

fn cases() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&root)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join("page.pdf").exists())
        .collect();
    dirs.sort();
    dirs
}

/// ToUnicode 無しの私用 CID がそのまま漢字として出たときの符号（復元漏れの検出用）。
fn garbled(c: char) -> bool {
    ('\u{630E}'..='\u{6317}').contains(&c) || ('\u{661D}'..='\u{6621}').contains(&c) || c == '\u{617A}'
}

#[test]
fn corpus_matches_expected() {
    let update = std::env::var_os("UPDATE_EXPECT").is_some();
    let mut failures: Vec<String> = Vec::new();
    for dir in cases() {
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        let case: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("case.json")).unwrap()).unwrap();
        assert!(case["title"].is_string() && case["notes"].is_string(), "{name}: case.json に title/notes");
        let pages: Vec<kanpo_parser::PageInput> = std::iter::once("page".to_string())
            .chain((2..).map(|n| format!("page-{n}")))
            .map_while(|stem| {
                let pdf = std::fs::read(dir.join(format!("{stem}.pdf"))).ok()?;
                let page = kanpo_parser::page_inputs(&pdf).unwrap().remove(0);
                let rules = serde_json::to_string(&page.rules).unwrap();
                for (file, actual) in [(format!("{stem}.bbox.html"), &page.xhtml), (format!("{stem}.rules.json"), &rules)] {
                    let path = dir.join(&file);
                    if update {
                        std::fs::write(&path, actual).unwrap();
                    } else {
                        let expected = std::fs::read_to_string(&path).ok();
                        let compared = expected.as_deref().map(|e| source_payload(actual, e)).unwrap_or_else(|| actual.to_owned());
                        if expected.as_deref() != Some(compared.as_str()) {
                            failures.push(format!("{name}/{file} が PDF から作ったものと違う（UPDATE_EXPECT=1 で作り直す）"));
                        }
                    }
                }
                Some(page)
            })
            .collect();
        let a = kanpo_parser::analyze_item(&pages);
        let text = format!("{}\n", a.text);
        let docs: Vec<_> = a.articles.iter().map(|art| &art.document).collect();
        let json = format!("{}\n", serde_json::to_string_pretty(&docs).unwrap());

        // 構造化用の内部の目印（私用領域 U+E000〜U+E002）が本文・構造化結果に漏れていない。
        let marks = ['\u{E000}', '\u{E001}', '\u{E002}'];
        if a.text.contains(marks) || json.contains(marks) || a.articles.iter().any(|x| x.text.contains(marks)) {
            failures.push(format!("{name}: 表・表の終わりの目印が漏れている"));
        }
        if let Some(c) = a.text.chars().find(|c| garbled(*c)) {
            failures.push(format!("{name}: 私用 CID の復元漏れ {c:?}"));
        }
        for (file, actual) in [("expected.txt", &text), ("expected.json", &json)] {
            let path = dir.join(file);
            if update {
                std::fs::write(&path, actual).unwrap();
                continue;
            }
            match std::fs::read_to_string(&path) {
                Ok(expected) if &expected == actual => {}
                Ok(expected) => failures.push(format!("{name}/{file} が変わった:\n{}", first_diff(&expected, actual))),
                Err(_) => failures.push(format!("{name}/{file} が無い（UPDATE_EXPECT=1 で作る）")),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// 座標付きテキストの `<word>` の並び（抽出器ごとに違う）を変えても、復元結果が変わらない。
#[test]
fn output_does_not_depend_on_word_order() {
    let mut failures: Vec<String> = Vec::new();
    for dir in cases() {
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        let pages: Vec<kanpo_parser::PageInput> = std::iter::once("page".to_string())
            .chain((2..).map(|n| format!("page-{n}")))
            .map_while(|stem| Some(kanpo_parser::page_inputs(&std::fs::read(dir.join(format!("{stem}.pdf"))).ok()?).unwrap().remove(0)))
            .collect();
        let render = |pages: &[kanpo_parser::PageInput]| {
            let a = kanpo_parser::analyze_item(pages);
            let docs: Vec<_> = a.articles.iter().map(|art| &art.document).collect();
            format!("{}\n{}", a.text, serde_json::to_string_pretty(&docs).unwrap())
        };
        let expected = render(&pages);
        for (label, order) in [("reversed", reversed as WordOrder), ("interleaved", interleaved)] {
            let shuffled: Vec<kanpo_parser::PageInput> = pages
                .iter()
                .map(|p| kanpo_parser::PageInput { xhtml: reorder_words(&p.xhtml, order), rules: p.rules.clone() })
                .collect();
            let actual = render(&shuffled);
            if actual != expected {
                failures.push(format!("{name} ({label}):\n{}", first_diff(&expected, &actual)));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

type WordOrder = fn(Vec<&str>) -> Vec<&str>;

fn reversed(words: Vec<&str>) -> Vec<&str> {
    words.into_iter().rev().collect()
}

/// 偶数番目の後に奇数番目（隣り合う語を引き離す）。
fn interleaved(words: Vec<&str>) -> Vec<&str> {
    let (even, odd): (Vec<_>, Vec<_>) = words.into_iter().enumerate().partition(|(i, _)| i % 2 == 0);
    even.into_iter().chain(odd).map(|(_, w)| w).collect()
}

/// 各ページの `<word>` を `order` で並べ替え、1 語 1 行の XHTML に組み直す。
fn reorder_words(xhtml: &str, order: WordOrder) -> String {
    let mut out = String::new();
    let mut rest = xhtml;
    while let Some(start) = rest.find("<page ") {
        let open_end = start + rest[start..].find('>').unwrap() + 1;
        let close = rest.find("</page>").unwrap();
        out.push_str(&rest[..open_end]);
        let mut words: Vec<&str> = Vec::new();
        let mut body = &rest[open_end..close];
        while let Some(w) = body.find("<word ") {
            let end = w + body[w..].find("</word>").unwrap() + "</word>".len();
            words.push(&body[w..end]);
            body = &body[end..];
        }
        for w in order(words) {
            out.push_str("<line>");
            out.push_str(w);
            out.push_str("</line>\n");
        }
        rest = &rest[close..];
    }
    out.push_str(rest);
    out
}

/// 最初に食い違う行の前後を示す。
fn first_diff(expected: &str, actual: &str) -> String {
    let (e, a): (Vec<&str>, Vec<&str>) = (expected.lines().collect(), actual.lines().collect());
    let i = e.iter().zip(&a).position(|(x, y)| x != y).unwrap_or(e.len().min(a.len()));
    format!(
        "  line {}:\n  - {}\n  + {}",
        i + 1,
        e.get(i).copied().unwrap_or("<EOF>"),
        a.get(i).copied().unwrap_or("<EOF>")
    )
}

#[test]
fn additive_band_metadata_does_not_hide_existing_or_current_payload() {
    let old="<word xmin='10' data-native-axis='right'>a b</word>";
    let band=old.replace(" data-native-axis", " data-native-advance-band=\"1 original complete origins\" data-native-axis");
    let both=band.replace(" data-native-axis", " data-native-cell-origin=\"2 original envelope and sizes\" data-native-axis");
    assert_eq!(source_payload(&band,old),old);
    assert_eq!(source_payload(&both,old),old);
    assert_eq!(source_payload(&both,&band),band);
    assert_eq!(source_payload(&both,&both),both);
    for changed in [both.replace("a b","ab"),both.replace("xmin='10'","xmin='11'"),both.replace("'right'","'left'"),both.replace("1 original","2 changed"),both.replace("2 original","3 changed")] {
        assert_ne!(source_payload(&changed,&both),both);
    }
}

#[test]
fn terminal_metadata156_keeps_legacy_bytes_and_checks_every_current_field() {
 let old="<word xmin='10' data-native-axis='right'>a b</word>";
 let current=old.replace(" data-native-axis"," data-native-terminal-glyph=\"1 0 2 14 18 20\" data-native-axis");
 assert_eq!(source_payload(&current,old),old);
 assert_eq!(source_payload(&current,&current),current);
 for changed in [current.replace("a b","ab"),current.replace("xmin='10'","xmin='11'"),current.replace("'right'","'left'"),current.replace("1 0 2 14 18 20","1 0 2 15 18 20"),current.replace(" data-native-axis"," data-native-unrecognized=\"changed\" data-native-axis")] {
  assert_ne!(source_payload(&changed,&current),current);
 }
}

#[test]
fn font_alias_metadata157_keeps_all_legacy_bytes_and_checks_current_evidence(){
 let old="<word xmin='10' data-native-axis='right'>a b</word>";
 let current=old.replace(" data-native-axis"," data-native-font-aliases=\"1 0 2 10 20 30 40 0:58495:3:23185\" data-native-axis");
 assert_eq!(source_payload(&current,old),old);
 assert_eq!(source_payload(&current,&current),current);
 for changed in [current.replace("a b","ab"),current.replace("xmin='10'","xmin='11'"),current.replace("'right'","'left'"),current.replace("0:58495:3:23185","0:58496:3:23185"),current.replace("10 20 30 40","11 20 30 40"),current.replace(" data-native-axis"," data-native-unrecognized=\"changed\" data-native-axis")] {assert_ne!(source_payload(&changed,&current),current);}
}

#[test]
fn line_move_metadata160_preserves_legacy_payload_and_checks_every_current_field(){
 let old="<page data-native-glyph-count=\"8\"><word xmin=\"10\" data-native-axis=\"right\">a b</word></page>";
 let current=old.replace("<page ","<page data-native-lineage-coverage=\"1 8 -\" data-native-lineage-obstacles=\"1 -\" data-native-lineage-ink=\"1 L:0,0,0,8,-1,-1,1,9\" ").replace(" data-native-axis", " data-native-line-moves=\"1 0 3 10 20 30 40 612062 0:3:8\" data-native-axis");
 assert_eq!(source_payload(&current,old),old);
 assert_eq!(source_payload(&current,&current),current);
 for changed in [current.replace("1 8 -","1 9 -"),current.replace("obstacles=\"1 -","obstacles=\"1 1,2,3,4"),current.replace("L:0,0,0,8,-1,-1,1,9","L:0,0,0,8,-2,-1,1,9"),current.replace("0:3:8","0:4:8"),current.replace("612062","612063"),current.replace("a b","ab"),current.replace("xmin=\"10","xmin=\"11"),current.replace(" data-native-axis"," data-native-unrecognized=\"changed\" data-native-axis")]{assert_ne!(source_payload(&changed,&current),current);}
}

#[test]
fn native_scene_metadata_preserves_legacy_bytes_and_checks_every_current_field(){
 let input=kanpo_parser::page_inputs(&std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/native-marker-records/source.pdf")).unwrap()).unwrap();
 let html=&input[0].xhtml;let document=scraper::Html::parse_document(html);
 let page=document.select(&scraper::Selector::parse("page").unwrap()).next().unwrap();
 let packet:serde_json::Value=serde_json::from_str(page.value().attr("data-native-scene").unwrap()).unwrap();
 let rewrite=|packet:Option<&serde_json::Value>|{
  let mut output=html.clone();let needle=" data-native-scene=\"";let start=output.find(needle).unwrap();let from=start+needle.len();let end=from+output[from..].find('"').unwrap()+1;
  let value=packet.map(|v|format!("{needle}{}\"",v.to_string().replace('&',"&amp;").replace('"',"&quot;").replace('<',"&lt;").replace('>',"&gt;"))).unwrap_or_default();output.replace_range(start..end,&value);output
 };
 let current=rewrite(Some(&packet));let historical=rewrite(None);
 assert_eq!(source_payload(&current,&historical),historical);
 assert_eq!(source_payload(&current,&current),current);
 for key in packet.as_object().unwrap().keys(){let mut changed=packet.clone();changed[key]=serde_json::Value::Null;assert_ne!(source_payload(&rewrite(Some(&changed)),&current),current,"{key}");}
 for changed in [current.replacen("<word ","<word data-native-unrecognized=\"changed\" ",1),current.replacen("xmin=\"","xmin=\"1",1),current.replacen("data-native-axis=\"down\"","data-native-axis=\"right\"",1)]{
  assert_ne!(source_payload(&changed,&historical),historical);
  assert_ne!(source_payload(&changed,&current),current);
 }
}

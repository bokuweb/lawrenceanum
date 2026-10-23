//! 横主体ページ内に混植された縦書きフォントの引用を、font由来の枠で復元する。

fn without_native_attributes(pages: &[kanpo_parser::PageInput]) -> Vec<kanpo_parser::PageInput> {
    pages
        .iter()
        .map(|p| {
            let mut xhtml = p.xhtml.clone();
            for attr in [
                "data-wmode",
                "data-advance-x",
                "data-advance-y",
                "data-native-xmin",
                "data-native-ymin",
                "data-native-xmax",
                "data-native-ymax",
            ] {
                let prefix = format!(" {attr}=\"");
                while let Some(i) = xhtml.find(&prefix) {
                    let end = i + prefix.len() + xhtml[i + prefix.len()..].find('"').unwrap() + 1;
                    xhtml.replace_range(i..end, "");
                }
            }
            kanpo_parser::PageInput {
                xhtml,
                rules: p.rules.clone(),
            }
        })
        .collect()
}

#[test]
fn all_seven_inline_quotations_follow_both_columns_and_keep_each_word_once() {
    let pdf = include_bytes!("archive/pages/20260727g00167/20260727g001670029.pdf");
    let pages = kanpo_parser::page_inputs(pdf).unwrap();
    assert!(pages[0].xhtml.contains("data-wmode=\"1\""));
    let out = kanpo_parser::analyze_item(&pages);
    let expected = [
        "別表第１中「全角文字コード表」の次に「（第２及び第４関係）」を加える。",
        "別表第２中「第１及び第３関係」を「第２及び第４関係」に改める。",
        "別表第３中「第１関係」を「第２関係」に改める。",
        "別表第８中「第３関係」を「第４関係」に改め、同表を別表第９とする。",
        "別表第７中「第３関係」を「第４関係」に改め、同表を別表第８とする。",
        "別表第６中「第３関係」を「第４関係」に改め、同表を別表第７とする。",
        "別表第５中「抹消登録）」の次に「（第２関係）」を加え、同表を別表第６とする。",
    ];
    assert!(out.text.ends_with(&expected.join("\n")));
    let revised = out.text.find("第４　様式省令第７条第２項に定める光ディスクへの記録方式").unwrap();
    let previous = out.text.find("第３　様式省令第７条第２項に定める光ディスクへの記録方式").unwrap();
    let tail = out.text.find(expected[0]).unwrap();
    assert!(revised < previous && previous < tail);
    let page = &out.pages[0];
    let mut ids = std::collections::HashSet::new();
    for id in page
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .flat_map(|l| l.words.iter().chain(l.table.iter().flat_map(|t| &t.cells).flat_map(|c| &c.words)))
    {
        assert!(ids.insert(*id), "word {id} is repeated");
    }
    let used: std::collections::HashSet<_> = page
        .words
        .iter()
        .filter(|w| matches!(w.status, kanpo_parser::trace::WordStatus::Used))
        .map(|w| w.id)
        .collect();
    assert_eq!(ids, used);
}

#[test]
fn an_ordinary_horizontal_neighbor_keeps_its_existing_reading_order() {
    let pdf = include_bytes!("archive/pages/20260727g00167/20260727g001670028.pdf");
    let pages = kanpo_parser::page_inputs(pdf).unwrap();
    assert_eq!(
        kanpo_parser::analyze_item(&pages).text,
        kanpo_parser::analyze_item(&without_native_attributes(&pages)).text
    );
}

#[test]
fn a_sideways_vertical_page_keeps_its_existing_coordinate_normalization() {
    let pdf = include_bytes!("archive/pages/20260825g00189/20260825g001890097.pdf");
    let pages = kanpo_parser::page_inputs(pdf).unwrap();
    assert!(pages[0].xhtml.contains("data-wmode=\"1\""));
    assert_eq!(
        kanpo_parser::analyze_item(&pages).text,
        kanpo_parser::analyze_item(&without_native_attributes(&pages)).text
    );
}

#[test]
fn a_short_form_amendment_keeps_both_quoted_field_names_inline() {
    let pdf = include_bytes!("archive/pages/20261001g00217/20261001g002170018.pdf");
    let pages = kanpo_parser::page_inputs(pdf).unwrap();
    let out = kanpo_parser::analyze_item(&pages);
    assert!(out
        .text
        .starts_with("第十号様式（その二）中「国籍」を「国籍等」に改め、備考を次のように改める。\n"));
    let legacy = kanpo_parser::analyze_item(&without_native_attributes(&pages));
    let chars = |s: &str| {
        let mut cs: Vec<_> = s.chars().filter(|c| !c.is_whitespace()).collect();
        cs.sort();
        cs
    };
    assert_eq!(chars(&out.text), chars(&legacy.text));
}

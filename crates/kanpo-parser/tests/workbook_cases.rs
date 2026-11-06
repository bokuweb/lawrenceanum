//! Source conservation for every main PDF in the user-supplied workbook.
//! These checks are necessary but do not certify semantic completeness.
use kanpo_parser::trace::{Role, WordStatus};
use kanpo_parser::{analyze_item, page_inputs};
use std::collections::BTreeSet;
use std::path::Path;

fn assert_used_once(page: &kanpo_parser::trace::PageTrace, context: &str) {
    let mut owned = BTreeSet::new();
    for region in &page.regions {
        for line in &region.lines {
            for &id in line.words.iter().chain(
                line.table
                    .iter()
                    .flat_map(|t| t.cells.iter().flat_map(|c| &c.words)),
            ) {
                assert!(owned.insert(id), "{context}: repeated Word {id}");
                assert_eq!(page.words[id].status, WordStatus::Used);
                assert_eq!(page.words[id].id, id);
            }
        }
    }
    let used = page
        .words
        .iter()
        .filter(|w| w.status == WordStatus::Used)
        .map(|w| w.id)
        .collect::<BTreeSet<_>>();
    assert_eq!(owned, used, "{context}");
}

#[test]
fn every_workbook_pdf_has_unique_complete_used_word_ownership() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/workbook");
    let index: serde_json::Value =
        serde_json::from_str(include_str!("data/workbook/index.json")).unwrap();
    let cases = index["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 18);
    let mut page_count = 0;
    for case in cases {
        let source = case["sources"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "pdf")
            .unwrap();
        let pdf = std::fs::read(root.join(source["fixture"].as_str().unwrap())).unwrap();
        assert_eq!(pdf.len() as u64, source["bytes"].as_u64().unwrap());
        let inputs = page_inputs(&pdf).unwrap();
        let result = analyze_item(&inputs);
        assert_eq!(result.pages.len(), inputs.len());
        assert!(!result.articles.is_empty());
        assert!(result
            .articles
            .iter()
            .all(|a| !a.document.blocks.is_empty()));
        page_count += result.pages.len();
        for (number, page) in result.pages.iter().enumerate() {
            assert_used_once(
                page,
                &format!("case {} page {}", case["number"], number + 1),
            );
            for table in page
                .regions
                .iter()
                .flat_map(|r| &r.lines)
                .filter_map(|l| l.table.as_ref())
            {
                let mut slots = BTreeSet::new();
                for cell in &table.cells {
                    for row in cell.row..cell.row + cell.rowspan {
                        for col in cell.col..cell.col + cell.colspan {
                            assert!(
                                slots.insert((row, col)),
                                "case {} page {}: overlapping logical cells ({row},{col})",
                                case["number"],
                                number + 1
                            );
                        }
                    }
                }
                let mut cells = table.cells.iter().collect::<Vec<_>>();
                cells.sort_by_key(|c| (c.row, c.col));
                let expected = cells.iter().map(|c| c.text.as_str()).collect::<String>();
                let actual = table
                    .to_nested()
                    .rows
                    .iter()
                    .flatten()
                    .flatten()
                    .map(|r| r.text.as_str())
                    .collect::<String>();
                let compact =
                    |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
                assert_eq!(
                    compact(&actual),
                    compact(&expected),
                    "case {} page {}: source table text lost during Document conversion",
                    case["number"],
                    number + 1
                );
            }
        }
    }
    assert_eq!(page_count, 182);
}

#[test]
fn listed_drug_notice_keeps_its_own_article_and_all_original_page_material() {
    let mut inputs = vec![];
    for bytes in [
        include_bytes!("data/workbook/case-10-listed-1.pdf").as_slice(),
        include_bytes!("data/workbook/case-10-listed-2.pdf").as_slice(),
        include_bytes!("data/workbook/case-10-listed-3.pdf").as_slice(),
    ] {
        inputs.extend(page_inputs(bytes).unwrap());
    }
    let result = analyze_item(&inputs);
    assert_eq!(result.pages.len(), 3);
    for (i, page) in result.pages.iter().enumerate() {
        assert_used_once(page, &format!("listed No.10 page {}", i + 1));
    }
    let selected = kanpo_parser::pick_article(
        "使用薬剤の薬価（薬価基準）等の一部を改正する告示",
        &result.articles,
    )
    .unwrap();
    let article = &result.articles[selected];
    assert!(article.text.starts_with("〇厚生労働省告示第三百七十三号"));
    assert!(!article.text.contains("〇厚生労働省告示第三百七十四号"));
    assert!(article
        .document
        .blocks
        .iter()
        .any(|b| matches!(b, kanpo_parser::Block::Shinkyu { .. })));
}

#[test]
fn original_nested_table_continuations_keep_their_comparison_fields() {
    let inputs = page_inputs(include_bytes!("data/workbook/case-11.pdf")).unwrap();
    let result = analyze_item(&inputs);
    for (i, p) in result.pages.iter().enumerate().take(8).skip(1) {
        assert!(
            p.regions.iter().any(|r| r.role == Role::After),
            "after page {}",
            i + 1
        );
        // Original pages 4, 7 and 8 have an empty before field.
        if [2, 3, 5, 6].contains(&(i + 1)) {
            assert!(
                p.regions.iter().any(|r| r.role == Role::Before),
                "before page {}",
                i + 1
            );
        }
        assert!(
            p.regions
                .iter()
                .all(|r| matches!(r.role, Role::After | Role::Before)),
            "comparison page {}",
            i + 1
        );
        assert_used_once(p, &format!("nested comparison page {}", i + 1));
    }
}

#[test]
fn linked_medical_survey_notice_keeps_the_original_native_column_order() {
    let inputs = page_inputs(include_bytes!("data/workbook/case-10.pdf")).unwrap();
    let a = analyze_item(&inputs);
    assert_eq!(a.text,concat!(
        "〇厚生労働省告示第二百二十四号\n",
        "医療施設調査規則（昭和二十八年厚生省令第二十五号）第六条第三項の規定に基づき、動態調査の調査票の様式（昭和四十八年厚生省告示第二百九十二号）の一部を次のように改正し、令和八年十月分\n",
        "の動態調査から適用する。\n",
        "令和八年五月二十一日\n",
        "厚生労働大臣上野賢一郎\n",
        "動態調査の調査票の様式を次のように改める。"
    ));
    assert_used_once(&a.pages[0], "linked medical-survey native text");
}

#[test]
fn sparse_original_comparison_keeps_both_fields_on_all_continuation_pages() {
    let inputs = page_inputs(include_bytes!("data/workbook/case-01.pdf")).unwrap();
    let result = analyze_item(&inputs);
    for page in &result.pages[1..8] {
        assert!(page.regions.iter().any(|r| r.role == Role::After));
        assert!(page.regions.iter().any(|r| r.role == Role::Before));
        assert!(page
            .regions
            .iter()
            .all(|r| matches!(r.role, Role::After | Role::Before)));
    }
    // These independently transcribed original provisions belong to the upper
    // after field; their before field contains the printed deletion/new marks.
    for (index, fragment) in [
        (1, "法第五条の六の二第二項"),
        (2, "法第六条の四第二項"),
        (
            7,
            "一般廃棄物最終処分場又は産業廃棄物最終処分場の技術上の基準",
        ),
    ] {
        let page = &result.pages[index];
        let text = |role| {
            page.regions
                .iter()
                .filter(|r| r.role == role)
                .flat_map(|r| &r.lines)
                .map(|l| l.text.as_str())
                .collect::<String>()
        };
        assert!(text(Role::After).contains(fragment));
        assert!(!text(Role::Before).contains(fragment));
    }
    assert!(result.pages[8]
        .regions
        .iter()
        .any(|r| r.role == Role::Body && r.lines.iter().any(|l| l.text == "附則")));
}

#[test]
fn original_four_band_comparison_keeps_the_same_page_and_later_tiers() {
    let inputs = page_inputs(include_bytes!("data/workbook/case-03.pdf")).unwrap();
    let result = analyze_item(&inputs);
    for page in &result.pages[..2] {
        assert!(
            page.regions
                .iter()
                .filter(|r| r.role == Role::After)
                .count()
                >= 2
        );
        assert!(!page.regions.iter().any(|r| r.role == Role::Body));
    }
    let first = &result.pages[0];
    assert!(first
        .regions
        .iter()
        .filter(|r| r.role == Role::After)
        .flat_map(|r| &r.lines)
        .any(|l| l.text.starts_with("第十五条の四の六")));
    assert!(first
        .regions
        .iter()
        .filter(|r| r.role == Role::Before)
        .flat_map(|r| &r.lines)
        .any(|l| l.text == "（新設）"));
    let last = &result.pages[2];
    assert!(
        last.regions
            .iter()
            .filter(|r| r.role == Role::After)
            .count()
            >= 2
    );
    assert!(last
        .regions
        .iter()
        .any(|r| r.role == Role::Body && r.lines.iter().any(|l| l.text == "附則")));
}

#[test]
fn original_new_article_marks_stay_in_their_before_field_on_continuation_pages() {
    let inputs = page_inputs(include_bytes!("data/workbook/case-02.pdf")).unwrap();
    let result = analyze_item(&inputs);
    for page in &result.pages[7..9] {
        assert!(page
            .regions
            .iter()
            .all(|r| matches!(r.role, Role::After | Role::Before)));
        let after: String = page
            .regions
            .iter()
            .filter(|r| r.role == Role::After)
            .flat_map(|r| &r.lines)
            .map(|l| l.text.as_str())
            .collect();
        let before: Vec<_> = page
            .regions
            .iter()
            .filter(|r| r.role == Role::Before)
            .flat_map(|r| &r.lines)
            .map(|l| l.text.as_str())
            .collect();
        assert!(!after.contains("〔条を加える。〕"));
        assert!(before.iter().all(|s| *s == "〔条を加える。〕"));
        assert!(!before.is_empty());
    }
}

#[test]
fn horizontal_change_captions_keep_their_fields_including_blank_continuations() {
    let inputs = page_inputs(include_bytes!("data/workbook/case-04.pdf")).unwrap();
    let result = analyze_item(&inputs);
    assert!(result.pages[0]
        .regions
        .iter()
        .any(|r| r.role == Role::Preamble));
    for page in &result.pages[..7] {
        assert!(page.regions.iter().any(|r| r.role == Role::After));
        assert!(!page.regions.iter().any(|r| r.role == Role::Body));
    }
    assert!(result.pages[3]
        .regions
        .iter()
        .filter(|r| r.role == Role::Before)
        .flat_map(|r| &r.lines)
        .any(|l| l.text == "［新設］"));
    assert!(result.pages[7]
        .regions
        .iter()
        .any(|r| r.role == Role::Before));
    assert!(result.pages[8].regions.iter().all(|r| r.role == Role::Body));
    assert!(result.articles.iter().flat_map(|a|&a.document.blocks).any(|b|matches!(b,kanpo_parser::Block::Shinkyu{labels:Some(l),..} if l.after=="変更後"&&l.before=="変更前")));
}

#[test]
fn every_original_glyph_and_word_survives_the_complete_attachment_trace() {
    let root=Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/workbook");
    let files=(1..=18).map(|i|format!("case-{i:02}.pdf"))
        .chain((1..=3).map(|i|format!("case-10-listed-{i}.pdf")));
    let mut pages=0;
    for name in files {
        let bytes=std::fs::read(root.join(&name)).unwrap();
        let sources=kanpo_pdf::extract_pages_with_aj1_sources(&bytes).unwrap();
        let media=kanpo_parser::media::analyze_pdf(&bytes).unwrap();
        let analysis=&media.native;
        assert_eq!(sources.len(),analysis.pages.len());
        for (pi,(source,trace)) in sources.iter().zip(&analysis.pages).enumerate() {
            let original=source.page();
            pages+=1;
            let words=kanpo_pdf::bbox::words(original);
            assert_eq!(words.len(),trace.words.len(),"{name} page {pi}");
            let mut glyphs=BTreeSet::new();
            let mut claimed=BTreeSet::new();
            for word in &trace.words {
                let matches=words.iter().enumerate().filter(|(id,native)|!claimed.contains(id)&&native.text==word.raw&&word.source_box.iter().zip([native.x_min,native.y_min,native.x_max,native.y_max]).all(|(&a,b)|(a as f64-b).abs()<0.001)).collect::<Vec<_>>();
                assert_eq!(matches.len(),1,"{name} page {pi} word {} original occurrence",word.id);
                let (index,native)=matches[0];assert!(claimed.insert(index));
                for &id in &native.glyphs {assert!(glyphs.insert(id),"{name} page {pi}: repeated glyph {id}");}
                let actual=word.source_box.map(f64::from);
                let expected=[native.x_min,native.y_min,native.x_max,native.y_max];
                assert!(actual.iter().zip(expected).all(|(a,b)|(a-b).abs()<0.001),"{name} page {pi} word {} original geometry changed",word.id);
            }
            for (id,glyph) in original.glyphs.iter().enumerate() {
                let decoration=media.underlines.glyph_bindings.iter().any(|b|b.page==pi&&b.decoration_glyphs.contains(&id));
                // A rule crossing whitespace has no parent text scalar. Its
                // independently proved CFF rectangle remains page geometry.
                let font_rule=media.page_graphics[pi]["font_rectangles"].as_array().unwrap().iter().any(|r|{
                    if r["glyph"].as_u64()!=Some(id as u64){return false;}
                    let rectangles=source.glyph_separate_painted_rectangles(id).map(|r|r.to_vec()).or_else(||source.glyph_painted_rectangle(id).map(|r|vec![r])).unwrap();
                    assert_eq!(r["rectangles"],serde_json::to_value(rectangles).unwrap());
                    true
                });
                assert!(glyphs.contains(&id)||decoration||font_rule||glyph.invisible||glyph.text.trim().is_empty(),"{name} page {pi}: original glyph {id} omitted");
            }
        }
    }
    assert_eq!(pages,185);
}

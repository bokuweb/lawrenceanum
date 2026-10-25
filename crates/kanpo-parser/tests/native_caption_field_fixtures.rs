//! Original caption cells and legal scope boundaries. Goldens freeze reviewed
//! regressions; these witnesses do not grade full-page or raster coverage.
use kanpo_parser::{analyze_item, page_inputs, trace::Role, Analysis, Block, Run, ShinkyuRow};
fn analyze(pdfs: &[&[u8]], golden: &str) -> Analysis {
    let inputs = pdfs
        .iter()
        .flat_map(|b| page_inputs(b).unwrap())
        .collect::<Vec<_>>();
    let a = analyze_item(&inputs);
    let actual: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    let expected: serde_json::Value = serde_json::from_str(golden).unwrap();
    assert_eq!(actual, expected, "reviewed Analysis regression");
    a
}
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
fn text(r: &[Run]) -> String {
    r.iter().map(|r| r.text.as_str()).collect()
}
fn rows(a: &Analysis) -> Vec<&ShinkyuRow> {
    a.articles
        .iter()
        .flat_map(|a| &a.document.blocks)
        .filter_map(|b| {
            if let Block::Shinkyu { rows } = b {
                Some(rows)
            } else {
                None
            }
        })
        .flatten()
        .collect()
}
fn fields(a: &Analysis) {
    let p = a.pages.last().unwrap();
    let field = |role| {
        compact(
            &p.regions
                .iter()
                .filter(|r| r.role == role)
                .flat_map(|r| r.lines.iter().map(|l| l.text.as_str()))
                .collect::<String>(),
        )
    };
    assert!(field(Role::After).contains("第二条〔①〕対内直接投資等に関する政令"));
    assert!(field(Role::Before).contains("第二条〔項を加える。〕"));
}
#[test]
fn original_caption_fields_retain_their_source_body() {
    let a = analyze(
        &[include_bytes!(
            "data/native-caption-fields/20260916g002060047.pdf"
        )],
        include_str!("data/native-caption-fields/20260916g002060047-reviewed134.json"),
    );
    fields(&a);
}
#[test]
fn original_long_cell_and_its_chapter_keep_their_own_articles() {
    let a = analyze(
        &[include_bytes!(
            "data/native-caption-fields/20260902g001960003.pdf"
        )],
        include_str!("data/native-caption-fields/20260902g001960003-reviewed134.json"),
    );
    let rows = rows(&a);
    let following = rows
        .iter()
        .find(|r| compact(&text(&r.after)).contains("第九条（略）"))
        .unwrap();
    for side in [&following.after, &following.before] {
        assert!(compact(&text(side)).starts_with("第二章南極地域活動計画の確認第九条"));
    }
    let previous = rows
        .iter()
        .find(|r| {
            text(&r.after)
                .lines()
                .any(|l| l.trim_start().starts_with("第八条の二　"))
        })
        .unwrap();
    assert!(!text(&previous.after).contains("第二章"));
    assert!(!text(&previous.before).contains("第二章"));
    let long = rows
        .iter()
        .find(|r| compact(&text(&r.after)).contains("第十二条の二"))
        .unwrap();
    assert!(compact(&text(&long.after)).contains("法第七条第一項第二号の環境省令で定める基準は、次の各号のいずれにも適合するものであることとする。"));
    assert!(compact(&text(&long.after)).ends_with("構造又は設備の内容が、環境上の緊急"));
    assert_eq!(compact(&text(&long.before)), "（新設）");
}
#[test]
fn original_chapter_is_not_the_previous_articles_tail() {
    let a = analyze(
        &[include_bytes!(
            "data/native-caption-fields/20260902g001960002.pdf"
        )],
        include_str!("data/native-caption-fields/20260902g001960002-reviewed134.json"),
    );
    let rows = rows(&a);
    let old = rows
        .iter()
        .find(|r| text(&r.after).lines().any(|l| l.starts_with("第十九条　")))
        .unwrap();
    assert!(!text(&old.after).contains("第三章"));
    assert!(!text(&old.before).contains("第三章"));
    let scope = rows
        .iter()
        .find(|r| text(&r.after).starts_with("第三章　"))
        .unwrap();
    assert!(text(&scope.after).contains("第二十条・第二十一条"));
    assert!(text(&scope.before).starts_with("第三章　"));
}
#[test]
fn original_omitted_section_starts_its_own_scope_across_pages() {
    let a = analyze(
        &[
            include_bytes!("data/native-caption-fields/20260730g001710001.pdf"),
            include_bytes!("data/native-caption-fields/20260730g001710002.pdf"),
            include_bytes!("data/native-caption-fields/20260730g001710003.pdf"),
            include_bytes!("data/native-caption-fields/20260730g001710004.pdf"),
            include_bytes!("data/native-caption-fields/20260730g001710005.pdf"),
            include_bytes!("data/native-caption-fields/20260730g001710006.pdf"),
            include_bytes!("data/native-caption-fields/20260730g001710007.pdf"),
        ],
        include_str!("data/native-caption-fields/20260730g00171-1-7-reviewed134.json"),
    );
    let rows = rows(&a);
    let old = rows
        .iter()
        .find(|r| text(&r.after).contains("第二十四条　"))
        .unwrap();
    assert!(!text(&old.after).contains("第四章"));
    assert!(!text(&old.before).contains("第四章"));
    let scope = rows
        .iter()
        .find(|r| text(&r.after).starts_with("第四章　"))
        .unwrap();
    assert!(text(&scope.before).starts_with("第四章　"));
}
#[test]
fn original_branched_chapter_follows_its_new_article_across_pages() {
    let a = analyze(
        &[
            include_bytes!("data/native-caption-fields/20260731g001720068.pdf"),
            include_bytes!("data/native-caption-fields/20260731g001720069.pdf"),
        ],
        include_str!("data/native-caption-fields/20260731g00172-68-69-reviewed134.json"),
    );
    let rows = rows(&a);
    let old = rows
        .iter()
        .find(|r| text(&r.after).contains("第三十条の二十七の二　"))
        .unwrap();
    assert!(!text(&old.after).contains("第四章の二の三"));
    assert!(!text(&old.before).contains("第四章の二の三"));
    let scope = rows
        .iter()
        .find(|r| text(&r.after).starts_with("第四章の二の三　"))
        .unwrap();
    assert!(text(&scope.after).contains("第三十条の三十三の二の五　"));
    assert!(text(&scope.before).starts_with("第四章の二の三　"));
}
#[test]
fn original_mixed_order_context_keeps_the_next_caption_fields() {
    let a = analyze(
        &[
            include_bytes!("data/native-caption-fields/20260916g002060046.pdf"),
            include_bytes!("data/native-caption-fields/20260916g002060047.pdf"),
        ],
        include_str!("data/native-caption-fields/20260916g00206-46-47-reviewed134.json"),
    );
    fields(&a);
}

#[test]
fn original_numbered_guideline_chapter_begins_its_own_row_across_pages() {
    let a = analyze(
        &[
            include_bytes!("data/native-caption-fields/20260827g001910001.pdf"),
            include_bytes!("data/native-caption-fields/20260827g001910002.pdf"),
            include_bytes!("data/native-caption-fields/20260827g001910003.pdf"),
            include_bytes!("data/native-caption-fields/20260827g001910004.pdf"),
            include_bytes!("data/native-caption-fields/20260827g001910005.pdf"),
            include_bytes!("data/native-caption-fields/20260827g001910006.pdf"),
        ],
        include_str!("data/native-caption-fields/20260827g00191-1-6-reviewed134.json"),
    );
    let rows = rows(&a);
    let scope = rows
        .iter()
        .find(|r| text(&r.after).starts_with("第３章　研究の適正な実施等"))
        .unwrap();
    assert!(text(&scope.after).contains("第６　研究計画書に関する手続"));
    assert!(text(&scope.before).starts_with("第３章　研究の適正な実施等"));
}

#[test]
fn original_renumbered_section_stays_with_its_following_article() {
    let a = analyze(
        &[
            include_bytes!("data/native-caption-fields/20260918g002090063.pdf"),
            include_bytes!("data/native-caption-fields/20260918g002090064.pdf"),
        ],
        include_str!("data/native-caption-fields/20260918g00209-63-64-reviewed134.json"),
    );
    let rows = rows(&a);
    let old = rows
        .iter()
        .find(|r| text(&r.after).contains("第三十六条の八　"))
        .unwrap();
    assert!(!text(&old.after).contains("第十一節"));
    assert!(!text(&old.before).contains("第九節"));
    let scope = rows
        .iter()
        .find(|r| text(&r.after).starts_with("第十一節　"))
        .unwrap();
    assert!(text(&scope.after).contains("第三十六条の十三　"));
    assert!(text(&scope.before).starts_with("第九節　"));
}

#[test]
fn original_omitted_part_is_not_the_previous_articles_tail() {
    let a = analyze(
        &[
            include_bytes!("data/native-caption-fields/20261009g002230055.pdf"),
            include_bytes!("data/native-caption-fields/20261009g002230056.pdf"),
        ],
        include_str!("data/native-caption-fields/20261009g00223-55-56-reviewed134.json"),
    );
    let rows = rows(&a);
    let old = rows
        .iter()
        .find(|r| text(&r.after).starts_with("第百八十三条　"))
        .unwrap();
    assert!(!text(&old.after).contains("第三編"));
    assert!(!text(&old.before).contains("第三編"));
    let scope = rows
        .iter()
        .find(|r| text(&r.after).starts_with("第三編　"))
        .unwrap();
    assert!(text(&scope.after).contains("第百八十四条〜第百九十四条"));
    assert!(text(&scope.before).starts_with("第三編　"));
}
#[test]
fn original_rescue_section_keeps_its_source_article_scope() {
    let a = analyze(
        &[
            include_bytes!("data/native-caption-fields/20261009g002230064.pdf"),
            include_bytes!("data/native-caption-fields/20261009g002230065.pdf"),
        ],
        include_str!("data/native-caption-fields/20261009g00223-64-65-reviewed134.json"),
    );
    let rows = rows(&a);
    let old = rows
        .iter()
        .find(|r| text(&r.after).contains("第二百七十条　"))
        .unwrap();
    assert!(!text(&old.after).contains("第四節"));
    let scope = rows
        .iter()
        .find(|r| text(&r.after).starts_with("第四節　緊急救助操法"))
        .unwrap();
    assert!(text(&scope.after).contains("第二百七十一条　"));
    assert!(text(&scope.before).starts_with("第四節　"));
}

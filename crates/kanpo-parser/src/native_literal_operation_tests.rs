use super::super::State;
use super::*;
fn states(rows: &[(&str, bool)]) -> Vec<State> {
    let ranges = (0..rows.len()).map(|i| i..i + 1).collect::<Vec<_>>();
    let scopes = crate::native_quote_scopes::Scopes::for_test(rows, &ranges);
    super::super::states_with_scopes(rows, &scopes)
}
fn certified(text: &str) -> bool {
    let c = text.chars().collect::<Vec<_>>();
    let mut p = Parser {
        c: &c,
        lists: HashMap::new(),
        clauses: HashMap::new(),
        neutral_suffix: false,
    };
    p.certificate(0) == Some(c.len())
}
fn after(text: &str) -> State {
    states(&[(text, true), ("後続の独立した本文", true)])[1]
}
#[test]
fn equal_unmatched_closing_context_is_a_literal_fragment_replacement() {
    let text = "第三条中「甲」と、乙」を「丙」と、丁」に改める。";
    assert!(certified(text));
    assert!(after(text).strict());
}
#[test]
fn equal_unmatched_opening_context_is_a_literal_fragment_replacement() {
    let text = "第十二条中「甲「乙」を「丙「丁」に改める。";
    assert!(certified(text));
    assert!(after(text).strict());
}
#[test]
fn unequal_interior_quote_context_cannot_authorize_a_replacement() {
    for text in [
        "第三条中「甲」乙」を「丙」に改める。",
        "第三条中「甲『乙」を「丙「丁」に改める。",
    ] {
        assert!(!certified(text));
        assert!(!after(text).strict());
    }
}
#[test]
fn balanced_insertions_and_replacements_share_one_complete_operation_list() {
    assert!(certified(
        "第一条中「甲」の下に「乙」を加え、「丙」を「丁」に改める。"
    ));
}
#[test]
fn one_insertion_can_have_multiple_literal_anchors() {
    assert!(certified(
        "第十二条中「甲」と、「乙」の下に「丙」を加える。"
    ));
}
#[test]
fn a_literal_anchor_can_contain_a_closing_quote_without_changing_inserted_context() {
    let text = "第三条中「甲」と、」の下に「乙」を加える。";
    assert!(certified(text));
    assert!(after(text).strict());
}
#[test]
fn a_true_outer_quote_is_kept_around_complete_fragment_operations() {
    let text = "第三条中「甲」と、乙」を「丙」と、丁」に改める。";
    let s = states(&[
        ("引用「", true),
        (text, true),
        ("後続本文", true),
        ("」", true),
        ("引用後", true),
    ]);
    assert!(!s[2].empty);
    assert!(s[4].strict());
}
#[test]
fn an_incomplete_insertion_cannot_borrow_a_later_publication_as_its_end() {
    for text in [
        "第三条中「甲」の下に「乙」を加える",
        "第三条中「甲」の下に「乙」を加え、未知の続き。",
    ] {
        assert!(!certified(text));
    }
}
#[test]
fn a_nonbody_barrier_prevents_complete_fragment_operation_authority() {
    let s = states(&[
        ("第三条中「甲」と、乙」を", true),
        ("表", false),
        ("「丙」と、丁」に改める。", true),
        ("後続", true),
    ]);
    assert!(!s[3].strict());
}
#[test]
fn original_article_ninety_keeps_literal_quotes_inside_complete_edit_operands() {
    let text=concat!(
        "第九十条中「第七十六条」の下に「（第四項を除く。）」を加え、「各開票区における」を「開票区ごと",
        "に、当該開票区の区域の全部又は一部をその区域に含む市町村の」に改め、「者」と、「市町村の選挙管",
        "理委員会」の下に「に届け出ることができる。ただし、同一人を他の開票区における開票立会人とな",
        "るべき者として届け出ることはできない」を加え、「国民投票分会長」と、同条第二項」を「国民投票",
        "分会長に届け出ることができる」と、同条第二項」に、「同条第四項中「又は」を「同条第五項本文中",
        "「達しないとき又は」とあるのは「達しないとき、」と、「」に、「、国民投票分会の」を「国民投票分会",
        "の」に、「その開票区における」を「その開票区の区域の全部又は一部をその区域に含む市町村の」に",
        "改め、「に」と、」の下に「同項ただし書中」を加える。");
    assert!(certified(text));
    assert!(after(text).strict());
}
#[test]
fn an_already_bound_replacement_cannot_become_a_later_clauses_insertion_anchor() {
    let first = "第三条中「甲」と、乙」を「丙」と、丁」に改める。";
    let text = format!("{first}第四条中「戊」の下に「己」を加える。");
    let c = text.chars().collect::<Vec<_>>();
    let mut p = Parser {
        c: &c,
        lists: HashMap::new(),
        clauses: HashMap::new(),
        neutral_suffix: false,
    };
    assert_eq!(p.certificate(0), Some(first.chars().count()));
    assert!(after(&text).strict());
}
#[test]
fn an_already_bound_insertion_does_not_consume_a_later_replacement_operand() {
    let first = "第三条中「甲」と、」の下に「乙」を加える。";
    let text = format!("{first}第四条中「丙」を「丁」に改める。");
    let c = text.chars().collect::<Vec<_>>();
    let mut p = Parser {
        c: &c,
        lists: HashMap::new(),
        clauses: HashMap::new(),
        neutral_suffix: false,
    };
    assert_eq!(p.certificate(0), Some(first.chars().count()));
}
#[test]
fn balanced_literal_anchors_can_quote_an_entire_replacement_operation() {
    assert!(certified(
        "第三条中「「甲」を「乙」に改める。」の下に「丙」を加える。"
    ));
}
#[test]
fn absent_native_scopes_keep_the_complete_legacy_abstention() {
    let text = "第三条中「甲」と、乙」を「丙」と、丁」に改める。";
    let rows = [(text, true), ("後続", true)];
    assert!(!super::super::states(&rows)[1].strict());
    assert!(states(&rows)[1].strict());
}
#[test]
fn a_native_scope_cannot_transfer_to_different_row_material_or_eligibility() {
    let text = "第三条中「甲」と、乙」を「丙」と、丁」に改める。";
    let rows = [(text, true), ("後続", true)];
    let scope = crate::native_quote_scopes::Scopes::for_test(&rows, &[0..1]);
    let other = text.replace('甲', "別");
    assert!(
        !super::super::states_with_scopes(&[(&other, true), ("後続", true)], &scope)[1].strict()
    );
    assert!(
        !super::super::states_with_scopes(&[(text, false), ("後続", true)], &scope)[1].strict()
    );
}
#[test]
fn an_incomplete_native_paragraph_cannot_borrow_the_next_paragraphs_operand() {
    let rows = [
        ("第三条中「甲」と、乙」を", true),
        ("「丙」と、丁」に改める。", true),
        ("後続", true),
    ];
    let short = crate::native_quote_scopes::Scopes::for_test(&rows, &[0..1]);
    assert!(!super::super::states_with_scopes(&rows, &short)[2].strict());
    let complete = crate::native_quote_scopes::Scopes::for_test(&rows, &[0..2]);
    assert!(super::super::states_with_scopes(&rows, &complete)[2].strict());
}
#[test]
fn native_paragraph_evidence_does_not_reset_an_already_invalid_quote_state() {
    let text = "第三条中「甲」と、乙」を「丙」と、丁」に改める。";
    let rows = [("未知の閉じ引用」", true), (text, true), ("後続", true)];
    let scope = crate::native_quote_scopes::Scopes::for_test(&rows, &[1..2]);
    assert!(!super::super::states_with_scopes(&rows, &scope)[2].valid);
}
#[test]
fn a_native_paragraph_start_is_not_a_substring_search_for_a_later_edit() {
    let text = "前置きのある第三条中「甲」と、乙」を「丙」と、丁」に改める。";
    let rows = [(text, true), ("後続", true)];
    let scope = crate::native_quote_scopes::Scopes::for_test(&rows, &[0..1]);
    assert!(!super::super::states_with_scopes(&rows, &scope)[1].strict());
}
#[test]
fn empty_or_out_of_bounds_scopes_cannot_create_an_operation_certificate() {
    assert!(super::super::states_with_scopes(
        &[],
        &crate::native_quote_scopes::Scopes::for_test(&[], &[])
    )
    .is_empty());
    let text = "第三条中「甲」と、乙」を「丙」と、丁」に改める。";
    let rows = [(text, true), ("後続", true)];
    let scope = crate::native_quote_scopes::Scopes::for_test(&rows, &[0..9]);
    assert!(!super::super::states_with_scopes(&rows, &scope)[1].strict());
}
#[test]
fn competing_ends_of_the_same_bound_operand_remain_ambiguous() {
    let text = "第三条中「甲「乙」を「丙「丁」に改める。引用「戊」に改める。";
    assert!(!certified(text));
}
#[test]
fn an_empty_source_row_at_the_scope_end_cannot_extend_the_operation_window() {
    let rows = [
        ("第三条中「甲」と、乙」を", true),
        ("", true),
        ("「丙」と、丁」に改める。", true),
        ("後続", true),
    ];
    let scope = crate::native_quote_scopes::Scopes::for_test(&rows, &[0..1]);
    assert!(
        !super::super::states_with_scopes(&rows, &scope)[3].strict(),
        "the empty row is an exclusive source end, not the end of the whole document"
    );
}
#[test]
fn a_source_bounded_heading_replacement_can_precede_literal_partial_edits() {
    let text = "第１２条の見出しを「任意の見出し」に改め、同条中「甲」乙」を「丙」丁」に改める。";
    assert!(states(&[(text, true), ("後続", true)])[1].strict());
    assert!(!super::super::states(&[(text, true), ("後続", true)])[1].strict());
}
#[test]
fn a_completed_literal_edit_can_have_an_arbitrary_quote_free_source_suffix() {
    for tail in [
        "未知の続き。",
        "番号を変えて別の項を設ける。",
        "arbitrary source-neutral tail",
    ] {
        let text = format!("第三条中「甲」乙」を「丙」丁」に改め、{tail}");
        assert!(
            states(&[(&text, true), ("後続", true)])[1].strict(),
            "{tail}"
        );
        assert!(!super::super::states(&[(&text, true), ("後続", true)])[1].strict());
    }
}
#[test]
fn a_quote_bearing_suffix_cannot_be_declared_neutral() {
    let text = "第三条中「甲」乙」を「丙」丁」に改め、後続の引用「未完";
    assert!(!states(&[(text, true), ("後続", true)])[1].strict());
}

#[test]
fn whole_heading_prefixes_use_a_forward_loop_without_a_clause_budget() {
    let mut text = "第三条の見出しを「任意」に改め、".repeat(4097);
    text.push_str("同条中「甲」乙」を「丙」丁」に改める。");
    assert!(states(&[(&text, true), ("後続", true)])[1].strict());
}

use super::*;
fn names(xs: &[&str]) -> BTreeSet<String> {
    xs.iter().map(|s| s.to_string()).collect()
}
#[test]
fn declared_actor_lists_use_arbitrary_header_names_annotations_and_source_wrapping() {
    let ns = names(&["任意の甲社", "青の空", "別の会社"]);
    let body="当社は再編手続により任意の甲社（所在地と任意の注記）、青の空（補足（旧称「仮の名称」））および別の会社の事業に関して有する権利義務を承継することにしました。最終貸借対照表は次のとおりです。";
    assert_eq!(rights_transfer_parties(body, "再編手続", &ns), Some(ns));
}
#[test]
fn action_values_actor_name_lengths_and_list_lengths_are_not_thresholds() {
    let action = "任意の権利の再編".repeat(60);
    let a = "任意の名称".repeat(100);
    let b = "別の名称".repeat(80);
    let ns = BTreeSet::from([a.clone(), b.clone()]);
    let body =
        format!("当社は、{action}により{a}（任意の注記）及び{b}の事業に係る権利義務を承継します。");
    assert_eq!(rights_transfer_parties(&body, &action, &ns), Some(ns));
    let ns = (0..40)
        .map(|i| format!("任意の発行者{i}"))
        .collect::<BTreeSet<_>>();
    let body = format!(
        "当社は移転により{}の資産に関する権利義務を承継します。",
        ns.iter().cloned().collect::<Vec<_>>().join("、")
    );
    assert_eq!(rights_transfer_parties(&body, "移転", &ns), Some(ns));
}
#[test]
fn mentions_in_a_property_annotation_quote_or_a_later_sentence_do_not_declare_an_actor() {
    let ns = names(&["本来の当事者", "比較対象"]);
    for body in [
 "当社は再編手続により本来の当事者（比較対象についての注記）の比較対象に関する事業の権利義務を承継します。",
 "当社は再編手続により本来の当事者の事業の権利義務を承継します。比較対象の表にも言及します。"]{
 assert_eq!(rights_transfer_parties(body,"再編手続",&ns),Some(names(&["本来の当事者"])));
 }
    for body in [
        "「当社は再編手続により比較対象の事業の権利義務を承継します」を引用します。",
        "当社は再編手続により「比較対象」の事業の権利義務を承継します。",
        "当社は別の手続を行います。当社は再編手続により比較対象の事業の権利義務を承継します。",
        "当社は再編手続について比較対象の権利義務を引用します。",
    ] {
        assert_eq!(
            rights_transfer_parties(body, "再編手続", &ns),
            None,
            "{body}"
        );
    }
}
#[test]
fn ambiguous_list_tokenizations_and_broken_records_do_not_obtain_a_relation() {
    let ns = names(&["甲", "乙", "甲および乙"]);
    assert!(rights_transfer_parties(
        "当社は移転により甲および乙の権利義務を承継します。",
        "移転",
        &ns
    )
    .is_none());
    for body in [
        "当社は移転により甲（未完の注記の権利義務を承継します。",
        "当社は移転により甲（別の注記））の権利義務を承継します。",
        "当社（甲）は移転により乙（甲）の権利義務を承継します。",
        "当社は移転により甲の「権利義務を承継」という文を引用します。",
        "当社は移転により甲の権利義務を引用します。",
    ] {
        assert!(
            rights_transfer_parties(body, "移転", &ns).is_none(),
            "{body}"
        );
    }
}

#[test]
fn negated_conditional_and_unresolved_predicates_do_not_declare_a_transfer() {
    let names = std::collections::BTreeSet::from(["任意の発行者".to_string()]);
    for tail in [
        "権利義務を承継しません",
        "権利義務を承継しないことにしました",
        "権利義務を承継するかどうかは未定です",
        "権利義務を承継する場合は別途公告します",
        "権利義務を承継すると仮定します",
    ] {
        let text = format!("当社は移転により任意の発行者の{tail}。");
        assert!(
            rights_transfer_parties(&text, "移転", &names).is_none(),
            "{text}"
        );
    }
}

fn held(tail: &str) {
    let names = std::collections::BTreeSet::from(["別の任意名".to_string()]);
    assert!(rights_transfer_parties(
        &format!("当社は移転により別の任意名の{tail}。"),
        "移転",
        &names
    )
    .is_none());
}
#[test]
fn negation() {
    held("権利義務を承継しません");
}
#[test]
fn negated_decision() {
    held("権利義務を承継しないことにしました");
}
#[test]
fn undecided_question() {
    held("権利義務を承継するかどうかは未定です");
}
#[test]
fn conditional() {
    held("権利義務を承継する場合は別途公告します");
}
#[test]
fn hypothesis() {
    held("権利義務を承継すると仮定します");
}
#[test]
fn affirmative_inflections_and_optional_formal_declaration_are_complete_predicates() {
    let ns = std::collections::BTreeSet::from(["任意の当事者".to_string()]);
    for ending in [
        "します",
        "しました",
        "いたします",
        "いたしました",
        "することにしました",
        "することにいたしました",
        "することとしました",
        "することといたしました",
    ] {
        for report in ["", "ので公告します", "ので公告いたします"] {
            let body =
                format!("当社は移転により任意の当事者の対象の権利義務を承継{ending}{report}。");
            assert_eq!(
                rights_transfer_parties(&body, "移転", &ns),
                Some(ns.clone()),
                "{body}"
            );
        }
    }
}

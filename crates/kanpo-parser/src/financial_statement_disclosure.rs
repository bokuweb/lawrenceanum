//! Only an affirmative, unquoted disclosure clause authorizes a local annex.
//! Legal party labels are opaque values tied to the notice's signed records.
use super::*;
use std::collections::BTreeMap;
fn role_tail(text: &str) -> Option<(&str, &str)> {
    let label = parenthesized_role(text)?;
    let mut stack = vec![];
    for (i, c) in text.char_indices() {
        match c {
            '(' => stack.push(')'),
            '（' => stack.push('）'),
            ')' | '）' => {
                if stack.pop() != Some(c) {
                    return None;
                }
                if stack.is_empty() {
                    return Some((label, &text[i + c.len_utf8()..]));
                }
            }
            _ => {}
        }
    }
    None
}
fn unquoted(text: &str) -> Option<String> {
    let mut out = String::new();
    let mut stack = vec![];
    for c in text.chars() {
        match c {
            '「' => {
                stack.push('」');
                out.push('\0')
            }
            '『' => {
                stack.push('』');
                out.push('\0')
            }
            '」' | '』' => {
                if stack.pop() != Some(c) {
                    return None;
                }
                out.push('\0')
            }
            _ => out.push(if stack.is_empty() { c } else { '\0' }),
        }
    }
    stack.is_empty().then_some(out)
}
fn local(text: &str) -> bool {
    ["下記", "左記", "右記", "上記", "次", "以下"]
        .iter()
        .any(|d| {
            text.strip_prefix(d).is_some_and(|s| {
                [
                    "のとおりです",
                    "のとおりであります",
                    "のとおりでございます",
                    "の通りです",
                    "の通りであります",
                    "の通りでございます",
                ]
                .contains(&s)
            })
        })
}
fn external(text: &str) -> bool {
    text == "掲載官報のとおりです"
        || text.strip_prefix("掲載官報掲載の日付").is_some_and(|s| {
            s.split_once("掲載頁").is_some_and(|(date, page)| {
                !date.is_empty() && !page.is_empty() && !text.contains('\0')
            })
        })
}
fn policy(text: &str) -> Option<bool> {
    if local(text) {
        Some(true)
    } else if external(text) {
        Some(false)
    } else {
        None
    }
}
fn same<T: Eq + Copy>(old: &mut Option<T>, value: T) -> Option<()> {
    if old.is_some_and(|v| v != value) {
        return None;
    }
    *old = Some(value);
    Some(())
}
pub(super) fn references_local(
    text: &str,
    attribution: &[BTreeSet<String>],
    name: &str,
) -> Option<bool> {
    let text = unquoted(text)?;
    let sentences = text
        .split('。')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    let mut global = None;
    let mut roles = BTreeMap::new();
    let mut declared = false;
    let mut role_list = false;
    for sentence in sentences {
        let clause = sentence.strip_prefix("なお、").unwrap_or(sentence);
        let clause = clause.strip_prefix("最終").unwrap_or(clause);
        if let Some(tail) = clause.strip_prefix("貸借対照表の要旨は") {
            declared = true;
            same(&mut global, policy(tail)?)?;
            role_list = false;
        } else if let Some(tail) = clause.strip_prefix("貸借対照表の開示状況は") {
            if !local(tail) {
                return None;
            }
            declared = true;
            role_list = true;
        } else if role_list {
            let (label, tail) = role_tail(clause)?;
            let value = policy(tail)?;
            if roles
                .insert(label.to_owned(), value)
                .is_some_and(|old| old != value)
            {
                return None;
            }
        } else if clause.contains("貸借対照表の要旨は") || clause.contains("貸借対照表の開示状況は")
        {
            // A quoted example, negated qualification or different subject
            // cannot become authority by finding a positive suffix inside it.
            return None;
        }
    }
    if !declared {
        return None;
    }
    if roles.is_empty() {
        return global;
    }
    let labels = attribution
        .iter()
        .filter(|fields| fields.contains(name))
        .flat_map(|fields| fields.iter())
        .filter_map(|field| {
            role_tail(field)
                .filter(|(_, tail)| *tail == name)
                .map(|(label, _)| label)
        })
        .collect::<BTreeSet<_>>();
    if labels.len() != 1 {
        return None;
    }
    let value = *roles.get(*labels.iter().next()?)?;
    if global.is_some_and(|g| g != value) {
        return None;
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fields(label: &str, name: &str) -> Vec<BTreeSet<String>> {
        vec![BTreeSet::from([name.into(), format!("（{label}）{name}")])]
    }
    fn body(label: &str, local: &str) -> String {
        format!("会社分割の債権者はお申し出下さい。なお、最終貸借対照表の開示状況は次のとおりです。（{label}）{local}。（別当事者）掲載官報掲載の日付令和七年十二月二十四日掲載頁五十五頁（号外第二八三号）")
    }
    #[test]
    fn signed_opaque_role_selects_only_its_own_disclosure() {
        let s = body("借受人", "下記のとおりです");
        assert_eq!(
            references_local(&s, &fields("借受人", "一"), "一"),
            Some(true)
        );
        assert_eq!(
            references_local(&s, &fields("別当事者", "二"), "二"),
            Some(false)
        );
    }
    #[test]
    fn role_labels_and_issuer_values_have_no_length_limits() {
        let label = "任意の当事者".repeat(150);
        let name = "任意の法人".repeat(200);
        assert_eq!(
            references_local(
                &body(&label, "下記のとおりです"),
                &fields(&label, &name),
                &name
            ),
            Some(true)
        );
    }
    #[test]
    fn role_identity_is_not_a_prefix_or_substring_match() {
        let s = body("甲", "下記のとおりです");
        assert_eq!(references_local(&s, &fields("甲乙", "一"), "一"), None);
        assert_eq!(references_local(&s, &fields("甲", "一二"), "一"), None);
    }
    #[test]
    fn undisclosed_or_ambiguous_role_is_held() {
        let s = body("甲", "下記のとおりです");
        let mut f = fields("甲", "一");
        f.extend(fields("乙", "一"));
        assert_eq!(references_local(&s, &f, "一"), None);
        assert_eq!(
            references_local(&s, &[BTreeSet::from(["一".into()])], "一"),
            None
        );
    }
    #[test]
    fn conflicting_party_disclosures_are_held() {
        let s = body("甲", "下記のとおりです") + "。（甲）掲載官報のとおりです";
        assert_eq!(references_local(&s, &fields("甲", "一"), "一"), None);
    }
    #[test]
    fn a_quoted_disclosure_does_not_supply_the_missing_party() {
        let s = "最終貸借対照表の開示状況は次のとおりです。「（甲）下記のとおりです。」";
        assert_eq!(references_local(s, &fields("甲", "一"), "一"), None);
    }
    #[test]
    fn an_unclosed_quote_never_supplies_disclosure() {
        assert_eq!(
            references_local("最終貸借対照表の要旨は下記のとおりです。「", &[], "一"),
            None
        );
    }
    #[test]
    fn unknown_reference_policy_does_not_become_external_or_local() {
        assert_eq!(
            references_local(&body("甲", "別途確認して下さい"), &fields("甲", "一"), "一"),
            None
        );
    }
    #[test]
    fn negated_and_qualified_disclosures_are_held() {
        for s in [
            "最終貸借対照表の要旨は下記のとおりではありません",
            "必ずしも最終貸借対照表の要旨は下記のとおりです",
            "最終貸借対照表の要旨は下記のとおりですとは限りません",
        ] {
            assert_eq!(references_local(s, &[], "一"), None, "{s}");
        }
    }
    #[test]
    fn balanced_incidental_quotes_do_not_hide_an_independent_positive_clause() {
        assert_eq!(
            references_local(
                "説明「別途の例」。なお、最終貸借対照表の要旨は下記のとおりです。",
                &[],
                "一"
            ),
            Some(true)
        );
    }
    #[test]
    fn bracketed_annotations_do_not_change_a_role_value() {
        let s = body("当事者、承継会社（特定事業）", "下記のとおりです");
        assert_eq!(
            references_local(&s, &fields("当事者、承継会社（特定事業）", "一"), "一"),
            Some(true)
        );
    }
    #[test]
    fn unbalanced_role_annotations_are_held() {
        let s = body("当事者、承継会社（特定事業", "下記のとおりです");
        assert_eq!(references_local(&s, &fields("当事者", "一"), "一"), None);
    }
}

use super::*;
fn words(parts: &[&str]) -> Vec<Word> {
    parts
        .iter()
        .enumerate()
        .map(|(id, s)| Word {
            id,
            x0: 0.,
            y0: 0.,
            x1: 1.,
            y1: 1.,
            source_box: [0., 0., 1., 1.],
            raw: s.to_string(),
            text: s.to_string(),
            status: WordStatus::Used,
        })
        .collect()
}
#[test]
fn exact_unicode_word_ranges_keep_the_existing_fragments_and_internal_whitespace() {
    for parts in [
        ["任意", "漢字", "同じ", "漢字"],
        ["alpha", "beta", "alpha", "beta"],
        ["é\u{301}", "🦀", "א", "ب"],
        ["𝒜", "１２", "Ж", "가"],
        ["John Doe", "x-y", "A/B", "@"],
    ] {
        let w = words(&parts);
        for sides in [[0, 0, 1, 1], [1, 1, 0, 0]] {
            let left = format!("　{}\t{}", parts[0], parts[1]);
            let right = format!("{}\n{}　", parts[2], parts[3]);
            let line = Line::text(format!("{left} \t　{right}"), vec![0, 1, 2, 3]);
            let result = partition(&line, &w, &sides).unwrap();
            assert_eq!(
                result[sides[0]].as_ref().unwrap(),
                &Line::text(left, vec![0, 1])
            );
            assert_eq!(
                result[sides[2]].as_ref().unwrap(),
                &Line::text(right, vec![2, 3])
            );
        }
    }
}
#[test]
fn repeated_text_uses_original_word_owners_without_a_length_or_neighbor_budget() {
    for count in [1, 2, 17, 1009] {
        let text = vec!["同"; count * 2];
        let w = words(&text);
        let line = Line::text(text.join("　"), (0..count * 2).collect());
        let sides = (0..count * 2)
            .map(|i| usize::from(i >= count))
            .collect::<Vec<_>>();
        let result = partition(&line, &w, &sides).unwrap();
        for side in 0..2 {
            let first = side * count;
            let part = result[side].as_ref().unwrap();
            assert_eq!(part.words, (first..first + count).collect::<Vec<_>>());
            assert_eq!(part.text, vec!["同"; count].join("　"));
        }
    }
}
#[test]
fn a_character_multiset_or_normalization_cannot_replace_the_ordered_source_stream() {
    let w = words(&["AB", "CD"]);
    for text in [
        "BA　CD",
        "AB　DC",
        "AB　CＤ",
        "ab　CD",
        "AB　C",
        "AB　CDE",
        "",
    ] {
        assert!(
            partition(&Line::text(text.into(), vec![0, 1]), &w, &[0, 1]).is_none(),
            "{text}"
        );
    }
}
#[test]
fn missing_duplicate_or_foreign_owners_never_supply_source_text_ranges() {
    let original = words(&["A", "B"]);
    let line = Line::text("A　B".into(), vec![0, 1]);
    for case in 0..5 {
        let mut w = original.clone();
        let mut l = line.clone();
        match case {
            0 => w[0].id = 1,
            1 => w[0].status = WordStatus::Noise,
            2 => l.words[1] = 0,
            3 => {
                w.pop();
            }
            _ => l.words[1] = usize::MAX,
        }
        assert!(partition(&l, &w, &[0, 1]).is_none(), "case{case}");
    }
}
#[test]
fn empty_or_boundary_whitespace_words_cannot_claim_an_ambiguous_seam() {
    for text in ["", " ", "A ", " A", "\nA", "A　"] {
        let w = words(&[text, "B"]);
        let line = Line::text(format!("{text}　B"), vec![0, 1]);
        assert!(partition(&line, &w, &[0, 1]).is_none());
    }
}
#[test]
fn interleaved_missing_or_unknown_fields_are_not_two_source_fragments() {
    let w = words(&["A", "B", "C"]);
    let line = Line::text("A　B　C".into(), vec![0, 1, 2]);
    for sides in [
        vec![],
        vec![0, 1],
        vec![0, 0, 0],
        vec![0, 1, 0],
        vec![0, 2, 1],
        vec![0, 1, 1, 1],
    ] {
        assert!(partition(&line, &w, &sides).is_none());
    }
}
#[test]
fn a_physical_table_is_never_flattened_to_prove_a_plain_text_split() {
    let w = words(&["A", "B"]);
    let mut line = Line::text("A　B".into(), vec![0, 1]);
    line.table = Some(crate::trace::Table {
        x0: 0.,
        y0: 0.,
        x1: 2.,
        y1: 2.,
        rows: 1,
        cols: 2,
        cells: vec![],
    });
    assert!(partition(&line, &w, &[0, 1]).is_none());
}

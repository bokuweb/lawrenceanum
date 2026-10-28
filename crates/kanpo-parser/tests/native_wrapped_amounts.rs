//! Original wrapped-label amount slots retain their printed rectangle.
#[test]
fn the_original_rectangle_is_owned_by_the_wrapped_label_amount_cell() {
    let a = kanpo_parser::analyze_item(
        &kanpo_parser::page_inputs(include_bytes!(
            "archive/pages/20260807g00178/20260807g001780121.pdf"
        ))
        .unwrap(),
    );
    let p = &a.pages[0];
    let mark = &p.words[0];
    assert_eq!(mark.source_box, [531.5, 138.3597, 539.5, 146.5437]);
    assert_eq!(
        mark.text, "－",
        "the printed rectangle in the wrapped-label amount slot is missing"
    );
    assert_eq!(mark.status, kanpo_parser::trace::WordStatus::Used);
    let table = p
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .find(|t| t.x0 == 437.75 && t.y0 == 101.75)
        .unwrap();
    let cells: Vec<_> = table
        .cells
        .iter()
        .filter(|c| c.row == 4 && c.col == 1)
        .collect();
    assert_eq!(cells.len(), 1);
    assert_eq!(cells[0].text, "－");
    assert_eq!(cells[0].words, vec![0]);
    assert!(a
        .text
        .contains("販売費及び一般管理費　－\n営業利益　1,419,551"));
    let mut owners = std::collections::HashSet::new();
    for id in p.regions.iter().flat_map(|r| &r.lines).flat_map(|l| {
        l.words.iter().chain(
            l.table
                .iter()
                .flat_map(|t| t.cells.iter().flat_map(|c| &c.words)),
        )
    }) {
        assert!(owners.insert(*id), "duplicate original Word{id}");
    }
}

fn complete_value(a: &kanpo_parser::Analysis) -> serde_json::Value {
    serde_json::from_str(&serde_json::to_string(a).unwrap()).unwrap()
}
fn expected() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "data/native-wrapped-amounts/expected/20260807g001780121.json"
    ))
    .unwrap()
}
#[test]
fn the_complete_original_page_matches_only_the_independent_source_insertion() {
    let a = kanpo_parser::analyze_item(
        &kanpo_parser::page_inputs(include_bytes!(
            "archive/pages/20260807g00178/20260807g001780121.pdf"
        ))
        .unwrap(),
    );
    assert!(complete_value(&a)==expected(),"only the original rectangle, its empty cell and corresponding text/AST may change; every other complete Analysis field stays exact");
}
#[test]
fn reversing_original_word_nodes_keeps_the_complete_source_owned_result() {
    let mut inputs = kanpo_parser::page_inputs(include_bytes!(
        "archive/pages/20260807g00178/20260807g001780121.pdf"
    ))
    .unwrap();
    let h = &inputs[0].xhtml;
    let mut nodes = vec![];
    let mut cursor = 0;
    while let Some(at) = h[cursor..].find("<word ") {
        let start = cursor + at;
        let end = start + h[start..].find("</word>").unwrap() + 7;
        nodes.push((start, end));
        cursor = end;
    }
    let mut out = String::new();
    cursor = 0;
    for ((start, end), (from, to)) in nodes.iter().zip(nodes.iter().rev()) {
        out.push_str(&h[cursor..*start]);
        out.push_str(&h[*from..*to]);
        cursor = *end;
    }
    out.push_str(&h[cursor..]);
    inputs[0].xhtml = out;
    assert!(complete_value(&kanpo_parser::analyze_item(&inputs))==expected(),"original source occurrences must remain authoritative independently of XHTML node emission order");
}

macro_rules! original_complete {
 ($name:ident,$pdf:literal,$expected:literal)=>{
 #[test]fn $name(){
  let a=kanpo_parser::analyze_item(&kanpo_parser::page_inputs(include_bytes!($pdf)).unwrap());
  let expected:serde_json::Value=serde_json::from_str(include_str!($expected)).unwrap();
  assert!(complete_value(&a)==expected,"only independently source-curated rectangle insertions may change any complete Analysis field");
 }
 };}
original_complete!(
    complete_20260716g001590094,
    "data/native-wrapped-amounts/20260716g001590094.pdf",
    "data/native-wrapped-amounts/expected/20260716g001590094.json"
);
original_complete!(
    complete_20260723g001630226,
    "data/native-wrapped-amounts/20260723g001630226.pdf",
    "data/native-wrapped-amounts/expected/20260723g001630226.json"
);
original_complete!(
    complete_20260727g001670084,
    "data/native-wrapped-amounts/20260727g001670084.pdf",
    "data/native-wrapped-amounts/expected/20260727g001670084.json"
);
original_complete!(
    complete_20260730g001710158,
    "data/native-wrapped-amounts/20260730g001710158.pdf",
    "data/native-wrapped-amounts/expected/20260730g001710158.json"
);
original_complete!(
    complete_20260731g001720300,
    "data/native-wrapped-amounts/20260731g001720300.pdf",
    "data/native-wrapped-amounts/expected/20260731g001720300.json"
);
original_complete!(
    complete_20260731g001720302,
    "data/native-wrapped-amounts/20260731g001720302.pdf",
    "data/native-wrapped-amounts/expected/20260731g001720302.json"
);
original_complete!(
    complete_20260902g001960131,
    "data/native-wrapped-amounts/20260902g001960131.pdf",
    "data/native-wrapped-amounts/expected/20260902g001960131.json"
);
original_complete!(
    complete_20261005g002190073,
    "data/native-wrapped-amounts/20261005g002190073.pdf",
    "data/native-wrapped-amounts/expected/20261005g002190073.json"
);
original_complete!(
    complete_20261014g002250071,
    "data/native-wrapped-amounts/20261014g002250071.pdf",
    "data/native-wrapped-amounts/expected/20261014g002250071.json"
);

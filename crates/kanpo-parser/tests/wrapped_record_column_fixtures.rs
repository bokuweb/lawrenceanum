#[test]
fn wrapped_road_qualifier_stays_with_its_record() {
    let pdf = include_bytes!("archive/pages/20260901g00195/20260901g001950029.pdf");
    let out = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    for expected in [
        "一般国道45号三陸　岩手県陸前高田市気仙町字水上152番１（陸　一般国道45号",
        "一般国道112号　山形県鶴岡市道形町（鶴岡市道形町交差点）　山形県道　代鶴岡線（山形県鶴\n岡市宝田２丁目１番40方向の車\n線に限る）",
        "山形県道　代鶴岡　山形県鶴岡市道形町（鶴岡市道形町交差点）　一般国道112号",
        "一般国道７号　山形県鶴岡市温海（鶴岡市温海交差点）　山形県道余目温海線",
        "山形県道鶴岡羽黒　山形県鶴岡市東原町（鶴岡市東原町交差点）　一般国道112号",
    ] {assert!(out.text.contains(expected),"{expected}: {}",out.text);}
    let mut ids = std::collections::HashSet::new();
    for id in out.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .flat_map(|l| {
            l.words
                .iter()
                .chain(l.table.iter().flat_map(|t| &t.cells).flat_map(|c| &c.words))
        })
    {
        assert!(ids.insert(id));
    }
}

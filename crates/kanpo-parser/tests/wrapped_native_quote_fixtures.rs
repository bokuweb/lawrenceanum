#[test]
fn a_wrapped_address_amendment_preserves_both_addresses_and_surrounding_notices() {
    let pdf = include_bytes!("archive/pages/20261009h01805/20261009h018050004.pdf");
    let pages = kanpo_parser::page_inputs(pdf).unwrap();
    let out = kanpo_parser::analyze_item(&pages);
    let expected = "指定営業所として指定する青森事務所の所在地中「青森市中央一丁目22番８号」を「青森市長島二\n丁目10番３号」に改める。";
    assert!(out.text.contains(expected));
    let no_native: Vec<_> = pages
        .iter()
        .map(|p| kanpo_parser::PageInput {
            xhtml: p.xhtml.replace("data-wmode=\"1\"", "data-wmode=\"9\""),
            rules: p.rules.clone(),
        })
        .collect();
    let old = kanpo_parser::analyze_item(&no_native);
    let start = "〇財務省告示第二百七十六号";
    let end = "〇財務省・農林水産省・経済産業省告示第七号";
    assert_eq!(out.text.split(start).next(), old.text.split(start).next());
    assert_eq!(out.text.split(end).nth(1), old.text.split(end).nth(1));
    let chars = |s: &str| {
        let mut c: Vec<_> = s.chars().filter(|c| !c.is_whitespace()).collect();
        c.sort();
        c
    };
    assert_eq!(chars(&out.text), chars(&old.text));
    let p = &out.pages[0];
    let mut ids = std::collections::HashSet::new();
    for id in p
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .flat_map(|l| l.words.iter().chain(l.table.iter().flat_map(|t| &t.cells).flat_map(|c| &c.words)))
    {
        assert!(ids.insert(*id));
    }
    let used: std::collections::HashSet<_> = p
        .words
        .iter()
        .filter(|w| matches!(w.status, kanpo_parser::trace::WordStatus::Used))
        .map(|w| w.id)
        .collect();
    assert_eq!(used.len(), 293);
    assert_eq!(used, ids);
    assert_eq!(
        p.words.iter().map(|w| (&w.id, &w.raw)).collect::<Vec<_>>(),
        old.pages[0].words.iter().map(|w| (&w.id, &w.raw)).collect::<Vec<_>>()
    );
}

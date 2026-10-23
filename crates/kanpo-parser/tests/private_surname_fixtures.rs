#[test]
fn the_source_private_glyph_remains_in_the_director_surname() {
    let pdf = include_bytes!("archive/pages/20260806g00177/20260806g001770079.pdf");
    let out = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    assert!(out.text.contains("取締役　鄭　武壽"), "{}", out.text);
    let word = out.pages[0].words.iter().find(|w| w.raw == "\u{E469}").unwrap();
    assert_eq!(word.text, "鄭");
    assert_eq!(word.status, kanpo_parser::trace::WordStatus::Used);
    assert_eq!(
        out.pages[0]
            .regions
            .iter()
            .flat_map(|r| &r.lines)
            .flat_map(|l| l.words.iter().chain(l.table.iter().flat_map(|t| &t.cells).flat_map(|c| &c.words)))
            .filter(|&&id| id == word.id)
            .count(),
        1
    );
}

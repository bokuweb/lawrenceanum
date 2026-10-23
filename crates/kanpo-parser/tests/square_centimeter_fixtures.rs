#[test]
fn medical_material_area_units_are_preserved_on_both_pages() {
    for pdf in [
        include_bytes!("archive/pages/20260831g00193/20260831g001930165.pdf").as_slice(),
        include_bytes!("archive/pages/20260831g00193/20260831g001930166.pdf").as_slice(),
    ] {
        let out = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
        assert!(out.text.contains("１㎠当たり48円"));
        let words: Vec<_> = out.pages.iter().flat_map(|p| &p.words).filter(|w| w.raw.contains('\u{E68C}')).collect();
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].text, "１㎠当たり48円");
    }
}

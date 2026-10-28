//! Original same-baseline spacing, plus a constructed font-size counterexample.
//! Whole-Analysis references freeze reviewed target changes; they are not
//! independently correct full-page transcriptions.
use kanpo_parser::Analysis;

fn analyze(pdf: &[u8], expected: &str) -> Analysis {
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    // Compare the serialized public JSON contract. Direct to_value expands f32
    // coordinates into f64 instead of using their normal JSON representation.
    let actual: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    let expected: serde_json::Value = serde_json::from_str(expected).unwrap();
    assert_eq!(actual, expected, "complete native extraction regression");
    a
}

fn cells(a: &Analysis) -> Vec<&str> {
    a.pages
        .iter()
        .flat_map(|p| &p.regions)
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .flat_map(|t| &t.cells)
        .map(|c| c.text.as_str())
        .collect()
}

#[test]
fn original_roman_designations_keep_their_printed_word_gaps() {
    let a = analyze(
        include_bytes!("data/native-cell-gaps/20260722g001620027.pdf"),
        include_str!("data/native-cell-gaps/20260722g001620027-reviewed136.json"),
    );
    assert_eq!(
        cells(&a)
            .iter()
            .filter(|&&s| s == "・モンテレイエス　No.4 LE 2021")
            .count(),
        1
    );
}

#[test]
fn both_original_address_occurrences_keep_their_own_postal_gap() {
    let a = analyze(
        include_bytes!("data/native-cell-gaps/20260813h017670004.pdf"),
        include_str!("data/native-cell-gaps/20260813h017670004-reviewed136.json"),
    );
    let target = "WinGen,L.L.C.\n6834　Old Lock‑hart Hwy, Mus‑tang Ridge, Tex‑as 78610,USA";
    assert_eq!(cells(&a).iter().filter(|&&s| s == target).count(), 2);
}

#[test]
fn original_symbol_and_alphanumeric_gaps_preserve_all_other_cell_text() {
    let a = analyze(
        include_bytes!("data/native-cell-gaps/20261013h018060004.pdf"),
        include_str!("data/native-cell-gaps/20261013h018060004-reviewed136.json"),
    );
    let c = cells(&a);
    assert!(
        c.contains(&"BARBERET & BLANC, S.A. Camino Viejo 205, 30890 Puerto Lumbreras, Murcia, Spain")
    );
    assert!(
        c.contains(&"Hyfryd Plants\nCefn Hyfryd, Kerry, Newtown, Powys, SY16 4DX, United King‑dom")
    );
}

#[test]
fn original_font_size_change_is_not_a_word_separator() {
    let a = analyze(
        include_bytes!("data/native-cell-gaps/same-baseline-size-change.pdf"),
        include_str!("data/native-cell-gaps/same-baseline-size-change-native135.json"),
    );
    assert_eq!(a.text, "First　Second\nab　value");
    assert!(cells(&a).contains(&"ab"));
}

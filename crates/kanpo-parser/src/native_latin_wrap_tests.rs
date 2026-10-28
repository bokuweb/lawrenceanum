use super::*;

struct F {
    units: Vec<Unit>,
    words: Vec<Word>,
    bounds: Vec<NativeBounds>,
    axes: Vec<NativeAxis>,
}
// Each cell is a list of (text, line) Words; a cell's text is its Words joined
// without separators, as legacy cell flows join physical lines.
fn fixture(cells: &[&[(&str, usize)]], axis: NativeAxis) -> F {
    let (mut words, mut bounds, mut table_cells) = (vec![], vec![], vec![]);
    for (col, cell) in cells.iter().enumerate() {
        let mut ids = vec![];
        let mut at = [0usize; 8];
        for &(text, line) in cell.iter() {
            let id = words.len();
            let (start, len) = (at[line] as f32, text.chars().count() as f32 * 4.);
            at[line] += len as usize + 2;
            let base = col as f32 * 200.;
            // Tight leading: adjacent lines overlap their glyph boxes slightly.
            let b = match axis {
                NativeAxis::Right => [base + start, line as f32 * 7.8, base + start + len, line as f32 * 7.8 + 8.],
                _ => [base + 100. - line as f32 * 7.8, start, base + 108. - line as f32 * 7.8, start + len],
            };
            words.push(Word { id, x0: b[0], y0: b[1], x1: b[2], y1: b[3], source_box: b, raw: text.into(), text: text.into(), status: WordStatus::Used });
            bounds.push(NativeBounds::Glyphs(b));
            ids.push(id);
        }
        let text = ids.iter().map(|&i| words[i].text.as_str()).collect::<String>();
        table_cells.push(TableCell { row: 0, col, rowspan: 1, colspan: 1, x0: col as f32 * 200., y0: 0., x1: col as f32 * 200. + 190., y1: 100., text, words: ids });
    }
    let table = Table { x0: 0., y0: 0., x1: 400., y1: 100., rows: 1, cols: cells.len(), cells: table_cells };
    let line = Line { text: table.render(), words: vec![], table: Some(table) };
    let axes = vec![axis; words.len()];
    F { units: vec![Unit::Plain(vec![line])], words, bounds, axes }
}
fn run(mut f: F) -> Vec<String> {
    restore(&mut f.units, &f.words, &f.bounds, &f.axes);
    let Unit::Plain(lines) = &f.units[0] else { unreachable!() };
    let t = lines[0].table.as_ref().unwrap();
    assert_eq!(lines[0].text, t.render());
    t.cells.iter().map(|c| c.text.clone()).collect()
}
const DIVIDED: &[(&str, usize)] = &[("Diffu‑", 0), ("sion", 1)];
#[test]
fn a_table_that_hyphenates_divided_words_restores_its_unhyphenated_word_wraps() {
    for axis in [NativeAxis::Right, NativeAxis::Down] {
        let cells = run(fixture(&[DIVIDED, &[("Hensbroek,", 0), ("The", 0), ("Netherlands", 1)]], axis));
        assert_eq!(cells, ["Diffu‑sion", "Hensbroek,The Netherlands"], "{axis:?}");
        let cells = run(fixture(&[DIVIDED, &[("S.A.", 0), ("Camino", 1)]], axis));
        assert_eq!(cells[1], "S.A. Camino", "{axis:?}");
    }
}
#[test]
fn without_an_original_divided_word_a_wrap_is_not_a_word_boundary() {
    let cells = run(fixture(&[&[("CAUTION", 0), ("FLA", 0), ("MMABLE", 1)]], NativeAxis::Right));
    assert_eq!(cells, ["CAUTIONFLAMMABLE"]);
    // A compound's own hyphen is not evidence of word division.
    let cells = run(fixture(&[&[("Marie-", 0), ("France", 1)], &[("The", 0), ("Netherlands", 1)]], NativeAxis::Right));
    assert_eq!(cells, ["Marie-France", "TheNetherlands"]);
}
#[test]
fn words_on_one_physical_line_or_other_scripts_are_unchanged() {
    let cells = run(fixture(&[DIVIDED, &[("Mount", 0), ("Maunganui", 0), ("東京", 1), ("2090", 2), ("South", 2)]], NativeAxis::Right));
    assert_eq!(cells[1], "MountMaunganui東京2090South");
}
#[test]
fn foreign_or_duplicated_source_words_keep_the_old_cell() {
    let mut f = fixture(&[DIVIDED, &[("The", 0), ("Netherlands", 1)]], NativeAxis::Right);
    let Unit::Plain(lines) = &mut f.units[0] else { unreachable!() };
    lines.push(Line::text("The".into(), vec![2]));
    assert_eq!(run(f)[1], "TheNetherlands");
    let mut f = fixture(&[DIVIDED, &[("The", 0), ("Netherlands", 1)]], NativeAxis::Right);
    f.bounds[3] = NativeBounds::Unknown;
    assert_eq!(run(f)[1], "TheNetherlands");
    let mut f = fixture(&[DIVIDED, &[("The", 0), ("Netherlands", 1)]], NativeAxis::Right);
    let Unit::Plain(lines) = &mut f.units[0] else { unreachable!() };
    let t = lines[0].table.as_mut().unwrap();
    t.cells[1].text = "TheNederland".into();
    lines[0].text = t.render();
    assert_eq!(run(f)[1], "TheNederland");
}

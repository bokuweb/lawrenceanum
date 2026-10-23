const ALL_DASH: &[u8]=include_bytes!("archive/pages/20261016g00228/20261016g002280483.pdf");
const FEW_NUMBERS: &[u8]=include_bytes!("archive/pages/20261016g00228/20261016g002280446.pdf");

fn check_once(out:&kanpo_parser::Analysis,expected:usize) {
    let p=&out.pages[0];
    let restored:Vec<_>=p.words.iter().filter(|w|w.raw=="\u{E61C}"&&w.text=="－").collect();
    assert_eq!(restored.len(),expected);
    let mut seen=std::collections::HashSet::new();
    for id in p.regions.iter().flat_map(|r|&r.lines)
        .flat_map(|l|l.words.iter().chain(l.table.iter().flat_map(|t|t.cells.iter().flat_map(|c|&c.words)))) {
        assert!(seen.insert(id),"語 {id} を複数回使用");
    }
    for w in restored {assert_eq!(w.status,kanpo_parser::trace::WordStatus::Used);assert!(seen.contains(&w.id));}
    for l in p.regions.iter().flat_map(|r|&r.lines).filter(|l|l.table.is_some()) {
        assert_eq!(l.text,l.table.as_ref().unwrap().render());
    }
}

#[test]
fn three_all_dash_rows_restore_their_original_three_columns() {
    let out=kanpo_parser::analyze_item(&kanpo_parser::page_inputs(ALL_DASH).unwrap());
    check_once(&out,9);
    let t=out.pages[0].regions.iter().flat_map(|r|&r.lines).filter_map(|l|l.table.as_ref())
        .find(|t|t.rows==4&&t.cols==5&&t.y0>240.0&&t.y0<245.0).unwrap();
    for row in 1..4 {for col in 2..5 {
        let c=t.cells.iter().find(|c|c.row==row&&c.col==col).unwrap();
        assert_eq!(c.text,"－");assert_eq!(c.words.len(),1);
        let w=&out.pages[0].words[c.words[0]];
        assert!(c.x0<=w.x0&&w.x1<=c.x1&&c.y0<=w.y0&&w.y1<=c.y1);
    }}
}

#[test]
fn rows_with_one_or_no_amounts_keep_all_original_dash_columns() {
    let out=kanpo_parser::analyze_item(&kanpo_parser::page_inputs(FEW_NUMBERS).unwrap());
    check_once(&out,165);
    for amount in ["768,943,673","108,069,500","548,821,382"] {assert!(out.text.contains(amount));}
    let t=out.pages[0].regions.iter().flat_map(|r|&r.lines).filter_map(|l|l.table.as_ref())
        .find(|t|t.cols==10).unwrap();
    // 見出し・区分名の行は元から空欄で、既知字形がないので何も補わない。
    assert!(!t.cells.iter().any(|c|c.row==1&&c.col>=1&&c.text=="－"));
}

#[test]
fn an_unknown_private_glyph_is_not_a_ruled_dash() {
    let mut pages=kanpo_parser::page_inputs(ALL_DASH).unwrap();
    pages[0].xhtml=pages[0].xhtml.replace('\u{E61C}',"\u{EFFF}");
    let out=kanpo_parser::analyze_item(&pages);
    assert!(!out.pages[0].words.iter().any(|w|w.raw=="\u{EFFF}"&&w.text=="－"));
}

#[test]
fn a_missing_column_rule_does_not_collapse_two_dash_cells_to_one() {
    let mut pages=kanpo_parser::page_inputs(ALL_DASH).unwrap();
    pages[0].rules.as_mut().unwrap().vertical.retain(|r|(r.x-620.5).abs()>0.5);
    let out=kanpo_parser::analyze_item(&pages);
    assert!(!out.pages[0].words.iter().any(|w|w.raw=="\u{E61C}"&&w.x0>556.5&&w.x1<700.5&&w.text=="－"));
}

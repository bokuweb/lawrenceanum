use kanpo_parser::{analyze_item,page_inputs,Analysis,PageInput};
const TARGET:&[u8]=include_bytes!("archive/pages/20261014g00225/20261014g002250007.pdf");
const NEARBY:&[u8]=include_bytes!("archive/pages/20261014g00225/20261014g002250006.pdf");
// Canonical grouping follows the two actual fraction bars and the printed
// composite parentheses. The final multiplication belongs inside the outer denominator.
const FORMULA:&str="(100＋表面利率×残存年数)/(１＋((第17号に規定する利回り＋募入利回り格差)/100)×残存年数)";
fn compact(s:&str)->String{s.chars().filter(|c|!c.is_whitespace()).collect()}
fn inputs()->Vec<PageInput>{page_inputs(TARGET).unwrap()}
fn rejected(p:Vec<PageInput>){let a=analyze_item(&p);assert!(!compact(&a.text).contains(FORMULA));}
fn remove_bar(p:&mut[PageInput],outer:bool){
 let r=p[0].rules.as_mut().unwrap();let before=r.horizontal.len();let y=if outer{466.643}else{477.643};let left=if outer{525.0}else{551.0};
 r.horizontal.retain(|h|!((h.y-y).abs()<0.03&&(h.x0-left).abs()<0.03));assert_eq!(r.horizontal.len()+1,before,"exact original fraction bar");
}
fn cells(a:&Analysis)->impl Iterator<Item=&kanpo_parser::trace::TableCell>{a.pages.iter().flat_map(|p|&p.regions).flat_map(|r|&r.lines).filter_map(|l|l.table.as_ref()).flat_map(|t|&t.cells)}
#[test]
fn real_nested_fraction_keeps_both_divisions_and_the_denominator_multiplication(){
 let a=analyze_item(&inputs());let got:Vec<_>=cells(&a).filter(|c|compact(&c.text).contains(FORMULA)).collect();assert_eq!(got.len(),1,"{}",a.text);
 assert!(got[0].text.starts_with("発行対象国債ごとに、額面金額100円につき、次の算式により算出した金額"));
 assert_eq!(got[0].words,vec![80,101,47,70,78,36,44],"all existing source word IDs and order retained");
 assert!(a.text.contains("12　利率"));assert!(a.text.contains("13　経過利子の払込み"));
 for l in a.pages.iter().flat_map(|p|&p.regions).flat_map(|r|&r.lines).filter(|l|l.table.is_some()){assert_eq!(l.text,l.table.as_ref().unwrap().render());}
}
#[test]
fn neighboring_real_nested_price_is_the_same_geometric_expression(){
 let a=analyze_item(&page_inputs(NEARBY).unwrap());assert_eq!(compact(&a.text).matches(FORMULA).count(),1,"{}",a.text);
 assert!(a.text.contains("額面金額の総額×(3.8/100)×(10/365)"));assert!(a.text.contains("額面金額×(3.8/100)×(１/２)"));
}
#[test]
fn all_original_formula_atoms_parenthesis_components_and_current_owners_are_witnessed(){
 let a=analyze_item(&inputs());let p=&a.pages[0];
 for (id,raw,text) in [(47,"100＋表面利率×残存年数","100＋表面利率×残存年数"),(70,"第17号に規定する利回り＋募入利回り格差","第17号に規定する利回り＋募入利回り格差"),(78,"１＋","１＋"),(44,"100","100"),(36,"×残存年数","×残存年数"),(72,"\u{eae9}\u{eaea}",""),(37,"\u{eaeb}\u{eaec}","")]{
  assert_eq!(p.words[id].id,id);assert_eq!(p.words[id].raw,raw);assert_eq!(p.words[id].text,text);
 }
 let c=cells(&a).find(|c|c.text.starts_with("発行対象国債ごとに")).unwrap();assert_eq!(c.words,vec![80,101,47,70,78,36,44]);
}
#[test]
fn ordinary_numeric_cell_fractions_keep_the_complete_previous_output(){
 let a=analyze_item(&page_inputs(include_bytes!("archive/pages/20261014g00225/20261014g002250002.pdf")).unwrap());
 assert_eq!(a.text,include_str!("fixtures/nested_fraction/20261014g002250002-after35.txt"));
}
#[test]
fn wrapped_numeric_cells_survive_the_separate_parallel_fraction_repair(){
 let a=analyze_item(&page_inputs(include_bytes!("archive/pages/20261014g00225/20261014g002250011.pdf")).unwrap());
 assert_eq!(a.text,include_str!("fixtures/nested_fraction/20261014g002250011-after38.txt"));
}
#[test]
fn two_side_by_side_fraction_bars_are_not_a_nested_tree(){
 // The separately certified parallel pass now restores both independent fractions.
 // Keep the whole output and reject treating either bar as a nested denominator.
 let a=analyze_item(&page_inputs(include_bytes!("archive/pages/20261014g00225/20261014g002250008.pdf")).unwrap());
 assert_eq!(a.text,include_str!("fixtures/nested_fraction/20261014g002250008-after38.txt"));
 assert!(!compact(&a.text).contains(FORMULA));
}
#[test]
fn without_the_outer_bar_the_inner_fraction_does_not_imply_a_parent(){let mut p=inputs();remove_bar(&mut p,true);rejected(p)}
#[test]
fn without_the_inner_bar_no_nested_division_is_invented(){let mut p=inputs();remove_bar(&mut p,false);rejected(p)}
#[test]
fn the_child_bar_cannot_extend_beyond_the_outer_bar(){
 let mut p=inputs();let h=p[0].rules.as_mut().unwrap().horizontal.iter_mut().find(|h|(h.y-477.643).abs()<0.03&&(h.x0-551.).abs()<0.03).unwrap();h.x0=521.;h.x1=761.;rejected(p)
}
#[test]
fn the_child_fraction_must_lie_below_the_outer_bar(){
 let mut p=inputs();let h=p[0].rules.as_mut().unwrap().horizontal.iter_mut().find(|h|(h.y-477.643).abs()<0.03&&(h.x0-551.).abs()<0.03).unwrap();h.y=455.;rejected(p)
}
#[test]
fn missing_native_left_parenthesis_is_not_guessed(){let mut p=inputs();assert!(p[0].xhtml.contains("\u{eae9}\u{eaea}"));p[0].xhtml=p[0].xhtml.replace("\u{eae9}\u{eaea}","");rejected(p)}
#[test]
fn missing_native_right_parenthesis_is_not_guessed(){let mut p=inputs();assert!(p[0].xhtml.contains("\u{eaeb}\u{eaec}"));p[0].xhtml=p[0].xhtml.replace("\u{eaeb}\u{eaec}","");rejected(p)}
#[test]
fn an_unknown_native_parenthesis_component_is_not_mapped_by_position_alone(){let mut p=inputs();assert!(p[0].xhtml.contains('\u{eae9}'));p[0].xhtml=p[0].xhtml.replace('\u{eae9}',"\u{e0aa}");rejected(p)}

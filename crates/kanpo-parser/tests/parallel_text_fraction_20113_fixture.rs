use kanpo_parser::{analyze_item,page_inputs};
#[test] fn additional_parallel_20113_keeps_all_other_numeric_wrapped_cells(){
 let a=analyze_item(&page_inputs(include_bytes!("archive/pages/20260909g00201/20260909g002010013.pdf")).unwrap());
 let cells:Vec<_>=a.pages.iter().flat_map(|p|&p.regions).flat_map(|r|&r.lines).filter_map(|l|l.table.as_ref()).flat_map(|t|&t.cells).collect();
 let target:Vec<_>=cells.iter().filter(|c|c.text.chars().filter(|c|!c.is_whitespace()).collect::<String>().contains("額面金額×(第10号に規定する第２期以後の利子の適用利率/100)×(１/２)")).collect();
 assert_eq!(target.len(),1);assert_eq!((target[0].row,target[0].col),(0,1));assert_eq!(target[0].words,vec![127,128,102,71,129,73,89,72]);
 let other:Vec<_>=cells.iter().filter(|c|c.words!=vec![127,128,102,71,129,73,89,72]).map(|c|c.text.as_str()).collect();
 let expected:Vec<String>=serde_json::from_str(include_str!("expected/parallel_text_fraction/20260909g002010013-other-cells37.json")).unwrap();assert_eq!(other,expected);
let lines:Vec<_>=a.pages.iter().flat_map(|p|&p.regions).flat_map(|r|&r.lines).filter(|l|l.table.is_none()).map(|l|(l.text.clone(),l.words.clone())).collect();
 let expected:Vec<(String,Vec<usize>)>=serde_json::from_str(include_str!("expected/numeric_compound_fraction/20260909g002010013-plain-lines.json")).unwrap();assert_eq!(lines,expected);
}
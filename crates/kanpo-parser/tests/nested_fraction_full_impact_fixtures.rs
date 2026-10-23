use kanpo_parser::{page_inputs,analyze_item};
const FORMULA:&str="(100＋表面利率×残存年数)/(１＋((第17号に規定する利回り＋募入利回り格差)/100)×残存年数)";
fn verify(pdf:&[u8],row:usize,expected_ids:&[usize]){let a=analyze_item(&page_inputs(pdf).unwrap());let cells:Vec<_>=a.pages.iter().flat_map(|p|&p.regions).flat_map(|r|&r.lines).filter_map(|l|l.table.as_ref()).flat_map(|t|&t.cells).filter(|c|c.text.contains(FORMULA)).collect();assert_eq!(cells.len(),1,"{}",a.text);assert_eq!(cells[0].row,row);assert_eq!(cells[0].col,1);assert_eq!(cells[0].words,expected_ids);assert!(cells[0].text.starts_with("発行対象国債ごとに、額面金額100円につき、次の算式により算出した金額"));}
#[test] fn actual_nested_20260812g001800016(){verify(include_bytes!("archive/pages/20260812g00180/20260812g001800016.pdf"),7,&[93, 110, 58, 82, 91, 39, 55]);}
#[test] fn actual_nested_20260812g001800017(){verify(include_bytes!("archive/pages/20260812g00180/20260812g001800017.pdf"),11,&[29, 57, 12, 20, 58, 3, 9]);}
#[test] fn actual_nested_20260909g002010008(){verify(include_bytes!("archive/pages/20260909g00201/20260909g002010008.pdf"),4,&[116, 129, 78, 107, 115, 58, 77]);}
#[test] fn actual_nested_20260909g002010009(){verify(include_bytes!("archive/pages/20260909g00201/20260909g002010009.pdf"),5,&[81, 95, 50, 71, 80, 32, 47]);}

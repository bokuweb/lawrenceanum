use super::*;
fn w(id:usize, raw:&str,text:&str)->Word { Word{id,raw:raw.into(),text:text.into(),x0:0.,y0:0.,x1:80.,y1:8.,status:WordStatus::Used} }
fn run(text:&str, ws:&mut[Word])->Vec<Unit> {let mut u=vec![Unit::Plain(vec![Line::text(text.into(),ws.iter().map(|w|w.id).collect())])];restore(&mut u,ws);u}
fn text(u:&[Unit])->&str {&all_lines(u)[0].text}
#[test] fn numeric_suffix_is_inserted_without_rebuilding_repaired_line(){
 let mut ws=vec![w(0,"この液40\u{E24F}Lを", "この液40Lを")];let before=ws[0].clone();
 let mut u=run("既修復ⅲ 1/2 システム機器更新費用 この液40Lを ㎡", &mut ws);
 assert_eq!(text(&u),"既修復ⅲ 1/2 システム機器更新費用 この液40μLを ㎡");
 assert_eq!(ws[0].raw,before.raw);assert_eq!((ws[0].x0,ws[0].y0,ws[0].x1,ws[0].y1),(before.x0,before.y0,before.x1,before.y1));assert_eq!(all_lines(&u)[0].words,vec![0]);
 let previous=serde_json::to_string(&(all_lines(&u),&ws)).unwrap();restore(&mut u,&mut ws);assert_eq!(serde_json::to_string(&(all_lines(&u),&ws)).unwrap(),previous);
}
#[test] fn decimal_and_two_separate_numeric_suffixes(){let mut ws=vec![w(0,"0.05\u{E24F}Lと50\u{E24F}L", "0.05Lと50L")];assert_eq!(text(&run("0.05Lと50L",&mut ws)),"0.05μLと50μL");}
#[test] fn non_volume_and_ambiguous_or_unknown_contexts_stay_exact(){
 for raw in ["\u{E24F}L","A\u{E24F}L","40\u{E24F}g","40\u{E24F}Log",".40\u{E24F}L","40.\u{E24F}L","1.2.3\u{E24F}L","A40\u{E24F}L","40\u{E24F}L\u{E952}","40\u{E24F}L\u{F4E00}","40\u{E24F}L\u{100123}"] {
  let t=crate::normalize::word_text(raw);let mut ws=vec![w(0,raw,&t)];assert_eq!(text(&run(&t,&mut ws)),t,"{raw}");
 }
}
#[test] fn repaired_or_empty_or_nonused_word_is_untouched(){for (t,status) in [("40L extra",WordStatus::Used),("",WordStatus::Empty),("40L",WordStatus::Empty)] {let mut ws=vec![w(0,"40\u{E24F}L",t)];ws[0].status=status;let before=serde_json::to_string(&ws).unwrap();assert_eq!(text(&run(t,&mut ws)),t);assert_eq!(serde_json::to_string(&ws).unwrap(),before);}}
#[test] fn repeated_substring_or_duplicate_owner_is_rejected(){let mut ws=vec![w(0,"40\u{E24F}L","40L")];assert_eq!(text(&run("40L 40L",&mut ws)),"40L 40L");let mut u=vec![Unit::Plain(vec![Line::text("40L".into(),vec![0]),Line::text("40L".into(),vec![0])])];let before=serde_json::to_string(&all_lines(&u)).unwrap();restore(&mut u,&mut ws);assert_eq!(serde_json::to_string(&all_lines(&u)).unwrap(),before);}
#[test] fn table_cell_owner_is_local_and_amounts_stay_exact(){use crate::trace::{Table,TableCell};let t=Table{x0:0.,y0:0.,x1:100.,y1:10.,rows:1,cols:2,cells:vec![TableCell{row:0,col:0,rowspan:1,colspan:1,x0:0.,y0:0.,x1:80.,y1:10.,text:"40L".into(),words:vec![0]},TableCell{row:0,col:1,rowspan:1,colspan:1,x0:80.,y0:0.,x1:100.,y1:10.,text:"2,000".into(),words:vec![1]}]};let mut u=vec![Unit::Plain(vec![Line{text:t.render(),words:vec![],table:Some(t)}])];let mut ws=vec![w(0,"40\u{E24F}L","40L"),w(1,"2,000","2,000")];restore(&mut u,&mut ws);let c=&all_lines(&u)[0].table.as_ref().unwrap().cells;assert_eq!(c[0].text,"40μL");assert_eq!(c[1].text,"2,000");assert_eq!(c[0].words,vec![0]);}

#[test] fn verified_numeric_micrometer_suffix_uses_the_same_local_owner(){let mut ws=vec![w(0,"3\u{E24F}mのDOP","3mのDOP")];assert_eq!(text(&run("0.3mのDOP",&mut ws)),"0.3μmのDOP");}
#[test] fn unverified_unit_letters_and_ascii_unit_continuations_stay_exact(){for raw in ["3\u{E24F}g","3\u{E24F}s","3\u{E24F}ms","3\u{E24F}meter","3\u{E24F}Litre"]{let t=crate::normalize::word_text(raw);let mut ws=vec![w(0,raw,&t)];assert_eq!(text(&run(&t,&mut ws)),t);}}

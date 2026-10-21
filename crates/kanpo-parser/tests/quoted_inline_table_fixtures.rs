use kanpo_parser::{Analysis, model::Block};
use serde_json::Value;
use std::collections::{BTreeMap,BTreeSet};
const PDF:&[u8]=include_bytes!("archive/pages/20260717g00160/20260717g001600048.pdf");
const OLD:&str=include_str!("expected/quoted_inline_tables/20260717g001600048-before46.analysis.json");
fn actual()->Analysis{kanpo_parser::analyze_item(&kanpo_parser::page_inputs(PDF).unwrap())}
fn value(a:&Analysis)->Value{serde_json::from_str(&serde_json::to_string(a).unwrap()).unwrap()}
#[test]fn adjacent_before_page_all_analysis_exact(){let p=include_bytes!("archive/pages/20260717g00160/20260717g001600047.pdf");let a=kanpo_parser::analyze_item(&kanpo_parser::page_inputs(p).unwrap());assert_eq!(value(&a),serde_json::from_str::<Value>(include_str!("expected/quoted_inline_tables/20260717g001600047-before46.analysis.json")).unwrap());}
#[test]fn adjacent_after_page_all_analysis_exact(){let p=include_bytes!("archive/pages/20260717g00160/20260717g001600049.pdf");let a=kanpo_parser::analyze_item(&kanpo_parser::page_inputs(p).unwrap());assert_eq!(value(&a),serde_json::from_str::<Value>(include_str!("expected/quoted_inline_tables/20260717g001600049-before46.analysis.json")).unwrap());}
#[test]fn all_quoted_tables_follow_ordinary_prose_and_brackets_in_original_id_order(){
 let a=actual();let p=&a.pages[0];
 let ids:Vec<_>=p.regions.iter().flat_map(|r|&r.lines).flat_map(|l|{
  let mut ids=l.words.clone();if let Some(t)=&l.table{for c in&t.cells{ids.extend(&c.words)}}ids
 }).filter(|id|[22,17,20,21,26,29,23,19,24,31,32,35,33].contains(id)).collect();
 assert_eq!(ids,[22,17,20,21,26,29,23,19,24,31,32,35,33]);
}
#[test]fn first_replacement_cell_joins_both_source_columns_and_doc_quotes_are_outside(){
 let a=actual();let doc=&a.articles[0].document;
 let Block::Paragraph{runs}=&doc.blocks[0]else{panic!()};
 let ts:Vec<_>=runs.iter().enumerate().filter(|(_,r)|r.table.is_some()).collect();
 assert_eq!(ts.len(),3);
 assert_eq!(ts[0].1.table.as_ref().unwrap().rows[0][0][0].text,"及びデジタル庁");
 assert_eq!(ts[1].1.table.as_ref().unwrap().rows[0][0][0].text,"及び防災庁");
 assert_eq!(ts[1].1.table.as_ref().unwrap().rows[0][1][0].text,"、防災庁及び復興庁");
 assert!(runs[ts[0].0-1].text.ends_with("「、防災庁及び復興庁の事務」に、「"));
 assert_eq!(runs[ts[0].0+1].text,"」を「");
 assert!(runs[ts[1].0+1].text.starts_with("」に改め、"));
}
#[test]fn all_original_words_and_single_output_ownership_are_preserved(){
 let a=actual();let v=value(&a);let b:Value=serde_json::from_str(OLD).unwrap();assert_eq!(v["pages"][0]["words"],b["pages"][0]["words"]);
 let mut counts=BTreeMap::new();for reg in&a.pages[0].regions{for l in&reg.lines{
  for&id in&l.words{*counts.entry(id).or_insert(0)+=1;}
  if let Some(t)=&l.table{for c in&t.cells{for&id in&c.words{*counts.entry(id).or_insert(0)+=1;}}}
 }}
 assert!(counts.values().all(|&n|n==1));let expected:BTreeSet<_>=a.pages[0].words.iter().filter(|w|format!("{:?}",w.status)=="Used").map(|w|w.id).collect();
 assert_eq!(counts.keys().cloned().collect::<BTreeSet<_>>(),expected);
}
#[test]fn ordinary_four_column_inline_addition_and_all_outside_text_stay_exact(){
 let a=actual();let v=value(&a);let b:Value=serde_json::from_str(OLD).unwrap();
 let old=&b["articles"][0]["document"]["blocks"][0]["runs"][5]["table"];
 let ts:Vec<_>=a.pages[0].regions.iter().flat_map(|r|&r.lines).filter_map(|l|l.table.as_ref()).collect();assert_eq!(ts.len(),3);
 let doc=&v["articles"][0]["document"]["blocks"][0]["runs"];
 let new=doc.as_array().unwrap().iter().filter_map(|r|r.get("table")).last().unwrap();assert_eq!(new,old);
 let suffix=b["text"].as_str().unwrap().split("」に改め、同表武力").nth(1).unwrap();assert!(a.text.ends_with(suffix));
}

#[test]fn original_glyph_word_provenance_and_both_wrapped_fragments_are_preserved(){
 let a=actual();let v=value(&a);let p:Value=serde_json::from_str(include_str!("expected/quoted_inline_tables/160p48-source-provenance.json")).unwrap();
 let native=kanpo_pdf::extract_pages(PDF).unwrap();let ng:Value=serde_json::from_str(&serde_json::to_string(&native[0]).unwrap()).unwrap();
 let mut glyphs=BTreeSet::new();for row in p["proof"].as_array().unwrap(){
  let w=&row["Word"];let id=w["id"].as_u64().unwrap() as usize;assert_eq!(&v["pages"][0]["words"][id],w);
  for g in row["native_glyphs"].as_array().unwrap(){let id=g["id"].as_u64().unwrap() as usize;assert!(glyphs.insert(id));assert_eq!(&ng["glyphs"][id],&g["glyph"]);}
 }assert_eq!(glyphs.len(),79);
 let ts:Vec<_>=a.pages[0].regions.iter().flat_map(|r|&r.lines).filter_map(|l|l.table.as_ref()).collect();
 assert_eq!(ts[1].cells[0].words,[24,31]);assert_eq!(ts[1].cells[0].text,"及び防災庁");
 let c=&ts[1].cells[0];assert_eq!((c.x0,c.y0,c.x1,c.y1),(431.214,69.,472.5,421.));
 let old:Value=serde_json::from_str(OLD).unwrap();let ots:Vec<_>=old["pages"][0]["regions"].as_array().unwrap().iter().flat_map(|r|r["lines"].as_array().unwrap()).filter_map(|l|l.get("table")).collect();
 let nts:Vec<_>=v["pages"][0]["regions"].as_array().unwrap().iter().flat_map(|r|r["lines"].as_array().unwrap()).filter_map(|l|l.get("table")).collect();
 assert_eq!(nts[0],ots[0]);assert_eq!(nts[1]["cells"][1],ots[1]["cells"][1]);assert_eq!(nts[2],ots[2]);
}

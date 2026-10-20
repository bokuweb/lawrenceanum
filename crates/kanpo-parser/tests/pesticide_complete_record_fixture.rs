use kanpo_parser::{analyze_item, page_inputs};
use std::collections::BTreeSet;
fn compact(s:&str)->String{s.chars().filter(|c|!c.is_whitespace()).collect()}
fn article_tables(v:&serde_json::Value,out:&mut Vec<Vec<Vec<String>>>) {
 if let Some(rows)=v.get("table").and_then(|t|t.get("rows")).and_then(|r|r.as_array()) {
  out.push(rows.iter().map(|r|r.as_array().unwrap().iter().map(|c|c.as_array().unwrap().iter().map(|x|x["text"].as_str().unwrap()).collect::<String>()).collect()).collect());
  return;
 }
 if let Some(o)=v.as_object(){for x in o.values(){article_tables(x,out)}}
 if let Some(a)=v.as_array(){for x in a{article_tables(x,out)}}
}
#[test]
fn all_thirty_six_four_field_records_headers_ditto_and_314_native_source_words_are_retained(){
 let a=analyze_item(&page_inputs(include_bytes!("archive/pages/20260824h01774/20260824h017740004.pdf")).unwrap());
 let expected:serde_json::Value=serde_json::from_str(include_str!("fixtures/pesticide_records/complete_records.json")).unwrap();
 let page=&a.pages[0];let tables:Vec<_>=page.regions.iter().flat_map(|r|&r.lines).filter_map(|l|l.table.as_ref()).collect();
 assert_eq!(tables.len(),6,"six original repeated-header record groups");
 let all:Vec<_>=tables.iter().flat_map(|t|&t.cells).flat_map(|c|&c.words).copied().collect();
 assert_eq!(all.len(),314);assert_eq!(all.iter().copied().collect::<BTreeSet<_>>().len(),314);
 for w in expected["source_words"].as_array().unwrap(){
  let id=w["id"].as_u64().unwrap() as usize;let actual=&page.words[id];assert_eq!(actual.id,id);
  assert_eq!(actual.raw,w["raw"].as_str().unwrap());assert_eq!(actual.text,w["text"].as_str().unwrap());
  assert_eq!(actual.status as u8,kanpo_parser::trace::WordStatus::Used as u8);
  for (n,v) in [("x0",actual.x0),("x1",actual.x1),("y0",actual.y0),("y1",actual.y1)]{assert!((v-w[n].as_f64().unwrap() as f32).abs()<0.001,"native source coordinate changed for {id}");}
 }
 let rs=expected["records"].as_array().unwrap();assert_eq!(rs.len(),36);
 for r in rs {
  let ti=r["table"].as_u64().unwrap() as usize;let ri=r["row"].as_u64().unwrap() as usize;
  let mut cs:Vec<_>=tables[ti].cells.iter().filter(|c|c.row==ri).collect();cs.sort_by_key(|c|c.col);assert_eq!(cs.len(),4);
  for (c,f) in cs.iter().zip(r["fields"].as_array().unwrap()){
   assert_eq!(compact(&c.text),f["text"].as_str().unwrap(),"complete native field for {}",r["registration"]);
   let ids:Vec<_>=f["words"].as_array().unwrap().iter().map(|x|x.as_u64().unwrap() as usize).collect();assert_eq!(c.words,ids);
  }
 }
 for (ti,t) in tables.iter().enumerate(){
  let hdr=&expected["headers"][ti]["fields"];
  for c in t.cells.iter().filter(|c|c.row==0){assert_eq!(compact(&c.text),hdr[c.col]["compact_literal"].as_str().unwrap());}
 }
 let mut document_tables=vec![];article_tables(&serde_json::to_value(&a.articles).unwrap(),&mut document_tables);
 assert_eq!(document_tables.len(),6,"article keeps all six caption-separated original groups");
 for r in rs {
  let ti=r["table"].as_u64().unwrap() as usize;let ri=r["row"].as_u64().unwrap() as usize;
  let row=&document_tables[ti][ri];assert_eq!(row.len(),4);
  for (cell,f) in row.iter().zip(r["fields"].as_array().unwrap()){assert_eq!(compact(cell),f["text"].as_str().unwrap(),"article field binding for {}",r["registration"]);}
 }
 let ditto=tables.iter().flat_map(|t|&t.cells).filter(|c|c.text=="〃").count();assert_eq!(ditto,22,"literal ditto, never inferred expansion");
 for cap in expected["captions"].as_array().unwrap(){assert!(page.regions.iter().flat_map(|r|&r.lines).any(|l|l.table.is_none()&&l.text==cap["text"].as_str().unwrap()));}
}

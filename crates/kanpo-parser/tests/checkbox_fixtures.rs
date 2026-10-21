use kanpo_parser::{analyze_item,page_inputs};
use serde_json::Value;
const P:&str=concat!(env!("CARGO_MANIFEST_DIR"),"/tests/expected/checkbox");
fn analyze(page:&str)->kanpo_parser::Analysis{let p=std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/archive/pages").join(&page[..14]).join(format!("{page}.pdf"));analyze_item(&page_inputs(&std::fs::read(p).unwrap()).unwrap())}
fn value(a:&kanpo_parser::Analysis)->Value{serde_json::from_slice(&serde_json::to_vec(a).unwrap()).unwrap()}

#[test]
fn instructions_keep_both_printed_checkbox_squares_in_their_original_lines(){let a=analyze("20260723g001630102");assert_eq!(a.text.lines().next().unwrap(),"注１　該当する□にレ印を付けること。");assert!(a.text.lines().any(|l|l=="□にレ印を付けること。なお、価額競争実施指針の参加者の資格の有の欄に記載した場"));for id in [2,48]{assert_eq!(a.pages[0].words[id].raw,"\u{e712}");assert_eq!(a.pages[0].words[id].text,"□");assert_eq!(a.pages[0].words[id].status,kanpo_parser::trace::WordStatus::Used);}}
#[test]
fn parking_choices_keep_one_printed_checkbox_before_each_option(){let a=analyze("20260731g001720141");assert!(a.text.lines().any(|l|l=="機械式駐車場設備の有無　□有　□無"));for id in [9,11]{assert_eq!(a.pages[0].words[id].raw,"\u{e712}");assert_eq!(a.pages[0].words[id].text,"□");assert_eq!(a.pages[0].words[id].status,kanpo_parser::trace::WordStatus::Used);}}
#[test]
fn neighboring_pages_keep_every_line_article_document_and_source_id(){for p in ["20260723g001630103","20260730g001710013","20260730g001710015","20260731g001720145"]{let old:Value=serde_json::from_slice(&std::fs::read(format!("{P}/{p}-baseline44.json")).unwrap()).unwrap();assert_eq!(value(&analyze(p)),old,"neighbor {p}");}}

#[test]
fn a_vertical_mixed_word_keeps_its_original_used_owner_and_square_suffix(){let a=analyze("20260730g001710014");let w=&a.pages[0].words[132];assert_eq!(w.raw,"表示は、次の様式に記号\u{E712}");assert_eq!(w.text,"表示は、次の様式に記号□");assert_eq!(w.status,kanpo_parser::trace::WordStatus::Used);assert!(a.text.contains("表示は、次の様式に記号□"));let b:Value=serde_json::from_slice(&std::fs::read(format!("{P}/20260730g001710014-baseline44.json")).unwrap()).unwrap();let v=value(&a);for k in ["id","raw","x0","y0","x1","y1","status"]{assert_eq!(v["pages"][0]["words"][132][k],b["pages"][0]["words"][132][k]);}}

#[test]
fn restored_square_sources_keep_geometry_ids_and_all_existing_owner_sequences(){
 for page in ["20260723g001630102","20260731g001720141","20260730g001710014"] {
  let a=value(&analyze(page));let b:Value=serde_json::from_slice(&std::fs::read(format!("{P}/{page}-baseline44.json")).unwrap()).unwrap();
  let old=&b["pages"][0];let new=&a["pages"][0];let mut restored=std::collections::BTreeSet::new();
  for (bw,nw) in old["words"].as_array().unwrap().iter().zip(new["words"].as_array().unwrap()) {
   for key in ["id","raw","x0","y0","x1","y1"]{assert_eq!(bw[key],nw[key]);}
   if bw["raw"].as_str().unwrap().contains('\u{E712}') {assert_eq!(nw["text"].as_str().unwrap().replace('□',""),bw["text"]);assert_eq!(nw["status"],"used");if bw["status"]=="empty"{restored.insert(bw["id"].as_u64().unwrap());}}
   else {assert_eq!(bw,nw);}
  }
  let owner_ids=|p:&Value|p["regions"].as_array().unwrap().iter().flat_map(|r|r["lines"].as_array().unwrap()).flat_map(|l|std::iter::once(l["words"].as_array().unwrap().clone()).chain(l.get("table").and_then(|t|t["cells"].as_array()).into_iter().flatten().map(|c|c["words"].as_array().unwrap().clone()))).collect::<Vec<_>>();
  let oldowners=owner_ids(old);let newowners=owner_ids(new);assert_eq!(oldowners.len(),newowners.len());
  for (bo,no) in oldowners.iter().zip(&newowners){assert_eq!(bo,&no.iter().filter(|i|!restored.contains(&i.as_u64().unwrap())).cloned().collect::<Vec<_>>());}
  for id in restored{assert_eq!(newowners.iter().flatten().filter(|v|v.as_u64()==Some(id)).count(),1);}
 }
}

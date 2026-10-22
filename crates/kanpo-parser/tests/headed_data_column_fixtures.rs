#[path = "support/legacy_analysis.rs"]
mod legacy_analysis;
use kanpo_parser::{Analysis,trace::Table};
const PDF:&[u8]=include_bytes!("archive/pages/20261020g00230/20261020g002300011.pdf");
const BASE:&str=include_str!("expected/headed_data_columns/20261020g002300011.json");
const EXPECT:&str=include_str!("expected/headed_data_columns/expected-17-records.json");
fn actual(p:&[u8])->Analysis{kanpo_parser::analyze_item(&kanpo_parser::page_inputs(p).unwrap())}
fn value(a:&Analysis)->serde_json::Value{legacy_analysis::value(a)}
fn expected()->Vec<Vec<String>>{serde_json::from_str(EXPECT).unwrap()}
fn find_table<'a>(a:&'a Analysis,code:&str)->Option<&'a Table>{a.pages.iter().flat_map(|p|&p.regions).flat_map(|r|&r.lines).filter_map(|l|l.table.as_ref()).find(|t|t.cells.iter().any(|c|c.col==0&&c.text==code))}
#[test]fn all_seventeen_original_records_have_five_exact_fields(){
 let a=actual(PDF);let t=find_table(&a,"生第110365号").unwrap();assert_eq!((t.rows,t.cols),(18,5));
 let rows=expected();for(r,fields)in rows.iter().enumerate(){for(c,text)in fields.iter().enumerate(){let cells:Vec<_>=t.cells.iter().filter(|x|x.row==r+1&&x.col==c).collect();assert_eq!(cells.len(),1);assert_eq!(cells[0].text,*text,"row{r}col{c}");assert_eq!((cells[0].rowspan,cells[0].colspan),(1,1));assert!(!cells[0].words.is_empty());}}
}
#[test]fn document_and_text_keep_every_record_together(){
 let a=actual(PDF);let v=value(&a);let e=expected();for row in &e{let rendered=row.join("　");assert!(a.text.contains(&rendered),"missing complete original record {}",row[0]);}
 fn scan<'a>(v:&'a serde_json::Value,out:&mut Vec<&'a serde_json::Value>){match v{serde_json::Value::Object(o)=>{if o.contains_key("rows")&&o.get("rows").unwrap().as_array().is_some_and(|r|r.get(1).is_some_and(|r|r[0][0]["text"]=="生第110365号")){out.push(v);}for x in o.values(){scan(x,out);}},serde_json::Value::Array(a)=>for x in a{scan(x,out)},_=>()}}
 let mut ts=vec![];scan(&v["articles"][0]["document"],&mut ts);assert_eq!(ts.len(),1);let rows=ts[0]["rows"].as_array().unwrap();assert_eq!(rows.len(),24);
 for(r,fields)in e.iter().enumerate(){assert_eq!(rows[r+1].as_array().unwrap().len(),5);for(c,text)in fields.iter().enumerate(){let got=rows[r+1][c].as_array().unwrap().iter().map(|r|r["text"].as_str().unwrap()).collect::<String>();assert_eq!(got,*text);}}
 let right:Vec<Vec<String>>=serde_json::from_str(include_str!("expected/headed_data_columns/expected-right-six-records.json")).unwrap();
 for(r,fields)in right.iter().enumerate(){assert_eq!(rows[r+18].as_array().unwrap().len(),5);for(c,text)in fields.iter().enumerate(){let got=rows[r+18][c].as_array().unwrap().iter().map(|r|r["text"].as_str().unwrap()).collect::<String>();assert_eq!(got,*text);}}

}
#[test]fn normal_three_years_three_records_right_six_and_jis_article_are_unchanged(){
 let a=actual(PDF);let v=value(&a);let b:serde_json::Value=serde_json::from_str(BASE).unwrap();
 let btables:Vec<_>=b["pages"][0]["regions"][0]["lines"].as_array().unwrap().iter().filter_map(|l|l.get("table")).collect();
 for idx in [0,2]{let t=btables[idx];assert!(v["pages"][0]["regions"][0]["lines"].as_array().unwrap().iter().any(|l|l.get("table")==Some(t)));}
 assert_eq!(v["articles"][1],b["articles"][1]);assert_eq!(a.articles.len(),2);
}
#[test]fn all_original_words_and_existing_unique_owner_counts_are_preserved(){
 let a=actual(PDF);let v=value(&a);let b:serde_json::Value=serde_json::from_str(BASE).unwrap();assert_eq!(v["pages"][0]["words"],b["pages"][0]["words"]);
 let owners=|x:&serde_json::Value|{let mut ids=std::collections::BTreeMap::new();for l in x["pages"][0]["regions"].as_array().unwrap().iter().flat_map(|r|r["lines"].as_array().unwrap()){if let Some(t)=l.get("table"){assert!(l["words"].as_array().unwrap().is_empty());for i in t["cells"].as_array().unwrap().iter().flat_map(|c|c["words"].as_array().unwrap()){*ids.entry(i.as_u64().unwrap()).or_insert(0usize)+=1;}}else{for i in l["words"].as_array().unwrap(){*ids.entry(i.as_u64().unwrap()).or_insert(0usize)+=1;}}}ids};let old=owners(&b);let new=owners(&v);assert_eq!(old,new);assert!(new.values().all(|&v|v==1));
 let mut oldchars:Vec<_>=b["text"].as_str().unwrap().chars().filter(|c|!c.is_whitespace()).collect();let mut newchars:Vec<_>=a.text.chars().filter(|c|!c.is_whitespace()).collect();oldchars.sort();newchars.sort();assert_eq!(oldchars,newchars);
}
#[test]fn preceding_page_complete_analysis_control(){let a=actual(include_bytes!("archive/pages/20261020g00230/20261020g002300010.pdf"));let b:serde_json::Value=serde_json::from_str(include_str!("expected/headed_data_columns/20261020g002300010.json")).unwrap();assert_eq!(value(&a),b);}
#[test]fn following_jis_page_complete_analysis_control(){let a=actual(include_bytes!("archive/pages/20261020g00230/20261020g002300012.pdf"));let b:serde_json::Value=serde_json::from_str(include_str!("expected/headed_data_columns/20261020g002300012.json")).unwrap();assert_eq!(value(&a),b);}

#[test]fn short_foreign_registration_is_a_different_pending_key_recognition_issue(){
 let pdf=include_bytes!("archive/pages/20260729g00169/20260729g001690181.pdf");
 let a=actual(pdf);let old:serde_json::Value=serde_json::from_str(include_str!("expected/headed_data_columns/20260729g001690181-after40.json")).unwrap();
 assert_eq!(value(&a),old); // Known remaining 外第３号 Plain record is not claimed fixed.
}
#[test]fn every_repaired_cell_binds_its_original_words_and_nine_hundred_fifty_glyphs(){
 let a=actual(PDF);let t=find_table(&a,"生第110365号").unwrap();
 let proof:serde_json::Value=serde_json::from_str(include_str!("expected/headed_data_columns/target-native-cell-proof.json")).unwrap();
 let native=kanpo_pdf::extract_pages(PDF).unwrap();let native_words=kanpo_pdf::bbox::words(&native[0]);
 let mut seen=std::collections::BTreeSet::new();let mut word_count=0;
 for e in proof["cells"].as_array().unwrap(){
  let c=t.cells.iter().find(|c|c.row==e["row"].as_u64().unwrap() as usize&&c.col==e["col"].as_u64().unwrap() as usize).unwrap();
  let ids=e["words"].as_array().unwrap().iter().map(|w|w["id"].as_u64().unwrap() as usize).collect::<Vec<_>>();assert_eq!(c.words,ids);
  for id in ids{let w=&a.pages[0].words[id];let box0=[w.x0,w.y0,w.x1,w.y1];
   let matches=native_words.iter().filter(|n|n.text==w.raw&&[n.x_min,n.y_min,n.x_max,n.y_max].iter().zip(box0).all(|(x,y)|(x-y as f64).abs()<0.0002)).collect::<Vec<_>>();assert_eq!(matches.len(),1);
   assert!(w.x0>=c.x0-0.01&&w.x1<=c.x1+0.01&&w.y0>=c.y0-0.01&&w.y1<=c.y1+0.01);
   assert_eq!(w.status,kanpo_parser::trace::WordStatus::Used);
   for gid in &matches[0].glyphs{assert!(seen.insert(*gid));}word_count+=1;
  }
 }
 assert_eq!(word_count,132);assert_eq!(seen.len(),950);
}

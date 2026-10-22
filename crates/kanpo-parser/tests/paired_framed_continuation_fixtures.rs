#[path = "support/legacy_analysis.rs"]
mod legacy_analysis;
use kanpo_parser::{Analysis,trace::{Role,WordStatus}};
fn actual(p:&[u8])->Analysis{kanpo_parser::analyze_item(&kanpo_parser::page_inputs(p).unwrap())}
fn value(a:&Analysis)->serde_json::Value{legacy_analysis::value(a)}
const PDF:&[u8]=include_bytes!("archive/pages/20260727g00167/20260727g001670021.pdf");
const BASE:&str=include_str!("expected/paired_framed_continuation/20260727g001670021.analysis.json");
const EXPECT:&str=include_str!("expected/paired_framed_continuation/expected-bands.json");
fn expected()->serde_json::Value{serde_json::from_str(EXPECT).unwrap()}
fn lines(v:&serde_json::Value)->String{v.as_array().unwrap().iter().map(|x|x.as_str().unwrap()).collect::<Vec<_>>().join("\n")}
#[test]fn original_both_cells_continue_before_supplement_and_next_notice(){
 let a=actual(PDF);let e=expected();let p=&a.pages[0];
 for(role,key)in[(Role::After,"after"),(Role::Before,"before")]{
  let r:Vec<_>=p.regions.iter().filter(|r|r.unit==0&&r.role==role).collect();assert_eq!(r.len(),2);
  for(i,r)in r.iter().enumerate(){assert_eq!(r.part,i);assert_eq!(serde_json::to_value(r.lines.iter().map(|l|&l.text).collect::<Vec<_>>()).unwrap(),e[key][i]);assert_eq!(serde_json::to_value(r.lines.iter().flat_map(|l|&l.words).collect::<Vec<_>>()).unwrap(),e[format!("{key}IDs")][i]);}
 }
 let text=format!("{}\n改正後\n{}\n{}\n改正前\n{}\n{}\n{}\n{}",lines(&e["preamble"]),lines(&e["after"][0]),lines(&e["after"][1]),lines(&e["before"][0]),lines(&e["before"][1]),lines(&e["suffix"]),lines(&e["nextNotice"]));
 assert_eq!(a.text,text);
 assert!(a.text.contains("及び当該光ディスクに記録された内容を告\n示で定めるところにより記載した書面をも"));
}
#[test]fn old_article7_complete_before_is_one_document_cell_and_not_legal_notice(){
 let a=actual(PDF);let v=value(&a);let e=expected();let old:serde_json::Value=serde_json::from_str(BASE).unwrap();
 assert_eq!(v["articles"].as_array().unwrap().len(),3);assert_eq!(v["articles"][1]["text"],"法規的告示");assert_eq!(v["articles"][2],old["articles"][2]);
 let blocks=v["articles"][0]["document"]["blocks"].as_array().unwrap();let rows:Vec<_>=blocks.iter().filter(|b|b["kind"]=="shinkyu").flat_map(|b|b["rows"].as_array().unwrap()).collect();assert_eq!(rows.len(),1);
 for key in ["after","before"]{let got=rows[0][key].as_array().unwrap().iter().map(|r|r["text"].as_str().unwrap()).collect::<String>();let want=format!("{}{}",lines(&e[key][0]),lines(&e[key][1]));let compact=|s:String|s.chars().filter(|c|!c.is_whitespace()).collect::<String>();assert_eq!(compact(got),compact(want));}
 assert!(!v["articles"][1]["text"].as_str().unwrap().contains("Ｘ六二八三"));
}
#[test]fn all_original_words_lines_and_preexisting_owner_counts_are_preserved(){
 let a=actual(PDF);let v=value(&a);let b:serde_json::Value=serde_json::from_str(BASE).unwrap();assert_eq!(v["pages"][0]["words"],b["pages"][0]["words"]);
 let bag=|x:&serde_json::Value|{let mut a:Vec<_>=x["pages"][0]["regions"].as_array().unwrap().iter().flat_map(|r|r["lines"].as_array().unwrap()).map(|l|serde_json::to_string(l).unwrap()).collect();a.sort();a};assert_eq!(bag(&v),bag(&b));
 let mut ids=std::collections::BTreeSet::new();for id in a.pages[0].regions.iter().flat_map(|r|&r.lines).flat_map(|l|&l.words){assert!(ids.insert(*id));assert_eq!(a.pages[0].words[*id].status,WordStatus::Used);}
 let oldids:std::collections::BTreeSet<_>=b["pages"][0]["regions"].as_array().unwrap().iter().flat_map(|r|r["lines"].as_array().unwrap()).flat_map(|l|l["words"].as_array().unwrap()).map(|i|i.as_u64().unwrap()as usize).collect();assert_eq!(ids,oldids);let unowned:Vec<_>=a.pages[0].words.iter().filter(|w|w.status==WordStatus::Used&&!ids.contains(&w.id)).map(|w|w.id).collect();assert_eq!(unowned,[87,88,93]);assert_eq!(ids.len(),152);
 let mut cs:Vec<_>=a.text.chars().collect();let mut old:Vec<_>=b["text"].as_str().unwrap().chars().collect();cs.sort();old.sort();assert_eq!(cs,old);
}
#[test]fn every_used_word_binds_to_one_native_word_and_unique_original_glyphs(){
 let a=actual(PDF);let n=kanpo_pdf::extract_pages(PDF).unwrap();let p=&n[0];let native=kanpo_pdf::bbox::words(p);let mut seen=std::collections::BTreeSet::new();
 for w in a.pages[0].words.iter().filter(|w|w.status==WordStatus::Used){let matching:Vec<_>=native.iter().filter(|n|{if n.text!=w.raw{return false;}let b=n.glyphs.iter().map(|&i|p.glyphs[i].bbox).fold([f64::INFINITY,f64::INFINITY,f64::NEG_INFINITY,f64::NEG_INFINITY],|a,b|[a[0].min(b[0]),a[1].min(b[1]),a[2].max(b[2]),a[3].max(b[3])]);[w.x0,w.y0,w.x1,w.y1].iter().zip(b).all(|(&x,y)|(x as f64-y).abs()<0.001)}).collect();assert_eq!(matching.len(),1,"sourceWord{}",w.id);for &i in &matching[0].glyphs{assert!(seen.insert(i));}}
 assert_eq!(seen.len(),2513);
}
#[test]fn preceding_explicit_table_stays_complete_analysis_exact(){let a=actual(include_bytes!("archive/pages/20260727g00167/20260727g001670020.pdf"));let b:serde_json::Value=serde_json::from_str(include_str!("expected/paired_framed_continuation/20260727g001670020.analysis.json")).unwrap();assert_eq!(value(&a),b);}
#[test]fn following_prose_notice_stays_complete_analysis_exact(){let a=actual(include_bytes!("archive/pages/20260727g00167/20260727g001670022.pdf"));let b:serde_json::Value=serde_json::from_str(include_str!("expected/paired_framed_continuation/20260727g001670022.analysis.json")).unwrap();assert_eq!(value(&a),b);}
#[test]fn accepted17_single_sided_framed_cell_stays_complete_analysis_exact(){let a=actual(include_bytes!("archive/pages/20261001h01799/20261001h017990004.pdf"));let b:serde_json::Value=serde_json::from_str(include_str!("expected/paired_framed_continuation/20261001h017990004.analysis.json")).unwrap();assert_eq!(value(&a),b);}
#[test]fn accepted18_headerless_four_bands_stays_complete_analysis_exact(){let a=actual(include_bytes!("archive/pages/20261001h01799/20261001h017990003.pdf"));let b:serde_json::Value=serde_json::from_str(include_str!("expected/paired_framed_continuation/20261001h017990003.analysis.json")).unwrap();assert_eq!(value(&a),b);}


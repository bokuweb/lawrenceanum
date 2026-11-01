#[path = "support/reviewed_comparison_boundaries.rs"]
mod reviewed_comparison_boundaries;
#[path = "support/legacy_analysis.rs"]
mod legacy_analysis;
use kanpo_parser::{Analysis,trace::{Role,WordStatus}};
fn actual(p:&[u8])->Analysis {kanpo_parser::analyze_item(&kanpo_parser::page_inputs(p).unwrap())}
fn canonical(a:&Analysis)->serde_json::Value{legacy_analysis::value(a)}
const TARGET:&[u8]=include_bytes!("archive/pages/20261001h01799/20261001h017990003.pdf");
const BASE:&str=include_str!("expected/headerless_four_bands/20261001h017990003.analysis.json");
const EXPECT:&str=include_str!("expected/headerless_four_bands/expected-bands.json");
#[test]fn four_printed_bands_form_complete_after_then_complete_before(){
 let a=actual(TARGET);let e:serde_json::Value=serde_json::from_str(EXPECT).unwrap();let p=&a.pages[0];
 for(role,key)in[(Role::After,"after"),(Role::Before,"before")]{
  let rs:Vec<_>=p.regions.iter().filter(|r|r.role==role).collect();assert_eq!(rs.len(),2,"expected two original physical parts for {key}");
  for(i,r)in rs.iter().enumerate(){assert_eq!(r.part,i);assert_eq!(r.unit,0);assert_eq!(serde_json::to_value(r.lines.iter().map(|l|&l.text).collect::<Vec<_>>()).unwrap(),e[key][i]);assert_eq!(serde_json::to_value(r.lines.iter().flat_map(|l|&l.words).collect::<Vec<_>>()).unwrap(),e[format!("{key}_ids")][i]);}
 }
 let texts=|key:&str|e[key].as_array().unwrap().iter().flat_map(|part|part.as_array().unwrap()).map(|l|l.as_str().unwrap()).collect::<Vec<_>>().join("\n");
 assert_eq!(a.text,format!("改正後\n{}\n改正前\n{}",texts("after"),texts("before")));
 assert!(a.text.contains("の技能実習の適正な実施及び技能\n実習生の保護に関する法律（平成"));
}
#[test]fn one_structured_change_row_keeps_both_physical_parts_in_each_side(){
 let a=actual(TARGET);let v=canonical(&a);assert_eq!(v["articles"].as_array().unwrap().len(),1);let blocks=v["articles"][0]["document"]["blocks"].as_array().unwrap();let rows:Vec<_>=blocks.iter().filter(|b|b["kind"]=="shinkyu").flat_map(|b|b["rows"].as_array().unwrap()).collect();assert_eq!(rows.len(),1);
 let e:serde_json::Value=serde_json::from_str(EXPECT).unwrap();
 for key in ["after","before"]{let t=rows[0][key].as_array().unwrap().iter().map(|r|r["text"].as_str().unwrap()).collect::<String>();let want=e[key].as_array().unwrap().iter().flat_map(|p|p.as_array().unwrap()).map(|l|l.as_str().unwrap()).collect::<String>();let compact=|s:String|s.chars().filter(|c|!c.is_whitespace()).collect::<String>();assert_eq!(compact(t),compact(want));}
}
#[test]fn all_original_words_status_and_line_objects_have_one_exact_owner(){
 let a=actual(TARGET);let v=canonical(&a);let old:serde_json::Value=serde_json::from_str(BASE).unwrap();assert_eq!(v["pages"][0]["words"],old["pages"][0]["words"]);
 let lines=|v:&serde_json::Value|{let mut ls:Vec<_>=v["pages"][0]["regions"].as_array().unwrap().iter().flat_map(|r|r["lines"].as_array().unwrap()).map(|l|serde_json::to_string(l).unwrap()).collect();ls.sort();ls};assert_eq!(lines(&v),lines(&old));
 let mut ids=std::collections::BTreeSet::new();for id in a.pages[0].regions.iter().flat_map(|r|&r.lines).flat_map(|l|&l.words){assert!(ids.insert(*id));assert_eq!(a.pages[0].words[*id].status,WordStatus::Used);}assert_eq!(ids.len(),164);
}
#[test]fn all_used_words_bind_exactly_to_original_native_word_glyphs(){
 let a=actual(TARGET);let pages=kanpo_pdf::extract_pages(TARGET).unwrap();let p=&pages[0];let native=kanpo_pdf::bbox::words(p);let mut seen=std::collections::BTreeSet::new();let mut count=0;
 for w in a.pages[0].words.iter().filter(|w|w.status==WordStatus::Used){let found:Vec<_>=native.iter().filter(|n|{if n.text!=w.raw{return false;}let b=n.glyphs.iter().map(|&i|p.glyphs[i].bbox).fold([f64::INFINITY,f64::INFINITY,f64::NEG_INFINITY,f64::NEG_INFINITY],|a,b|[a[0].min(b[0]),a[1].min(b[1]),a[2].max(b[2]),a[3].max(b[3])]);[w.x0,w.y0,w.x1,w.y1].iter().zip(b).all(|(&x,y)|(x as f64-y).abs()<0.001)}).collect();assert_eq!(found.len(),1,"source word {}",w.id);for &id in &found[0].glyphs{assert!(seen.insert(id));assert!(p.glyphs[id].vertical);count+=1;}}
 assert_eq!(count,1637);
}
#[test]fn previous_page_has_explicit_labels_and_other_article(){let a=actual(include_bytes!("archive/pages/20261001h01799/20261001h017990002.pdf"));let e:serde_json::Value=serde_json::from_str(include_str!("expected/headerless_four_bands/20261001h017990002.analysis.json")).unwrap();assert_eq!(canonical(&a),e);}
#[test]fn following_page_has_separate_notices(){let a=actual(include_bytes!("archive/pages/20261001h01799/20261001h017990005.pdf"));let e:serde_json::Value=serde_json::from_str(include_str!("expected/headerless_four_bands/20261001h017990005.analysis.json")).unwrap();assert_eq!(canonical(&a),reviewed_comparison_boundaries::preceding_supplement(e));}
#[test]fn ordinary_four_tiers_of_similar_forest_notices_stay_exact(){let a=actual(include_bytes!("archive/pages/20260812h01766/20260812h017660002.pdf"));let e:serde_json::Value=serde_json::from_str(include_str!("expected/headerless_four_bands/20260812h017660002.analysis.json")).unwrap();assert_eq!(canonical(&a),e);}

const NEIGHBOR4:&[u8]=include_bytes!("archive/pages/20261001h01799/20261001h017990004.pdf");
fn sequence()->Analysis {
 let mut input=Vec::new();
 for pdf in [include_bytes!("archive/pages/20261001h01799/20261001h017990002.pdf").as_slice(),TARGET,NEIGHBOR4,include_bytes!("archive/pages/20261001h01799/20261001h017990005.pdf").as_slice()] {input.extend(kanpo_parser::page_inputs(pdf).unwrap());}
 kanpo_parser::analyze_item(&input)
}
#[test]fn neighboring_pages_do_not_disable_both_continuation_parts(){
 let a=sequence();let e:serde_json::Value=serde_json::from_str(EXPECT).unwrap();
 for(role,key)in [(Role::After,"after"),(Role::Before,"before")] {
  let rs:Vec<_>=a.pages[1].regions.iter().filter(|r|r.role==role).collect();assert_eq!(rs.len(),2);
  for(i,r)in rs.iter().enumerate(){assert_eq!(serde_json::to_value(r.lines.iter().map(|l|&l.text).collect::<Vec<_>>()).unwrap(),e[key][i]);}
 }
 assert_eq!(a.articles.len(),11);
}
#[test]fn continuation_state_does_not_leak_into_three_surrounding_pages(){
 let a=sequence();let v=canonical(&a);
 for (i,pdf) in [(0,include_bytes!("archive/pages/20261001h01799/20261001h017990002.pdf").as_slice()),(2,NEIGHBOR4),(3,include_bytes!("archive/pages/20261001h01799/20261001h017990005.pdf").as_slice())] {
  let standalone=actual(pdf);assert_eq!(v["pages"][i],canonical(&standalone)["pages"][0]);
 }
 // The neighboring framed helper is deliberately repaired in batch17. Its exact new
 // standalone result is the contextual oracle, while all original source Word fields
 // retain the literal pre-repair original-Word oracle below.
 let old:serde_json::Value=serde_json::from_str(include_str!("expected/headerless_four_bands/20261001h017990004.analysis.json")).unwrap();
 assert_eq!(v["pages"][2]["words"],old["pages"][0]["words"]);
}

// The date and the signer 金融庁長官伊藤　豊 are one printed column: the gold is the accepted one with that line joined.
#[test]fn accepted17_framed_after_continuation_page_is_complete_analysis_exact(){
 let a=actual(NEIGHBOR4);
 let expected:serde_json::Value=serde_json::from_str(include_str!("expected/headerless_four_bands/20261001h017990004-after38-signer263m.analysis.json")).unwrap();
 assert_eq!(canonical(&a),expected);
}

use kanpo_parser::{Analysis,trace::{Role,WordStatus}};
fn actual(pdf:&[u8])->Analysis{kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap())}
fn canonical(a:&Analysis)->serde_json::Value{serde_json::from_str(&serde_json::to_string(a).unwrap()).unwrap()}
const PDF0:&[u8]=include_bytes!("archive/pages/20261001h01799/20261001h017990004.pdf");
const BASE0:&str=include_str!("expected/framed_after_continuation/20261001h017990004.analysis.json");
const PDF1:&[u8]=include_bytes!("archive/pages/20261001h01799/20261001h017990003.pdf");
const BASE1:&str=include_str!("expected/framed_after_continuation/20261001h017990003-after39.analysis.json");
const PDF2:&[u8]=include_bytes!("archive/pages/20261001h01799/20261001h017990005.pdf");
const BASE2:&str=include_str!("expected/framed_after_continuation/20261001h017990005.analysis.json");
const PDF3:&[u8]=include_bytes!("archive/pages/20260810g00179/20260810g001790057.pdf");
const BASE3:&str=include_str!("expected/framed_after_continuation/20260810g001790057.analysis.json");
const PDF4:&[u8]=include_bytes!("archive/pages/20260902g00196/20260902g001960024.pdf");
const BASE4:&str=include_str!("expected/framed_after_continuation/20260902g001960024.analysis.json");
const EXPECT_AFTER:[&str;12]=["（理事の住所についてその一部のみを公告","することを求める旨の申出）","第十六条の三の二　法第二十九条第二項ただ","し書の規定による申出は、当該申出に係る","理事の住所のうち都道府県名及び市町村名","（同一氏名組合員（当該申出に係る理事と","同一の氏名を有する当該理事以外の組合員","をいう。）が当該申出に係る理事と同一の市","町村の区域内に住所を有する場合は、都道","府県名、市町村名及び市町村の区域内の町","又は字の名称）を公告することを求める旨","を申し出ることにより行うものとする。"];
const SOURCE_IDS:[usize;14]=[173,176,179,180,183,185,191,181,0,11,20,23,29,31];
#[test]fn both_framed_halves_belong_to_one_after_before_the_new_item_marker(){
let a=actual(PDF0);let page=&a.pages[0];let after:Vec<_>=page.regions.iter().filter(|r|r.role==Role::After&&r.unit==2).flat_map(|r|&r.lines).collect();
assert_eq!(after.iter().map(|l|l.text.as_str()).collect::<Vec<_>>(),EXPECT_AFTER);
assert_eq!(after.iter().flat_map(|l|&l.words).copied().collect::<Vec<_>>(),[173,176,179,180,183,185,191,0,11,20,23,29,31]);
let before:Vec<_>=page.regions.iter().filter(|r|r.role==Role::Before&&r.unit==2).flat_map(|r|&r.lines).map(|l|l.text.as_str()).collect();assert_eq!(before,["（新設）"]);
let exact=format!("改正後\n{}\n改正前\n（新設）\n附則",EXPECT_AFTER.join("\n"));assert!(a.text.contains(&exact),"{}",a.text);
}
#[test]fn tail_is_inside_its_original_shinkyu_row_and_not_a_following_body(){let a=actual(PDF0);let value=canonical(&a);let article=&value["articles"][1];let text=article["text"].as_str().unwrap();assert!(text.contains(&format!("{}\n改正前\n（新設）",EXPECT_AFTER.join("\n"))));for id in [0,11,20,23,29,31]{let owners:Vec<_>=a.pages[0].regions.iter().filter(|r|r.lines.iter().any(|l|l.words.contains(&id))).collect();assert_eq!(owners.len(),1);assert_eq!(owners[0].role,Role::After);assert_eq!(owners[0].unit,2);} }
#[test]fn every_original_word_and_all_unrelated_lines_and_articles_are_exact(){
let a=actual(PDF0);let v=canonical(&a);let old:serde_json::Value=serde_json::from_str(BASE0).unwrap();assert_eq!(v["pages"][0]["words"],old["pages"][0]["words"]);assert_eq!(v["format"],old["format"]);
for id in SOURCE_IDS{assert_eq!(a.pages[0].words[id].status,WordStatus::Used);let n:usize=a.pages[0].regions.iter().flat_map(|r|&r.lines).map(|l|l.words.iter().filter(|&&x|x==id).count()).sum();assert_eq!(n,1);}
for j in [0,1,2,3,4,5,7,8]{let r=&old["pages"][0]["regions"][j];let same=v["pages"][0]["regions"].as_array().unwrap().iter().find(|n|n["role"]==r["role"]&&n["unit"]==r["unit"]&&n["part"]==r["part"]).unwrap();assert_eq!(same,r);}
let newbody=&a.pages[0].regions.iter().find(|r|r.role==Role::Body&&r.unit==3).unwrap().lines;let oldbody=&old["pages"][0]["regions"][6]["lines"].as_array().unwrap()[6..];assert_eq!(serde_json::to_value(newbody).unwrap(),serde_json::Value::Array(oldbody.to_vec()));
for i in [0,2,3,4]{assert_eq!(v["articles"][i],old["articles"][i]);}
let mut chars:Vec<_>=a.text.chars().collect();let mut before:Vec<_>=old["text"].as_str().unwrap().chars().collect();chars.sort();before.sort();assert_eq!(chars,before);
}
#[test]fn all_fourteen_source_words_bind_to_the_original_vertical_glyphs_once(){let a=actual(PDF0);let native=kanpo_pdf::extract_pages(PDF0).unwrap();let mut seen=std::collections::BTreeSet::new();let mut total=0;for id in SOURCE_IDS{let w=&a.pages[0].words[id];let gs:Vec<_>=native[0].glyphs.iter().enumerate().filter(|(_,g)|{let b=g.bbox;let x=(b[0]+b[2])*0.5;let y=(b[1]+b[3])*0.5;x>=w.x0 as f64-0.01&&x<w.x1 as f64+0.01&&y>=w.y0 as f64-0.01&&y<w.y1 as f64+0.01}).collect();assert_eq!(gs.iter().map(|(_,g)|g.text.as_str()).collect::<String>(),w.raw);for(i,g)in gs{assert!(seen.insert(i));assert!(g.vertical);assert!((g.size-8.).abs()<0.001);total+=1;}}assert_eq!(total,226);}
#[test]fn preceding_page_has_its_own_paired_change_bands(){let a=actual(PDF1);let old:serde_json::Value=serde_json::from_str(BASE1).unwrap();assert_eq!(canonical(&a),old);}
#[test]fn following_page_has_separate_notices(){let a=actual(PDF2);let old:serde_json::Value=serde_json::from_str(BASE2).unwrap();assert_eq!(canonical(&a),old);}
#[test]fn full_height_frame_finishes_before_next_article(){let a=actual(PDF3);let old:serde_json::Value=serde_json::from_str(BASE3).unwrap();assert_eq!(canonical(&a),old);}
#[test]fn another_change_frame_stays_inside_its_own_notice(){let a=actual(PDF4);let old:serde_json::Value=serde_json::from_str(BASE4).unwrap();assert_eq!(canonical(&a),old);}

#[test]fn structured_after_cell_contains_all_twelve_columns_and_no_continuation_paragraph(){
 let a=actual(PDF0);let v=canonical(&a);let blocks=v["articles"][1]["document"]["blocks"].as_array().unwrap();let changes:Vec<_>=blocks.iter().filter(|b|b["kind"]=="shinkyu").collect();assert_eq!(changes.len(),1);let rows=changes[0]["rows"].as_array().unwrap();assert_eq!(rows.len(),1);
 let text=|field:&str|rows[0][field].as_array().unwrap().iter().map(|r|r["text"].as_str().unwrap()).collect::<String>().chars().filter(|c|!c.is_whitespace()).collect::<String>();
 let expected=EXPECT_AFTER.join("").chars().filter(|c|!c.is_whitespace()).collect::<String>();assert_eq!(text("after"),expected);assert_eq!(text("before"),"（新設）");
 for b in blocks.iter().filter(|b|b["kind"]=="paragraph"){let t=b["runs"].as_array().unwrap().iter().map(|r|r["text"].as_str().unwrap()).collect::<String>();assert!(!t.contains(EXPECT_AFTER[6]));assert!(!t.contains(EXPECT_AFTER[11]));}
}

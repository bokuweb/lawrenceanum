mod trace {
 pub use kanpo_parser::trace::{Line,Word,WordStatus,Table};
 pub enum Unit {Plain(Vec<Line>),Shinkyu{preamble:Vec<Line>,after:Vec<Vec<Line>>,before:Vec<Vec<Line>>,outside:Vec<Vec<Line>>}}
}
mod lines {pub use kanpo_parser::lines::{HRule,PageRules,VRule};}
#[path="../src/headerless_four_bands.rs"]mod implementation;
use trace::{Line,Unit,Word,WordStatus};use lines::{PageRules,HRule,VRule};
fn fixture()->(Vec<Unit>,Vec<Word>,PageRules){
 let a:serde_json::Value=serde_json::from_str(include_str!("expected/headerless_four_bands/20261001h017990003.analysis.json")).unwrap();let p=&a["pages"][0];
 let units=p["regions"].as_array().unwrap().iter().map(|r|Unit::Plain(r["lines"].as_array().unwrap().iter().map(|l|Line::text(l["text"].as_str().unwrap().into(),l["words"].as_array().unwrap().iter().map(|id|id.as_u64().unwrap()as usize).collect())).collect())).collect();
 let words=p["words"].as_array().unwrap().iter().map(|w|Word{source_box: [w["x0"].as_f64().unwrap()as f32, w["y0"].as_f64().unwrap()as f32, w["x1"].as_f64().unwrap()as f32, w["y1"].as_f64().unwrap()as f32],id:w["id"].as_u64().unwrap()as usize,x0:w["x0"].as_f64().unwrap()as f32,x1:w["x1"].as_f64().unwrap()as f32,y0:w["y0"].as_f64().unwrap()as f32,y1:w["y1"].as_f64().unwrap()as f32,raw:w["raw"].as_str().unwrap().into(),text:w["text"].as_str().unwrap().into(),status:match w["status"].as_str().unwrap(){"used"=>WordStatus::Used,"margin"=>WordStatus::Margin,_=>panic!()}}).collect();
 let r:serde_json::Value=serde_json::from_str(include_str!("expected/headerless_four_bands/20261001h017990003-rules.json")).unwrap();let rules=PageRules{enclosures:vec![],short_fraction_bars:vec![],vertical:vec![],horizontal:r["rules"]["horizontal"].as_array().unwrap().iter().map(|h|HRule{x0:h["x0"].as_f64().unwrap()as f32,x1:h["x1"].as_f64().unwrap()as f32,y:h["y"].as_f64().unwrap()as f32}).collect()};(units,words,rules)
}
fn state(us:&[Unit],ws:&[Word])->serde_json::Value{serde_json::json!({"units":us.iter().map(|u|match u{Unit::Plain(ls)=>serde_json::json!({"plain":ls}),Unit::Shinkyu{preamble,after,before,outside}=>serde_json::json!({"preamble":preamble,"after":after,"before":before,"outside":outside})}).collect::<Vec<_>>(),"words":ws})}
fn line(us:&mut[Unit],u:usize,l:usize)->&mut Line{let Unit::Plain(ls)=&mut us[u]else{panic!()}; &mut ls[l]}
fn reject(mutate:impl FnOnce(&mut Vec<Unit>,&mut Vec<Word>,&mut PageRules)) {let(mut u,mut w,mut r)=fixture();mutate(&mut u,&mut w,&mut r);let before=state(&u,&w);implementation::restore(&mut u,&w,Some(&r),595.);assert_eq!(state(&u,&w),before);}
#[test]fn real_four_bands_rejoin_and_preserve_every_line_and_word(){let(mut u,w,r)=fixture();let before=state(&u,&w);implementation::restore(&mut u,&w,Some(&r),595.);assert_eq!(u.len(),1);let Unit::Shinkyu{after,before:old,preamble,outside}=&u[0]else{panic!("not a shinkyu")};assert!(preamble.is_empty()&&outside.is_empty());assert_eq!(after.iter().map(Vec::len).collect::<Vec<_>>(),[29,23]);assert_eq!(old.iter().map(Vec::len).collect::<Vec<_>>(),[25,41]);assert_eq!(serde_json::to_value(&after[0]).unwrap(),before["units"][0]["plain"]);assert_eq!(serde_json::to_value(&old[0]).unwrap(),before["units"][1]["plain"]);assert_eq!(serde_json::to_value(&after[1]).unwrap(),before["units"][2]["plain"]);assert_eq!(serde_json::to_value(&old[1]).unwrap(),before["units"][3]["plain"]);assert_eq!(serde_json::to_value(&w).unwrap(),before["words"]);}
#[test]fn no_native_rules(){let(mut u,w,_)=fixture();let before=state(&u,&w);implementation::restore(&mut u,&w,None,595.);assert_eq!(state(&u,&w),before);}
#[test]fn missing_one_split_horizontal_piece(){reject(|_,_,r|r.horizontal.retain(|h|h.y!=241.||h.x0>46.1));}
#[test]fn missing_pair_boundary(){reject(|_,_,r|r.horizontal.retain(|h|h.y!=609.));}
#[test]fn unequal_band_height(){reject(|_,_,r|r.horizontal.iter_mut().filter(|h|h.y==609.).for_each(|h|h.y+=5.));}
#[test]fn frame_endpoints_differ(){reject(|_,_,r|r.horizontal.iter_mut().filter(|h|h.y==780.5&&h.x0>300.).for_each(|h|h.x1-=5.));}
#[test]fn interior_vertical_cells(){reject(|_,_,r|r.vertical.push(VRule{x:200.,y0:69.5,y1:412.5}));}
#[test]fn independent_notice_header(){reject(|u,_,_|line(u,0,0).text="〇某省告示第十号".into());}
#[test]fn another_circle_notice_header(){reject(|u,_,_|line(u,2,0).text="○公告".into());}
#[test]fn amending_instructions(){reject(|u,_,_|line(u,0,0).text="別表を次のように改める。".into());}
#[test]fn no_standalone_change_mark(){reject(|u,_,_|line(u,0,28).text="通常の本文を削る。".into());}
#[test]fn only_omission_mark_is_not_a_change_table(){reject(|u,_,_|line(u,0,28).text="（略）".into());}
#[test]fn lower_before_omission_mark_missing(){reject(|u,_,_|{line(u,3,25).text="本文".into();line(u,3,36).text="本文".into();line(u,3,40).text="本文".into();});}
#[test]fn common_footer_only_two_columns(){reject(|u,_,_|line(u,3,38).text="別の条文".into());}
#[test]fn common_footer_same_text_wrong_column(){reject(|u,w,_|{let id=line(u,3,40).words[0];w[id].x0+=3.;w[id].x1+=3.;});}
#[test]fn common_footer_same_text_wrong_height(){reject(|u,w,_|{let id=line(u,3,40).words[0];w[id].y0+=3.;w[id].y1+=3.;});}
#[test]fn repeated_short_footer_not_enough(){reject(|u,_,_|for j in [2,3]{let len=if j==2{23}else{41};for k in 1..=5{line(u,j,len-k).text="（略）".into();}});}
#[test]fn unrelated_long_before_body(){reject(|u,_,_|for j in [1,3]{let Unit::Plain(ls)=&mut u[j]else{panic!()};let len=ls.len();for l in &mut ls[..len-5]{l.text="価格数量寸法金額所在地".repeat(5);}});}
#[test]fn extra_body_unit(){reject(|u,_,_|u.push(Unit::Plain(vec![])));}
#[test]fn preexisting_shinkyu_unit(){reject(|u,_,_|u[0]=Unit::Shinkyu{preamble:vec![],after:vec![],before:vec![],outside:vec![]});}
#[test]fn too_few_columns(){reject(|u,_,_|{let Unit::Plain(ls)=&mut u[0]else{panic!()};ls.truncate(3);});}
#[test]fn shared_word_owner(){reject(|u,_,_|{let id=line(u,0,0).words[0];line(u,1,0).words.push(id);});}
#[test]fn unknown_source_word(){reject(|u,_,_|line(u,0,0).words.push(10000));}
#[test]fn margin_word_anchor(){reject(|_,w,_|w[3].status=WordStatus::Margin);}
#[test]fn empty_word_anchor(){reject(|_,w,_|w[3].status=WordStatus::Empty);}
#[test]fn noise_word_anchor(){reject(|_,w,_|w[3].status=WordStatus::Noise);}
#[test]fn header_word_anchor(){reject(|_,w,_|w[3].status=WordStatus::HeaderLabel);}
#[test]fn extra_unowned_printed_word_inside_frame(){reject(|_,w,_|{let mut n=w[3].clone();n.id=w.len();w.push(n);});}
#[test]fn duplicate_word_identity(){reject(|_,w,_|w.push(w[3].clone()));}
#[test]fn horizontal_line_in_vertical_bands(){reject(|_,w,_|w[3].x1+=40.);}
#[test]fn line_crosses_rule(){reject(|_,w,_|w[3].y1=245.);}
#[test]fn mixed_em_sizes(){reject(|_,w,_|w[3].x1+=2.);}
#[test]fn reversed_column_order(){reject(|u,_,_|{let Unit::Plain(ls)=&mut u[0]else{panic!()};ls.swap(0,10);});}
#[test]fn missing_line_word_ids(){reject(|u,_,_|line(u,0,0).words.clear());}
#[test]fn invalid_page_width(){let(mut u,w,r)=fixture();let before=state(&u,&w);implementation::restore(&mut u,&w,Some(&r),f32::NAN);assert_eq!(state(&u,&w),before);}
#[test]fn translated_frame_has_no_page_specific_coordinates(){let(mut u,mut w,mut r)=fixture();for w in &mut w {w.x0+=10.;w.x1+=10.;w.y0+=12.;w.y1+=12.;}for h in &mut r.horizontal{h.x0+=10.;h.x1+=10.;h.y+=12.;}implementation::restore(&mut u,&w,Some(&r),595.);assert!(matches!(u.as_slice(),[Unit::Shinkyu{..}]));}
#[test]fn scaled_frame_uses_type_size_and_rule_geometry(){let(mut u,mut w,mut r)=fixture();for w in &mut w {w.x0*=1.25;w.x1*=1.25;w.y0*=1.25;w.y1*=1.25;}for h in &mut r.horizontal{h.x0*=1.25;h.x1*=1.25;h.y*=1.25;}implementation::restore(&mut u,&w,Some(&r),595.*1.25);assert!(matches!(u.as_slice(),[Unit::Shinkyu{..}]));}
#[test]fn amending_instruction_split_over_columns(){reject(|u,_,_|{line(u,0,0).text="別表を次のように".into();line(u,0,1).text="改める。".into();});}

#[test]fn existing_table_line(){reject(|u,_,_|line(u,0,0).table=Some(trace::Table{x0:0.,y0:0.,x1:1.,y1:1.,rows:1,cols:1,cells:vec![]}));}

#[test]fn no_original_word_ids_in_any_line(){reject(|u,_,_|for unit in u {if let Unit::Plain(ls)=unit {for l in ls {l.words.clear();}}});}

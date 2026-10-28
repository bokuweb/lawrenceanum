use crate::paired_framed_continuations as implementation;
use crate::trace::{Line,Unit,Word,WordStatus};use crate::lines::{PageRules,HRule,VRule};
fn fixture()->(Vec<Unit>,Vec<Word>,PageRules){
let a:serde_json::Value=serde_json::from_str(include_str!("../tests/expected/paired_framed_continuation/20260727g001670021.analysis.json")).unwrap();let p=&a["pages"][0];
let line=|k:usize|p["regions"][k]["lines"].as_array().unwrap().iter().map(|l|Line::text(l["text"].as_str().unwrap().into(),l["words"].as_array().unwrap().iter().map(|id|id.as_u64().unwrap()as usize).collect())).collect::<Vec<_>>();
let units=vec![Unit::Shinkyu{labels:None,preamble:line(0),after:vec![line(1)],before:vec![line(2)],outside:vec![]},Unit::Plain(line(3)),Unit::Plain(line(4)),Unit::Plain(line(5))];
let words=p["words"].as_array().unwrap().iter().map(|w|Word{source_box: [w["x0"].as_f64().unwrap()as f32, w["y0"].as_f64().unwrap()as f32, w["x1"].as_f64().unwrap()as f32, w["y1"].as_f64().unwrap()as f32],id:w["id"].as_u64().unwrap()as usize,x0:w["x0"].as_f64().unwrap()as f32,x1:w["x1"].as_f64().unwrap()as f32,y0:w["y0"].as_f64().unwrap()as f32,y1:w["y1"].as_f64().unwrap()as f32,raw:w["raw"].as_str().unwrap().into(),text:w["text"].as_str().unwrap().into(),status:match w["status"].as_str().unwrap(){"used"=>WordStatus::Used,"margin"=>WordStatus::Margin,"header_label"=>WordStatus::HeaderLabel,_=>panic!()}}).collect();
let v:serde_json::Value=serde_json::from_str(include_str!("../tests/expected/paired_framed_continuation/target-rules.json")).unwrap();let r=&v["rules"];let rules=PageRules{enclosures:vec![],short_fraction_bars:vec![],horizontal:r["horizontal"].as_array().unwrap().iter().map(|h|HRule{x0:h["x0"].as_f64().unwrap()as f32,x1:h["x1"].as_f64().unwrap()as f32,y:h["y"].as_f64().unwrap()as f32}).collect(),vertical:r["vertical"].as_array().unwrap().iter().map(|h|VRule{x:h["x"].as_f64().unwrap()as f32,y0:h["y0"].as_f64().unwrap()as f32,y1:h["y1"].as_f64().unwrap()as f32}).collect()};(units,words,rules)}
fn state(us:&[Unit],ws:&[Word])->serde_json::Value{serde_json::json!({"words":ws,"units":us.iter().map(|u|match u{Unit::Plain(ls)=>serde_json::json!({"plain":ls}),Unit::Shinkyu{preamble,after,before,outside,..}=>serde_json::json!({"preamble":preamble,"after":after,"before":before,"outside":outside})}).collect::<Vec<_>>()})}
fn reject(f:impl FnOnce(&mut Vec<Unit>,&mut Vec<Word>,&mut PageRules)){let(mut u,mut w,mut r)=fixture();f(&mut u,&mut w,&mut r);let old=state(&u,&w);implementation::restore(&mut u,&w,Some(&r),595.,842.);assert_eq!(state(&u,&w),old);}
fn joined(us:&[Unit]){let Unit::Shinkyu{after,before,..}=&us[0]else{panic!()};assert_eq!(after.iter().map(Vec::len).collect::<Vec<_>>(),[27,7]);assert_eq!(before.iter().map(Vec::len).collect::<Vec<_>>(),[23,15]);let Unit::Plain(rest)=&us[1]else{panic!()};assert_eq!(rest.iter().map(|l|l.text.as_str()).collect::<Vec<_>>(),["附則","この省令は、公布の日から施行する。","法規的告示"]);let Unit::Plain(rest)=&us[2]else{panic!()};assert!(rest.is_empty());}
#[test]fn real_both_framed_cells_rejoin_original_parts(){let(mut u,w,r)=fixture();let old=state(&u,&w);implementation::restore(&mut u,&w,Some(&r),595.,842.);joined(&u);assert_eq!(state(&u,&w)["words"],old["words"]);let Unit::Shinkyu{after,before,..}=&u[0]else{panic!()};assert_eq!(serde_json::to_value(&after[0]).unwrap(),old["units"][0]["after"][0]);assert_eq!(serde_json::to_value(&before[0]).unwrap(),old["units"][0]["before"][0]);assert_eq!(serde_json::to_value(&after[1]).unwrap(),serde_json::Value::Array(old["units"][1]["plain"].as_array().unwrap()[..7].to_vec()));assert_eq!(serde_json::to_value(&before[1]).unwrap(),old["units"][2]["plain"]);}
#[test]fn runtime_headers_still_used_but_unowned_are_allowed(){let(mut u,mut w,r)=fixture();for x in w.iter_mut().filter(|w|w.status==WordStatus::HeaderLabel){x.status=WordStatus::Used;}implementation::restore(&mut u,&w,Some(&r),595.,842.);joined(&u);}
#[test]fn missing_rules(){let(mut u,w,_)=fixture();let old=state(&u,&w);implementation::restore(&mut u,&w,None,595.,842.);assert_eq!(state(&u,&w),old);}
#[test]fn missing_upper_cap(){reject(|_,_,r|r.horizontal.retain(|h|h.y!=241.));}
#[test]fn missing_lower_cap(){reject(|_,_,r|r.horizontal.retain(|h|h.y!=609.));}
#[test]fn missing_lower_outer_border(){reject(|_,_,r|r.vertical.retain(|v|v.x!=349.5));}
#[test]fn missing_tier_separator(){reject(|_,_,r|r.horizontal.retain(|h|h.y!=425.));}
#[test]fn lower_half_height_different(){reject(|_,_,r|r.horizontal.iter_mut().filter(|h|h.y==609.).for_each(|h|h.y+=4.));}
#[test]fn real_vertical_cell_inside_lower_frame(){reject(|_,_,r|r.vertical.push(VRule{x:450.,y0:437.5,y1:780.5}));}
#[test]fn lower_cap_endpoints_differ(){reject(|_,_,r|r.horizontal.iter_mut().filter(|h|h.y==780.5).for_each(|h|h.x1-=16.));}
#[test]fn header_word_is_noise(){reject(|_,w,_|w[40].status=WordStatus::Noise);}
#[test]fn header_word_is_empty(){reject(|_,w,_|w[40].text.clear());}
#[test]fn header_text_is_not_full_original_label(){reject(|_,w,_|w[42].text="異".into());}
#[test]fn body_word_is_margin(){reject(|_,w,_|w[1].status=WordStatus::Margin);}
#[test]fn body_word_is_empty(){reject(|_,w,_|w[1].text.clear());}
#[test]fn body_word_crosses_middle_rule(){reject(|_,w,_|w[1].y0=600.);}
#[test]fn body_word_crosses_left_rule(){reject(|_,w,_|w[61].x0=345.);}
#[test]fn duplicate_word_id(){reject(|_,w,_|w[1].id=w[3].id);}
#[test]fn duplicate_source_owner(){reject(|u,_,_|{let Unit::Plain(ls)=&u[2]else{panic!()};let l=ls[0].clone();let Unit::Plain(other)=&mut u[3]else{panic!()};other.push(l);});}
#[test]fn an_extra_unowned_word_inside_lower_cell(){reject(|_,w,_|{let mut x=w[1].clone();x.id=999;x.x0=520.;x.x1=528.;x.y0=670.;x.y1=694.;w.push(x);});}
#[test]fn one_line_has_no_original_ids(){reject(|u,_,_|{let Unit::Plain(ls)=&mut u[2]else{panic!()};ls[0].words.clear();});}
#[test]fn rendered_text_disagrees_with_its_words(){reject(|u,_,_|{let Unit::Plain(ls)=&mut u[2]else{panic!()};ls[0].text="違う本文".into();});}
#[test]fn horizontal_words_in_lower_cell(){reject(|_,w,_|{w[1].x1=w[1].x0+100.;w[1].y1=w[1].y0+8.;});}
#[test]fn lower_cell_contains_an_independent_notice(){reject(|u,w,_|{let Unit::Plain(ls)=&mut u[2]else{panic!()};ls[0].text.insert(0,'〇');w[1].text.insert(0,'〇');w[1].raw.insert(0,'〇');});}
#[test]fn both_source_sides_finish_sentences(){reject(|u,w,_|{let Unit::Shinkyu{before,..}=&mut u[0]else{panic!()};let l=before[0].last_mut().unwrap();l.text.pop();l.text.push('。');let id=*l.words.last().unwrap();w[id].text.pop();w[id].text.push('。');w[id].raw.pop();w[id].raw.push('。');});}
#[test]fn continuation_does_not_reach_source_left_edge(){reject(|_,w,_|{for x in w.iter_mut().filter(|w|w.x0<50.&&w.y0>241.&&w.y1<412.5){x.x0+=20.;x.x1+=20.;}});}
#[test]fn first_lower_before_column_starts_too_low(){reject(|_,w,_|w[1].y0+=24.);}
#[test]fn unrelated_large_font_in_lower_cell(){reject(|_,w,_|{w[1].x1+=4.;});}
#[test]fn lower_cell_has_a_non_lattice_column(){reject(|_,w,_|{w[1].x0-=4.;w[1].x1-=4.;});}
#[test]fn preexisting_multiple_parts_do_not_rejoin(){reject(|u,_,_|{let Unit::Shinkyu{after,..}=&mut u[0]else{panic!()};after.push(vec![]);});}
#[test]fn one_sided_continuation_is_left_to_existing_framed_helper(){reject(|u,w,_|{let Unit::Plain(ls)=&mut u[2]else{panic!()};let ids=ls.iter().flat_map(|l|&l.words).copied().collect::<std::collections::HashSet<_>>();ls.clear();for x in w.iter_mut().filter(|x|ids.contains(&x.id)){x.x0-=250.;x.x1-=250.;}});}

fn transformed(scale:f32,dx:f32,dy:f32){let(mut u,mut w,mut r)=fixture();for x in &mut w{x.x0=x.x0*scale+dx;x.x1=x.x1*scale+dx;x.y0=x.y0*scale+dy;x.y1=x.y1*scale+dy;}for h in &mut r.horizontal{h.x0=h.x0*scale+dx;h.x1=h.x1*scale+dx;h.y=h.y*scale+dy;}for v in &mut r.vertical{v.x=v.x*scale+dx;v.y0=v.y0*scale+dy;v.y1=v.y1*scale+dy;}let old=state(&u,&w);implementation::restore(&mut u,&w,Some(&r),595.*scale+dx*2.,842.*scale+dy*2.);joined(&u);assert_eq!(state(&u,&w)["words"],old["words"]);}
#[test]fn translated_native_geometry_does_not_depend_on_page_coordinates(){transformed(1.,20.,30.);}
#[test]fn scaled_native_geometry_does_not_depend_on_eight_point_font(){transformed(1.25,0.,0.);}

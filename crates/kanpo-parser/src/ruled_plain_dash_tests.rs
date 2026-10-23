use crate::{grid::*, trace::*, ruled_plain_dashes};
fn fixture()->(Vec<Unit>,Vec<Word>,Vec<Grid>){
 let a:serde_json::Value=serde_json::from_str(include_str!("../tests/expected/ruled_plain_dashes/20260911g002030083-before42.analysis.json")).unwrap();
 let ws=a["pages"][0]["words"].as_array().unwrap().iter().map(|w|Word{source_box: [w["x0"].as_f64().unwrap() as f32, w["y0"].as_f64().unwrap() as f32, w["x1"].as_f64().unwrap() as f32, w["y1"].as_f64().unwrap() as f32],id:w["id"].as_u64().unwrap() as usize,x0:w["x0"].as_f64().unwrap() as f32,y0:w["y0"].as_f64().unwrap() as f32,x1:w["x1"].as_f64().unwrap() as f32,y1:w["y1"].as_f64().unwrap() as f32,raw:w["raw"].as_str().unwrap().into(),text:w["text"].as_str().unwrap().into(),status:match w["status"].as_str().unwrap(){"used"=>WordStatus::Used,"empty"=>WordStatus::Empty,"margin"=>WordStatus::Margin,"noise"=>WordStatus::Noise,_=>panic!()}}).collect();
 let ls=a["pages"][0]["regions"][0]["lines"].as_array().unwrap().iter().map(|l|Line::text(l["text"].as_str().unwrap().into(),l["words"].as_array().unwrap().iter().map(|i|i.as_u64().unwrap() as usize).collect())).collect();
 let g=Grid{xs:vec![69.5,220.5,300.5,380.5,460.5,540.5,620.5,700.5,780.5],ys:vec![70.429,97.357,549.0],cells:(0..8).flat_map(|c|(0..2).map(move|r|GridCell{x0:c,x1:c+1,y0:r,y1:r+1})).collect()};
 (vec![Unit::Plain(ls)],ws,vec![g])
}
fn state(u:&[Unit],w:&[Word])->String{
 let ls:Vec<_>=u.iter().flat_map(|u|match u{Unit::Plain(ls)=>ls.clone(),_=>vec![]}).collect();
 serde_json::to_string(&(ls,w)).unwrap()
}
fn unchanged(mut u:Vec<Unit>,mut w:Vec<Word>,g:Vec<Grid>){let before=state(&u,&w);ruled_plain_dashes::restore(&mut u,&mut w,&g);assert_eq!(state(&u,&w),before);}
#[test]fn original_grid_returns_109_unique_word_ids_and_keeps_old_line_order(){
 let(mut u,mut w,g)=fixture();let before=state(&u,&w);ruled_plain_dashes::restore(&mut u,&mut w,&g);assert_ne!(state(&u,&w),before);
 assert_eq!(w.iter().filter(|w|w.raw=="\u{e61c}"&&w.status==WordStatus::Used).count(),109);
 let Unit::Plain(ls)=&u[0]else{panic!()};let mut ids=std::collections::BTreeSet::new();for l in ls{for &id in &l.words{assert!(ids.insert(id));}}assert_eq!(ids.len(),w.iter().filter(|w|w.status==WordStatus::Used).count());
}
#[test]fn no_grid(){let(mut u,mut w,mut g)=fixture();g.clear();unchanged(u,w,g);}
#[test]fn missing_physical_cell(){let(mut u,mut w,mut g)=fixture();g[0].cells.pop();unchanged(u,w,g);}
#[test]fn wrong_header_row_count(){let(mut u,mut w,mut g)=fixture();g[0].ys.push(600.);unchanged(u,w,g);}
#[test]fn merged_cell_not_physical_column(){let(mut u,mut w,mut g)=fixture();g[0].cells[0].x1+=1;unchanged(u,w,g);}
#[test]fn header_without_cjk_label(){let(mut u,mut w,mut g)=fixture();w.iter_mut().filter(|w|w.y1<97.357&&w.x0>700.&&w.x1<780.).for_each(|w|w.text="123".into());unchanged(u,w,g);}
#[test]fn dash_noise_status(){let(mut u,mut w,mut g)=fixture();w[0].status=WordStatus::Noise;unchanged(u,w,g);}
#[test]fn dash_already_used(){let(mut u,mut w,mut g)=fixture();w[0].status=WordStatus::Used;unchanged(u,w,g);}
#[test]fn unknown_pua_source(){let(mut u,mut w,mut g)=fixture();w[0].raw="\u{e61d}".into();unchanged(u,w,g);}
#[test]fn unit_wide_dash_source(){let(mut u,mut w,mut g)=fixture();w[0].x0-=8.;unchanged(u,w,g);}
#[test]fn wrong_numeric_edge(){let(mut u,mut w,mut g)=fixture();w[0].x0-=2.;w[0].x1-=2.;unchanged(u,w,g);}
#[test]fn dash_in_label_column(){let(mut u,mut w,mut g)=fixture();w[0].x0=190.;w[0].x1=198.;unchanged(u,w,g);}
#[test]fn off_row_dash(){let(mut u,mut w,mut g)=fixture();w[0].y0+=4.;w[0].y1+=4.;unchanged(u,w,g);}
#[test]fn duplicate_dash_same_slot(){let(mut u,mut w,mut g)=fixture();let mut d=w[0].clone();d.id=w.len();w.push(d);unchanged(u,w,g);}
#[test]fn prose_in_numeric_column(){let(mut u,mut w,mut g)=fixture();w[755].text="説明".into();unchanged(u,w,g);}
#[test]fn missing_numeric_slot(){let(mut u,mut w,mut g)=fixture();w[755].x0=800.;w[755].x1=820.;unchanged(u,w,g);}
#[test]fn body_label_without_cjk(){let(mut u,mut w,mut g)=fixture();for i in [875,876]{w[i].text="123".into();}unchanged(u,w,g);}
#[test]fn owner_word_missing(){let(mut u,mut w,mut g)=fixture();if let Unit::Plain(ls)=&mut u[0]{ls[4].words.remove(2);}unchanged(u,w,g);}
#[test]fn duplicate_existing_owner(){let(mut u,mut w,mut g)=fixture();if let Unit::Plain(ls)=&mut u[0]{ls[4].words.push(755);}unchanged(u,w,g);}
#[test]fn two_plain_units(){let(mut u,mut w,mut g)=fixture();u.push(Unit::Plain(vec![]));unchanged(u,w,g);}
#[test]fn new_old_unit_boundary(){let(mut u,mut w,mut g)=fixture();u.push(Unit::Shinkyu{preamble:vec![],after:vec![],before:vec![],outside:vec![]});unchanged(u,w,g);}
#[test]fn text_source_not_matching(){let(mut u,mut w,mut g)=fixture();if let Unit::Plain(ls)=&mut u[0]{ls[7].text="別の本文".into();}unchanged(u,w,g);}
#[test]fn crossing_physical_rule(){let(mut u,mut w,mut g)=fixture();w[755].x0=210.;unchanged(u,w,g);}
#[test]fn numeric_word_is_margin(){let(mut u,mut w,mut g)=fixture();w[755].status=WordStatus::Margin;unchanged(u,w,g);}
#[test]fn largely_shifted_header(){let(mut u,mut w,mut g)=fixture();g[0].ys[1]=50.;unchanged(u,w,g);}

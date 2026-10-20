use crate::headed_data_columns as implementation;
use crate::vertical::Col;
fn fixture()->(Vec<Col>,Vec<Col>,Vec<(f32,f32,f32)>){
 let a:serde_json::Value=serde_json::from_str(include_str!("../tests/expected/headed_data_columns/20261020g002300011.json")).unwrap();let p=&a["pages"][0];
 let words:Vec<_>=p["words"].as_array().unwrap().iter().filter(|w|w["status"]=="used").map(|w|Col{id:w["id"].as_u64().unwrap()as usize,x:w["x0"].as_f64().unwrap()as f32,x1:w["x1"].as_f64().unwrap()as f32,y:w["y0"].as_f64().unwrap()as f32,y1:w["y1"].as_f64().unwrap()as f32,text:w["text"].as_str().unwrap().into()}).collect();
 let t=&p["regions"][0]["lines"][9]["table"];let keys:Vec<_>=(1..18).map(|row|{let id=t["cells"].as_array().unwrap().iter().find(|c|c["row"]==row&&c["col"]==0).unwrap()["words"][0].as_u64().unwrap()as usize;words.iter().find(|w|w.id==id).unwrap().clone()}).collect();
 (words,keys,vec![(425.,46.,549.)])
}
fn got(w:&[Col],k:&[Col],r:&[(f32,f32,f32)])->Vec<usize>{let mut v:Vec<_>=implementation::embedded_key_ids(w,k,r).into_iter().collect();v.sort();v}
fn yes(scale:f32,dx:f32,dy:f32){let(mut w,mut k,mut r)=fixture();for a in w.iter_mut().chain(k.iter_mut()){a.x=a.x*scale+dx;a.x1=a.x1*scale+dx;a.y=a.y*scale+dy;a.y1=a.y1*scale+dy;}for v in &mut r{v.0=v.0*scale+dx;v.1=v.1*scale+dy;v.2=v.2*scale+dy;}assert_eq!(got(&w,&k,&r),[194,195,210]);}
fn reject(f:impl FnOnce(&mut Vec<Col>,&mut Vec<Col>,&mut Vec<(f32,f32,f32)>)){let(mut w,mut k,mut r)=fixture();f(&mut w,&mut k,&mut r);assert!(got(&w,&k,&r).is_empty());}
#[test]fn real_source_wrapped_values_are_not_a_second_key_table(){yes(1.,0.,0.);}
#[test]fn translated_geometry(){yes(1.,30.,45.);}
#[test]fn scaled_geometry(){yes(1.25,0.,0.);}
#[test]fn no_explicit_rule_boundary(){reject(|_,_,r|r.clear());}
#[test]fn boundary_does_not_cover_source_rows(){reject(|_,_,r|r[0].2=530.);}
#[test]fn nearer_real_boundary_prevents_ownership(){reject(|_,_,r|r.push((165.,46.,549.)));}
#[test]fn no_original_headers(){reject(|w,_,_|w.retain(|w|w.y<201.||w.y>212.));}
#[test]fn no_source_key_header(){reject(|w,_,_|w.retain(|w|w.id!=248));}
#[test]fn no_candidate_column_header(){reject(|w,_,_|w.retain(|w|w.id!=193));}
#[test]fn numeric_candidate_header_is_not_text_header(){reject(|w,_,_|w.iter_mut().find(|w|w.id==193).unwrap().text="123".into());}
#[test]fn one_source_row_has_no_value(){reject(|w,_,_|w.retain(|w|w.id!=211));}
#[test]fn a_row_has_two_starting_values(){reject(|w,_,_|{let mut a=w.iter().find(|w|w.id==211).unwrap().clone();a.id=999;w.push(a);});}
#[test]fn a_value_starts_in_a_different_column(){reject(|w,_,_|{let a=w.iter_mut().find(|w|w.id==212).unwrap();a.x+=4.;a.x1+=4.;});}
#[test]fn following_column_does_not_align(){reject(|w,_,_|{let a=w.iter_mut().find(|w|w.id==176).unwrap();a.x+=8.;a.x1+=8.;});}
#[test]fn wrapped_key_crosses_column_edge(){reject(|w,_,_|w.iter_mut().find(|w|w.id==195).unwrap().x1=263.);}
#[test]fn wrapped_key_is_a_distant_new_line(){reject(|w,_,_|{let a=w.iter_mut().find(|w|w.id==195).unwrap();a.y+=5.;a.y1+=5.;});}
#[test]fn preceding_value_does_not_reach_wrap_edge(){reject(|w,_,_|w.iter_mut().find(|w|w.id==209).unwrap().x1=225.);}
#[test]fn only_one_wrapped_key_is_insufficient(){reject(|w,_,_|w.retain(|w|w.id!=194));}
#[test]fn three_independent_primary_keys_keep_their_fence(){reject(|w,_,_|{w.retain(|w|w.id!=199&&w.id!=209);for(id,y)in[(194,487.8511),(195,468.8511)]{let a=w.iter_mut().find(|w|w.id==id).unwrap();let h=a.y1-a.y;a.y=y;a.y1=y+h;}});}
#[test]fn duplicate_original_word_id(){reject(|w,_,_|{let a=w[0].clone();w.push(a);});}
#[test]fn duplicate_primary_key_id(){reject(|_,k,_|k[1].id=k[0].id);}
#[test]fn one_primary_key_is_not_finite(){reject(|_,k,_|k[0].x=f32::NAN);}
#[test]fn primary_row_font_size_is_different(){reject(|_,k,_|k[0].y1+=3.);}
#[test]fn primary_rows_are_not_separate_record_bands(){reject(|_,k,_|k[1].y=k[0].y+8.);}

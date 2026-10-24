use super::native_style_tests::{glyph,page};
use super::*;
fn word(p:&Page)->Word { words(p).remove(0) }
fn styled(p:&Page)->String { layout(std::iter::once((p,None,None)),true) }
#[test]fn original_baseline_and_true_first_advance_are_not_averaged_from_word_width(){
 let mut a=glyph("甲",10.,0,8.);a.end_x=13.;a.bbox[2]=13.;let mut b=glyph("乙",13.,0,8.);b.end_x=24.;b.bbox[2]=24.;let p=page(vec![a,b]);let w=word(&p);assert_eq!(w.glyphs,vec![0,1]);assert_eq!(native_flow_tuple(&p,&w),Some([10.,100.,24.,3.]));
}
#[test]fn original_flow_is_independent_of_font_mode_and_font_labels(){
 for vertical in [false,true]{let mut g=glyph("x",10.,0,8.);g.vertical=vertical;let mut p=page(vec![g]);p.fonts[0].name="unseen arbitrary font".into();assert_eq!(native_flow_tuple(&p,&word(&p)),Some([10.,100.,18.,8.]));}
}
#[test]fn baseline_drift_is_not_rounded_into_one_source_line(){
 let mut p=page(vec![glyph("甲",10.,0,8.),glyph("乙",18.,0,8.)]);let w=word(&p);assert_eq!(native_flow_tuple(&p,&w),Some([10.,100.,26.,8.]));p.glyphs[1].y+=1e-10;p.glyphs[1].end_y+=1e-10;assert_eq!(native_flow_tuple(&p,&w),None);
}
#[test]fn missing_duplicated_reversed_and_foreign_glyph_memberships_are_not_partial_flow(){
 let p=page(vec![glyph("甲",10.,0,8.),glyph("乙",18.,0,8.)]);let w=word(&p);for ids in [vec![],vec![0,0],vec![1,0],vec![0,99]]{let mut bad=w.clone();bad.glyphs=ids;assert_eq!(native_flow_tuple(&p,&bad),None);}
}
#[test]fn changed_word_text_or_original_box_cannot_borrow_native_origins(){
 let p=page(vec![glyph("甲",10.,0,8.)]);let w=word(&p);for k in 0..5{let mut bad=w.clone();match k{0=>bad.text="乙".into(),1=>bad.x_min-=1.,2=>bad.x_max+=1.,3=>bad.y_min-=1.,_=>bad.y_max+=1.};assert_eq!(native_flow_tuple(&p,&bad),None);}
}
#[test]fn invisible_invalid_and_non_right_advances_supply_no_flow(){
 for k in 0..9{let mut g=glyph("甲",10.,0,8.);match k{0=>g.invisible=true,1=>g.x=f64::NAN,2=>g.y=f64::INFINITY,3=>g.end_x=g.x,4=>g.end_x=g.x-8.,5=>g.end_y+=8.,6=>g.size=0.,7=>g.size=f64::NAN,_=>g.font=99};let p=page(vec![g]);let w=Word{text:"甲".into(),x_min:10.,y_min:93.,x_max:18.,y_max:102.,glyphs:vec![0]};assert_eq!(native_flow_tuple(&p,&w),None);}
}
#[test]fn independent_scaling_translation_and_f64_source_precision_are_preserved(){
 for (sx,sy,dx,dy)in[(1.,1.,0.,0.),(0.125,64.,32.,-64.),(64.,0.125,-32.,64.)]{let mut g=glyph("任",10.,0,8.);g.y=100.00000000001;g.end_y=g.y;g.x=g.x*sx+dx;g.end_x=g.end_x*sx+dx;g.y=g.y*sy+dy;g.end_y=g.y;g.size*=sy;g.bbox=[g.bbox[0]*sx+dx,g.bbox[1]*sy+dy,g.bbox[2]*sx+dx,g.bbox[3]*sy+dy];let expected=[g.x,g.y,g.end_x,g.end_x-g.x];let p=page(vec![g]);assert_eq!(native_flow_tuple(&p,&word(&p)),Some(expected));}
}
#[test]fn identical_words_keep_their_own_source_baselines_without_a_line_count_cap(){
 for count in [1,3,29]{let gs=(0..count).map(|i|{let mut g=glyph("同",10.,0,8.);g.y+=20.*i as f64;g.end_y=g.y;g.bbox[1]+=20.*i as f64;g.bbox[3]+=20.*i as f64;g}).collect();let p=page(gs);let ws=words(&p);assert_eq!(ws.len(),count);for(i,w)in ws.iter().enumerate(){assert_eq!(native_flow_tuple(&p,w),Some([10.,100.+20.*i as f64,18.,8.]));}}
}
#[test]fn only_the_opt_in_source_renderer_adds_complete_flow_metadata(){
 let p=page(vec![glyph("甲",10.,0,8.)]);let legacy=bbox_layout(&[p.clone()]);assert!(!legacy.contains("data-native-flow-"));let s=styled(&p);for (key,value)in[("start",10.),("baseline",100.),("end",18.),("leading",8.)]{assert!(s.contains(&format!("data-native-flow-{key}=\"{value}\"")),"complete source flow metadata missing: {s}");}
}
#[test]fn source_tracking_and_first_TJ_adjustment_are_not_silently_assumed_zero(){let mut g=glyph("甲",10.,0,8.);g.spacing=-4.;g.kern=-2.;let p=page(vec![g]);let s=styled(&p);assert!(s.contains("data-native-flow-trailing=\"-4\""));assert!(s.contains("data-native-flow-prefix=\"-2\""));}
#[test]fn nonfinite_tracking_and_kerning_do_not_certify_word_flow(){for k in 0..4{let mut g=glyph("甲",10.,0,8.);match k{0=>g.spacing=f64::NAN,1=>g.spacing=f64::INFINITY,2=>g.kern=f64::NAN,_=>g.kern=f64::NEG_INFINITY};let p=page(vec![g]);assert_eq!(native_flow_tuple(&p,&word(&p)),None);}}

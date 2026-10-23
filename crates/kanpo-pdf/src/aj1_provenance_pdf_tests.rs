use super::*;
fn sample(ordering:&str,cid:u32,text:&str,has_to_unicode:bool)->Page{
 Page{width:800.,height:600.,rotate:0,glyphs:vec![Glyph{text:text.into(),code:cid,cid,font:0,x:100.,y:100.,end_x:108.,end_y:100.,size:8.,vertical:false,bbox:[100.,94.,108.,102.],invisible:false,spacing:0.,kern:0.,show:0}],paths:vec![],fonts:vec![FontInfo{name:"Test".into(),vertical:false,ordering:Some(ordering.into()),has_to_unicode}]}
}
fn marked(p:Page)->bool{bbox::bbox_layout(&[p]).contains("data-unicode-aj1=\"1\"")}
#[test]fn resolved_aj1_CID1657_literal_unicode_is_marked(){assert!(marked(sample("Japan1",1657,"救",false)));}
#[test]fn valid_AJ1_with_matching_partial_ToUnicode_is_still_proven(){assert!(marked(sample("Japan1",1657,"救",true)));}
#[test]fn unresolved_CID_missing_AJ1_mapping_has_no_marker(){assert!(!marked(sample("Japan1",65535,"救",false)));}
#[test]fn mismatching_partial_ToUnicode_text_has_no_marker(){assert!(!marked(sample("Japan1",1657,"（チ）",true)));}
#[test]fn Identity_same_Unicode_codepoint_is_not_AJ1_resolution(){assert!(!marked(sample("Identity",25937,"救",false)));}
#[test]fn Identity_ToUnicode_bool_does_not_prove_a_specific_code(){assert!(!marked(sample("Identity",25937,"救",true)));}
#[test]fn true_chi_PUA_and_tagged_identity_stay_unmarked(){for tu in[false,true]{assert!(!marked(sample("Identity",25937,if tu{"\u{e9f5}"}else{"救"},tu)));}}
#[test]fn mixed_fonts_keep_individual_Word_provenance(){let mut p=sample("Japan1",1657,"救",false);p.fonts.push(FontInfo{name:"Identity".into(),vertical:false,ordering:Some("Identity".into()),has_to_unicode:false});let mut g=p.glyphs[0].clone();g.font=1;g.cid=25937;g.code=25937;g.x=120.;g.end_x=128.;g.bbox=[120.,94.,128.,102.];p.glyphs.push(g);let html=bbox::bbox_layout(&[p]);assert_eq!(html.matches("data-unicode-aj1=\"1\"").count(),1);assert!(html.contains(char::from_u32(bbox::IDENTITY_CID_BASE+25937).unwrap()));}

fn adjacent(mut p:Page,cid:u32,text:&str,font:usize)->Page {
 let mut g=p.glyphs[0].clone();g.cid=cid;g.code=cid;g.text=text.into();g.font=font;
 g.x=108.;g.end_x=116.;g.bbox=[108.,94.,116.,102.];p.glyphs.push(g);p
}
#[test]fn all_Glyphs_of_one_Word_must_have_matching_AJ1_mappings(){
 let p=adjacent(sample("Japan1",1657,"救",true),1424,"害",0);
 let ws=bbox::words(&p);assert_eq!(ws.len(),1);assert_eq!(ws[0].glyphs,vec![0,1]);
 assert!(marked(p));
}
#[test]fn partially_resolved_one_Word_never_gets_the_marker(){
 for(cid,text)in[(65535,"害"),(1424,"救")]{
  let p=adjacent(sample("Japan1",1657,"救",true),cid,text,0);
  let ws=bbox::words(&p);assert_eq!(ws.len(),1);assert_eq!(ws[0].glyphs,vec![0,1]);
  assert!(!marked(p));
 }
}
#[test]fn one_Word_with_a_second_Identity_font_is_not_AJ1_proven(){
 let mut p=sample("Japan1",1657,"救",false);
 p.fonts.push(FontInfo{name:"Identity".into(),vertical:false,ordering:Some("Identity".into()),has_to_unicode:false});
 let p=adjacent(p,25937,"救",1);let ws=bbox::words(&p);
 assert_eq!(ws.len(),1);assert_eq!(ws[0].glyphs,vec![0,1]);assert!(!marked(p));
}

extern crate kanpo_parser;
mod grid {pub use kanpo_parser::grid::*;}
mod lines {pub use kanpo_parser::lines::*;}
mod trace {
 pub use kanpo_parser::trace::{Line,Word,WordStatus};
 #[derive(Debug,Clone)]pub(crate) enum Unit {Plain(Vec<Line>),Shinkyu{preamble:Vec<Line>,after:Vec<Vec<Line>>,before:Vec<Vec<Line>>,outside:Vec<Vec<Line>>}}
}
#[path = "../src/framed_band_continuations.rs"]
mod candidate;
use trace::*;use lines::*;
fn w(id:usize,text:&str,x:f32,y:f32,len:f32)->Word{Word{source_box: [x, y, x+8., y+len],id,x0:x,x1:x+8.,y0:y,y1:y+len,raw:text.into(),text:text.into(),status:WordStatus::Used}}
fn fixture()->(Vec<Unit>,Vec<Word>,PageRules){
 let mut words=Vec::new();let mut after=Vec::new();let mut tail=Vec::new();
 for (i,text)in["（項目）","第一項の規定","本文その二","本文その三","本文その四","（条件（内側"].iter().enumerate(){words.push(w(i,text,106.-i as f32*12.,83.,152.));after.push(Line::text(text.to_string(),vec![i]));}
 words.push(w(6,"（削除）",82.,250.,32.));
 for(i,text)in["続きの本文","内側）を続け","本文その五","本文その六","外側）を続け","終わり。"].iter().enumerate(){let id=7+i;words.push(w(id,text,537.-i as f32*11.4,451.,152.));tail.push(Line::text(text.to_string(),vec![id]));}
 words.push(w(13,"次の独立本文",455.,437.,168.));tail.push(Line::text("次の独立本文".into(),vec![13]));
 for i in 0..6{let mut v=w(14+i,["改","正","後","改","正","前"][i],123.,100.+(i%3)as f32*16.+(i/3)as f32*171.5,8.);v.status=WordStatus::HeaderLabel;words.push(v);}
 let u=vec![Unit::Shinkyu{preamble:vec![],after:vec![after],before:vec![vec![Line::text("（削除）".into(),vec![6])]],outside:vec![]},Unit::Plain(tail)];
 let mut r=PageRules::default();for (l,m,b,t,z) in [(46.,118.6,135.7,69.5,412.5),(476.5,548.75,548.75,437.5,780.5)]{for y in[t,(t+z)/2.,z]{r.horizontal.push(HRule{y,x0:l,x1:b});}for x in[l,m,b]{r.vertical.push(VRule{x,y0:t,y1:z});}}
 r.horizontal.push(HRule{y:425.,x0:46.,x1:549.});(u,words,r)
}
fn apply(u:&mut [Unit],w:&[Word],r:&PageRules){candidate::restore(u,w,Some(r),595.,842.);}
fn no(f:impl FnOnce(&mut Vec<Unit>,&mut Vec<Word>,&mut PageRules)){let(mut u,mut w,mut r)=fixture();f(&mut u,&mut w,&mut r);let old=format!("{u:?}");apply(&mut u,&w,&r);assert_eq!(format!("{u:?}"),old);}
#[test]fn identical_empty_opposite_frame_and_two_crossing_parentheses_move_original_six_lines(){let(mut u,w,r)=fixture();let original=match &u[1]{Unit::Plain(v)=>v[..6].to_vec(),_=>unreachable!()};apply(&mut u,&w,&r);let Unit::Shinkyu{after,before,..}=&u[0]else{panic!()};assert_eq!(after.len(),2);assert_eq!(after[1],original);assert!(before[1].is_empty());let Unit::Plain(rest)=&u[1]else{panic!()};assert_eq!(rest.len(),1);assert_eq!(rest[0].words,[13]);}
#[test]fn unrelated_complete_paragraph_is_not_a_continuation(){no(|u,w,_|{w[5].text="完結した本文".into();if let Unit::Shinkyu{after,..}=&mut u[0]{after[0][5].text=w[5].text.clone();}});}
#[test]fn unmatched_closing_bracket_does_not_prove_a_wrap(){no(|u,w,_|{w[8].text="別の種類」".into();if let Unit::Plain(t)=&mut u[1]{t[1].text=w[8].text.clone();}});}
#[test]fn open_bracket_that_never_closes_is_not_a_cell_pair(){no(|u,w,_|{w[11].text="未完の条件".into();if let Unit::Plain(t)=&mut u[1]{t[4].text=w[11].text.clone();}});}
#[test]fn different_article_heading_inside_frame_blocks_join(){no(|u,w,_|{w[7].text="〇別告示".into();if let Unit::Plain(t)=&mut u[1]{t[0].text=w[7].text.clone();}});}
#[test]fn before_content_in_the_continuation_frame_is_not_empty(){no(|_,ws,_|ws.push(w(90,"別条文",510.,640.,80.)));}
#[test]fn empty_status_raw_word_in_opposite_cell_also_blocks_join(){no(|_,ws,_|{let mut q=w(90,"傍線",510.,640.,80.);q.text.clear();q.status=WordStatus::Empty;ws.push(q);});}
#[test]fn word_crossing_cell_edge_blocks_join(){no(|_,w,_|w[7].x1=551.);}
#[test]fn word_on_divider_is_not_silently_excluded(){no(|_,ws,_|ws.push(w(90,"境界",510.,603.,16.)));}
#[test]fn unowned_word_in_first_cell_blocks_join(){no(|_,ws,_|ws.push(w(90,"別",75.,120.,32.)));}
#[test]fn an_extra_line_in_the_frame_is_not_lost(){no(|u,w,_|{let q=w[7].clone();w.push(Word{id:90,..q});if let Unit::Plain(t)=&mut u[1]{t.push(Line::text(w.last().unwrap().text.clone(),vec![90]));}});}
#[test]fn duplicate_existing_owner_blocks_join(){no(|u,_,_|{if let Unit::Plain(t)=&mut u[1]{t.push(t[0].clone());}});}
#[test]fn duplicate_word_id_blocks_join(){no(|_,w,_|w.push(w[0].clone()));}
#[test]fn margin_word_cannot_anchor_the_first_cell(){no(|_,w,_|w[0].status=WordStatus::Margin);}
#[test]fn unused_tail_word_cannot_anchor_the_wrap(){no(|_,w,_|w[7].status=WordStatus::Empty);}
#[test]fn mismatched_line_text_is_never_rebuilt(){no(|u,_,_|{if let Unit::Plain(t)=&mut u[1]{t[0].text.push('別');}});}
#[test]fn horizontal_word_is_not_a_vertical_column(){no(|_,w,_|w[7].y1=w[7].y0+4.);}
#[test]fn different_letter_size_blocks_join(){no(|_,w,_|w[7].x1=w[7].x0+6.);}
#[test]fn nonuniform_column_geometry_blocks_join(){no(|_,w,_|{w[8].x0-=8.;w[8].x1-=8.;});}
#[test]fn differing_band_height_blocks_join(){no(|_,_,r|{for h in &mut r.horizontal{if h.y==609.{h.y+=8.;}}});}
#[test]fn differing_frame_width_blocks_join(){no(|_,_,r|{for h in &mut r.horizontal{if h.y>430.{h.x0-=8.;}}for v in &mut r.vertical{if v.y0>430.&&v.x==476.5{v.x-=8.;}}});}
#[test]fn missing_middle_rule_blocks_join(){no(|_,_,r|r.horizontal.retain(|h|h.y!=609.));}
#[test]fn missing_tier_separator_blocks_join(){no(|_,_,r|r.horizontal.retain(|h|h.y!=425.));}
#[test]fn continuation_in_another_column_does_not_join(){no(|_,_,r|{for h in &mut r.horizontal{if h.y>430.{h.x0-=20.;h.x1-=20.;}}for v in &mut r.vertical{if v.y0>430.{v.x-=20.;}}});}
#[test]fn missing_label_word_blocks_join(){no(|_,w,_|w.retain(|w|w.id<14||w.id>=17));}
#[test]fn ordinary_plain_paragraphs_do_not_become_new_old_cells(){no(|u,_,_|u[0]=Unit::Plain(vec![]));}
#[test]fn short_before_prose_is_not_a_parenthesized_change_mark(){no(|u,_,_|{if let Unit::Shinkyu{before,..}=&mut u[0]{before[0][0].text="別の本文".into();}});}
#[test]fn existing_outside_article_blocks_cross_unit_absorption(){no(|u,_,_|{if let Unit::Shinkyu{outside,..}=&mut u[0]{outside.push(vec![Line::text("別記事".into(),vec![])]);}});}
#[test]fn no_rule_metadata_is_an_exact_noop(){let(mut u,w,_)=fixture();let old=format!("{u:?}");candidate::restore(&mut u,&w,None,595.,842.);assert_eq!(format!("{u:?}"),old);}

#[test]fn runtime_unowned_used_header_letters_are_proven_before_mark_unused(){let(mut u,mut w,r)=fixture();for v in w.iter_mut().filter(|v|v.id>=14){v.status=WordStatus::Used;}apply(&mut u,&w,&r);let Unit::Shinkyu{after,..}=&u[0]else{panic!()};assert_eq!(after.len(),2);}
#[test]fn runtime_owned_label_word_is_not_a_consumed_shinkyu_header(){no(|u,w,_|{w[14].status=WordStatus::Used;if let Unit::Plain(t)=&mut u[1]{t.push(Line::text(w[14].text.clone(),vec![14]));}});}
#[test]fn unowned_but_different_header_text_does_not_prove_new_old(){no(|_,w,_|w[16].text="別".into());}
#[test]fn noise_or_margin_header_cannot_anchor_a_frame(){no(|_,w,_|w[16].status=WordStatus::Noise);}

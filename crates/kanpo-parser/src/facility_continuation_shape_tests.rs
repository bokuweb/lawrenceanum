use super::*;
use crate::lines::HRule;
struct Case { units:Vec<Unit>, words:Vec<Word>, rules:PageRules }
fn add(words:&mut Vec<Word>, text:&str,x:f32,y:f32,end:f32)->usize {
 let id=words.len();words.push(Word{id,x0:x,y0:y,x1:x+8.0,y1:end,raw:text.into(),text:text.into(),status:WordStatus::Used});id
}
fn line(words:&[Word], ids:Vec<usize>)->Line {Line::text(ids.iter().map(|id|words[*id].text.as_str()).collect(),ids)}
fn case()->Case {
 let mut words=Vec::new();let mut headers=Vec::new();
 for (t,y,z) in [("施設番号",60.,92.),("施設名",108.,156.),("所在地名",180.,236.),("所有関係",252.,284.),("摘要",308.,404.)] {headers.push(add(&mut words,t,120.,y,z));}
 let h=line(&words,headers);let prev=add(&mut words,"一〇七六",108.,60.,92.);
 let mut bands:Vec<Vec<Line>>=(0..4).map(|_|Vec::new()).collect();
 for (r,(number,name,city)) in [("三〇八三","第一施設","第一市"),("五一二七","第二施設","第二市")].into_iter().enumerate() {
  let x=544.-r as f32*32.;
  let n=add(&mut words,number,x,428.,460.);let a=add(&mut words,name,x,468.,524.);
  bands[0].push(line(&words,vec![n,a]));
  let city=add(&mut words,city,x,540.,564.);bands[1].push(line(&words,vec![city]));
  let owner=add(&mut words,"国有",x,620.,636.);bands[2].push(line(&words,vec![owner]));
 }
 for (text,x,y,end) in [("工作物：門",544.,660.,708.),("同じ施設の備考",536.,660.,788.),("建物：一式",512.,660.,708.),("次の施設の備考",504.,660.,788.)] {
  let id=add(&mut words,text,x,y,end);bands[3].push(line(&words,vec![id]));
 }
 let mut units=vec![Unit::Plain(vec![h,line(&words,vec![prev])])];units.extend(bands.into_iter().map(Unit::Plain));
 Case{units,words,rules:PageRules{horizontal:vec![HRule{y:420.,x0:40.,x1:560.},HRule{y:800.,x0:40.,x1:560.}],..Default::default()}}
}
fn view(us:&[Unit])->Vec<String>{us.iter().map(Unit::text).collect()}
fn no_change(mut c:Case){let was=view(&c.units);restore(&mut c.units,&c.words,Some(&c.rules),600.,840.);assert_eq!(view(&c.units),was);}
#[test]fn pair_restores_five_fields_without_touching_source_words_or_ids(){
 let mut c=case();let ids=ownership(&c.units);let ws=format!("{:?}",c.words);
 restore(&mut c.units,&c.words,Some(&c.rules),600.,840.);assert_eq!(c.units.len(),2);
 let Unit::Plain(ls)=&c.units[1] else {panic!()};assert_eq!(ls.len(),2);
 assert_eq!(ls[0].text,"三〇八三　第一施設　第一市　国有　工作物：門同じ施設の備考");
 assert_eq!(ls[1].text,"五一二七　第二施設　第二市　国有　建物：一式次の施設の備考");
 assert_eq!(ownership(&c.units),ids);assert_eq!(format!("{:?}",c.words),ws);
}
#[test]fn physical_wrap_beyond_record_midpoint_stays_with_prior_record(){
 let mut c=case();let id=match &c.units[4]{Unit::Plain(ls)=>ls[1].words[0],_=>unreachable!()};
 c.words[id].x0=524.;c.words[id].x1=532.;
 restore(&mut c.units,&c.words,Some(&c.rules),600.,840.);assert_eq!(c.units.len(),2);
 let Unit::Plain(ls)=&c.units[1] else{panic!()};assert!(ls[0].text.ends_with("同じ施設の備考"));assert!(!ls[1].text.contains("同じ施設"));
}
#[test]fn wrong_or_missing_heading_is_not_numeric_table_evidence(){let mut c=case();c.words[0].text="価格番号".into();if let Unit::Plain(ls)=&mut c.units[0]{ls[0].text=ls[0].text.replace("施設番号","価格番号");}no_change(c);}
#[test]fn repeated_numeric_identifier_is_ambiguous(){let mut c=case();c.words[10].text="三〇八三".into();if let Unit::Plain(ls)=&mut c.units[1]{ls[1].text="三〇八三第二施設".into();}no_change(c);}
#[test]fn mismatched_city_lane_is_rejected(){let mut c=case();c.words[8].x0-=10.;c.words[8].x1-=10.;no_change(c);}
#[test]fn crossing_summary_lane_is_rejected(){let mut c=case();let id=match &c.units[4]{Unit::Plain(ls)=>ls[3].words[0],_=>unreachable!()};c.words[id].x0=532.;c.words[id].x1=540.;no_change(c);}
#[test]fn margin_or_empty_numeric_anchor_is_not_evidence(){for status in [WordStatus::Margin,WordStatus::Empty,WordStatus::Noise]{let mut c=case();c.words[6].status=status;no_change(c);}}
#[test]fn foreign_used_word_inside_band_is_rejected(){let mut c=case();let id=add(&mut c.words,"別記事",528.,540.,564.);c.units.push(Unit::Plain(vec![line(&c.words,vec![id])]));no_change(c);}
#[test]fn duplicate_owner_is_rejected(){let mut c=case();c.units.push(Unit::Plain(vec![line(&c.words,vec![6])]));no_change(c);}
#[test]fn missing_real_divider_is_rejected(){let mut c=case();c.rules.horizontal.remove(0);no_change(c);}
#[test]fn intervening_article_or_second_header_is_rejected(){
 let mut c=case();let id=add(&mut c.words,"〇別の記事",80.,60.,92.);if let Unit::Plain(ls)=&mut c.units[0]{ls.push(line(&c.words,vec![id]));}no_change(c);
 let mut c=case();let ids=vec![0,1,2,3,4];let l=line(&c.words,ids);if let Unit::Plain(ls)=&mut c.units[0]{ls.push(l);}no_change(c);
}
#[test]fn internal_vertical_separator_is_a_different_table(){let mut c=case();c.rules.vertical.push(crate::lines::VRule{x:530.,y0:420.,y1:800.});no_change(c);}
#[test]fn field_start_offset_must_agree_with_source_header(){let mut c=case();c.words[8].y0+=32.;c.words[8].y1+=32.;c.words[12].y0+=32.;c.words[12].y1+=32.;no_change(c);}
#[test]fn sideways_or_unruled_page_is_not_an_upright_continuation(){let mut c=case();let was=view(&c.units);restore(&mut c.units,&c.words,Some(&c.rules),840.,600.);assert_eq!(view(&c.units),was);restore(&mut c.units,&c.words,None,600.,840.);assert_eq!(view(&c.units),was);}

#[test]fn another_article_title_inside_remarks_is_not_part_of_a_facility(){let mut c=case();let id=match &c.units[4]{Unit::Plain(ls)=>ls[3].words[0],_=>unreachable!()};c.words[id].text="〇別記事の題名".into();if let Unit::Plain(ls)=&mut c.units[4]{ls[3].text="〇別記事の題名".into();}no_change(c);}
#[test]fn duplicate_facility_name_is_not_two_distinct_records(){let mut c=case();c.words[11].text="第一施設".into();if let Unit::Plain(ls)=&mut c.units[1]{ls[1].text="五一二七第一施設".into();}no_change(c);}

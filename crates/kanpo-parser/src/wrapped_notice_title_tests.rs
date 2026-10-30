
 use super::*;use crate::trace::*;
 fn source()->(Vec<Word>,Vec<Region>){
 let data=[("年度業務実績及び財務諸表に関する",100.,50.,235.,59.),("公告",100.,59.,118.,68.),("次の規定に基づき、次のとおり財務諸表を公告します。",90.,72.,330.,80.)];
 let words=data.iter().enumerate().map(|(id,(t,x,y,r,b))|Word{source_box: [*x, *y, *r, *b],id,x0:*x,y0:*y,x1:*r,y1:*b,raw:t.to_string(),text:t.to_string(),status:WordStatus::Used}).collect();
 let lines=data.iter().enumerate().map(|(id,(t,..))|Line::text(t.to_string(),vec![id])).collect();
 (words,vec![Region{role:Role::Body,unit:0,part:0,lines}]) }
 fn no(f:impl FnOnce(&mut Vec<Word>,&mut Vec<Region>)){let(mut w,mut r)=source();f(&mut w,&mut r);assert!(prefixes(&w,&r,"公告").is_empty());}
 #[test]fn separate_horizontal_columns_are_not_one_title(){no(|w,_|{w[1].x0+=100.;w[1].x1+=100.;});}
 #[test]fn large_vertical_gap_is_not_a_title_wrap(){no(|w,_|{w[1].y0+=30.;w[1].y1+=30.;});}
 #[test]fn different_regions_are_not_joined(){no(|_,r|{let end=r[0].lines.split_off(1);r.push(Region{role:Role::Body,unit:1,part:0,lines:end});});}
 #[test]fn table_cell_title_is_not_a_plain_heading(){no(|_,r|{r[0].lines[0].table=Some(Table{x0:0.,y0:0.,x1:1.,y1:1.,rows:1,cols:1,cells:vec![]});});}
 #[test]fn ordinary_body_size_does_not_prove_a_heading(){no(|w,_|{w[2].y1=81.;});}
 #[test]fn unused_word_cannot_anchor_a_title(){no(|w,_|w[0].status=WordStatus::Margin);}
 #[test]fn duplicate_source_id_is_ambiguous(){no(|w,_|w.push(w[0].clone()));}
 #[test]fn duplicate_logical_owner_is_ambiguous(){no(|_,r|{let l=r[0].lines[0].clone();r[0].lines.push(l);});}
 #[test]fn source_text_mismatch_is_not_reconstructed(){no(|w,_|w[0].text.push('別'));}
 #[test]fn punctuation_in_body_continuation_is_not_a_title(){no(|w,r|{w[0].text.push('。');r[0].lines[0].text.push('。');});}
 #[test]fn vertical_word_is_not_a_horizontal_title(){no(|w,_|{w[1].x1=108.;w[1].y1=77.;});}
 #[test]fn competing_word_in_title_rectangle_blocks_repair(){no(|w,_|{let mut a=w[0].clone();a.id=3;a.text="別".into();w.push(a);});}
 #[test]fn body_of_another_column_blocks_repair(){no(|w,_|w[2].x0=101.);}
 #[test]fn missing_title_word_is_not_recovered(){no(|w,_|{w.remove(0);});}

 #[test]fn three_original_title_lines_are_supported(){let(mut w,mut r)=source();w[0].text="年度業務実績及び".into();w[0].raw=w[0].text.clone();w[0].x1=181.;r[0].lines[0].text=w[0].text.clone();let mut next=w[0].clone();next.id=3;next.text="令和九年度財務諸表に関する".into();next.raw=next.text.clone();next.y0+=9.;next.y1+=9.;w.push(next);r[0].lines.insert(1,Line::text("令和九年度財務諸表に関する".into(),vec![3]));w[1].y0+=9.;w[1].y1+=9.;w[2].y0+=9.;w[2].y1+=9.;assert_eq!(prefixes(&w,&r,"公告"),vec!["年度業務実績及び\n令和九年度財務諸表に関する"]);}
 #[test]fn new_old_table_region_is_not_a_body_notice(){no(|_,r|r[0].role=Role::After);}
 #[test]fn aligned_larger_original_title_is_a_candidate(){let(w,r)=source();assert_eq!(prefixes(&w,&r,"公告"),vec!["年度業務実績及び財務諸表に関する"]);}

 // A notice title set in its own original font, after the previous notice's last line.
 fn typeset(lines:&[(&str,usize)])->PageTrace{
  let mut words=vec![];let mut out=vec![];let mut styles=vec![];let mut bounds=vec![];let mut content=vec![];
  for(i,(t,font))in lines.iter().enumerate(){let y=50.+10.*i as f32;let x=if i==0{110.}else{100.};let r=x+8.*t.chars().count() as f32;words.push(Word{source_box:[x,y,r,y+8.],id:i,x0:x,y0:y,x1:r,y1:y+8.,raw:t.to_string(),text:t.to_string(),status:WordStatus::Used});out.push(Line::text(t.to_string(),vec![i]));let s:NativeStyles=[(*font,8)].into_iter().collect();styles.push(Some(s.clone()));bounds.push(NativeBounds::Glyphs([x,y,r,y+8.]));content.push(ContentStyles::Glyphs(s));}
  let axes=vec![crate::native_word_axes::NativeAxis::Right;words.len()];
  PageTrace{native_comparison_labels:vec![],native_horizontal_baselines:vec![],native_atomic_columns:vec![],native_axes:axes,native_rulings:vec![],width:600.,height:800.,layout:"tiers",words,native_styles:styles,native_bounds:bounds,native_content_styles:content,regions:vec![Region{role:Role::Body,unit:0,part:0,lines:out}],text:String::new(),doc_text:String::new(),tables:vec![]}
 }
 const REPORT:(&str,usize)=("官庁報告",3);
 const PREVIOUS:(&str,usize)=("三変更年月日令和八年四月一日",1);
 const BODY:(&str,usize)=("船舶職員及び小型船舶操縦者法（昭和二十六年法律第百四十九号）第十七条の五の規定に基づき",1);
 fn split(lines:&[(&str,usize)],articles:&[&str])->Vec<String>{let page=typeset(lines);let mut a:Vec<String>=articles.iter().map(|s|s.to_string()).collect();split_typeset_notice_titles(&mut a,&[page],&[Some(crate::lines::PageRules::default())]);a}
 #[test]fn wrapped_title_in_its_own_font_starts_an_article(){
  let lines=[REPORT,PREVIOUS,("登録海技免許講習実施機関の登録事項の変更に関",0),("する公示",0),BODY];
  let joined=lines.iter().map(|l|l.0).collect::<Vec<_>>().join("\n");
  assert_eq!(split(&lines,&[&joined]),vec![format!("{}\n{}",REPORT.0,PREVIOUS.0),lines[2..].iter().map(|l|l.0).collect::<Vec<_>>().join("\n")]);
 }
 #[test]fn label_split_inside_a_title_moves_to_the_title_start(){
  let lines=[PREVIOUS,("登録船舶職員養成施設の登録事項の変更に関する",0),("公示",0),BODY];
  let got=split(&lines,&[&format!("{}\n{}",PREVIOUS.0,lines[1].0),&format!("公示\n{}",BODY.0)]);
  assert_eq!(got,vec![PREVIOUS.0.to_string(),format!("{}\n公示\n{}",lines[1].0,BODY.0)]);
 }
 #[test]fn three_title_lines_start_at_the_first(){
  let lines=[REPORT,PREVIOUS,("一般社団法人及び一般財団法",0),("人に関する法律第百四十九条第一",0),("項の届出に関する公告",0),BODY];
  let joined=lines.iter().map(|l|l.0).collect::<Vec<_>>().join("\n");
  assert_eq!(split(&lines,&[&joined])[1].lines().next(),Some(lines[2].0));
 }
 #[test]fn body_font_line_is_not_a_title(){
  let lines=[REPORT,PREVIOUS,("登録海技免許講習実施機関の登録事項の変更に関",1),("する公示",1),BODY];
  let joined=lines.iter().map(|l|l.0).collect::<Vec<_>>().join("\n");
  assert_eq!(split(&lines,&[&joined]).len(),1);
 }
 #[test]fn title_under_a_field_label_stays_with_the_label(){
  let lines=[("官庁事項",2),("登録海技免許講習実施機関の登録事項の変更に関",0),("する公示",0),BODY];
  let joined=lines.iter().map(|l|l.0).collect::<Vec<_>>().join("\n");
  assert_eq!(split(&lines,&[&joined]).len(),1);
 }
 #[test]fn repeated_title_text_is_ambiguous(){
  let lines=[REPORT,PREVIOUS,("登録海技免許講習実施機関の登録事項の変更に関",0),("する公示",0),BODY];
  let joined=lines.iter().map(|l|l.0).collect::<Vec<_>>().join("\n");
  assert_eq!(split(&lines,&[&joined,&joined]).len(),2);
 }
 #[test]fn titles_in_the_public_notice_section_are_not_split(){
  // The table of contents groups several notices of the 公告 section into one item.
  let lines=[REPORT,("公告",3),("諸事項",2),PREVIOUS,("登録を受けたクレジットカード番号等取扱契約締結事業者の営業の",0),("廃止に関する公示",0),BODY];
  let joined=lines.iter().map(|l|l.0).collect::<Vec<_>>().join("\n");
  assert_eq!(split(&lines,&[&joined]).len(),1);
 }
 #[test]fn one_line_title_ending_in_the_bare_notice_noun_starts_an_article(){
  let lines=[REPORT,("（七）図面縦覧場所四国地方整備局及び同局香川河川国道事務所",1),("基本測量関係事項公告",0),BODY];
  let joined=lines.iter().map(|l|l.0).collect::<Vec<_>>().join("\n");
  assert_eq!(split(&lines,&[&joined])[1].lines().next(),Some("基本測量関係事項公告"));
  // A standalone section label is not a title.
  let lines=[REPORT,PREVIOUS,("公示",0),BODY];
  let joined=lines.iter().map(|l|l.0).collect::<Vec<_>>().join("\n");
  assert_eq!(split(&lines,&[&joined]).len(),1);
 }
 #[test]fn unknown_native_evidence_does_not_prove_a_title(){
  let lines=[REPORT,PREVIOUS,("登録海技免許講習実施機関の登録事項の変更に関",0),("する公示",0),BODY];
  let joined=lines.iter().map(|l|l.0).collect::<Vec<_>>().join("\n");
  for spoil in 0..3 {
   let mut page=typeset(&lines);
   match spoil {0=>page.native_axes[2]=crate::native_word_axes::NativeAxis::Unknown,1=>page.native_bounds[2]=NativeBounds::Compatibility,_=>page.native_content_styles[2]=ContentStyles::Unknown}
   let mut a=vec![joined.clone()];split_typeset_notice_titles(&mut a,&[page],&[Some(crate::lines::PageRules::default())]);assert_eq!(a.len(),1,"{spoil}");
  }
 }
 #[test]fn vertical_title_columns_align_on_their_top_edge(){
  let lines=[REPORT,PREVIOUS,("登録海技免許講習実施機関の登録事項の変更に関",0),("する公示",0),BODY];
  let mut page=typeset(&lines);
  // Columns run right to left; each title column starts at the same top edge.
  for(i,(t,_))in lines.iter().enumerate(){let x=500.-10.*i as f32;let y=if i==1{60.}else{50.};page.native_bounds[i]=NativeBounds::Glyphs([x,y,x+8.,y+8.*t.chars().count() as f32]);page.native_axes[i]=crate::native_word_axes::NativeAxis::Down;}
  let joined=lines.iter().map(|l|l.0).collect::<Vec<_>>().join("\n");
  let mut a=vec![joined];split_typeset_notice_titles(&mut a,&[page],&[Some(crate::lines::PageRules::default())]);
  assert_eq!(a[1].lines().next(),Some(lines[2].0));
 }
 #[test]fn the_public_notice_field_without_its_section_label_is_not_split(){
  let lines=[REPORT,("諸事項",2),PREVIOUS,("訂正公告",0),BODY];
  let joined=lines.iter().map(|l|l.0).collect::<Vec<_>>().join("\n");
  assert_eq!(split(&lines,&[&joined]).len(),1);
 }
 #[test]fn a_page_without_original_rules_is_left_alone(){
  let lines=[REPORT,PREVIOUS,("登録海技免許講習実施機関の登録事項の変更に関",0),("する公示",0),BODY];
  let joined=lines.iter().map(|l|l.0).collect::<Vec<_>>().join("\n");
  let mut a=vec![joined];split_typeset_notice_titles(&mut a,&[typeset(&lines)],&[None]);assert_eq!(a.len(),1);
 }
 #[test]fn a_neighbor_only_needs_known_direction_and_fonts(){
  // An item marker above the title may have unknown content fonts; it still differs.
  let lines=[REPORT,("（七）図面縦覧場所四国地方整備局及び同局香川河川国道事務所",1),("基本測量関係事項公告",0),BODY];
  let mut page=typeset(&lines);page.native_content_styles[1]=ContentStyles::Unknown;
  let joined=lines.iter().map(|l|l.0).collect::<Vec<_>>().join("\n");
  let mut a=vec![joined.clone()];split_typeset_notice_titles(&mut a,&[page],&[Some(crate::lines::PageRules::default())]);assert_eq!(a.len(),2);
  // An unknown writing direction of the neighbor is still not evidence.
  let mut page=typeset(&lines);page.native_axes[1]=crate::native_word_axes::NativeAxis::Unknown;
  let mut a=vec![joined];split_typeset_notice_titles(&mut a,&[page],&[Some(crate::lines::PageRules::default())]);assert_eq!(a.len(),1);
 }
 #[test]fn a_comma_in_the_body_font_stays_inside_the_title(){
  // 「、」 of the title is set in the body font; the title's content glyphs are not.
  let lines=[REPORT,("に改める。",1),("船員の特定最低賃金の改正の決定に関し、関係船",0),("員及び関係使用者の意見聴取に関する公示",0),BODY];
  let mut page=typeset(&lines);page.native_styles[2]=Some([(0,8),(1,8)].into_iter().collect());
  let joined=lines.iter().map(|l|l.0).collect::<Vec<_>>().join("\n");
  let mut a=vec![joined];split_typeset_notice_titles(&mut a,&[page],&[Some(crate::lines::PageRules::default())]);
  assert_eq!(a[1].lines().next(),Some(lines[2].0));
 }

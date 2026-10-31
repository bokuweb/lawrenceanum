
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
 // The body's first line is indented one character from the title's line start.
 fn typeset(lines:&[(&str,usize)])->PageTrace{
  let mut words=vec![];let mut out=vec![];let mut styles=vec![];let mut bounds=vec![];let mut content=vec![];
  for(i,(t,font))in lines.iter().enumerate(){let y=50.+10.*i as f32;let x=if i==0{110.}else if *t==BODY.0{108.}else{100.};let r=x+8.*t.chars().count() as f32;words.push(Word{source_box:[x,y,r,y+8.],id:i,x0:x,y0:y,x1:r,y1:y+8.,raw:t.to_string(),text:t.to_string(),status:WordStatus::Used});out.push(Line::text(t.to_string(),vec![i]));let s:NativeStyles=[(*font,8)].into_iter().collect();styles.push(Some(s.clone()));bounds.push(NativeBounds::Glyphs([x,y,r,y+8.]));content.push(ContentStyles::Glyphs(s));}
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
  let lines=[REPORT,PREVIOUS,("登録船舶職員養成施設の登録事項の変更に関する",0),("公示",0),BODY];
  let got=split(&lines,&[&format!("{}\n{}\n{}",REPORT.0,PREVIOUS.0,lines[2].0),&format!("公示\n{}",BODY.0)]);
  assert_eq!(got,vec![format!("{}\n{}",REPORT.0,PREVIOUS.0),format!("{}\n公示\n{}",lines[2].0,BODY.0)]);
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
  // A centered section label is not a title.
  let lines=[REPORT,PREVIOUS,("公示",0),BODY];
  let joined=lines.iter().map(|l|l.0).collect::<Vec<_>>().join("\n");
  let mut page=typeset(&lines);page.native_bounds[2]=NativeBounds::Glyphs([250.,70.,266.,78.]);
  let mut a=vec![joined];split_typeset_notice_titles(&mut a,&[page],&[Some(crate::lines::PageRules::default())]);assert_eq!(a.len(),1);
 }
 #[test]fn a_title_starts_an_article_whatever_its_last_word(){
  let lines=[REPORT,("に改める。",1),("厚生労働省国民保護計画の変更について",0),BODY];
  let joined=lines.iter().map(|l|l.0).collect::<Vec<_>>().join("\n");
  assert_eq!(split(&lines,&[&joined])[1].lines().next(),Some(lines[2].0));
 }
 #[test]fn a_heading_indented_past_its_body_is_not_a_title(){
  // 附則 of the previous notice stands further in than the paragraph indent of its body.
  let lines=[REPORT,("に改める。",1),("附則",0),BODY];
  let joined=lines.iter().map(|l|l.0).collect::<Vec<_>>().join("\n");
  let mut page=typeset(&lines);page.native_bounds[2]=NativeBounds::Glyphs([124.,70.,140.,78.]);
  let mut a=vec![joined.clone()];split_typeset_notice_titles(&mut a,&[page],&[Some(crate::lines::PageRules::default())]);assert_eq!(a.len(),1);
  // A body line flush with the title is not its first paragraph either.
  let mut page=typeset(&lines);page.native_bounds[3]=NativeBounds::Glyphs([100.,80.,400.,88.]);
  let mut a=vec![joined];split_typeset_notice_titles(&mut a,&[page],&[Some(crate::lines::PageRules::default())]);assert_eq!(a.len(),1);
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
  for(i,(t,_))in lines.iter().enumerate(){let x=500.-10.*i as f32;let y=if i==1{60.}else if i==lines.len()-1{58.}else{50.};page.native_bounds[i]=NativeBounds::Glyphs([x,y,x+8.,y+8.*t.chars().count() as f32]);page.native_axes[i]=crate::native_word_axes::NativeAxis::Down;}
  let joined=lines.iter().map(|l|l.0).collect::<Vec<_>>().join("\n");
  let mut a=vec![joined];split_typeset_notice_titles(&mut a,&[page],&[Some(crate::lines::PageRules::default())]);
  assert_eq!(a[1].lines().next(),Some(lines[2].0));
 }
 #[test]fn the_public_notice_field_without_its_section_label_is_not_split(){
  let lines=[REPORT,("諸事項",2),PREVIOUS,("訂正公告",0),BODY];
  let joined=lines.iter().map(|l|l.0).collect::<Vec<_>>().join("\n");
  assert_eq!(split(&lines,&[&joined]).len(),1);
 }
 #[test]fn a_title_under_a_one_line_article_stays_with_it(){
  // 「資料」 already starts an article; the title does not leave it empty.
  let lines=[REPORT,("資料",2),("国庫歳入歳出状況",0),BODY];
  let a=split(&lines,&[REPORT.0,&lines[1..].iter().map(|l|l.0).collect::<Vec<_>>().join("\n")]);
  assert_eq!(a.len(),2);
 }
 #[test]fn headings_of_the_local_government_field_are_not_split(){
  // The table of contents lists 地方自治事項 as one item.
  let lines=[REPORT,("地方自治事項",2),("長野県",1),("選挙",0),BODY];
  let joined=lines.iter().map(|l|l.0).collect::<Vec<_>>().join("\n");
  assert_eq!(split(&lines,&[&joined]).len(),1);
 }
 #[test]fn a_field_of_the_report_section_shows_the_section_on_a_later_page(){
  // The item's pages begin at 労働; 官庁報告 is printed on an earlier page.
  let lines=[("労働",2),("に改める。",1),("船員の特定最低賃金の改正の決定に関し、関係船",0),("員及び関係使用者の意見聴取に関する公示",0),BODY];
  let joined=lines.iter().map(|l|l.0).collect::<Vec<_>>().join("\n");
  let a=split(&lines,&[&joined]);
  assert_eq!(a,vec!["労働\nに改める。".to_string(),lines[2..].iter().map(|l|l.0).collect::<Vec<_>>().join("\n")]);
  // Without any label the section is unknown.
  let joined=lines[1..].iter().map(|l|l.0).collect::<Vec<_>>().join("\n");
  assert_eq!(split(&lines[1..],&[&joined]).len(),1);
 }
 #[test]fn a_label_below_shows_the_section_when_none_is_above(){
  // The item's pages begin inside 官庁報告; a field of it, or 諸事項 (the first field of 公告),
  // comes later.
  let title=[("令和８年６月19日付けで代表者の変更があっ",1),("たため",1),("登録消火講習機関の登録事項の変更に関する公示",0),BODY];
  let run=|below:&[(&str,usize)]|{
   let lines:Vec<_>=title.iter().copied().chain(below.iter().copied()).collect();
   split(&lines,&[&lines.iter().map(|l|l.0).collect::<Vec<_>>().join("\n")])
  };
  for below in [&[("国家試験",2)][..],&[("公告",2),("諸事項",2)]]{
   let a=run(below);
   assert_eq!(a.len(),2);assert!(a[0].ends_with("\nたため"));assert!(a[1].starts_with(title[2].0));
  }
  // 官庁報告 below: the title precedes the section. 地方自治事項 below: the title may be in 公告.
  // A lone 「公告」 line can end a wrapped title.
  for below in [&[REPORT][..],&[("地方自治事項",2)],&[("公告",2)]]{assert_eq!(run(below).len(),1);}
  // 皇室事項 is printed before 官庁報告 and can be followed directly by 公告 and 諸事項.
  let lines:Vec<_>=[("皇室事項",3)].into_iter().chain(title).chain([("公告",2),("諸事項",2)]).collect();
  assert_eq!(split(&lines,&[&lines.iter().map(|l|l.0).collect::<Vec<_>>().join("\n")]).len(),1);
 }
 #[test]fn fields_listed_in_the_table_of_contents_do_not_show_the_section(){
  // The table of contents prints 〔官庁報告〕 and its fields before a cabinet order.
  let lines=[("〔官庁報告〕",1),("国家試験",2),("に改める。",1),("政令第二百三十三号",0),BODY];
  let joined=lines.iter().map(|l|l.0).collect::<Vec<_>>().join("\n");
  assert_eq!(split(&lines,&[&joined]).len(),1);
 }
 #[test]fn personnel_changes_are_not_a_field_of_the_report_section(){
  // 人事異動 precedes 官庁報告; the 褒章 headings below it are not notice titles.
  let lines=[("人事異動",2),("に改める。",1),("紺綬褒章並びに賞杯",0),BODY];
  let joined=lines.iter().map(|l|l.0).collect::<Vec<_>>().join("\n");
  assert_eq!(split(&lines,&[&joined]).len(),1);
 }
 // A title at the top of a page, under the previous page's last body line.
 fn next_page(previous:&[(&str,usize)],title:&str)->(Vec<PageTrace>,String){
  let mut top=typeset(&[(title,0),BODY]);top.native_bounds[0]=NativeBounds::Glyphs([100.,50.,100.+8.*title.chars().count() as f32,58.]);
  let joined=previous.iter().map(|l|l.0).chain([title,BODY.0]).collect::<Vec<_>>().join("\n");
  (vec![typeset(previous),top],joined)
 }
 fn split_pages(pages:&[PageTrace],joined:&str)->Vec<String>{let mut a=vec![joined.to_string()];split_typeset_notice_titles(&mut a,pages,&vec![Some(crate::lines::PageRules::default());pages.len()]);a}
 #[test]fn a_title_at_the_top_of_a_page_is_compared_with_the_previous_page(){
  let (pages,joined)=next_page(&[REPORT,PREVIOUS],"基本測量関係事項公告");
  assert_eq!(split_pages(&pages,&joined),vec![format!("{}\n{}",REPORT.0,PREVIOUS.0),format!("基本測量関係事項公告\n{}",BODY.0)]);
  // The previous page may be vertical; its fonts still differ from the title's.
  let (mut pages,joined)=next_page(&[REPORT,PREVIOUS],"基本測量関係事項公告");
  pages[0].native_axes=vec![crate::native_word_axes::NativeAxis::Down;2];
  assert_eq!(split_pages(&pages,&joined).len(),2);
  // Without the previous page there is no line above to compare.
  let (pages,joined)=next_page(&[REPORT,PREVIOUS],"基本測量関係事項公告");
  assert_eq!(split_pages(&pages[1..],&joined).len(),1);
  // A previous page without original rules is not evidence either.
  let (pages,joined)=next_page(&[REPORT,PREVIOUS],"基本測量関係事項公告");
  let mut a=vec![joined];split_typeset_notice_titles(&mut a,&pages,&[None,Some(crate::lines::PageRules::default())]);assert_eq!(a.len(),1);
 }
 #[test]fn a_line_in_the_previous_page_s_font_continues_it(){
  let (pages,joined)=next_page(&[REPORT,("（３）縦覧の場所",0)],"気仙沼市役所及び宮城県庁");
  assert_eq!(split_pages(&pages,&joined).len(),1);
 }
 #[test]fn a_title_under_a_table_row_starts_an_article(){
  // The previous notice ends in a table (a register of names); no title wraps out of a row.
  let lines=[REPORT,("沖縄県知事第00937号　上原金融",1),("試験事務を行う登録実施機関について",0),BODY];
  let joined=lines.iter().map(|l|l.0).collect::<Vec<_>>().join("\n");
  let mut page=typeset(&lines);page.regions[0].lines[1].table=Some(Table{x0:0.,y0:0.,x1:1.,y1:1.,rows:1,cols:1,cells:vec![]});
  let mut a=vec![joined];split_typeset_notice_titles(&mut a,&[page],&[Some(crate::lines::PageRules::default())]);
  assert_eq!(a,vec![format!("{}\n{}",REPORT.0,lines[1].0),format!("{}\n{}",lines[2].0,BODY.0)]);
 }
 // Instruments amended through comparison tables: each table unit's regions in reading order.
 fn comparison(lines:&[(&str,Role,usize)])->(Vec<PageTrace>,String){
  let mut page=typeset(&lines.iter().map(|l|(l.0,1)).collect::<Vec<_>>());
  let mut regions:Vec<Region>=vec![];
  for(i,(t,role,unit))in lines.iter().enumerate(){
   match regions.last_mut(){Some(r) if r.role==*role&&r.unit==*unit=>r.lines.push(Line::text(t.to_string(),vec![i])),_=>regions.push(Region{role:*role,unit:*unit,part:0,lines:vec![Line::text(t.to_string(),vec![i])]})}
  }
  page.regions=regions;
  (vec![page],lines.iter().map(|l|l.0).collect::<Vec<_>>().join("\n"))
 }
 const RULE_A:[(&str,Role,usize);8]=[("人事院は、一般職の職員の給与に関する法律に基づき、次の人事院規則を制定する。",Role::Preamble,0),("令和八年十月一日　人事院総裁川本裕子",Role::Preamble,0),("人事院規則九－一七－一七九",Role::Preamble,0),("人事院規則九－一七の一部を改正する人事院規則",Role::Preamble,0),("（俸給の特別調整額）",Role::After,0),("（俸給の特別調整額）",Role::Before,0),("附則",Role::Outside,0),("この規則は、公布の日から施行する。",Role::Outside,0)];
 #[test]fn an_instrument_heading_its_own_comparison_table_starts_an_article(){
  let lines:Vec<_>=RULE_A.iter().copied().chain([("人事院は、一般職の職員の給与に関する法律に基づき、次の人事院規則を制定する。",Role::Preamble,1),("令和八年十月一日　人事院総裁川本裕子",Role::Preamble,1),("人事院規則九－一二三－四八",Role::Preamble,1),("人事院規則九－一二三の一部を改正する人事院規則",Role::Preamble,1),("（本府省業務調整手当）",Role::After,1)]).collect();
  let (pages,joined)=comparison(&lines);
  let a=split_pages(&pages,&joined);
  assert_eq!(a.len(),2);assert!(a[1].starts_with("人事院は、")&&a[1].contains("九－一二三－四八")&&!a[0].contains("九－一二三－四八"));
 }
 #[test]fn an_instrument_starts_at_its_enactment_inside_a_shared_preamble_region(){
  // The previous instrument's 附則 article shares the preamble region; the next one starts at its enactment.
  let lines:Vec<_>=RULE_A[..6].iter().copied().chain([("第二条令和八年九月二日から同月三十日までの間に支給する手当について適用する。",Role::Preamble,1),("人事院は、一般職の職員の給与に関する法律に基づき、",Role::Preamble,1),("次の人事院規則を制定する。",Role::Preamble,1),("令和八年九月一日　人事院総裁川本裕子",Role::Preamble,1),("人事院規則九－五五－一五八",Role::Preamble,1),("（特地勤務手当）",Role::After,1)]).collect();
  let (pages,joined)=comparison(&lines);
  let a=split_pages(&pages,&joined);
  assert_eq!(a.len(),2);assert!(a[0].ends_with("適用する。")&&a[1].starts_with("人事院は、"));
 }
 #[test]fn a_preamble_read_after_an_unfinished_line_does_not_start_an_instrument(){
  // A joint notice's signatures are read in between its own preamble lines.
  let lines:Vec<_>=RULE_A[..6].iter().copied().chain([("についての開示事項（平成十九年",Role::Body,1),("農林水産省",Role::Preamble,2),("令和八年八月二十四日",Role::Preamble,2),("（定義）",Role::After,2)]).collect();
  let (pages,joined)=comparison(&lines);
  assert_eq!(split_pages(&pages,&joined).len(),1);
  // Its enactment ends before the agencies' names, not right above the date.
  let lines:Vec<_>=RULE_A[..6].iter().copied().chain([("告示第四号）の一部を次のように改正し、令和九年三月三十一日から適用する。",Role::Body,1),("農林水産省",Role::Preamble,2),("令和八年八月二十四日",Role::Preamble,2),("（定義）",Role::After,2)]).collect();
  let (pages,joined)=comparison(&lines);
  assert_eq!(split_pages(&pages,&joined).len(),1);
 }
 #[test]fn an_instrument_read_outside_the_previous_table_starts_after_that_table(){
  // The next unit has no preamble region; its enactment is read outside the previous unit's
  // table, right after the table's end marker.
  let next=[("人事院は、国と民間企業との間の人事交流に関する法律に基づき、次の人事院規則を制定する。",Role::Outside,0),("令和八年十月一日　人事院総裁川本裕子",Role::Outside,0),("人事院規則二一－〇－一二",Role::Outside,0),("人事院規則二一－〇の一部を改正する人事院規則",Role::Outside,0)];
  let lines:Vec<_>=RULE_A[..6].iter().copied().chain(next).chain([("（定義）",Role::After,1)]).collect();
  let (pages,_)=comparison(&lines);
  let text=|l:&[(&str,Role,usize)]|l.iter().map(|l|l.0).collect::<Vec<_>>().join("\n");
  let joined=format!("{}\n{}\n{}",text(&lines[..6]),crate::trace::SHINKYU_END,text(&lines[6..]));
  let a=split_pages(&pages,&joined);
  assert_eq!(a.len(),2);assert!(a[0].ends_with(crate::trace::SHINKYU_END)&&a[1].starts_with("人事院は、"));
  // Without the table's end right above, the line above is not where an instrument ends.
  assert_eq!(split_pages(&pages,&text(&lines)).len(),1);
  // Outside text followed by another table of its own unit is not the next unit's preamble.
  let lines:Vec<_>=RULE_A[..6].iter().copied().chain(next).chain([("（定義）",Role::After,0)]).collect();
  let (pages,_)=comparison(&lines);
  let joined=format!("{}\n{}\n{}",text(&lines[..6]),crate::trace::SHINKYU_END,text(&lines[6..]));
  assert_eq!(split_pages(&pages,&joined).len(),1);
 }
 #[test]fn a_comparison_preamble_without_its_own_date_continues_the_instrument(){
  // A second table of the same instrument is introduced without an enactment date.
  let lines:Vec<_>=RULE_A.iter().copied().chain([("次の表により、改正前欄に掲げる規定を改正後欄に掲げる規定のように改める。",Role::Preamble,1),("（本府省業務調整手当）",Role::After,1)]).collect();
  let (pages,joined)=comparison(&lines);
  assert_eq!(split_pages(&pages,&joined).len(),1);
 }
 #[test]fn a_comparison_preamble_under_its_heading_stays_with_it(){
  // The 〇 heading already starts the instrument; its preamble does not move the start.
  let lines:Vec<_>=RULE_A.iter().copied().chain([("〇内閣府令第六十二号",Role::Preamble,1),("令和八年十月一日　内閣総理大臣高市早苗",Role::Preamble,1),("投資法人の計算に関する規則の一部を改正する内閣府令",Role::Preamble,1),("（定義）",Role::After,1)]).collect();
  let (pages,joined)=comparison(&lines);
  let mut articles=vec![RULE_A.iter().map(|l|l.0).collect::<Vec<_>>().join("\n"),lines[8..].iter().map(|l|l.0).collect::<Vec<_>>().join("\n")];
  let before=articles.clone();split_typeset_notice_titles(&mut articles,&pages,&[Some(crate::lines::PageRules::default())]);
  assert_eq!(articles,before);let _=joined;
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

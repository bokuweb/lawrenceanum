
 use super::*;use crate::trace::*;
 fn source()->(Vec<Word>,Vec<Region>){
 let data=[("年度業務実績及び財務諸表に関する",100.,50.,235.,59.),("公告",100.,59.,118.,68.),("次の規定に基づき、次のとおり財務諸表を公告します。",90.,72.,330.,80.)];
 let words=data.iter().enumerate().map(|(id,(t,x,y,r,b))|Word{id,x0:*x,y0:*y,x1:*r,y1:*b,raw:t.to_string(),text:t.to_string(),status:WordStatus::Used}).collect();
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

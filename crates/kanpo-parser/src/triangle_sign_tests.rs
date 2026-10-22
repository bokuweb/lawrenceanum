use super::*;
fn fixture(y:f32,x:f32,neighbor:bool)->(Vec<Col>,Vec<Word>){
 let mut v=vec![(0.,0.,8.,8.184,"△"),(x,y,x+9.,y+6.138,"128"),(4.,9.848,16.,18.032,"165")];
 if neighbor {v.push((x+6.75,y,x+12.75,y+6.138,","));}
 let words:Vec<_>=v.iter().enumerate().map(|(id,(x,y,x1,y1,t))|Word{source_box: [*x, *y, *x1, *y1],id,x0:*x,y0:*y,x1:*x1,y1:*y1,raw:t.to_string(),text:t.to_string(),status:WordStatus::Used}).collect();
 let cols=v.into_iter().enumerate().map(|(id,(x,y,x1,y1,t))|Col{id,x,y,x1,y1,text:t.to_string()}).collect();(cols,words)
}
#[test]fn smaller_horizontal_amount_keeps_original_triangle(){let(mut c,w)=fixture(0.75,17.5,true);let t=attach_minus_signs(&mut c,&w);assert!(t.is_empty());assert_eq!(c.len(),4);assert_eq!(c[0].id,0);}
#[test]fn legitimate_below_amount_without_horizontal_run_retains_legacy(){let(mut c,w)=fixture(30.,17.5,true);let t=attach_minus_signs(&mut c,&w);assert!(t.contains(&2));assert!(!c.iter().any(|c|c.id==0));}
#[test]fn horizontal_number_requires_real_neighbor(){let(mut c,w)=fixture(0.75,17.5,false);let t=attach_minus_signs(&mut c,&w);assert!(t.contains(&2));}
#[test]fn next_column_beyond_three_em_does_not_claim_sign(){let(mut c,w)=fixture(0.75,40.,true);let t=attach_minus_signs(&mut c,&w);assert!(t.contains(&2));}
#[test]fn weak_vertical_overlap_does_not_change_legacy_target(){let(mut c,w)=fixture(4.,17.5,true);let t=attach_minus_signs(&mut c,&w);assert!(t.contains(&2));}
#[test]fn explanatory_triangle_inside_parentheses_stays_source_word(){
 let(mut c,mut w)=fixture(0.75,17.5,true);c[1].text="減少（".into();c[1].x=-20.;c[1].x1=4.;c[1].y=0.;c[1].y1=8.;c[2].text="は減少）".into();c[2].x=9.;c[2].x1=33.;c[2].y=0.;c[2].y1=8.;
 for a in &c {let z=&mut w[a.id];z.x0=a.x;z.x1=a.x1;z.y0=a.y;z.y1=a.y1;z.text=a.text.clone();}
 let t=attach_minus_signs(&mut c,&w);assert!(t.is_empty());assert_eq!(c.len(),4);
}

use kanpo_parser::{analyze_item,page_inputs};
fn compact(s:&str)->String{s.chars().filter(|c|!c.is_whitespace()).collect()}
fn run(pdf:&[u8])->kanpo_parser::Analysis {analyze_item(&page_inputs(pdf).unwrap())}
#[test] fn numeric_two_fractions_inside_prose_line(){
 let a=run(include_bytes!("archive/pages/20261014g00225/20261014g002250011.pdf"));
 let expected="額面金額×(1.87/100)×(１/２)";
 assert_eq!(compact(&a.text).matches(expected).count(),1,"{}",a.text);
 assert_eq!(a.articles.iter().filter(|art|compact(&art.text).contains(expected)&&compact(&art.document.to_text()).contains(expected)).count(),1,"Article/Document formula owner");
 let ls:Vec<_>=a.pages.iter().flat_map(|p|&p.regions).flat_map(|r|&r.lines).filter(|l|compact(&l.text).contains(expected)).collect();
 assert_eq!(ls.len(),1);assert_eq!(ls[0].words,vec![115,105,98,93,88,58,27,22,61,23,28,26,24,21,60,59]);
 assert!(ls[0].text.starts_with("11　初期利子　令和９年３月15日"));
 assert!(ls[0].text.contains("下、次号及び第13号において規定する期日について同じ。）。"));
}
#[test] fn native_parenthesized_difference_of_numeric_fractions(){
 let a=run(include_bytes!("archive/pages/20260909g00201/20260909g002010013.pdf"));
 let expected="額面金額×(1.8/100)×((１/２)−(２/365))";
 assert_eq!(compact(&a.text).matches(expected).count(),1,"{}",a.text);
 assert_eq!(a.articles.iter().filter(|art|compact(&art.text).contains(expected)&&compact(&art.document.to_text()).contains(expected)).count(),1,"Article/Document formula owner");
 let ls:Vec<_>=a.pages.iter().flat_map(|p|&p.regions).flat_map(|r|&r.lines).filter(|l|compact(&l.text).contains(expected)).collect();
 assert_eq!(ls.len(),1);assert_eq!(ls[0].words,vec![100,99,98,94,91,126,97,93,101,95,92]);
 for id in [96,90]{let w=a.pages[0].words.iter().find(|w|w.id==id).unwrap();assert_eq!(w.text,"");assert_eq!(format!("{:?}",w.status),"Empty");assert!(!ls[0].words.contains(&id));}
}

fn rejected_22511(mut change:impl FnMut(&mut[kanpo_parser::PageInput])){let mut p=page_inputs(include_bytes!("archive/pages/20261014g00225/20261014g002250011.pdf")).unwrap();change(&mut p);let a=analyze_item(&p);assert!(!compact(&a.text).contains("額面金額×(1.87/100)×(１/２)"));}
#[test]fn actual_missing_short_bar_not_guessed(){rejected_22511(|p|{let r=p[0].rules.as_mut().unwrap();let n=r.short_fraction_bars.len();r.short_fraction_bars.retain(|h|!((h.y-138.929).abs()<0.03&&(h.x0-591.).abs()<0.03));assert_eq!(r.short_fraction_bars.len()+1,n);})}
#[test]fn actual_missing_long_bar_not_guessed(){rejected_22511(|p|{let r=p[0].rules.as_mut().unwrap();let n=r.horizontal.len();r.horizontal.retain(|h|!((h.y-138.929).abs()<0.03&&(h.x0-565.).abs()<0.03));assert_eq!(r.horizontal.len()+1,n);})}
#[test]fn actual_missing_native_bracket_not_guessed(){let mut p=page_inputs(include_bytes!("archive/pages/20260909g00201/20260909g002010013.pdf")).unwrap();assert!(p[0].xhtml.contains("\u{eae9}\u{eaea}"));p[0].xhtml=p[0].xhtml.replace("\u{eae9}\u{eaea}","");let a=analyze_item(&p);assert!(!compact(&a.text).contains("額面金額×(1.8/100)×((１/２)−(２/365))"));}
#[test]fn actual_unknown_native_bracket_not_guessed(){let mut p=page_inputs(include_bytes!("archive/pages/20260909g00201/20260909g002010013.pdf")).unwrap();assert!(p[0].xhtml.contains('\u{eaeb}'));p[0].xhtml=p[0].xhtml.replace('\u{eaeb}',"\u{efff}");let a=analyze_item(&p);assert!(!compact(&a.text).contains("額面金額×(1.8/100)×((１/２)−(２/365))"));}

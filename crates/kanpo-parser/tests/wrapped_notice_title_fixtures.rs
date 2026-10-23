#[path = "support/legacy_analysis.rs"]
mod legacy_analysis;
use kanpo_parser::Analysis;
fn actual(pdf:&[u8])->Analysis { kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap()) }
const PDF0:&[u8]=include_bytes!("archive/pages/20260803g00174/20260803g001740087.pdf");
const BASE0:&str=include_str!("archive/pages/20260803g00174/20260803g001740087.expected.analysis.json");
const PDF1:&[u8]=include_bytes!("archive/pages/20260803g00174/20260803g001740086.pdf");
const BASE1:&str=include_str!("archive/pages/20260803g00174/20260803g001740086.expected.analysis.json");
const PDF2:&[u8]=include_bytes!("archive/pages/20260803g00174/20260803g001740088.pdf");
const BASE2:&str=include_str!("archive/pages/20260803g00174/20260803g001740088.expected.analysis.json");
const PDF3:&[u8]=include_bytes!("archive/pages/20260716g00159/20260716g001590045.pdf");
const BASE3:&str=include_str!("archive/pages/20260716g00159/20260716g001590045.expected.analysis.json");
fn target()->Analysis{actual(PDF0)}
#[test]fn full_two_line_notice_heading_starts_the_new_article(){let a=target();assert_eq!(a.articles.len(),2);assert!(a.articles[1].text.starts_with("国立研究開発法人建築研究所令和７事業年度財務諸表に関する\n公告\n"),"the complete source heading belongs to its own notice");}
#[test]fn preceding_profit_appropriation_article_ends_before_the_next_heading(){let a=target();assert!(a.articles[0].text.ends_with("積立金　80,555,474"));assert!(!a.articles[0].text.contains("建築研究所"));}
#[test]fn original_page_words_regions_text_and_ids_are_unchanged(){let a=target();let old:serde_json::Value=serde_json::from_str(BASE0).unwrap();let v=legacy_analysis::value(&a);assert_eq!(v["pages"],old["pages"]);assert_eq!(v["text"],old["text"]);assert_eq!(v["format"],old["format"]);for (id,text) in [(190,"国立研究開発法人建築研究所令和７事業年度財務諸表に関する"),(191,"公告")]{let w=&a.pages[0].words[id];assert_eq!(w.id,id);assert_eq!(w.text,text);assert_eq!(w.status,kanpo_parser::trace::WordStatus::Used);}}
#[test]fn previous_financial_page_all_analysis_is_unchanged(){let a=actual(PDF1);let old:serde_json::Value=serde_json::from_str(BASE1).unwrap();assert_eq!(legacy_analysis::value(&a),old);}
#[test]fn following_financial_page_all_analysis_is_unchanged(){let a=actual(PDF2);let old:serde_json::Value=serde_json::from_str(BASE2).unwrap();assert_eq!(legacy_analysis::value(&a),old);}
#[test]fn separate_notice_title_and_body_all_analysis_is_unchanged(){let a=actual(PDF3);let old:serde_json::Value=serde_json::from_str(BASE3).unwrap();assert_eq!(legacy_analysis::value(&a),old);}

const PDF4:&[u8]=include_bytes!("archive/pages/20260810g00179/20260810g001790115.pdf");
const BASE4:&str=include_str!("archive/pages/20260810g00179/20260810g001790115.expected.analysis.json");
#[test]fn complete_source_title_and_previous_article_end_20260810g001790115(){let a=actual(PDF4);assert_eq!(a.articles.len(),2);assert!(a.articles[1].text.starts_with("国立研究開発法人国立環境研究所令和７年度財務諸表に関する\n公告\n"));assert!(a.articles[0].text.ends_with("https://www.ffpri.go.jp/koukaijouhou/zaimu/documents/r07zaimushohyou.pdf"));let old:serde_json::Value=serde_json::from_str(BASE4).unwrap();let v=legacy_analysis::value(&a);assert_eq!(v["pages"],old["pages"]);assert_eq!(v["text"],old["text"]);assert_eq!(v["format"],old["format"]);}
const PDF5:&[u8]=include_bytes!("archive/pages/20261016g00228/20261016g002280011.pdf");
const BASE5:&str=include_str!("archive/pages/20261016g00228/20261016g002280011.expected.analysis.json");
#[test]fn complete_source_title_and_previous_article_end_20261016g002280011(){let a=actual(PDF5);assert_eq!(a.articles.len(),2);assert!(a.articles[1].text.starts_with("国立大学法人北海道教育大学令和７事業年度財務諸表に関する\n公告\n"));assert!(a.articles[0].text.ends_with("あります。"));let old:serde_json::Value=serde_json::from_str(BASE5).unwrap();let v=legacy_analysis::value(&a);assert_eq!(v["pages"],old["pages"]);assert_eq!(v["text"],old["text"]);assert_eq!(v["format"],old["format"]);}
const PDF6:&[u8]=include_bytes!("archive/pages/20261016g00228/20261016g002280168.pdf");
const BASE6:&str=include_str!("archive/pages/20261016g00228/20261016g002280168.expected.analysis.json");
#[test]fn complete_source_title_and_previous_article_end_20261016g002280168(){let a=actual(PDF6);assert_eq!(a.articles.len(),2);assert!(a.articles[1].text.starts_with("国立大学法人東京外国語大学令和７事業年度財務諸表に関する\n公告\n"));assert!(a.articles[0].text.ends_with("省略しております。"));let old:serde_json::Value=serde_json::from_str(BASE6).unwrap();let v=legacy_analysis::value(&a);assert_eq!(v["pages"],old["pages"]);assert_eq!(v["text"],old["text"]);assert_eq!(v["format"],old["format"]);}
const PDF7:&[u8]=include_bytes!("archive/pages/20261016g00228/20261016g002280345.pdf");
const BASE7:&str=include_str!("archive/pages/20261016g00228/20261016g002280345.expected.analysis.json");
#[test]fn complete_source_title_and_previous_article_end_20261016g002280345(){let a=actual(PDF7);assert_eq!(a.articles.len(),2);assert!(a.articles[1].text.starts_with("国立大学法人名古屋工業大学令和７事業年度財務諸表に関する\n公告\n"));assert!(a.articles[0].text.ends_with("該当事項無し"));let old:serde_json::Value=serde_json::from_str(BASE7).unwrap();let v=legacy_analysis::value(&a);assert_eq!(v["pages"],old["pages"]);assert_eq!(v["text"],old["text"]);assert_eq!(v["format"],old["format"]);}
#[test]fn all_five_original_30_glyph_titles_bind_once_to_the_same_used_source_words(){
 for (pdf,ids) in [(PDF0,[190,191]),(PDF4,[231,232]),(PDF5,[261,262]),(PDF6,[190,191]),(PDF7,[136,137])] {
 let n=kanpo_pdf::extract_pages(pdf).unwrap();let a=actual(pdf);let mut seen=std::collections::BTreeSet::new();let mut font=None;
 for (id,count) in [(ids[0],28),(ids[1],2)] {let w=a.pages[0].words.iter().find(|w|w.id==id).unwrap();assert_eq!(w.status,kanpo_parser::trace::WordStatus::Used);let gs:Vec<_>=n[0].glyphs.iter().enumerate().filter(|(_,g)|{let x=(g.bbox[0]+g.bbox[2])*0.5;let y=(g.bbox[1]+g.bbox[3])*0.5;x>=w.x0 as f64-0.05&&x<w.x1 as f64+0.05&&y>=w.y0 as f64-0.05&&y<w.y1 as f64+0.05}).collect();assert_eq!(gs.len(),count);assert_eq!(gs.iter().map(|(_,g)|g.text.as_str()).collect::<String>(),w.raw);for(i,g)in gs {assert!(seen.insert(i));if let Some(f)=font{assert_eq!(g.font,f)}else{font=Some(g.font);}assert!((g.size-9.).abs()<0.001);}let owners:usize=a.pages[0].regions.iter().flat_map(|r|&r.lines).map(|l|l.words.iter().filter(|&&v|v==id).count()+l.table.iter().flat_map(|t|&t.cells).flat_map(|c|&c.words).filter(|&&v|v==id).count()).sum();assert_eq!(owners,1);}
 assert_eq!(seen.len(),30);}
}

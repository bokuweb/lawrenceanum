use kanpo_parser::trace::{WordStatus,Line};
const PDF:&[u8]=include_bytes!("archive/pages/20260915h01790/20260915h017900005.pdf");
const ATSUGI:[&str;5]=["三〇八三","厚木海軍飛行場","綾瀬市","国有","工作物：門等ユーティリティとして追加提供する。"];
const KANOYA:[&str;5]=["五一二七","鹿屋飛行場","鹿屋市","国有","建物：約六、〇〇〇平方メートル工作物：水道等訓練施設として追加提供する。使用期間：一令和八年九月十五日から同月二十五日までの間二必要に応じ、訓練の展開及び撤収のための追加期間海上自衛隊鹿屋航空基地の施設の一部を、地位協定第二条四⒝の適用ある施設及び区域として提供する。提供期間中は、地位協定の関連ある条項が適用される。"];
const ATSUGI_IDS:[usize;7]=[0,1,2,3,4,6,7];
const KANOYA_IDS:[usize;19]=[10,11,12,13,14,15,17,18,20,21,23,24,25,27,29,31,33,35,36];
fn actual()->kanpo_parser::Analysis {kanpo_parser::analyze_item(&kanpo_parser::page_inputs(PDF).unwrap())}
fn compact(t:&str)->String {t.chars().filter(|c|!c.is_whitespace()).collect()}
fn rows(l:&Line)->Vec<String> {
 if let Some(t)=&l.table {(0..t.rows).map(|r|compact(&t.cells.iter().filter(|c|c.row==r).map(|c|c.text.as_str()).collect::<String>())).collect()}
 else {vec![compact(&l.text)]}
}
fn expect_record(fields:[&str;5], ids:&[usize]) {
 let a=actual();let want=fields.concat();
 let matches:Vec<_>=a.pages[0].regions.iter().flat_map(|r|&r.lines).filter(|l|l.table.is_none()&&compact(&l.text)==want).collect();
 assert_eq!(matches.len(),1,"five fields and all remarks must belong to one source record");
 assert_eq!(matches[0].words,ids,"each original source Word belongs to its own record, exactly once");
 let parts:Vec<_>=matches[0].text.split('　').map(compact).collect();
 assert_eq!(parts,fields.into_iter().map(str::to_string).collect::<Vec<_>>());
}
#[test]fn atsugi_five_fields_and_complete_summary_belong_to_one_logical_record(){expect_record(ATSUGI,&ATSUGI_IDS);}
#[test]fn kanoya_five_fields_and_complete_date_summary_belong_to_one_logical_record(){expect_record(KANOYA,&KANOYA_IDS);}
#[test]fn all_original_ids_remain_used_once_including_both_identical_national_owners(){let a=actual();let p=&a.pages[0];for id in ATSUGI_IDS.into_iter().chain(KANOYA_IDS){let w=p.words.iter().find(|w|w.id==id).unwrap();assert_eq!(w.status,WordStatus::Used);let n:usize=p.regions.iter().flat_map(|r|&r.lines).map(|l|l.words.iter().filter(|&&v|v==id).count()+l.table.iter().flat_map(|t|&t.cells).flat_map(|c|&c.words).filter(|&&v|v==id).count()).sum();assert_eq!(n,1);}assert_eq!(p.words.iter().find(|w|w.id==3).unwrap().text,"国有");assert_eq!(p.words.iter().find(|w|w.id==13).unwrap().text,"国有");}
#[test]fn headings_previous_facility_and_later_road_notice_are_not_part_of_either_record(){let a=actual();let p=&a.pages[0];let all:Vec<_>=p.regions.iter().flat_map(|r|&r.lines).flat_map(rows).collect();let f=all.iter().position(|s|s.contains("施設番号")).unwrap();let prev=all.iter().position(|s|s.contains("一〇七六旭川近文台演習場")).unwrap();let ats=all.iter().position(|s|s.starts_with("三〇八三")).unwrap();let after=all.iter().position(|s|s.starts_with("〇関東地方整備局告示第二百十一号")).unwrap();assert!(f<prev&&prev<ats&&ats<after);assert!(all.iter().filter(|s|s.starts_with("三〇八三")||s.starts_with("五一二七")).all(|s|!s.contains("一〇七六")&&!s.contains("施設番号")&&!s.contains("道路の区域")));}
#[test]fn neighboring_heading_and_registration_page_keeps_complete_baseline_output(){let a=kanpo_parser::analyze_item(&kanpo_parser::page_inputs(include_bytes!("archive/pages/20260915h01790/20260915h017900004.pdf")).unwrap());assert_eq!(a.text.trim_end(),include_str!("archive/pages/20260915h01790/20260915h017900004.expected.txt").trim_end());}
#[test]fn neighboring_ordinary_personnel_columns_keep_complete_baseline_output(){let a=kanpo_parser::analyze_item(&kanpo_parser::page_inputs(include_bytes!("archive/pages/20260915h01790/20260915h017900006.pdf")).unwrap());assert_eq!(a.text.trim_end(),include_str!("archive/pages/20260915h01790/20260915h017900006.expected.txt").trim_end());}

#[test]fn all_212_native_glyphs_bind_to_the_original_26_word_ids_in_the_lower_band(){
 let native=kanpo_pdf::extract_pages(PDF).unwrap();
 let glyphs:Vec<_>=native[0].glyphs.iter().filter(|g|g.bbox[0]<550.&&g.bbox[2]>415.&&g.bbox[1]<790.&&g.bbox[3]>428.).collect();
 assert_eq!(glyphs.len(),212,"all native Glyphs, including empty Unicode or invisible, intersect the original band");
 let original:String=glyphs.iter().map(|g|g.text.as_str()).collect();
 let a=actual();let p=&a.pages[0];let ids:Vec<_>=ATSUGI_IDS.into_iter().chain(KANOYA_IDS).collect();assert_eq!(ids.len(),26);
 let raw:String=ids.iter().map(|id|p.words.iter().find(|w|w.id==*id).unwrap().raw.as_str()).collect();
 assert_eq!(original,raw);assert_eq!(raw.chars().count(),212);
 for id in ids {let w=p.words.iter().find(|w|w.id==id).unwrap();assert!(w.x0>=415.&&w.x1<=550.&&w.y0>=428.&&w.y1<=790.);}
}

// Four already-correct facility records beneath an actual repeated header must stay unchanged.
// The page also includes 24 coordinate rows: their native angle symbols must survive
// alongside the unchanged license notice and four facility records.
#[test]fn four_record_facility_table_and_preceding_license_notice_keep_complete_output(){
 let a=kanpo_parser::analyze_item(&kanpo_parser::page_inputs(include_bytes!("archive/pages/20260804h01761/20260804h017610002.pdf")).unwrap());
 assert_eq!(a.text,include_str!("archive/pages/20260804h01761/20260804h017610002.expected.txt"));
 for symbol in ['°','′','″'] {assert_eq!(a.text.matches(symbol).count(),48);}
 for key in ["一〇六六　東千歳駐屯地　千歳市　国有", "一〇七六　旭川近文台演習場　旭川市　国有", "三一八九　朝霞駐屯地　朝霞市、和光市、新座市、東京都練馬区　国有", "五一二四　北熊本駐屯地　熊本市　国有"] {assert!(a.text.lines().any(|l|l.starts_with(key)));}
}

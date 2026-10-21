use super::*;
fn words(body:&str)->Vec<Word>{let html=scraper::Html::parse_document(&format!("<doc><page width=\"800\" height=\"600\">{body}</page></doc>"));let p=html.select(&scraper::Selector::parse("page").unwrap()).next().unwrap();page_words_traced(p,800.,600.,None).1}
fn w(x:i32,text:&str,flag:&str)->String{format!("<word xMin=\"{x}\" yMin=\"100\" xMax=\"{}\" yMax=\"108\" {flag}>{text}</word>",x+8)}
#[test]fn resolved_AJ1_Unicode_rescue_is_not_legacy_CID(){assert_eq!(words(&w(200,"救","data-unicode-aj1=\"1\""))[0].text,"救");}
#[test]fn bare_Poppler_legacy_CID_is_preserved(){assert_eq!(words(&w(200,"救",""))[0].text,"（チ）");}
#[test]fn WMode_is_not_a_Unicode_resolution_proof(){assert_eq!(words(&w(200,"救","data-wmode=\"0\""))[0].text,"（チ）");}
#[test]fn nonasserting_flag_values_do_not_gain_trust(){for f in["data-unicode-aj1=\"0\"","data-unicode-aj1=\"true\"",""]{assert_eq!(words(&w(200,"救",f))[0].text,"（チ）");}}
#[test]fn tagged_Identity_CID_still_maps_even_with_marker(){let t=char::from_u32(kanpo_pdf::bbox::IDENTITY_CID_BASE+25937).unwrap().to_string();for f in["data-unicode-aj1=\"1\"",""]{assert_eq!(words(&w(200,&t,f))[0].text,"（チ）");}}
#[test]fn raw_and_flag_sort_together_in_canonical_order(){let r=words(&(w(100,"救","data-unicode-aj1=\"1\"")+&w(200,"救","")));assert_eq!(r.iter().map(|w|w.text.as_str()).collect::<Vec<_>>(),["（チ）","救"]);assert_eq!(r.iter().map(|w|w.id).collect::<Vec<_>>(),[0,1]);}
#[test]fn same_bbox_same_text_keeps_each_source_occurrence_flag(){let r=words(&(w(200,"救","data-unicode-aj1=\"1\"")+&w(200,"救","")));assert_eq!(r.iter().map(|w|w.text.as_str()).collect::<Vec<_>>(),["救","（チ）"]);}
#[test]fn same_bbox_reversed_sources_do_not_share_resolution(){let r=words(&(w(200,"救","")+&w(200,"救","data-unicode-aj1=\"1\"")));assert_eq!(r.iter().map(|w|w.text.as_str()).collect::<Vec<_>>(),["（チ）","救"]);}
#[test]fn other_resolved_CJK_is_literal_but_bare_fallback_remains(){let r=words(&(w(200,"伽","data-unicode-aj1=\"1\"")+&w(100,"伽","")));assert_eq!(r.iter().map(|w|w.text.as_str()).collect::<Vec<_>>(),["伽","竈"]);}
#[test]fn real_kana_family_neighbor_cannot_repromote_resolved_Unicode(){let t=char::from_u32(kanpo_pdf::bbox::IDENTITY_CID_BASE+0x6541).unwrap().to_string();let r=words(&(w(208,"救","data-unicode-aj1=\"1\"")+&w(200,&t,"")));assert_eq!(r.iter().map(|w|w.text.as_str()).collect::<Vec<_>>(),["救","（ア）"]);}

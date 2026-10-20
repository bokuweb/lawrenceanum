use kanpo_parser::{analyze_item, page_inputs};
use serde_json::Value;
fn pdf(page:&str)->&'static[u8]{match page{"20260828g001920029"=>include_bytes!("archive/pages/20260828g00192/20260828g001920029.pdf"),
"20260828g001920030"=>include_bytes!("archive/pages/20260828g00192/20260828g001920030.pdf"),
"20260828g001920031"=>include_bytes!("archive/pages/20260828g00192/20260828g001920031.pdf"),
"20260930g002150375"=>include_bytes!("archive/pages/20260930g00215/20260930g002150375.pdf"),
"20260930g002150376"=>include_bytes!("archive/pages/20260930g00215/20260930g002150376.pdf"),
"20260930g002150377"=>include_bytes!("archive/pages/20260930g00215/20260930g002150377.pdf"),_=>panic!("unknown fixture")}}
fn expected(page:&str)->&'static str{match page{"20260828g001920029"=>include_str!("expected/micro_liter/20260828g001920029-baseline36.json"),
"20260828g001920030"=>include_str!("expected/micro_liter/192p30-baseline36.json"),
"20260828g001920031"=>include_str!("expected/micro_liter/20260828g001920031-baseline36.json"),
"20260930g002150375"=>include_str!("expected/micro_liter/20260930g002150375-baseline36.json"),
"20260930g002150376"=>include_str!("expected/micro_liter/215p376-baseline36.json"),
"20260930g002150377"=>include_str!("expected/micro_liter/20260930g002150377-baseline36.json"),_=>panic!("unknown fixture")}}
fn analyze(page:&str)->kanpo_parser::Analysis{analyze_item(&page_inputs(pdf(page)).unwrap())}
fn value(a: &kanpo_parser::Analysis) -> Value {
    serde_json::from_slice(&serde_json::to_vec(a).unwrap()).unwrap()
}
#[test]
fn original_three_micro_liter_prefixes_keep_their_unique_used_word_owners() {
    let a = analyze("20260828g001920030");
    let av = value(&a);
    let before: Value = serde_json::from_str(expected("20260828g001920030")).unwrap();
    assert_eq!(a.text.matches("40μL").count(),1,"source 40 microliters must not become 40 liters");
    assert_eq!(a.text.matches("50μL").count(),2,"both source 50 microliter aliquots must retain their magnitude");
    for id in [146,172,169] {
        let w = &av["pages"][0]["words"][id]; let b = &before["pages"][0]["words"][id];
        assert_eq!(w["raw"],b["raw"]); assert_eq!(w["id"],b["id"]);
        for k in ["x0","y0","x1","y1","status"] { assert_eq!(w[k],b[k],"existing mixed Word metadata {id} {k}"); }
        assert_eq!(w["status"],"used");
        assert_eq!(w["text"],b["raw"].as_str().unwrap().replace('\u{E24F}',"μ"));
        let mut owners = vec![];
        for (ri,r) in av["pages"][0]["regions"].as_array().unwrap().iter().enumerate() {
            for (li,l) in r["lines"].as_array().unwrap().iter().enumerate() {
                if l["words"].as_array().unwrap().contains(&Value::from(id)) { owners.push((ri,li)); }
            }
        }
        let expected = match id {146=>(0,3),172=>(0,27),169=>(0,30),_=>unreachable!()};
        assert_eq!(owners,vec![expected],"same one existing native owner");
    }
    assert_eq!(a.articles[0].text,a.text,"article does not lose the restored unit");
}
#[test]
fn four_printed_chemical_line_end_hyphens_remain_with_l_or_d() {
    let a = analyze("20260828g001920030");
    let lines: Vec<_> = a.text.lines().collect();
    for (line,prefix) in [(3,"Ｌ－"),(31,"Ｄ－"),(40,"Ｌ－"),(64,"Ｄ－")] {
        assert!(lines[line-1].ends_with(prefix),"source chemical prefix at line {line} must include its printed hyphen");
    }
    let av=value(&a);
    for (id,line) in [(104,2),(105,30),(0,39),(1,63)] {
        let w=&av["pages"][0]["words"][id];
        assert_eq!(w["raw"],"\u{E61C}");assert_eq!(w["status"],"used");assert_eq!(w["text"],"－");
        assert!(av["pages"][0]["regions"][0]["lines"][line]["words"].as_array().unwrap().contains(&Value::from(id)),"source hyphen must attach to its preceding chemical prefix line");
    }
}
#[test]
fn neighboring_full_text_articles_ids_and_all_metadata_remain_exact() {
    for page in ["20260828g001920029","20260828g001920031"] {
        let a=analyze(page);let b:Value=serde_json::from_str(expected(page)).unwrap();
        assert_eq!(value(&a),b,"all Analysis must remain exact for nearby negative {page}");
        assert_eq!(a.text,b["text"].as_str().unwrap());
    }
}
#[test]
fn native_micro_suffixes_keep_the_documented_point_sixteen_bbox_overhang() {
    let a=value(&analyze("20260828g001920030"));
    let native:Value=serde_json::from_slice(&serde_json::to_vec(&kanpo_pdf::extract_pages(pdf("20260828g001920030")).unwrap()).unwrap()).unwrap();
    let gs=native[0]["glyphs"].as_array().unwrap();
    for id in [146,172,169] {
        let w=&a["pages"][0]["words"][id];
        let hits:Vec<_>=gs.iter().filter(|g|g["text"]=="\u{E24F}" && g["bbox"][0].as_f64().unwrap()>=w["x0"].as_f64().unwrap() && g["bbox"][2].as_f64().unwrap()<=w["x1"].as_f64().unwrap() && (g["bbox"][1].as_f64().unwrap()-w["y0"].as_f64().unwrap()).abs()<0.2).collect();
        assert_eq!(hits.len(),1,"one native micro glyph in the original mixed Word");
        let g=hits[0];assert_eq!(g["cid"],23659);assert_eq!(g["font"],1);
        assert!((g["bbox"][3].as_f64().unwrap()-w["y1"].as_f64().unwrap()-0.16).abs()<0.0002,"native glyph overhang is retained honestly; the coarse Word bbox is not expanded");
    }
    let glyphs:Vec<_>=gs.iter().filter(|g|g["text"]=="\u{E24F}").collect();assert_eq!(glyphs.len(),3);
    assert_eq!(native[0]["fonts"][1]["ordering"],"Npb1");assert_eq!(native[0]["fonts"][1]["has_to_unicode"],true);
}
#[test]
fn both_original_micrometer_sizes_keep_their_unique_source_word_owners() {
 let a=analyze("20260930g002150376");assert_eq!(a.text.matches("0.3μm").count(),2,"both printed0.3 micrometers must retain their magnitude");
 let av=value(&a);let b:Value=serde_json::from_str(expected("20260930g002150376")).unwrap();
 for (id,line) in [(95,12),(9,50)]{
  let w=&av["pages"][0]["words"][id];let old=&b["pages"][0]["words"][id];
  for k in ["id","raw","status","x0","y0","x1","y1"]{assert_eq!(w[k],old[k]);}assert_eq!(w["status"],"used");assert_eq!(w["text"],old["raw"].as_str().unwrap().replace('\u{E24F}',"μ"));
  let mut os=vec![];for(ri,r)in av["pages"][0]["regions"].as_array().unwrap().iter().enumerate(){for(li,l)in r["lines"].as_array().unwrap().iter().enumerate(){if l["words"].as_array().unwrap().contains(&Value::from(id)){os.push((ri,li));}}}assert_eq!(os,vec![(0,line)]);
 }
 assert_eq!(a.articles[0].text,a.text);
}
#[test]
fn micrometer_native_bbox_offsets_are_recorded_without_expanding_source_words(){
 let a=value(&analyze("20260930g002150376"));let native:Value=serde_json::from_slice(&serde_json::to_vec(&kanpo_pdf::extract_pages(pdf("20260930g002150376")).unwrap()).unwrap()).unwrap();
 let gs=native[0]["glyphs"].as_array().unwrap();assert_eq!(gs.iter().filter(|g|g["text"]=="\u{E24F}").count(),2);
 for(id,offset)in [(95,0.304),(9,0.160)]{let w=&a["pages"][0]["words"][id];let hits:Vec<_>=gs.iter().filter(|g|g["text"]=="\u{E24F}"&&g["bbox"][0].as_f64().unwrap()>=w["x0"].as_f64().unwrap()&&g["bbox"][2].as_f64().unwrap()<=w["x1"].as_f64().unwrap()&&(g["bbox"][1].as_f64().unwrap()-w["y0"].as_f64().unwrap()).abs()<0.4).collect();assert_eq!(hits.len(),1);let g=hits[0];assert_eq!(g["cid"],23659);assert!((g["bbox"][3].as_f64().unwrap()-w["y1"].as_f64().unwrap()-offset).abs()<0.0002);}
}
#[test]
fn neighboring_micrometer_pages_keep_full_analysis_and_articles_exact(){for page in ["20260930g002150375","20260930g002150377"]{let a=analyze(page);let b:Value=serde_json::from_str(expected(page)).unwrap();assert_eq!(value(&a),b);assert_eq!(a.text,b["text"].as_str().unwrap());}}


use kanpo_parser::{Analysis,trace::WordStatus};use serde_json::Value;
const PDF:&[u8]=include_bytes!("archive/pages/20260930g00215/20260930g002150030.pdf");
const BEFORE:&str=include_str!("expected/range_endpoints/20260930g002150030-before43.json");
const PAIRS:&str=include_str!("expected/range_endpoints/original-four-pairs.json");
fn actual(p:&[u8])->Analysis{kanpo_parser::analyze_item(&kanpo_parser::page_inputs(p).unwrap())}
fn value(a:&Analysis)->Value{serde_json::from_str(&serde_json::to_string(a).unwrap()).unwrap()}
#[test]fn four_original_range_endpoints_have_exact_text_roles_and_original_source_ids(){
 let a=actual(PDF);let p=&a.pages[0];let expected=[("（９）〜（２１）　［略］",165,149,"（２","１）"),("（８）〜（２０）　［同左］",71,61,"（２","０）"),("（９）〜（１７）　［略］",159,147,"（１","７）"),("（８）〜（１６）　［同左］",67,60,"（１","６）")];
 for(t,left,right,l,r)in expected{
  assert!(a.text.contains(t),"missing original endpoint {t}");let owners:Vec<_>=p.regions.iter().flat_map(|z|&z.lines).filter(|z|z.words.contains(&left)).collect();assert_eq!(owners.len(),1);assert_eq!(owners[0].text,t);
  let pos=owners[0].words.iter().position(|&i|i==left).unwrap();assert_eq!(owners[0].words[pos+1],right);
  assert!(p.words[left].text.ends_with(l));assert_eq!(p.words[right].text,r);assert_eq!(p.words[right].status,WordStatus::Used);
 }
 let v=value(&a);let s=v["articles"][0]["document"].to_string();for n in ["２１","２０","１７","１６"]{assert!(s.contains(n));}
}
#[test]fn all_eight_original_component_glyphs_match_original_word_final_em_geometry(){
 let a=actual(PDF);let native=kanpo_pdf::extract_pages(PDF).unwrap();let words=kanpo_pdf::bbox::words(&native[0]);let p:Value=serde_json::from_str(PAIRS).unwrap();let mut seen=std::collections::BTreeSet::new();
 for pair in p["pairs"].as_array().unwrap(){
  let id=pair["Empty_component_Word"]["id"].as_u64().unwrap()as usize;let w=&a.pages[0].words[id];let matching:Vec<_>=words.iter().filter(|z|z.text==w.raw&&[z.x_min,z.y_min,z.x_max,z.y_max].iter().zip([w.x0,w.y0,w.x1,w.y1]).all(|(x,y)|(x-y as f64).abs()<0.0002)).collect();assert_eq!(matching.len(),1);assert_eq!(matching[0].glyphs.len(),1);
  for gid in pair["native_Glyph_ids"].as_array().unwrap(){let gid=gid.as_u64().unwrap()as usize;assert!(seen.insert(gid));}
 }assert_eq!(seen.len(),8);
}
#[test]fn non_target_words_old_owner_lines_and_structure_are_preserved(){
 let a=actual(PDF);let v=value(&a);let b:Value=serde_json::from_str(BEFORE).unwrap();let affected=[165,149,71,61,159,147,67,60];
 for(i,(o,n))in b["pages"][0]["words"].as_array().unwrap().iter().zip(v["pages"][0]["words"].as_array().unwrap()).enumerate(){let mut n=n.clone();if affected.contains(&i){n["text"]=o["text"].clone();n["status"]=o["status"].clone();}assert_eq!(o,&n);}
 assert_eq!(a.text.lines().count(),b["text"].as_str().unwrap().lines().count());assert_eq!(a.articles.len(),1);
 for (old,new)in b["pages"][0]["regions"].as_array().unwrap().iter().zip(v["pages"][0]["regions"].as_array().unwrap()){
  for k in ["unit","part","role"]{assert_eq!(old[k],new[k]);}
  for (o,n)in old["lines"].as_array().unwrap().iter().zip(new["lines"].as_array().unwrap()){let filtered:Vec<_>=n["words"].as_array().unwrap().iter().filter(|v|![149,61,147,60].contains(&(v.as_u64().unwrap()as usize))).cloned().collect();assert_eq!(o["words"],Value::Array(filtered));}
 }
}
#[test]fn complete_analysis_negative_20260930g002150028(){let a=actual(include_bytes!("archive/pages/20260930g00215/20260930g002150028.pdf"));let b:Value=serde_json::from_str(include_str!("expected/range_endpoints/20260930g002150028-before43.json")).unwrap();assert_eq!(value(&a),b);}
#[test]fn complete_analysis_negative_20260828g001920015(){let a=actual(include_bytes!("archive/pages/20260828g00192/20260828g001920015.pdf"));let b:Value=serde_json::from_str(include_str!("expected/range_endpoints/20260828g001920015-before43.json")).unwrap();assert_eq!(value(&a),b);}

#[test]
fn all_ten_original_pages_twenty_pairs_keep_source_glyph_ids_owners_and_complete_documents() {
 let proof:Value=serde_json::from_str(include_str!("expected/range_endpoints/all20-pairs-provenance-expected.json")).unwrap();
 let literals: &[(&str,usize,usize,&str)] = &[
 ("20260930g002150029",181,172,"（２１）"),("20260930g002150029",80,74,"（２０）"),
 ("20260930g002150030",165,149,"（２１）"),("20260930g002150030",159,147,"（１７）"),("20260930g002150030",71,61,"（２０）"),("20260930g002150030",67,60,"（１６）"),
 ("20260930g002150031",208,200,"（１７）"),("20260930g002150031",95,90,"（１６）"),
 ("20260930g002150032",76,63,"（２１）"),
 ("20260930g002150033",178,163,"（１７）"),("20260930g002150033",81,67,"（２１）"),("20260930g002150033",76,66,"（１６）"),
 ("20260930g002150034",202,188,"（１７）"),("20260930g002150034",90,79,"（１６）"),
 ("20260930g002150035",230,217,"（２０）"),("20260930g002150036",187,177,"（２０）"),
 ("20260930g002150053",149,141,"（２１）"),("20260930g002150053",70,66,"（２０）"),
 ("20260930g002150055",174,162,"（２０）"),("20260930g002150055",77,67,"（２１）")];
 let pdfs: &[(&str,&[u8])] = &[("20260930g002150029",include_bytes!("archive/pages/20260930g00215/20260930g002150029.pdf")),("20260930g002150030",include_bytes!("archive/pages/20260930g00215/20260930g002150030.pdf")),("20260930g002150031",include_bytes!("archive/pages/20260930g00215/20260930g002150031.pdf")),("20260930g002150032",include_bytes!("archive/pages/20260930g00215/20260930g002150032.pdf")),("20260930g002150033",include_bytes!("archive/pages/20260930g00215/20260930g002150033.pdf")),("20260930g002150034",include_bytes!("archive/pages/20260930g00215/20260930g002150034.pdf")),("20260930g002150035",include_bytes!("archive/pages/20260930g00215/20260930g002150035.pdf")),("20260930g002150036",include_bytes!("archive/pages/20260930g00215/20260930g002150036.pdf")),("20260930g002150053",include_bytes!("archive/pages/20260930g00215/20260930g002150053.pdf")),("20260930g002150055",include_bytes!("archive/pages/20260930g00215/20260930g002150055.pdf"))];
 let mut checked=0;let mut missing=vec![];let mut unequal=vec![];
 for row in proof["pages"].as_array().unwrap() {
  let page=row["page"].as_str().unwrap();let pdf=pdfs.iter().find(|x|x.0==page).unwrap().1;
  let a=actual(pdf);let actual=value(&a);let native=kanpo_pdf::extract_pages(pdf).unwrap();let nativewords=kanpo_pdf::bbox::words(&native[0]);
  let before=&row["beforeAnalysis"];let p=&actual["pages"][0];
  for pair in row["pairs"].as_array().unwrap() {
   let l=pair["left"].as_u64().unwrap()as usize;let r=pair["right"].as_u64().unwrap()as usize;let ep=pair["endpoint"].as_str().unwrap();
   assert!(literals.contains(&(page,l,r,ep)));checked+=1;
   assert_eq!(before["pages"][0]["words"][l],pair["prefixWord"]);assert_eq!(before["pages"][0]["words"][r],pair["emptySuffixWord"]);
   for id in [l,r]{
    let w=&a.pages[0].words[id];let ns:Vec<_>=nativewords.iter().filter(|n|n.text==w.raw&&[n.x_min,n.y_min,n.x_max,n.y_max].iter().zip([w.x0,w.y0,w.x1,w.y1]).all(|(x,y)|(x-y as f64).abs()<0.0002)).collect();assert_eq!(ns.len(),1,"{page} Word{id} native mapping");
    let g=pair["original_native_Glyphs"][if id==l{0}else{1}].clone();let gid=g["id"].as_u64().unwrap()as usize;assert!(ns[0].glyphs.contains(&gid));
    let ng:Value=serde_json::from_str(&serde_json::to_string(&native[0].glyphs[gid]).unwrap()).unwrap();let mut expected=g.as_object().unwrap().clone();expected.remove("id");expected.remove("font_descriptor");assert_eq!(ng,Value::Object(expected));
   }
   let ri=pair["prefix_owner"]["region"].as_u64().unwrap()as usize;let li=pair["prefix_owner"]["line"].as_u64().unwrap()as usize;
   let owner=&p["regions"][ri]["lines"][li];
   if p["words"][l]!=pair["candidatePrefixWord"]||p["words"][r]!=pair["candidateSuffixWord"]||owner["words"]!=pair["prefix_owner"]["new_ids"]||owner["text"]!=pair["prefix_owner"]["new_text"]{missing.push(format!("{page} {l}/{r} {ep}"));}
  }
  for (i,(o,n)) in before["pages"][0]["words"].as_array().unwrap().iter().zip(p["words"].as_array().unwrap()).enumerate(){
   let mut restored=n.clone();if row["pairs"].as_array().unwrap().iter().any(|v|v["left"]==i||v["right"]==i){restored["text"]=o["text"].clone();restored["status"]=o["status"].clone();}assert_eq!(&restored,o,"{page} Word{i} raw/ID/bbox/non-target attributes");
  }
  if actual!=row["candidateAnalysis"]{unequal.push(page.to_owned());}
 }
 assert_eq!(checked,20);assert!(missing.is_empty(),"all original endpoints still absent/wrong: {missing:?}");assert!(unequal.is_empty(),"complete Analysis differs from reviewed expected: {unequal:?}");
}

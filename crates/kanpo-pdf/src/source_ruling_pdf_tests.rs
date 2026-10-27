use crate::cff_aj1::rectangles::tests::{font, program};
use crate::*;
fn stream(data: &[u8], extra: &str) -> Vec<u8> {
    let mut s = format!("<< /Length {} {extra} >>\nstream\n", data.len()).into_bytes();
    s.extend(data);
    s.extend(b"\nendstream");
    s
}
fn pdf(cid: u16, cff: Vec<u8>, prefix: &str, text: &str, gs: &str, encoding: &str) -> Vec<u8> {
    pdf_scene(cid,cff,prefix,text,gs,encoding,true)
}
fn pdf_scene(cid:u16,cff:Vec<u8>,prefix:&str,text:&str,gs:&str,encoding:&str,mapped:bool)->Vec<u8>{
    pdf_scene_unicode(cid,cff,prefix,text,gs,(encoding,mapped,"E101",""))
}
fn pdf_scene_unicode(cid:u16,cff:Vec<u8>,prefix:&str,text:&str,gs:&str,config:(&str,bool,&str,&str))->Vec<u8>{
    let (encoding,mapped,unicode_hex,metrics)=config;
    let unicode=if mapped {"/ToUnicode 9 0 R"}else{""};
    let hex = format!("{cid:04X}").repeat(3);
    let content = format!("{prefix} BT /F1 8 Tf 1 0 0 1 20 150 Tm {text} <{hex}> Tj ET");
    let objects=vec![
 b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
 b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << /Font << /F1 5 0 R >> /ExtGState << /ArbitraryState 10 0 R >> >> /Contents 4 0 R >>".to_vec(),
 stream(content.as_bytes(),""),
 format!("<< /Type /Font /Subtype /Type0 /BaseFont /Unrelated /Encoding /{encoding} /DescendantFonts [6 0 R] {unicode} >>").into_bytes(),
 format!("<< /Type /Font /Subtype /CIDFontType0 /BaseFont /Unrelated /CIDSystemInfo << /Registry (Other) /Ordering (Other) /Supplement 0 >> /FontDescriptor 7 0 R /DW 1000 {metrics} >>").into_bytes(),
 b"<< /Type /FontDescriptor /FontName /Unrelated /Flags 4 /FontBBox [0 -120 1000 880] /Ascent 880 /Descent -120 /FontFile3 8 0 R >>".to_vec(),
 stream(&cff,"/Subtype /CIDFontType0C"),
 stream(format!("1 begincodespacerange <0000> <FFFF> endcodespacerange 1 beginbfchar <{cid:04X}> <{unicode_hex}> endbfchar").as_bytes(),""),
 format!("<< /Type /ExtGState {gs} >>").into_bytes()];
    let mut result = b"%PDF-1.7\n".to_vec();
    let mut offsets = vec![0];
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(result.len());
        result.extend(format!("{} 0 obj\n", i + 1).as_bytes());
        result.extend(obj);
        result.extend(b"\nendobj\n");
    }
    let start = result.len();
    result.extend(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()).as_bytes());
    for p in &offsets[1..] {
        result.extend(format!("{p:010} 00000 n \n").as_bytes());
    }
    result.extend(
        format!(
            "trailer << /Size {} /Root 1 0 R >>\nstartxref\n{start}\n%%EOF\n",
            offsets.len()
        )
        .as_bytes(),
    );
    result
}
fn cff(cid: u16, points: &[[i16; 2]]) -> Vec<u8> {
    font(
        cid,
        program(points),
        "AnyName",
        "AnyRegistry",
        "AnyOrdering",
        &[],
        &[],
    )
}
fn rectangle(cid: u16) -> Vec<u8> {
    cff(cid, &[[0, 0], [1000, 0], [1000, 125], [0, 125]])
}
fn html(bytes: &[u8]) -> String {
    bbox::bbox_layout_with_native_styles(&extract_pages_with_aj1_sources(bytes).unwrap())
}
fn has_rule(s: &str) -> bool {
    s.contains("data-native-ruling=\"horizontal\"") || s.contains("data-native-ruling=\"vertical\"")
}
#[test]
fn embedded_outline_authority_is_added_only_to_the_owned_native_styled_renderer() {
    for cid in [73, 43021] {
        let bytes = pdf(cid, rectangle(cid), "", "", "", "Identity-H");
        let old = extract_pages(&bytes).unwrap();
        let source = extract_pages_with_aj1_sources(&bytes).unwrap();
        assert_eq!(
            serde_json::to_value(&old).unwrap(),
            serde_json::to_value(source.iter().map(|s| s.page()).collect::<Vec<_>>()).unwrap()
        );
        assert_eq!(
            bbox::bbox_layout(&old),
            bbox::bbox_layout_with_aj1_sources(&source)
        );
        let styled = bbox::bbox_layout_with_native_styles(&source);
        assert!(styled.contains("data-native-ruling=\"horizontal\""));
        assert!(styled.contains("data-native-ink-xmin=\"20\""));
        assert!(styled.contains("data-native-ink-xmax=\"44\""));
        assert!(styled.contains("data-native-ink-ymin=\"49\""));
        assert!(styled.contains("data-native-ink-ymax=\"50\""));
        assert_eq!(old[0].glyphs.len(), 3);
        assert!(old[0]
            .glyphs
            .iter()
            .all(|g| g.cid == cid as u32 && g.text == "\u{e101}"));
        assert!(!styled.contains("data-source-aj1-text"));
    }
}
#[test]
fn real_pdf_transforms_and_solid_default_graphics_states_keep_the_outline_proof() {
    for (prefix, text, gs, axis) in [
        ("", "", "", "horizontal"),
        ("/ArbitraryState gs", "", "/SA true /SM 0.7", "horizontal"),
        ("0 0 200 200 re W n", "", "", "horizontal"),
        ("2 0 0 0.5 0 0 cm", "", "", "horizontal"),
        ("0 1 -1 0 200 0 cm", "", "", "vertical"),
        (
            "/ArbitraryState gs",
            "",
            "/ca 1 /CA 0.25 /SMask /None /BM /Normal",
            "horizontal",
        ),
    ] {
        let bytes = pdf(73, rectangle(73), prefix, text, gs, "Identity-H");
        assert!(
            html(&bytes).contains(&format!("data-native-ruling=\"{axis}\"")),
            "{prefix} {gs}"
        );
    }
}
#[test]
fn unsupported_masks_patterns_clips_text_modes_and_shears_are_not_certificates() {
    for (prefix, text, gs) in [
        ("", "1 Tr", ""),
        ("", "3 Tr", ""),
        ("", "4 Tr", ""),
        ("1 0.5 0 1 0 0 cm", "", ""),
        ("0 0 10 10 re W n", "", ""),
        ("0 0 m 100 20 100 30 0 100 c W n", "", ""),
        ("/ArbitraryState gs", "", "/ca 0.5"),
        ("/ArbitraryState gs", "", "/SMask << /S /Alpha >>"),
        ("/Pattern cs /SomePattern scn", "", ""),
        ("Q", "", ""),
        ("1 0 cm", "", ""),
        ("/ArbitraryState gs", "", "/Font [/F1 8]"),
    ] {
        let bytes = pdf(73, rectangle(73), prefix, text, gs, "Identity-H");
        assert!(!has_rule(&html(&bytes)), "{prefix} {text} {gs}");
    }
}
#[test]
fn incomplete_outlines_disconnected_glyphs_and_unknown_encoding_remain_unknown() {
    for points in [
        vec![[125, 0], [875, 0], [875, 125], [125, 125]],
        vec![[0, 0], [1000, 0], [0, 125]],
    ] {
        let bytes = pdf(73, cff(73, &points), "", "", "", "Identity-H");
        assert!(!has_rule(&html(&bytes)));
    }
    for encoding in ["Unknown-H", "Identity-V"] {
        let bytes = pdf(73, rectangle(73), "", "", "", encoding);
        assert!(!has_rule(&html(&bytes)));
    }
}

#[test]
fn scene_rectangles_follow_vertical_origins_with_arbitrary_CIDs_and_unmapped_fonts(){
 for cid in [73,43021]{
  let bytes=pdf_scene(cid,rectangle(cid),"","","","Identity-V",false);
  let sources=extract_pages_with_aj1_sources(&bytes).unwrap();
  let scene=crate::source_scene::capture(&sources[0]).unwrap();
  assert_eq!(scene.glyphs,3);assert!(scene.unknown.is_empty());
  assert_eq!(scene.rectangles.len(),3);
  for (got,want) in scene.rectangles.iter().zip([[16.,56.04,24.,57.04],[16.,64.04,24.,65.04],[16.,72.04,24.,73.04]]){
   assert!(got.iter().zip(want).all(|(a,b)|(*a-b).abs()<1e-10),"{got:?} vs {want:?}");
  }
  assert!(!has_rule(&bbox::bbox_layout_with_native_styles(&sources)),"scene proof never becomes a legacy vertical ruling");
  assert_eq!(serde_json::to_value(extract_pages(&bytes).unwrap()).unwrap(),serde_json::to_value(sources.iter().map(|s|s.page()).collect::<Vec<_>>()).unwrap());
 }
}
#[test]
fn nonrectangular_or_unproved_omitted_glyphs_remain_unknown_in_the_scene(){
 for (points,prefix,text,gs,encoding) in [
  (vec![[0,0],[1000,0],[0,125]],"","","","Identity-V"),
  (vec![[0,0],[1000,0],[1000,125],[0,125]],"1 0.5 0 1 0 0 cm","","","Identity-V"),
  (vec![[0,0],[1000,0],[1000,125],[0,125]],"","1 Tr","","Identity-V"),
  (vec![[0,0],[1000,0],[1000,125],[0,125]],"/ArbitraryState gs","","/ca 0.5","Identity-V"),
 ]{
 let bytes=pdf_scene(81,cff(81,&points),prefix,text,gs,encoding,false);
 let sources=extract_pages_with_aj1_sources(&bytes).unwrap();
 let candidate=crate::source_scene::capture(&sources[0]);
 if !gs.is_empty(){assert!(candidate.is_none(),"unsupported graphics state");continue;}
 let scene=candidate.unwrap();
 assert!(scene.rectangles.is_empty());assert_eq!(scene.unknown,vec![0,1,2],"{prefix} {text} {gs}");
 }
}

#[test]
fn an_unmapped_CID_that_looks_like_Unicode_space_is_not_source_whitespace(){
 let bytes=pdf_scene(32,cff(32,&[[0,0],[1000,0],[0,125]]),"","","","Identity-V",false);
 let sources=extract_pages_with_aj1_sources(&bytes).unwrap();let scene=crate::source_scene::capture(&sources[0]).unwrap();
 assert!(scene.ignored.is_empty());assert_eq!(scene.unknown,vec![0,1,2]);
}

#[test]
fn explicit_Unicode_space_also_cannot_erase_a_visible_unknown_outline(){
 let bytes=pdf_scene_unicode(73,cff(73,&[[0,0],[1000,0],[0,125]]),"","","",("Identity-V",true,"0020",""));
 let sources=extract_pages_with_aj1_sources(&bytes).unwrap();let scene=crate::source_scene::capture(&sources[0]).unwrap();
 assert!(scene.ignored.is_empty());assert_eq!(scene.unknown,vec![0,1,2]);
}

#[test]
fn vertical_outline_positions_use_actual_W2_metrics_under_independent_transforms(){
 for (prefix,expected) in [("",[18.4,54.6,26.4,55.6]),("2 0 0 0.5 0 0 cm",[36.8,127.3,52.8,127.8]),("-1 0 0 1 100 0 cm",[73.6,54.6,81.6,55.6])]{
  for cid in [73,43021]{
   let metrics=format!("/W2 [{cid} [-900 200 700]]");
   let bytes=pdf_scene_unicode(cid,rectangle(cid),prefix,"","",("Identity-V",false,"E101",&metrics));
   let sources=extract_pages_with_aj1_sources(&bytes).unwrap();let scene=crate::source_scene::capture(&sources[0]).unwrap();
   assert_eq!(scene.rectangles.len(),3);
   assert!(scene.rectangles[0].iter().zip(expected).all(|(a,b)|(*a-b).abs()<1e-10),"{prefix} {:?} vs {expected:?}",scene.rectangles[0]);
   assert!(!has_rule(&bbox::bbox_layout_with_native_styles(&sources)));
  }
 }
}

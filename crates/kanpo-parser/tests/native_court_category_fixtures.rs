//! Original category ownership witnesses and reviewed regression freezes.
//! Raster-body omissions and unrelated existing boundaries are not full-page graded.
fn check(id: &str, pdf: &[u8], golden: &str) {
    let a=kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let witnesses:serde_json::Value=serde_json::from_str(include_str!("data/native-court-categories/source-category-witnesses133.json")).unwrap();
    let row=witnesses["rows"].as_array().unwrap().iter().find(|r|r["page"]==id).unwrap();
    for witness in row["witnesses"].as_array().unwrap(){
        let mut prefix=vec![witness["first_line"].as_str().unwrap()];
        prefix.extend(witness["next_lines"].as_array().unwrap().iter().map(|s|s.as_str().unwrap()));
        let prefix=prefix.join("\n");
        let owners=a.articles.iter().filter(|article|article.text.starts_with(&prefix)).collect::<Vec<_>>();
        assert_eq!(owners.len(),1,"original category must begin its first case: {id}: {prefix}");
    }
    // Preserve exact f32 serialization; Value::from can promote geometry to f64.
    let actual:serde_json::Value=serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    let expected:serde_json::Value=serde_json::from_str(golden).unwrap();
    assert!(actual==expected,"reviewed complete Analysis regression: {id}");
}
macro_rules! original {
    ($name:ident,$id:literal,$pdf:literal)=>{
        #[test] fn $name(){check($id,include_bytes!($pdf),include_str!(concat!("data/native-court-categories/",$id,"-reviewed133.json")));}
    };
}
original!(page_20260717h017500021,"20260717h017500021","data/orthogonal-gutters/20260717h017500021.pdf");
original!(page_20260727h017550018,"20260727h017550018","data/native-court-categories/20260727h017550018.pdf");
original!(page_20260728h017560022,"20260728h017560022","data/native-atomic-columns/20260728h017560022.pdf");
original!(page_20260729h017570017,"20260729h017570017","data/native-court-categories/20260729h017570017.pdf");
original!(page_20260729h017570018,"20260729h017570018","data/native-court-categories/20260729h017570018.pdf");
original!(page_20260731h017590022,"20260731h017590022","data/native-court-categories/20260731h017590022.pdf");
original!(page_20260803h017600020,"20260803h017600020","data/native-court-categories/20260803h017600020.pdf");
original!(page_20260805h017620026,"20260805h017620026","data/native-court-categories/20260805h017620026.pdf");
original!(page_20260807h017640023,"20260807h017640023","data/native-court-categories/20260807h017640023.pdf");
original!(page_20260810h017650020,"20260810h017650020","data/native-court-categories/20260810h017650020.pdf");
original!(page_20260813h017670024,"20260813h017670024","data/native-court-categories/20260813h017670024.pdf");
original!(page_20260814h017680025,"20260814h017680025","data/native-court-categories/20260814h017680025.pdf");
original!(page_20260817h017690018,"20260817h017690018","data/native-court-categories/20260817h017690018.pdf");
original!(page_20260819h017710021,"20260819h017710021","data/native-court-categories/20260819h017710021.pdf");
original!(page_20260820h017720021,"20260820h017720021","data/native-court-categories/20260820h017720021.pdf");
original!(page_20260826h017760023,"20260826h017760023","data/native-atomic-columns/20260826h017760023.pdf");
original!(page_20260826h017760024,"20260826h017760024","data/native-court-categories/20260826h017760024.pdf");
original!(page_20260901h017800020,"20260901h017800020","data/native-court-categories/20260901h017800020.pdf");
original!(page_20260903h017820028,"20260903h017820028","data/native-court-categories/20260903h017820028.pdf");
original!(page_20260904h017830021,"20260904h017830021","data/native-court-categories/20260904h017830021.pdf");
original!(page_20260907h017840027,"20260907h017840027","data/native-court-categories/20260907h017840027.pdf");
original!(page_20260908h017850023,"20260908h017850023","data/native-court-categories/20260908h017850023.pdf");
original!(page_20260910h017870019,"20260910h017870019","data/native-court-categories/20260910h017870019.pdf");
original!(page_20260911h017880022,"20260911h017880022","data/native-court-categories/20260911h017880022.pdf");
original!(page_20260914h017890021,"20260914h017890021","data/native-court-categories/20260914h017890021.pdf");
original!(page_20260916h017910021,"20260916h017910021","data/native-court-categories/20260916h017910021.pdf");
original!(page_20260917h017920023,"20260917h017920023","data/native-atomic-columns/20260917h017920023.pdf");
original!(page_20260918h017930022,"20260918h017930022","data/native-court-categories/20260918h017930022.pdf");
original!(page_20260924h017940021,"20260924h017940021","data/native-court-categories/20260924h017940021.pdf");
original!(page_20260928h017960017,"20260928h017960017","data/native-court-categories/20260928h017960017.pdf");
original!(page_20260928h017960018,"20260928h017960018","data/native-court-categories/20260928h017960018.pdf");
original!(page_20260930h017980021,"20260930h017980021","data/native-court-categories/20260930h017980021.pdf");
original!(page_20261002h018000023,"20261002h018000023","data/native-court-categories/20261002h018000023.pdf");
original!(page_20261006h018020023,"20261006h018020023","data/native-court-categories/20261006h018020023.pdf");
original!(page_20261008h018040023,"20261008h018040023","data/native-court-categories/20261008h018040023.pdf");
original!(page_20261008h018040024,"20261008h018040024","data/native-court-categories/20261008h018040024.pdf");
original!(page_20261013h018060019,"20261013h018060019","data/native-court-categories/20261013h018060019.pdf");
original!(page_20261014h018070022,"20261014h018070022","data/native-court-categories/20261014h018070022.pdf");
original!(page_20261015h018080024,"20261015h018080024","data/native-court-categories/20261015h018080024.pdf");
original!(page_20261016h018090023,"20261016h018090023","data/native-court-categories/20261016h018090023.pdf");
original!(page_20261016h018090024,"20261016h018090024","data/native-court-categories/20261016h018090024.pdf");
original!(page_20261019h018100022,"20261019h018100022","data/native-court-categories/20261019h018100022.pdf");
original!(page_20261020h018110017,"20261020h018110017","data/native-atomic-columns/20261020h018110017.pdf");

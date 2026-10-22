#[path = "support/legacy_analysis.rs"]
mod legacy_analysis;
use kanpo_parser::{analyze_item,page_inputs};
fn analyzed(page:&str)->kanpo_parser::Analysis {
    let bytes: &[u8] = match page {
        "20260730g001710049" => include_bytes!("archive/pages/20260730g00171/20260730g001710049.pdf"),
        "20260730g001710050" => include_bytes!("archive/pages/20260730g00171/20260730g001710050.pdf"),
        _ => panic!("unknown fixture"),
    };
    analyze_item(&page_inputs(bytes).unwrap())
}
#[test]
fn first_personal_goal_retains_native_parenthesized_roman_I() {
    let a=analyzed("20260730g001710049");
    assert_eq!(a.text,include_str!("archive/expected-roman-one/20260730g001710049.txt"));
    assert_eq!(a.text.lines().nth(2).unwrap(),"（Ⅰ）　食育に関心を持つ");
    assert_eq!(a.pages[0].words[131].raw,"\u{e82f}");
    assert_eq!(a.pages[0].words[131].text,"（Ⅰ）");
    assert_eq!(a.pages[0].words[131].status,kanpo_parser::trace::WordStatus::Used);
}
#[test]
fn first_school_goal_keeps_its_native_marker_in_the_right_logical_position() {
    let a=analyzed("20260730g001710050");
    assert_eq!(a.text,include_str!("archive/expected-roman-one/20260730g001710050.txt"));
    assert!(a.text.lines().any(|s|s=="（Ⅰ）　学校給食における地場産物等を活用した食育に関する取組等を増やす"));
    assert_eq!(a.pages[0].words[113].raw,"\u{e82f}");
    assert_eq!(a.pages[0].words[113].text,"（Ⅰ）");
    assert_eq!(a.pages[0].words[113].status,kanpo_parser::trace::WordStatus::Used);
}
#[test]
fn all_other_words_and_the_already_supported_roman_goals_remain_exact() {
    for (page,id) in [("20260730g001710049",131),("20260730g001710050",113)] {
        let a=analyzed(page);
        let old:serde_json::Value=serde_json::from_str(match page {
            "20260730g001710049" => include_str!("archive/expected-roman-one/20260730g001710049-before.json"),
            "20260730g001710050" => include_str!("archive/expected-roman-one/20260730g001710050-before.json"),
            _ => unreachable!(),
        }).unwrap();
        let new:serde_json::Value=legacy_analysis::value(&a);
        for (i,(b,n)) in old["words"].as_array().unwrap().iter().zip(new["pages"][0]["words"].as_array().unwrap()).enumerate(){
            for k in ["id","raw","x0","x1","y0","y1"] {assert_eq!(b[k],n[k],"{page} word{i} {k}");}
            if i!=id {assert_eq!(b,n,"unchanged word{page}/{i}");}
        }
        let count = |v: &serde_json::Value, word: usize| -> usize {
            v["regions"].as_array().unwrap().iter().flat_map(|r|r["lines"].as_array().unwrap())
                .map(|l| l["words"].as_array().unwrap().iter().filter(|n|n.as_u64()==Some(word as u64)).count()
                    + l.get("table").and_then(|t|t.get("cells")).and_then(|c|c.as_array()).map(|cs|cs.iter().flat_map(|c|c["words"].as_array().unwrap()).filter(|n|n.as_u64()==Some(word as u64)).count()).unwrap_or(0)).sum()
        };
        for i in 0..old["words"].as_array().unwrap().len() {
            if i!=id { assert_eq!(count(&old,i),count(&new["pages"][0],i),"existing owner {page}/{i}"); }
        }
    }
}

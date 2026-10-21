#[path = "support/expected_article_partitions.rs"]
mod expected_article_partitions;
use serde_json::Value;
fn actual(pdf: &[u8]) -> Value {
    serde_json::from_str(
        &serde_json::to_string(&kanpo_parser::analyze_item(
            &kanpo_parser::page_inputs(pdf).unwrap(),
        ))
        .unwrap(),
    )
    .unwrap()
}
fn replace_text(v: &mut Value, old: char, new: char) {
    match v {
        Value::String(s) => *s = s.replace(old, &new.to_string()),
        Value::Array(xs) => {
            for x in xs {
                replace_text(x, old, new)
            }
        }
        Value::Object(xs) => {
            for (key, x) in xs {
                if key != "raw" {
                    replace_text(x, old, new)
                }
            }
        }
        _ => {}
    }
}
#[test]
fn page_20260716g001590048() {
    let a = actual(include_bytes!(
        "archive/pages/20260716g00159/20260716g001590048.pdf"
    ));
    let mut b: Value = serde_json::from_str(include_str!(
        "expected/identity_surnames/20260716g001590048-before44.json"
    ))
    .unwrap();
    assert_eq!(a["pages"][0]["words"][170]["text"], "樽");
    assert!(a["text"].as_str().unwrap().contains("代表清算人樽見利徳"));
    replace_text(&mut b, '婲', '樽');
    let b = expected_article_partitions::with_articles(
        b,
        include_str!("expected-native-article-partitions/20260716g001590048.json"),
    );
    assert_eq!(a, b);
}
#[test]
fn page_20260723g001630196() {
    let a = actual(include_bytes!(
        "archive/pages/20260723g00163/20260723g001630196.pdf"
    ));
    let mut b: Value = serde_json::from_str(include_str!(
        "expected/identity_surnames/20260723g001630196-before44.json"
    ))
    .unwrap();
    assert_eq!(a["pages"][0]["words"][20]["text"], "濰");
    assert!(a["text"].as_str().unwrap().contains("代表清算人鄒濰鍇"));
    replace_text(&mut b, '䈹', '濰');
    let b = expected_article_partitions::with_articles(
        b,
        include_str!("expected-native-article-partitions/20260723g001630196.json"),
    );
    assert_eq!(a, b);
}
#[test]
fn page_20260729h017570030() {
    let a = actual(include_bytes!(
        "archive/pages/20260729h01757/20260729h017570030.pdf"
    ));
    let mut b: Value = serde_json::from_str(include_str!(
        "expected/identity_surnames/20260729h017570030-before44.json"
    ))
    .unwrap();
    assert_eq!(a["pages"][0]["words"][224]["text"], "團");
    assert!(a["text"].as_str().unwrap().contains("代表取締役團塚真一郎"));
    replace_text(&mut b, '升', '團');
    let b = expected_article_partitions::with_articles(
        b,
        include_str!("expected-native-article-partitions/20260729h017570030.json"),
    );
    assert_eq!(a, b);
}
#[test]
fn page_20260729h017570031() {
    let a = actual(include_bytes!(
        "archive/pages/20260729h01757/20260729h017570031.pdf"
    ));
    let mut b: Value = serde_json::from_str(include_str!(
        "expected/identity_surnames/20260729h017570031-before44.json"
    ))
    .unwrap();
    assert_eq!(a["pages"][0]["words"][200]["text"], "蓬");
    assert!(a["text"].as_str().unwrap().contains("代表取締役蓬田和平"));
    replace_text(&mut b, '媡', '蓬');
    let b = expected_article_partitions::with_articles(
        b,
        include_str!("expected-native-article-partitions/20260729h017570031.json"),
    );
    assert_eq!(a, b);
}
#[test]
fn page_20260716g001590049() {
    let a = actual(include_bytes!(
        "archive/pages/20260716g00159/20260716g001590049.pdf"
    ));
    let b: Value = serde_json::from_str(include_str!(
        "expected/identity_surnames/20260716g001590049-before44.json"
    ))
    .unwrap();
    let b = expected_article_partitions::with_articles(
        b,
        include_str!("expected-native-article-partitions/20260716g001590049.json"),
    );
    assert_eq!(a, b);
}
#[test]
fn page_20260723g001630195() {
    let a = actual(include_bytes!(
        "archive/pages/20260723g00163/20260723g001630195.pdf"
    ));
    let b: Value = serde_json::from_str(include_str!(
        "expected/identity_surnames/20260723g001630195-before44.json"
    ))
    .unwrap();
    let b = expected_article_partitions::with_articles(
        b,
        include_str!("expected-native-article-partitions/20260723g001630195.json"),
    );
    assert_eq!(a, b);
}
#[test]
fn page_20260729h017570029() {
    let a = actual(include_bytes!(
        "archive/pages/20260729h01757/20260729h017570029.pdf"
    ));
    let b: Value = serde_json::from_str(include_str!(
        "expected/identity_surnames/20260729h017570029-before44.json"
    ))
    .unwrap();
    assert_eq!(a, b);
}
#[test]
fn page_20260729h017570032() {
    let a = actual(include_bytes!(
        "archive/pages/20260729h01757/20260729h017570032.pdf"
    ));
    let b: Value = serde_json::from_str(include_str!(
        "expected/identity_surnames/20260729h017570032-before44.json"
    ))
    .unwrap();
    let b = expected_article_partitions::with_articles(
        b,
        include_str!("expected-native-article-partitions/20260729h017570032.json"),
    );
    assert_eq!(a, b);
}

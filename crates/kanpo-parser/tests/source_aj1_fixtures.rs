#[path = "support/legacy_analysis.rs"]
mod legacy_analysis;
#[path = "support/expected_article_partitions.rs"]
mod expected_article_partitions;
use serde_json::Value;
fn actual(pdf: &[u8]) -> Value {
    legacy_analysis::value(&kanpo_parser::analyze_item(
        &kanpo_parser::page_inputs(pdf).unwrap(),
    ))
}
fn replace_text(v: &mut Value, old: char, new: char) {
    match v {
        Value::String(s) => *s = s.replace(old, &new.to_string()),
        Value::Array(a) => {
            for x in a {
                replace_text(x, old, new)
            }
        }
        Value::Object(a) => {
            for (k, x) in a {
                if k != "raw" {
                    replace_text(x, old, new)
                }
            }
        }
        _ => {}
    }
}
#[test]
fn page_20260818g001840038() {
    let a = actual(include_bytes!(
        "archive/pages/20260818g00184/20260818g001840038.pdf"
    ));
    let mut b: Value = serde_json::from_str(include_str!(
        "expected-source-aj1/20260818g001840038-before49.json"
    ))
    .unwrap();
    assert_eq!(a["pages"][0]["words"][57]["text"], "嬴");
    assert!(a["text"].as_str().unwrap().contains("代表清算人劉嬴"));
    replace_text(&mut b, '\u{446E}', '\u{5B34}');
    let b = expected_article_partitions::with_articles(
        b,
        include_str!("expected-native-article-partitions/20260818g001840038.json"),
    );
    assert_eq!(a, b);
}
#[test]
fn page_20260817g001830048() {
    let a = actual(include_bytes!(
        "archive/pages/20260817g00183/20260817g001830048.pdf"
    ));
    let mut b: Value = serde_json::from_str(include_str!(
        "expected-source-aj1/20260817g001830048-before49.json"
    ))
    .unwrap();
    assert_eq!(a["pages"][0]["words"][157]["text"], "瀀");
    assert!(a["text"].as_str().unwrap().contains("清算人高田瀀資"));
    replace_text(&mut b, '\u{4650}', '\u{7000}');
    let b = expected_article_partitions::with_articles(
        b,
        include_str!("expected-native-article-partitions/20260817g001830048.json"),
    );
    assert_eq!(a, b);
}
#[test]
fn page_20260819g001850040() {
    let a = actual(include_bytes!(
        "archive/pages/20260819g00185/20260819g001850040.pdf"
    ));
    let mut b: Value = serde_json::from_str(include_str!(
        "expected-source-aj1/20260819g001850040-before49.json"
    ))
    .unwrap();
    assert_eq!(a["pages"][0]["words"][49]["text"], "釬");
    assert!(a["text"].as_str().unwrap().contains("代表清算人金釬泳"));
    replace_text(&mut b, '\u{497D}', '\u{91EC}');
    let b = expected_article_partitions::with_articles(
        b,
        include_str!("expected-native-article-partitions/20260819g001850040.json"),
    );
    assert_eq!(a, b);
}
#[test]
fn page_20260818g001840037() {
    let a = actual(include_bytes!(
        "archive/pages/20260818g00184/20260818g001840037.pdf"
    ));
    let mut b: Value = serde_json::from_str(include_str!(
        "expected-source-aj1/20260818g001840037-before49.json"
    ))
    .unwrap();
    let b = expected_article_partitions::with_articles(
        b,
        include_str!("expected-native-article-partitions/20260818g001840037.json"),
    );
    assert_eq!(a, b);
}
#[test]
fn page_20260817g001830047() {
    let a = actual(include_bytes!(
        "archive/pages/20260817g00183/20260817g001830047.pdf"
    ));
    let mut b: Value = serde_json::from_str(include_str!(
        "expected-source-aj1/20260817g001830047-before49.json"
    ))
    .unwrap();
    let b = expected_article_partitions::with_articles(
        b,
        include_str!("expected-native-article-partitions/20260817g001830047.json"),
    );
    assert_eq!(a, b);
}
#[test]
fn page_20260819g001850039() {
    let a = actual(include_bytes!(
        "archive/pages/20260819g00185/20260819g001850039.pdf"
    ));
    let mut b: Value = serde_json::from_str(include_str!(
        "expected-source-aj1/20260819g001850039-before49.json"
    ))
    .unwrap();
    let b = expected_article_partitions::with_articles(
        b,
        include_str!("expected-native-article-partitions/20260819g001850039.json"),
    );
    assert_eq!(a, b);
}
#[test]
fn unicode_control_18000() {
    let a = actual(include_bytes!(
        "fixtures/source-aj1/20260817g001830048-unicode-control.pdf"
    ));
    let b: Value = serde_json::from_str(include_str!(
        "expected-source-aj1/20260817g001830048-unicode-control-before49.json"
    ))
    .unwrap();
    assert_eq!(a["pages"][0]["words"][157]["raw"], "瀀");
    assert_eq!(a["pages"][0]["words"][157]["text"], "瀀");
    let b = expected_article_partitions::with_articles(
        b,
        include_str!("expected-native-article-partitions/20260817g001830048-unicode-control.json"),
    );
    assert_eq!(a, b);
}
#[test]
fn unicode_control_17518() {
    let a = actual(include_bytes!(
        "fixtures/source-aj1/20260818g001840038-unicode-control.pdf"
    ));
    let b: Value = serde_json::from_str(include_str!(
        "expected-source-aj1/20260818g001840038-unicode-control-before49.json"
    ))
    .unwrap();
    assert_eq!(a["pages"][0]["words"][57]["raw"], "嬴");
    assert_eq!(a["pages"][0]["words"][57]["text"], "嬴");
    let b = expected_article_partitions::with_articles(
        b,
        include_str!("expected-native-article-partitions/20260818g001840038-unicode-control.json"),
    );
    assert_eq!(a, b);
}
#[test]
fn unicode_control_18813() {
    let a = actual(include_bytes!(
        "fixtures/source-aj1/20260819g001850040-unicode-control.pdf"
    ));
    let b: Value = serde_json::from_str(include_str!(
        "expected-source-aj1/20260819g001850040-unicode-control-before49.json"
    ))
    .unwrap();
    assert_eq!(a["pages"][0]["words"][49]["raw"], "釬");
    assert_eq!(a["pages"][0]["words"][49]["text"], "釬");
    let b = expected_article_partitions::with_articles(
        b,
        include_str!("expected-native-article-partitions/20260819g001850040-unicode-control.json"),
    );
    assert_eq!(a, b);
}

// Further source-proven positives discovered by the fresh complete corpus gate.
#[test]
fn source_20260730g001710139_complete_original() {
    let a = actual(include_bytes!(
        "archive/pages/20260730g00171/20260730g001710139.pdf"
    ));
    let e: Value = serde_json::from_str(include_str!(
        "expected-source-aj1/20260730g001710139-candidate51.json"
    ))
    .unwrap();
    assert_eq!(a["pages"][0]["words"][48],serde_json::from_str::<Value>("{\"id\": 48, \"x0\": 444.1921, \"y0\": 404.8399, \"x1\": 452.1921, \"y1\": 412.8399, \"raw\": \"󴧴\", \"text\": \"頔\", \"status\": \"used\"}").unwrap());
    let e = expected_article_partitions::with_articles(
        e,
        include_str!("expected-native-article-partitions/20260730g001710139.json"),
    );
    assert_eq!(a, e);
}
#[test]
fn source_20260812g001800058_complete_original() {
    let a = actual(include_bytes!(
        "archive/pages/20260812g00180/20260812g001800058.pdf"
    ));
    let e: Value = serde_json::from_str(include_str!(
        "expected-source-aj1/20260812g001800058-candidate51.json"
    ))
    .unwrap();
    assert_eq!(a["pages"][0]["words"][161],serde_json::from_str::<Value>("{\"id\": 161, \"x0\": 175.7361, \"y0\": 580.8399, \"x1\": 183.7361, \"y1\": 588.8399, \"raw\": \"󴜾\", \"text\": \"禕\", \"status\": \"used\"}").unwrap());
    let e = expected_article_partitions::with_articles(
        e,
        include_str!("expected-native-article-partitions/20260812g001800058.json"),
    );
    assert_eq!(a, e);
}
#[test]
fn source_20260814g001820078_complete_original() {
    let a = actual(include_bytes!(
        "archive/pages/20260814g00182/20260814g001820078.pdf"
    ));
    let e: Value = serde_json::from_str(include_str!(
        "expected-source-aj1/20260814g001820078-candidate51.json"
    ))
    .unwrap();
    assert_eq!(a["pages"][0]["words"][174],serde_json::from_str::<Value>("{\"id\": 174, \"x0\": 167.2481, \"y0\": 772.8399, \"x1\": 175.2481, \"y1\": 780.8399, \"raw\": \"󴙳\", \"text\": \"熠\", \"status\": \"used\"}").unwrap());
    let e = expected_article_partitions::with_articles(
        e,
        include_str!("expected-native-article-partitions/20260814g001820078.json"),
    );
    assert_eq!(a, e);
}
#[test]
fn source_20260828g001920071_complete_original() {
    let a = actual(include_bytes!(
        "archive/pages/20260828g00192/20260828g001920071.pdf"
    ));
    let e: Value = serde_json::from_str(include_str!(
        "expected-source-aj1/20260828g001920071-candidate51.json"
    ))
    .unwrap();
    assert_eq!(a["pages"][0]["words"][59],serde_json::from_str::<Value>("{\"id\": 59, \"x0\": 417.8001, \"y0\": 388.8399, \"x1\": 425.8001, \"y1\": 396.8399, \"raw\": \"󴽎\", \"text\": \"寬\", \"status\": \"used\"}").unwrap());
    assert_eq!(a["pages"][0]["words"][78],serde_json::from_str::<Value>("{\"id\": 78, \"x0\": 388.0161, \"y0\": 388.8399, \"x1\": 396.0161, \"y1\": 396.8399, \"raw\": \"󴽎\", \"text\": \"寬\", \"status\": \"used\"}").unwrap());
    let e = expected_article_partitions::with_articles(
        e,
        include_str!("expected-native-article-partitions/20260828g001920071.json"),
    );
    assert_eq!(a, e);
}
#[test]
fn source_20260909g002010054_complete_original() {
    let a = actual(include_bytes!(
        "archive/pages/20260909g00201/20260909g002010054.pdf"
    ));
    let e: Value = serde_json::from_str(include_str!(
        "expected-source-aj1/20260909g002010054-candidate51.json"
    ))
    .unwrap();
    assert_eq!(a["pages"][0]["words"][51],serde_json::from_str::<Value>("{\"id\": 51, \"x0\": 437.2538, \"y0\": 772.8398, \"x1\": 445.2538, \"y1\": 780.8398, \"raw\": \"󵨍\", \"text\": \"龑\", \"status\": \"used\"}").unwrap());
    let e = expected_article_partitions::with_articles(
        e,
        include_str!("expected-native-article-partitions/20260909g002010054.json"),
    );
    assert_eq!(a, e);
}
#[test]
fn source_20260910g002020104_complete_original() {
    let a = actual(include_bytes!(
        "archive/pages/20260910g00202/20260910g002020104.pdf"
    ));
    let e: Value = serde_json::from_str(include_str!(
        "expected-source-aj1/20260910g002020104-candidate51.json"
    ))
    .unwrap();
    assert_eq!(a["pages"][0]["words"][133],serde_json::from_str::<Value>("{\"id\": 133, \"x0\": 252.40811, \"y0\": 404.8399, \"x1\": 260.4081, \"y1\": 412.8399, \"raw\": \"󴧈\", \"text\": \"阳\", \"status\": \"used\"}").unwrap());
    let e = expected_article_partitions::with_articles(
        e,
        include_str!("expected-native-article-partitions/20260910g002020104.json"),
    );
    assert_eq!(a, e);
}
#[test]
fn source_20260910h017870005_complete_original() {
    let a = actual(include_bytes!(
        "archive/pages/20260910h01787/20260910h017870005.pdf"
    ));
    let e: Value = serde_json::from_str(include_str!(
        "expected-source-aj1/20260910h017870005-candidate51.json"
    ))
    .unwrap();
    assert_eq!(a["pages"][0]["words"][208],serde_json::from_str::<Value>("{\"id\": 208, \"x0\": 167.4047, \"y0\": 780.8399, \"x1\": 175.4047, \"y1\": 788.8399, \"raw\": \"󴽑\", \"text\": \"朗\", \"status\": \"used\"}").unwrap());
    assert_eq!(a, e);
}
#[test]
fn source_20260915g002050081_complete_original() {
    let a = actual(include_bytes!(
        "archive/pages/20260915g00205/20260915g002050081.pdf"
    ));
    let e: Value = serde_json::from_str(include_str!(
        "expected-source-aj1/20260915g002050081-candidate51.json"
    ))
    .unwrap();
    assert_eq!(a["pages"][0]["words"][50],serde_json::from_str::<Value>("{\"id\": 50, \"x0\": 439.8801, \"y0\": 372.8399, \"x1\": 447.8801, \"y1\": 380.8399, \"raw\": \"󴚦\", \"text\": \"玥\", \"status\": \"used\"}").unwrap());
    assert_eq!(a["pages"][0]["words"][77],serde_json::from_str::<Value>("{\"id\": 77, \"x0\": 394.9761, \"y0\": 772.8399, \"x1\": 402.9761, \"y1\": 780.8399, \"raw\": \"󵛓\", \"text\": \"繥\", \"status\": \"used\"}").unwrap());
    let e = expected_article_partitions::with_articles(
        e,
        include_str!("expected-native-article-partitions/20260915g002050081.json"),
    );
    assert_eq!(a, e);
}
#[test]
fn source_20260917g002080094_complete_original() {
    let a = actual(include_bytes!(
        "archive/pages/20260917g00208/20260917g002080094.pdf"
    ));
    let e: Value = serde_json::from_str(include_str!(
        "expected-source-aj1/20260917g002080094-candidate51.json"
    ))
    .unwrap();
    assert_eq!(a["pages"][0]["words"][122],serde_json::from_str::<Value>("{\"id\": 122, \"x0\": 343.5482, \"y0\": 172.8398, \"x1\": 351.5482, \"y1\": 180.8398, \"raw\": \"󴼧\", \"text\": \"笈\", \"status\": \"used\"}").unwrap());
    let e = expected_article_partitions::with_articles(
        e,
        include_str!("expected-native-article-partitions/20260917g002080094.json"),
    );
    assert_eq!(a, e);
}
#[test]
fn source_20260924g002110102_complete_original() {
    let a = actual(include_bytes!(
        "archive/pages/20260924g00211/20260924g002110102.pdf"
    ));
    let e: Value = serde_json::from_str(include_str!(
        "expected-source-aj1/20260924g002110102-candidate51.json"
    ))
    .unwrap();
    assert_eq!(a["pages"][0]["words"][213],serde_json::from_str::<Value>("{\"id\": 213, \"x0\": 110.6641, \"y0\": 220.8399, \"x1\": 118.6641, \"y1\": 228.8399, \"raw\": \"󴑍\", \"text\": \"妮\", \"status\": \"used\"}").unwrap());
    let e = expected_article_partitions::with_articles(
        e,
        include_str!("expected-native-article-partitions/20260924g002110102.json"),
    );
    assert_eq!(a, e);
}
#[test]
fn source_20260930g002160104_complete_original() {
    let a = actual(include_bytes!(
        "archive/pages/20260930g00216/20260930g002160104.pdf"
    ));
    let e: Value = serde_json::from_str(include_str!(
        "expected-source-aj1/20260930g002160104-candidate51.json"
    ))
    .unwrap();
    assert_eq!(a["pages"][0]["words"][106],serde_json::from_str::<Value>("{\"id\": 106, \"x0\": 317.9841, \"y0\": 196.8398, \"x1\": 325.9841, \"y1\": 204.8398, \"raw\": \"󴼹\", \"text\": \"叉\", \"status\": \"used\"}").unwrap());
    let e = expected_article_partitions::with_articles(
        e,
        include_str!("expected-native-article-partitions/20260930g002160104.json"),
    );
    assert_eq!(a, e);
}
#[test]
fn source_20261001g002170140_complete_original() {
    let a = actual(include_bytes!(
        "archive/pages/20261001g00217/20261001g002170140.pdf"
    ));
    let e: Value = serde_json::from_str(include_str!(
        "expected-source-aj1/20261001g002170140-candidate51.json"
    ))
    .unwrap();
    assert_eq!(a["pages"][0]["words"][53],serde_json::from_str::<Value>("{\"id\": 53, \"x0\": 434.9441, \"y0\": 164.8399, \"x1\": 442.9441, \"y1\": 172.8399, \"raw\": \"󴗸\", \"text\": \"汛\", \"status\": \"used\"}").unwrap());
    let e = expected_article_partitions::with_articles(
        e,
        include_str!("expected-native-article-partitions/20261001g002170140.json"),
    );
    assert_eq!(a, e);
}
#[test]
fn source_20261007g002210046_complete_original() {
    let a = actual(include_bytes!(
        "archive/pages/20261007g00221/20261007g002210046.pdf"
    ));
    let e: Value = serde_json::from_str(include_str!(
        "expected-source-aj1/20261007g002210046-candidate51.json"
    ))
    .unwrap();
    assert_eq!(a["pages"][0]["words"][41],serde_json::from_str::<Value>("{\"id\": 41, \"x0\": 455.0401, \"y0\": 532.8399, \"x1\": 463.0401, \"y1\": 540.8399, \"raw\": \"󴧕\", \"text\": \"隽\", \"status\": \"used\"}").unwrap());
    assert_eq!(a["pages"][0]["words"][49],serde_json::from_str::<Value>("{\"id\": 49, \"x0\": 444.2881, \"y0\": 588.8399, \"x1\": 452.2881, \"y1\": 596.8399, \"raw\": \"󴧕\", \"text\": \"隽\", \"status\": \"used\"}").unwrap());
    let e = expected_article_partitions::with_articles(
        e,
        include_str!("expected-native-article-partitions/20261007g002210046.json"),
    );
    assert_eq!(a, e);
}
#[test]
fn source_20261009h018050030_complete_original() {
    let a = actual(include_bytes!(
        "archive/pages/20261009h01805/20261009h018050030.pdf"
    ));
    let e: Value = serde_json::from_str(include_str!(
        "expected-source-aj1/20261009h018050030-candidate51.json"
    ))
    .unwrap();
    assert_eq!(a["pages"][0]["words"][210],serde_json::from_str::<Value>("{\"id\": 210, \"x0\": 144.9681, \"y0\": 572.8399, \"x1\": 152.9681, \"y1\": 580.8399, \"raw\": \"󴚦\", \"text\": \"玥\", \"status\": \"used\"}").unwrap());
    let e = expected_article_partitions::with_articles(
        e,
        include_str!("expected-native-article-partitions/20261009h018050030.json"),
    );
    assert_eq!(a, e);
}
#[test]
fn source_20261019g002290002_complete_original() {
    let a = actual(include_bytes!(
        "archive/pages/20261019g00229/20261019g002290002.pdf"
    ));
    let e: Value = serde_json::from_str(include_str!(
        "expected-source-aj1/20261019g002290002-candidate51.json"
    ))
    .unwrap();
    assert_eq!(a["pages"][0]["words"][91],serde_json::from_str::<Value>("{\"id\": 91, \"x0\": 284.7427, \"y0\": 658.5939, \"x1\": 292.7427, \"y1\": 666.5939, \"raw\": \"󴮄\", \"text\": \"崁\", \"status\": \"used\"}").unwrap());
    assert_eq!(a["pages"][0]["words"][93],serde_json::from_str::<Value>("{\"id\": 93, \"x0\": 284.7427, \"y0\": 682.5939, \"x1\": 292.7427, \"y1\": 690.5939, \"raw\": \"󴮄\", \"text\": \"崁\", \"status\": \"used\"}").unwrap());
    assert_eq!(a, e);
}
#[test]
fn trusted_20260729h017570030_whole_original_unchanged() {
    let a = actual(include_bytes!(
        "archive/pages/20260729h01757/20260729h017570030.pdf"
    ));
    let e: Value = serde_json::from_str(include_str!(
        "expected-source-aj1/20260729h017570030-trusted-before51.json"
    ))
    .unwrap();
    let e = expected_article_partitions::with_articles(
        e,
        include_str!("expected-native-article-partitions/20260729h017570030.json"),
    );
    assert_eq!(a, e);
}
#[test]
fn trusted_20261008g002220080_whole_original_unchanged() {
    let a = actual(include_bytes!(
        "archive/pages/20261008g00222/20261008g002220080.pdf"
    ));
    let e: Value = serde_json::from_str(include_str!(
        "expected-source-aj1/20261008g002220080-trusted-before51.json"
    ))
    .unwrap();
    let e = expected_article_partitions::with_articles(
        e,
        include_str!("expected-native-article-partitions/20261008g002220080.json"),
    );
    assert_eq!(a, e);
}

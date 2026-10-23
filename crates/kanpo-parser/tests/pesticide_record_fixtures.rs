use kanpo_parser::{analyze_item, page_inputs, Analysis};
use std::collections::BTreeSet;
fn analysis() -> Analysis {
    analyze_item(
        &page_inputs(include_bytes!(
            "archive/pages/20260824h01774/20260824h017740004.pdf"
        ))
        .unwrap(),
    )
}
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
struct Binding {
    registration: &'static str,
    fields: [(&'static str, &'static [usize]); 4],
}
const BINDINGS: &[Binding] = &[
Binding { registration: "23592", fields: [("23592", &[140]), ("フルチアニル・ＴＰＮ水和剤", &[110, 111]), ("フレンダーフロアブル", &[76]), ("東京都千代田区神田小川町一丁目３番１号ＯＡＴアグリオ株式会社代表取締役岡尚", &[26, 27, 19, 5, 28, 20])] },
Binding { registration: "20026", fields: [("20026", &[141]), ("メトミノストロビン粒剤", &[112, 113]), ("オリブライト１キロ粒剤", &[77, 78]), ("東京都千代田区丸の内一丁目６番５号バイエルクロップサイエンス株式会社代表取締役大島美紀", &[29, 30, 31, 14])] },
Binding { registration: "21133", fields: [("21133", &[142]), ("メトリブジン・ＤＢＮ・ＤＣＭＵ粒剤", &[114, 115]), ("ラーチＨ粒剤", &[79]), ("東京都港区東新橋一丁目９番２号保土谷アグロテック株式会社代表取締役脇坂孝夫", &[32, 1, 33, 9, 34])] },
Binding { registration: "21136", fields: [("21136", &[143]), ("メトリブジン・ＤＢＮ粒剤", &[116, 117]), ("ラーチ粒剤", &[80]), ("〃", &[35])] },
Binding { registration: "23593", fields: [("23593", &[144]), ("フルチアニル・ＴＰＮ水和剤", &[118, 119]), ("ＳＤＳフレンダーフロアブル", &[81, 82]), ("東京都千代田区神田練塀町３番地株式会社エス・ディー・エスバイオテック代表取締役社長竹田正俊", &[36, 2, 37, 10, 25, 13])] },
Binding { registration: "21135", fields: [("21135", &[145]), ("メトリブジン・ＤＢＮ・ＤＣＭＵ粒剤", &[120, 121]), ("クサノンＤＸ粒剤", &[83]), ("東京都中央区東日本橋二丁目６番12号ＫＩＮＣＨＯ園芸株式会社代表取締役社長森本義之", &[38, 39, 6, 40, 17])] },
Binding { registration: "21137", fields: [("21137", &[146]), ("メトリブジン・ＤＢＮ粒剤", &[122, 123]), ("ＧＦ草退治Ｖ粒剤", &[84]), ("〃", &[41])] },
Binding { registration: "23135", fields: [("23135", &[147]), ("チオジカルブ水和剤", &[124]), ("リラークＤＦ", &[85]), ("東京都千代田区丸の内一丁目６番５号バイエルクロップサイエンス株式会社代表取締役大島美紀", &[42, 43, 44, 15])] },
Binding { registration: "19249", fields: [("19249", &[148]), ("カーバム剤", &[125]), ("ヤシマＮＣＳ", &[86]), ("東京都台東区上野一丁目19番10号レインボー薬品株式会社代表取締役滝口健一", &[45, 3, 46, 11, 4, 47])] },
Binding { registration: "23740", fields: [("23740", &[149]), ("カルブチレート・ピラフルフェンエチル水和剤", &[126, 127, 128]), ("ネコソギメガロングＦＬ", &[87, 88]), ("〃", &[48])] },
Binding { registration: "23899", fields: [("23899", &[150]), ("グリホサートイソプロピルアミン塩・ヘキサジノン液剤", &[129, 130, 131]), ("ネコソギロング液剤", &[89]), ("〃", &[49])] },
Binding { registration: "19666", fields: [("19666", &[151]), ("ペルメトリン・イミベンコナゾール乳剤", &[132, 133]), ("ムシキントール", &[90]), ("東京都中央区東日本橋二丁目６番12号ＫＩＮＣＨＯ園芸株式会社代表取締役社長森本義之", &[50, 51, 7, 52, 18])] },
Binding { registration: "6272", fields: [("6272", &[139]), ("カーバム剤", &[134]), ("ＮＣＳ", &[91]), ("東京都千代田区永田町二丁目11番１号コルテバ・アグリサイエンス日本株式会社代表取締役社長野村真一郎", &[53, 54, 55, 21, 12])] },
Binding { registration: "19233", fields: [("19233", &[152]), ("カーバム剤", &[135]), ("三洋ＮＣＳ", &[92]), ("〃", &[56])] },
Binding { registration: "20970", fields: [("20970", &[153]), ("フルスルファミド・フルトラニル粉剤", &[136, 137]), ("ネビモン粉剤", &[93]), ("東京都中央区日本橋一丁目19番１号三井化学クロップ＆ライフソリューション株式会社代表取締役社長垣元剛", &[57, 0, 58, 59, 16, 8])] },
Binding { registration: "24190", fields: [("24190", &[154]), ("フルスルファミド粉剤", &[138]), ("ネビライト粉剤", &[94]), ("〃", &[60])] },
 ];
fn row_binding(reg: &str) {
    let a = analysis();
    let p = &a.pages[0];
    let b = BINDINGS.iter().find(|b| b.registration == reg).unwrap();
    let expected: BTreeSet<usize> = b.fields.iter().flat_map(|f| f.1.iter().copied()).collect();
    let mut candidates = Vec::new();
    for l in p.regions.iter().flat_map(|r| &r.lines) {
        if let Some(t) = &l.table {
            for r in 0..t.rows {
                let cells: Vec<_> = t.cells.iter().filter(|c| c.row == r).collect();
                let ids: BTreeSet<_> = cells.iter().flat_map(|c| c.words.iter().copied()).collect();
                if ids.contains(&b.fields[0].1[0]) {
                    candidates.push((
                        ids,
                        cells.iter().map(|c| c.text.as_str()).collect::<String>(),
                    ));
                }
            }
        } else if l.words.contains(&b.fields[0].1[0]) {
            candidates.push((l.words.iter().copied().collect(), l.text.clone()));
        }
    }
    assert_eq!(candidates.len(), 1, "unique logical row for {reg}");
    let (ids, text) = &candidates[0];
    assert!(
        expected.is_subset(ids),
        "{reg}: manufacturer/address source IDs detached; missing {:?}; actual row {}",
        expected.difference(ids).collect::<Vec<_>>(),
        text
    );
    for (literal, _) in &b.fields {
        assert!(
            compact(text).contains(literal),
            "{reg}: complete original field {literal:?} missing in logical row {text:?}"
        );
    }
    for other in BINDINGS.iter().filter(|o| o.registration != reg) {
        assert!(
            !ids.contains(&other.fields[0].1[0]),
            "different registration {} merged into {reg}",
            other.registration
        );
    }
}
#[test]
fn all_sixteen_literal_fields_are_witnessed_by_the_original_word_ids() {
    let a = analysis();
    let p = &a.pages[0];
    let mut ids = BTreeSet::new();
    for b in BINDINGS {
        for (literal, source) in &b.fields {
            let t: String = source
                .iter()
                .map(|i| {
                    assert!(ids.insert(*i), "duplicated binding Word {i}");
                    p.words[*i].text.as_str()
                })
                .collect();
            assert_eq!(compact(&t), *literal, "source binding {}", b.registration);
        }
    }
    assert_eq!(ids.len(), 122);
}
#[test]
fn target_source_words_have_unique_owners_before_and_after_reconstruction() {
    let a = analysis();
    let p = &a.pages[0];
    let ids: Vec<_> = p
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .flat_map(|l| {
            l.words
                .iter()
                .chain(l.table.iter().flat_map(|t| &t.cells).flat_map(|c| &c.words))
        })
        .copied()
        .collect();
    let unique: BTreeSet<_> = ids.iter().copied().collect();
    assert_eq!(ids.len(), unique.len());
    for b in BINDINGS {
        for (_, source) in &b.fields {
            for id in *source {
                assert!(unique.contains(id));
            }
        }
    }
}
#[test]
fn neighboring_ruled_coordinate_tables_keep_their_complete_output() {
    let a = analyze_item(
        &page_inputs(include_bytes!(
            "archive/pages/20260824h01774/20260824h017740005.pdf"
        ))
        .unwrap(),
    );
    assert_eq!(
        a.text,
        include_str!("fixtures/pesticide_records/1774p5-before.txt")
    );
}
#[test]
fn preceding_unrelated_notices_keep_their_complete_prefix() {
    let a = analyze_item(
        &page_inputs(include_bytes!(
            "archive/pages/20260824h01774/20260824h017740003.pdf"
        ))
        .unwrap(),
    );
    let i = a.text.find("〇農林水産省告示第千二百四十八号").unwrap();
    assert_eq!(
        &a.text[..i],
        include_str!("fixtures/pesticide_records/1774p3-prefix-before.txt")
    );
}
#[test]
fn registration_23592_retains_its_literal_name_type_and_original_owner() {
    row_binding("23592");
}
#[test]
fn registration_20026_retains_its_literal_name_type_and_original_owner() {
    row_binding("20026");
}
#[test]
fn registration_21133_retains_its_literal_name_type_and_original_owner() {
    row_binding("21133");
}
#[test]
fn registration_21136_retains_its_literal_name_type_and_original_owner() {
    row_binding("21136");
}
#[test]
fn registration_23593_retains_its_literal_name_type_and_original_owner() {
    row_binding("23593");
}
#[test]
fn registration_21135_retains_its_literal_name_type_and_original_owner() {
    row_binding("21135");
}
#[test]
fn registration_21137_retains_its_literal_name_type_and_original_owner() {
    row_binding("21137");
}
#[test]
fn registration_23135_retains_its_literal_name_type_and_original_owner() {
    row_binding("23135");
}
#[test]
fn registration_19249_retains_its_literal_name_type_and_original_owner() {
    row_binding("19249");
}
#[test]
fn registration_23740_retains_its_literal_name_type_and_original_owner() {
    row_binding("23740");
}
#[test]
fn registration_23899_retains_its_literal_name_type_and_original_owner() {
    row_binding("23899");
}
#[test]
fn registration_19666_retains_its_literal_name_type_and_original_owner() {
    row_binding("19666");
}
#[test]
fn registration_6272_retains_its_literal_name_type_and_original_owner() {
    row_binding("6272");
}
#[test]
fn registration_19233_retains_its_literal_name_type_and_original_owner() {
    row_binding("19233");
}
#[test]
fn registration_20970_retains_its_literal_name_type_and_original_owner() {
    row_binding("20970");
}
#[test]
fn registration_24190_retains_its_literal_name_type_and_original_owner() {
    row_binding("24190");
}

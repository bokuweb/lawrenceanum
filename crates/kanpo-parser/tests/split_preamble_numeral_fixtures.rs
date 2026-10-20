//! 横倒し数字のために前文が左右の段へ分かれた告示を、既存行の局所結合で戻す。
fn input_mode(inputs: &mut [kanpo_parser::PageInput], mode: &str) {
    fn attr<'a>(line: &'a str, key: &str) -> Option<&'a str> {
        line.split_once(&format!("{key}=\""))?.1.split_once('"').map(|(v, _)| v)
    }
    for input in inputs {
        if mode == "legacy" {
            input.xhtml = input.xhtml.replace("data-wmode=", "data-disabled-wmode=");
        }
        if mode == "truecoords" {
            input.xhtml = input
                .xhtml
                .lines()
                .map(|line| {
                    let mut out = line.to_owned();
                    let ax = attr(line, "data-advance-x").and_then(|s| s.parse::<f32>().ok()).unwrap_or(0.);
                    let ay = attr(line, "data-advance-y").and_then(|s| s.parse::<f32>().ok()).unwrap_or(0.);
                    if attr(line, "data-wmode") == Some("1") && ax.abs() > ay.abs() * 1.5 && ax.abs() > 0. {
                        for (primary, native) in [
                            ("xMin", "data-native-xmin"),
                            ("yMin", "data-native-ymin"),
                            ("xMax", "data-native-xmax"),
                            ("yMax", "data-native-ymax"),
                        ] {
                            if let (Some(a), Some(b)) = (attr(line, primary), attr(line, native)) {
                                out = out.replacen(&format!("{primary}=\"{a}\""), &format!("{primary}=\"{b}\""), 1);
                            }
                        }
                    }
                    out.replace("data-wmode=", "data-disabled-wmode=")
                })
                .collect::<Vec<_>>()
                .join("\n");
        }
    }
}

#[test]
fn a_split_notice_preamble_keeps_its_full_sentence_before_the_date() {
    let pdf = include_bytes!("archive/pages/20260730g00171/20260730g001710024.pdf");
    let expected="無線設備規則（昭和二十五年電波監理委員会規則第十八号）別表第二号第28の規定に基づき、平成十八年総務省告示第六百五十九号（特定小電力無線局の無線設備の占有周波数帯幅の許容値を定める件）\nの一部を次のように改正する。\n令和八年七月三十日\n総務大臣　林　芳正\n次の表により、";
    for mode in ["legacy", "truecoords"] {
        let mut inputs = kanpo_parser::page_inputs(pdf).unwrap();
        input_mode(&mut inputs, mode);
        let a = kanpo_parser::analyze_item(&inputs);
        assert!(a.text.contains(expected), "{mode}: {}", a.text);
        let p = &a.pages[0];
        let l = p
            .regions
            .iter()
            .flat_map(|r| &r.lines)
            .find(|l| l.text.starts_with("無線設備規則"))
            .unwrap();
        assert_eq!(l.words.len(), 3);
        assert_eq!(p.words[l.words[1]].text, "28");
        let mut ids = std::collections::HashSet::new();
        for id in p
            .regions
            .iter()
            .flat_map(|r| &r.lines)
            .flat_map(|l| l.words.iter().chain(l.table.iter().flat_map(|t| t.cells.iter().flat_map(|c| &c.words))))
        {
            assert!(ids.insert(*id), "duplicate {id}");
        }
    }
}

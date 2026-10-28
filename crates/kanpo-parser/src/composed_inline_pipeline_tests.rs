fn check(n: usize) {
    let mut xml = String::from("<html><body><doc><page width=\"595\" height=\"842\">");
    let mut add = |x: f32, y: f32, y1: f32, text: &str| {
        xml.push_str(&format!(
            "<word xMin=\"{x}\" xMax=\"{}\" yMin=\"{y}\" yMax=\"{y1}\">{text}</word>",
            x + 8.
        ))
    };
    add(396., 100., 140., "引用（任意の記載");
    add(396., 180., 260., "）本文の続き");
    for k in 0..n {
        for j in 0..3 {
            let x = 400. + n as f32 * 4. - (k + 1) as f32 * 8.;
            let y = 140. + j as f32 * 16.;
            let text = char::from_u32(0x4e00 + (k * 3 + j) as u32)
                .unwrap()
                .to_string();
            add(x, y, y + 8., &text);
        }
    }
    xml.push_str("</page></doc></body></html>");
    let a = crate::analyze_item(&[crate::PageInput {
        xhtml: xml,
        rules: Some(Default::default()),
    }]);
    let used: std::collections::BTreeSet<_> = a.pages[0]
        .words
        .iter()
        .filter(|w| matches!(w.status, crate::trace::WordStatus::Used))
        .map(|w| w.id)
        .collect();
    let emitted: Vec<_> = a.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .flat_map(|l| {
            l.words.iter().chain(
                l.table
                    .iter()
                    .flat_map(|t| t.cells.iter().flat_map(|c| &c.words)),
            )
        })
        .copied()
        .collect();
    assert_eq!(used.len(), n * 3 + 2, "all input words should remain used");
    assert_eq!(
        emitted
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>(),
        used,
        "all source words must retain ownership after composing generic and legacy stages; text={}",
        a.text
    );
    assert_eq!(
        emitted.len(),
        used.len(),
        "each complete source word is owned once"
    );
}
macro_rules! case {
    ($name:ident,$n:literal) => {
        #[test]
        fn $name() {
            check($n)
        }
    };
}
case!(two_lanes, 2);
case!(three_lanes, 3);
case!(four_lanes, 4);
case!(five_lanes, 5);
case!(six_lanes, 6);
case!(seven_lanes, 7);

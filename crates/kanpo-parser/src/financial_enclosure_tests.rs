use super::*;
use crate::trace::{Table, WordStatus};
fn add(words: &mut Vec<Word>, text: &str, b: Box4) -> Line {
    let id = words.len();
    words.push(Word {
        source_box: [b[0], b[1], b[2], b[3]],
        id,
        x0: b[0],
        y0: b[1],
        x1: b[2],
        y1: b[3],
        raw: text.into(),
        text: text.into(),
        status: WordStatus::Used,
    });
    Line::text(text.into(), vec![id])
}
fn notice(words: &mut Vec<Word>, title: &str, x: f32) -> Vec<Line> {
    [
        title,
        "最終貸借対照表の要旨は左記のとおりです",
        "株式会社みどり",
        "代表取締役山田太郎",
    ]
    .iter()
    .enumerate()
    .map(|(i, t)| {
        add(
            words,
            t,
            [x - i as f32 * 10., 10., x + 8. - i as f32 * 10., 100.],
        )
    })
    .collect()
}
fn fixture() -> (Vec<Unit>, Vec<Word>) {
    let mut w = vec![];
    let mut ls = vec![
        add(&mut w, "第1期決算公告", [10., 10., 120., 18.]),
        add(&mut w, "株式会社みどり", [10., 25., 120., 33.]),
        add(&mut w, "貸借対照表の要旨", [10., 40., 120., 48.]),
    ];
    ls.push(Line {
        text: "金額123".into(),
        words: vec![],
        table: Some(Table {
            x0: 10.,
            y0: 50.,
            x1: 120.,
            y1: 150.,
            rows: 0,
            cols: 0,
            cells: vec![],
        }),
    });
    ls.push(add(&mut w, "独立枠", [350., 10., 400., 18.]));
    ls.extend(notice(&mut w, "新設分割公告", 200.));
    (vec![Unit::Plain(ls)], w)
}
fn text(us: &[Unit]) -> String {
    us.iter().map(Unit::text).collect::<Vec<_>>().join("\n")
}
fn run(us: &mut [Unit], w: &[Word]) {
    order_financial_enclosures(us, w, &[[0., 0., 300., 200.]], 600., 800.);
}
#[test]
fn moves_only_referenced_frame_and_preserves_every_line() {
    let (mut us, w) = fixture();
    let before = match &us[0] {
        Unit::Plain(ls) => ls.clone(),
        _ => unreachable!(),
    };
    run(&mut us, &w);
    let t = text(&us);
    assert!(t.find("新設分割公告") < t.find("第1期決算公告"));
    assert!(t.ends_with("独立枠"));
    let Unit::Plain(ls) = &us[0] else { panic!() };
    for l in &before {
        assert_eq!(ls.iter().filter(|n| *n == l).count(), 1);
    }
    assert_eq!(before.len(), ls.len());
    let once = text(&us);
    run(&mut us, &w);
    assert_eq!(once, text(&us));
}
#[test]
fn simultaneous_vertical_notices_read_from_right() {
    let (mut us, mut w) = fixture();
    let more = notice(&mut w, "資本金の額の減少公告", 270.);
    let Unit::Plain(ls) = &mut us[0] else {
        panic!()
    };
    ls.extend(more);
    run(&mut us, &w);
    let t = text(&us);
    assert!(t.find("資本金の額の減少公告") < t.find("新設分割公告"));
}
#[test]
fn no_native_frame_is_no_op() {
    let (mut us, w) = fixture();
    let t = text(&us);
    order_financial_enclosures(&mut us, &w, &[], 600., 800.);
    assert_eq!(t, text(&us));
}
#[test]
fn unrelated_company_is_not_pulled_into_group() {
    let (mut us, w) = fixture();
    let Unit::Plain(ls) = &mut us[0] else {
        panic!()
    };
    ls[1].text = "株式会社あかね".into();
    let t = text(&us);
    run(&mut us, &w);
    assert_eq!(t, text(&us));
}
#[test]
fn no_balance_sheet_reference_is_no_op() {
    let (mut us, w) = fixture();
    let Unit::Plain(ls) = &mut us[0] else {
        panic!()
    };
    ls[6].text = "債権者はお申し出ください".into();
    let t = text(&us);
    run(&mut us, &w);
    assert_eq!(t, text(&us));
}
#[test]
fn no_table_is_no_op() {
    let (mut us, w) = fixture();
    let Unit::Plain(ls) = &mut us[0] else {
        panic!()
    };
    ls[3].table = None;
    let t = text(&us);
    run(&mut us, &w);
    assert_eq!(t, text(&us));
}
#[test]
fn unknown_prose_inside_frame_is_no_op() {
    let (mut us, mut w) = fixture();
    let l = add(&mut w, "別の会社の無関係な文", [10., 160., 120., 168.]);
    let Unit::Plain(ls) = &mut us[0] else {
        panic!()
    };
    ls.insert(0, l);
    let t = text(&us);
    run(&mut us, &w);
    assert_eq!(t, text(&us));
}
#[test]
fn line_crossing_native_frame_is_not_split() {
    let (mut us, mut w) = fixture();
    let l = add(&mut w, "枠内", [290., 160., 298., 168.]);
    let r = add(&mut w, "枠外", [305., 160., 313., 168.]);
    let mut cross = l;
    cross.text.push_str(&r.text);
    cross.words.extend(r.words);
    let Unit::Plain(ls) = &mut us[0] else {
        panic!()
    };
    ls.push(cross);
    let t = text(&us);
    run(&mut us, &w);
    assert_eq!(t, text(&us));
}
#[test]
fn horizontal_notice_is_not_moved() {
    let (mut us, mut w) = fixture();
    for word in &mut w[4..] {
        word.y1 = word.y0 + 8.;
        word.x1 = word.x0 + 100.;
    }
    let t = text(&us);
    run(&mut us, &w);
    assert_eq!(t, text(&us));
}
#[test]
fn page_decoration_is_not_an_article_enclosure() {
    let (mut us, w) = fixture();
    let t = text(&us);
    order_financial_enclosures(&mut us, &w, &[[0., 0., 599., 799.]], 600., 800.);
    assert_eq!(t, text(&us));
}
#[test]
fn unmatched_sibling_notice_rejects_the_whole_change() {
    let (mut us, mut w) = fixture();
    let mut more = notice(&mut w, "合併公告", 270.);
    more[2].text = "株式会社あかね".into();
    let Unit::Plain(ls) = &mut us[0] else {
        panic!()
    };
    ls.extend(more);
    let t = text(&us);
    run(&mut us, &w);
    assert_eq!(t, text(&us));
}
#[test]
fn nested_double_border_has_no_second_reorder() {
    let (mut us, w) = fixture();
    run(&mut us, &w);
    let t = text(&us);
    order_financial_enclosures(
        &mut us,
        &w,
        &[[0., 0., 300., 200.], [2., 2., 298., 198.]],
        600.,
        800.,
    );
    assert_eq!(t, text(&us));
}
#[test]
fn staggered_overlapping_notices_are_ambiguous() {
    let (mut us, mut w) = fixture();
    let mut more = notice(&mut w, "資本金の額の減少公告", 270.);
    let ids: Vec<_> = more.iter().flat_map(|l| l.words.iter()).copied().collect();
    for i in ids {
        w[i].y0 += 10.;
        w[i].y1 += 10.;
    }
    let Unit::Plain(ls) = &mut us[0] else {
        panic!()
    };
    ls.append(&mut more);
    let t = text(&us);
    run(&mut us, &w);
    assert_eq!(t, text(&us));
}
#[test]
fn a_parent_frame_does_not_merge_its_separate_article_children() {
    let (mut us, mut w) = fixture();
    let Unit::Plain(ls) = &mut us[0] else {
        panic!()
    };
    ls.remove(4);
    let old = ls.clone();
    let oldw = w.clone();
    let shift = w.len();
    for mut word in oldw {
        word.id += shift;
        word.x0 += 350.;
        word.x1 += 350.;
        w.push(word);
    }
    for mut l in old {
        for id in &mut l.words {
            *id += shift;
        }
        if let Some(t) = &mut l.table {
            t.x0 += 350.;
            t.x1 += 350.;
        }
        ls.push(l);
    }
    order_financial_enclosures(
        &mut us,
        &w,
        &[
            [0., 0., 650., 400.],
            [0., 0., 300., 200.],
            [350., 0., 650., 200.],
        ],
        1000.,
        800.,
    );
    let Unit::Plain(ls) = &us[0] else { panic!() };
    let titles: Vec<_> = ls
        .iter()
        .filter(|l| notice_title(&l.text) || bs_title(&l.text))
        .map(|l| l.text.as_str())
        .collect();
    assert_eq!(
        titles,
        vec![
            "新設分割公告",
            "第1期決算公告",
            "新設分割公告",
            "第1期決算公告"
        ]
    );
}
#[test]
fn a_body_sentence_ending_in_hon_kokoku_is_not_a_new_notice() {
    let (mut us, mut w) = fixture();
    let Unit::Plain(ls) = &mut us[0] else {
        panic!()
    };
    let l = add(
        &mut w,
        "この合併に対し異議のある債権者は、本公告",
        [190., 10., 198., 100.],
    );
    ls.insert(6, l);
    run(&mut us, &w);
    assert!(text(&us).starts_with("新設分割公告"));
}

#[test]
fn a_table_crossing_the_enclosure_is_not_split_or_partially_reordered() {
    let (mut us, w) = fixture();
    let Unit::Plain(ls) = &mut us[0] else {
        panic!()
    };
    ls.insert(
        0,
        Line {
            text: "別の表".into(),
            words: vec![],
            table: Some(Table {
                x0: 290.,
                y0: 160.,
                x1: 400.,
                y1: 180.,
                rows: 0,
                cols: 0,
                cells: vec![],
            }),
        },
    );
    let before = text(&us);
    run(&mut us, &w);
    assert_eq!(before, text(&us));
}

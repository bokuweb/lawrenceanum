use crate::notice_boundaries::cuts;
use crate::trace::{Line, PageTrace, Region, Role, Word, WordStatus};
use std::collections::BTreeSet;
fn word(p: &mut PageTrace, t: &str, b: [f32; 4], font: usize) {
    let id = p.words.len();
    p.words.push(Word {
        id,
        raw: t.into(),
        text: t.into(),
        x0: b[0],
        y0: b[1],
        x1: b[2],
        y1: b[3],
        status: WordStatus::Used,
    });
    p.native_styles
        .push(Some(BTreeSet::from([(font, 8f64.to_bits())])));
    p.regions[0].lines.push(Line::text(t.into(), vec![id]));
}
fn statement(p: &mut PageTrace, x: f32, title: &str, font: usize, date: &str) {
    let end = 50. + title.chars().count() as f32 * 8.;
    word(p, title, [x, 50., x + 8., end], font);
    word(
        p,
        "手続をお知らせします。",
        [x - 11., 58., x - 3., end + 100.],
        2,
    );
    word(p, date, [x - 22., 60., x - 14., 140.], 2);
    word(p, "Unseen organization", [x - 33., 100., x - 25., 180.], 2);
    word(p, "清算人任意", [x - 44., 120., x - 36., 210.], 2);
}
#[test]
fn structurally_separate_unparsed_titles_do_not_release_their_date_to_the_previous_notice() {
    for t in [
        "関連公告（対象Ａ",
        "関連公告対象Ａ）",
        "関連公告(対象Ａ）",
        "関連公告日付未定",
        "公告（対象未定",
    ] {
        let mut p = PageTrace {
            width: 600.,
            height: 800.,
            layout: "tiers",
            words: vec![],
            native_styles: vec![],
        native_bounds: vec![],
            regions: vec![Region {
                role: Role::Body,
                unit: 0,
                part: 0,
                lines: vec![],
            }],
            text: String::new(),
            doc_text: String::new(),
            tables: vec![],
        };
        statement(&mut p, 400., "訂正公告", 1, "日付未定");
        statement(&mut p, 250., t, 3, "2026年7月1日");
        assert!(cuts(&p, &Default::default()).is_empty(), "{t}");
    }
}

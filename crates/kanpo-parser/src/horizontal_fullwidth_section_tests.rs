use super::*;
fn word(id: usize, x: f32, x1: f32, y: f32, text: &str) -> Col {
    Col {
        id,
        x,
        x1,
        y,
        y1: y + 8.,
        text: text.into(),
    }
}
fn setup() -> (Vec<Col>, Vec<Block>, Vec<(f32, f32, f32)>) {
    let ws = vec![
        word(0, 61., 413., 46., "左段の本文"),
        word(1, 429., 789., 46., "右段の本文"),
        word(2, 61., 413., 350., "左段の末尾"),
        word(3, 429., 789., 350., "右段の末尾"),
        word(4, 61., 200., 366., "Ⅵ　新しい節"),
        word(5, 69., 160., 378., "１　項目"),
        word(6, 77., 789., 432., "段抜き表の下の説明"),
    ];
    (ws, vec![(69., 389., 781., 426.)], vec![(425., 46., 362.)])
}
#[test]
fn a_spanning_table_and_its_short_numbered_captions_end_two_columns() {
    let (w, b, r) = setup();
    assert_eq!(divider_end_cut(&w, &b, &r), Some(362.));
}
#[test]
fn ordinary_columns_and_independent_article_headings_are_not_table_captions() {
    let (w, b, r) = setup();
    assert_eq!(divider_end_cut(&w, &[], &r), None, "no spanning table");
    assert_eq!(
        divider_end_cut(&w, &[(69., 389., 420., 426.)], &r),
        None,
        "single-column table"
    );
    assert_eq!(
        divider_end_cut(&w, &[(69., 409., 781., 446.)], &r),
        None,
        "distant table"
    );
    assert_eq!(
        divider_end_cut(&w, &b, &[(425., 60., 362.)]),
        None,
        "divider does not begin at the top"
    );
    assert_eq!(
        divider_end_cut(&w, &b, &[(425., 46., 450.)]),
        None,
        "columns continue below captions"
    );
    let mut early = w.clone();
    for c in &mut early {
        if c.id == 3 {
            c.y = 310.;
            c.y1 = 318.;
        }
    }
    assert_eq!(
        divider_end_cut(&early, &b, &r),
        None,
        "right column ends much earlier"
    );
    for text in [
        "〇別の告示第十号",
        "この文章は左段の続きです。",
        "Ⅵ　支出額100円",
        "Ⅵ　独立した文章、見出しではない",
    ] {
        let mut q = w.clone();
        q[4].text = text.into();
        assert_eq!(divider_end_cut(&q, &b, &r), None, "{text}");
    }
    let mut q = w.clone();
    q[5].x = 429.;
    q[5].x1 = 600.;
    assert_eq!(divider_end_cut(&q, &b, &r), None, "right-side continuation");
    let mut q = w.clone();
    q[4].x = 310.;
    q[4].x1 = 440.;
    assert_eq!(
        divider_end_cut(&q, &b, &r),
        None,
        "off-center title straddling gutter"
    );
    let mut q = w.clone();
    q.push(word(7, 69., 200., 360., "2　line crossing cut"));
    assert_eq!(divider_end_cut(&q, &b, &r), None, "word crosses cut");
}
#[test]
fn ordinary_fullwidth_centered_titles_keep_the_existing_cut() {
    let (mut w, _, r) = setup();
    w.retain(|w| w.id != 4 && w.id != 5);
    w.push(word(4, 335., 515., 370., "中央に組まれた標題"));
    assert_eq!(divider_end_cut(&w, &[], &r), Some(362.));
}

fn analyze(page: &str) -> kanpo_parser::Analysis {
    kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf(page)).unwrap())
}

#[test]
fn the_shared_notice_frame_precedes_its_referenced_balance_sheets() {
    let a = analyze("20260828g001920078");
    let t = &a.text;
    let capital = t.find("資本金の額の減少公告").unwrap();
    let division = t.find("新設分割公告").unwrap();
    let merger = t.rfind("合併公告").unwrap();
    let bs40 = t.find("第40期決算公告").unwrap();
    let bs8 = t.find("第８期決算公告").unwrap();
    assert!(capital < division && division < merger && merger < bs40 && merger < bs8,
            "notice references must precede both balance sheets; capital={capital}, division={division}, merger={merger}, bs40={bs40}, bs8={bs8}");
    // Source figures and the adjacent prior framed companies remain intact.
    for s in [
        "第38期決算公告",
        "第51期決算公告",
        "第63期決算公告",
        "第31期決算公告",
        "資産合計　1,931,327",
        "純資産合計　1,311,830",
        "合計　255,543",
    ] {
        assert!(t.contains(s), "{s}");
    }
}

#[test]
fn independent_left_frames_do_not_interrupt_the_notice_and_its_lower_balance_sheets() {
    let a = analyze("20261020g002300087");
    let t = &a.text;
    let start = t.find("新設分割公告及び合併公告").unwrap();
    let bs48 = t.find("第48期決算公告").unwrap();
    let bs24 = t.find("第24期決算公告").unwrap();
    let end = bs48.max(bs24);
    assert!(start < bs48 && start < bs24);
    for s in ["第10期決算公告", "第50期決算公告"] {
        let pos = t.find(s).unwrap();
        assert!(
            pos < start || pos > end,
            "independent frame {s} interrupts the common enclosure"
        );
    }
    for s in [
        "合計　430,761",
        "合計　2,862,970",
        "資産合計　513,370",
        "合計　471,987",
    ] {
        assert!(t.contains(s), "{s}");
    }
}

fn pdf(page: &str) -> &'static [u8] {
    match page {
        "20260828g001920078" => {
            include_bytes!("archive/pages/20260828g00192/20260828g001920078.pdf")
        }
        "20261020g002300087" => {
            include_bytes!("archive/pages/20261020g00230/20261020g002300087.pdf")
        }
        "20260828g001920077" => {
            include_bytes!("archive/pages/20260828g00192/20260828g001920077.pdf")
        }
        "20261020g002300086" => {
            include_bytes!("archive/pages/20261020g00230/20261020g002300086.pdf")
        }
        _ => panic!("unknown fixture {page}"),
    }
}

#[test]
fn neighboring_independent_frames_retain_their_existing_order() {
    for (p, expected) in [
        (
            "20260828g001920077",
            include_str!("archive/expected-financial-enclosures/20260828g001920077.txt"),
        ),
        (
            "20261020g002300086",
            include_str!("archive/expected-financial-enclosures/20261020g002300086.txt"),
        ),
    ] {
        assert_eq!(analyze(p).text, expected, "{p}");
    }
}
#[test]
fn native_enclosures_are_required_and_existing_source_lines_are_unchanged() {
    for (p, expected) in [
        (
            "20260828g001920078",
            include_str!("archive/expected-financial-enclosures/20260828g001920078.txt"),
        ),
        (
            "20261020g002300087",
            include_str!("archive/expected-financial-enclosures/20261020g002300087.txt"),
        ),
    ] {
        let mut input = kanpo_parser::page_inputs(pdf(p)).unwrap();
        assert!(!input[0].rules.as_ref().unwrap().enclosures.is_empty());
        let after = kanpo_parser::analyze_item(&input);
        for page in &mut input {
            page.rules.as_mut().unwrap().enclosures.clear();
        }
        let before = kanpo_parser::analyze_item(&input);
        assert_eq!(before.text, expected, "without a native enclosure: {p}");
        assert_eq!(format!("{:?}",before.pages[0].words),format!("{:?}",after.pages[0].words));
        let all_lines = |a: &kanpo_parser::Analysis| {
            let mut lines: Vec<_> = a
                .pages
                .iter()
                .flat_map(|p| p.regions.iter())
                .flat_map(|r| r.lines.iter())
                .map(|l| format!("{l:?}"))
                .collect();
            lines.sort();
            lines
        };
        assert_eq!(
            all_lines(&before),
            all_lines(&after),
            "all source IDs and Table values preserved: {p}"
        );
    }
}

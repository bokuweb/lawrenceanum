fn analysis() -> kanpo_parser::Analysis {
    let pdf = include_bytes!("archive/pages/20260806g00177/20260806g001770007.pdf");
    kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap())
}
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
#[test]
fn lower_thirteen_branch_records_keep_office_mark_and_filing_date_with_their_leader() {
    let a = analysis();
    for (leader, date) in [
        ("河西宏一", "八、二、一六"),
        ("大森江里子", "八、二、一六"),
        ("中川宏昌", "八、二、二四"),
        ("赤羽一嘉", "八、二、一六"),
        ("中野洋昌", "八、二、一八"),
        ("池田香苗", "八、二、一六"),
        ("斉藤鉄夫", "八、二、一六"),
        ("平林晃", "八、二、二四"),
        ("山崎正恭", "八、二、一六"),
        ("濵地雅一", "八、二、一六"),
        ("吉田宣弘", "八、二、一七"),
        ("河野義博", "八、二、一六"),
        ("山本譲司", "八、二、二六"),
    ] {
        let rows: Vec<_> = a
            .text
            .lines()
            .filter(|l| compact(l).contains(leader))
            .collect();
        assert_eq!(rows.len(), 1, "leader {leader}");
        assert!(
            rows[0].contains("衆議院議員") && rows[0].contains('〇') && rows[0].contains(date),
            "{leader}: {}",
            rows[0]
        );
    }
}
#[test]
fn existing_upper_nine_branch_records_keep_all_their_original_fields() {
    let a = analysis();
    for (leader, office, date) in [
        ("西條昌良", "衆議院議員", "八、四、九"),
        ("石川昭政", "衆議院議員", "八、四、九"),
        ("尾身朝子", "衆議院議員", "八、四、一〇"),
        ("俵田祐児", "衆議院議員", "八、四、一三"),
        ("河野正美", "衆議院議員", "八、四、九"),
        ("木村義雄", "参議院議員", "八、三、九"),
        ("佐藤英道", "衆議院議員", "八、二、一六"),
        ("石井啓一", "衆議院議員", "八、二、一六"),
        ("角田秀穂", "衆議院議員", "八、二、一六"),
    ] {
        let rows: Vec<_> = a
            .text
            .lines()
            .filter(|l| compact(l).contains(leader))
            .collect();
        assert_eq!(rows.len(), 1, "leader {leader}");
        assert!(
            rows[0].contains(office) && rows[0].contains('〇') && rows[0].contains(date),
            "{leader}: {}",
            rows[0]
        );
    }
}

#[test]
fn every_lower_record_preserves_one_native_column_anchor_and_unique_source_ids() {
    let a = analysis();
    let p = &a.pages[0];
    let lines: Vec<_> = p.regions.iter().flat_map(|r| r.lines.iter()).collect();
    let mut ids: Vec<_> = lines.iter().flat_map(|l| l.words.iter()).copied().collect();
    let count = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(count, ids.len());
    let lower: Vec<_> = lines
        .iter()
        .filter(|l| {
            l.words
                .first()
                .is_some_and(|i| p.words[*i].y0 > p.height / 2.0)
        })
        .collect();
    assert_eq!(lower.len(), 13);
    for line in lower {
        let ws: Vec<_> = line.words.iter().map(|i| &p.words[*i]).collect();
        assert_eq!(
            compact(&line.text),
            compact(&ws.iter().map(|w| w.text.as_str()).collect::<String>())
        );
        let office = ws.iter().position(|w| w.text == "衆議院議員").unwrap();
        let edge = ws[..office]
            .iter()
            .map(|w| w.x1)
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(ws[office..].iter().all(|w| (w.x1 - edge).abs() < 1.0));
        let suffix: Vec<_> = line.text.rsplit('　').take(3).collect();
        assert_eq!(suffix[1], "〇");
        assert_eq!(suffix[2], "衆議院議員");
        assert_eq!(suffix[0].split('、').count(), 3);
    }
}

use super::*;
#[test]
fn a_standalone_outline_row_is_already_valid_structure_and_is_not_reparented(){
 let original=vec![Block::Paragraph{runs:vec![run("独立した前文")]},Block::Shinkyu{rows:vec![row("第一章　独立した章","第一章　独立した章"),row("第一条　後続する本文。","第一条　従前の本文。")]}];assert_eq!(merge_continued_tables(original.clone()),original);
}
#[test]
fn an_outline_prefix_with_a_typed_table_keeps_its_cell_material() {
    let table = NestedTable {
        rows: vec![vec![vec![run("原本のセル")]]],
        merges: vec![],
    };
    let mut prefix = row("第一章　表のある欄", "第一章　表のある欄");
    prefix.after_table = Some(table.clone());
    let rows = merged(prefix);
    assert!(rows.iter().any(|r| r.after_table.as_ref() == Some(&table)));
}
#[test]
fn outline_prefix_normalization_is_idempotent_and_keeps_run_styles() {
    let rows = merged(row("第三節　任意の節名", "第三節　任意の節名"));
    let once = vec![Block::Shinkyu { rows }];
    assert_eq!(merge_continued_tables(once.clone()), once);
}
#[test]
fn a_terminal_outline_heading_is_preserved_when_no_following_article_exists() {
    let prior = row("第一条　本文。", "第一条　旧本文。");
    let heading = row("第二章　後続する章", "第二章　後続する章");
    let out = merge_continued_tables(vec![
        Block::Shinkyu {
            rows: vec![prior.clone()],
        },
        Block::Shinkyu {
            rows: vec![heading.clone()],
        },
    ]);
    let Block::Shinkyu { rows } = &out[0] else {
        panic!()
    };
    assert_eq!(rows, &vec![prior, heading]);
}
fn run(text: &str) -> Run {
    Run {
        text: text.into(),
        underline: false,
        table: None,
    }
}
fn row(a: &str, b: &str) -> ShinkyuRow {
    ShinkyuRow {
        after: vec![run(a)],
        before: vec![run(b)],
        after_table: None,
        before_table: None,
    }
}
fn merged(prefix: ShinkyuRow) -> Vec<ShinkyuRow> {
    let blocks = vec![
        Block::Shinkyu {
            rows: vec![row("第一条　先行する本文。", "第一条　先行する従前本文。")],
        },
        Block::Shinkyu {
            rows: vec![
                prefix,
                row(
                    "第九十九条　後続する本文。",
                    "第九十九条　後続する従前本文。",
                ),
            ],
        },
    ];
    let result = merge_continued_tables(blocks);
    assert_eq!(result.len(), 1);
    let Block::Shinkyu { rows } = result.into_iter().next().unwrap() else {
        panic!()
    };
    rows
}
#[test]
fn an_outline_heading_at_the_next_field_start_cannot_become_the_previous_articles_tail() {
    let rows = merged(row("第二章　任意の章名", "第二章　任意の章名"));
    assert_eq!(
        rows[0],
        row("第一条　先行する本文。", "第一条　先行する従前本文。")
    );
    assert!(runs_plain(&rows[1].after).starts_with("第二章　任意の章名\n第九十九条"));
    assert!(runs_plain(&rows[1].before).starts_with("第二章　任意の章名\n第九十九条"));
}
#[test]
fn all_higher_legal_levels_and_unbounded_wrapped_titles_keep_their_following_body() {
    for key in [
        "第一編",
        "第三章",
        "第１６節",
        "第5款",
        "第百目",
        "第十六章の二の二",
    ] {
        let title=format!("{key}　任意の非常に長い見出し\n折返しの続きを含め\nさらに続く長い見出しの部分\n最後の見出しの部分");
        let mut prefix = row(&title, &title);
        prefix.after[0].underline = true;
        let rows = merged(prefix);
        assert_eq!(rows.len(), 2, "{key}");
        assert!(!runs_plain(&rows[0].after).contains(key));
        assert_eq!(
            runs_plain(&rows[1].after),
            format!("{title}\n第九十九条　後続する本文。")
        );
        assert!(rows[1].after[0].underline);
    }
}
#[test]
fn a_reference_to_a_chapter_is_still_a_sentence_continuation() {
    let result = merge_continued_tables(vec![
        Block::Shinkyu {
            rows: vec![row("甲に係る", "乙に係る")],
        },
        Block::Shinkyu {
            rows: vec![row(
                "第二章に規定する要件を満たすもの。",
                "第二章に規定する旧要件を満たすもの。",
            )],
        },
    ]);
    let Block::Shinkyu { rows } = &result[0] else {
        panic!()
    };
    assert_eq!(rows.len(), 1);
    assert_eq!(
        runs_plain(&rows[0].after),
        "甲に係る\n第二章に規定する要件を満たすもの。"
    );
}
#[test]
fn a_new_chapter_with_a_creation_note_never_moves_into_the_previous_article() {
    let rows = merged(row("第二章　新しい構造", "（新設）"));
    assert_eq!(
        rows[0],
        row("第一条　先行する本文。", "第一条　先行する従前本文。")
    );
    assert!(runs_plain(&rows[1].after).starts_with("第二章"));
}

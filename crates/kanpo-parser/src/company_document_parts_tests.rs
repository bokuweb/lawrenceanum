use super::*;
fn doc(text: &str) -> Document {
    Document {
        format: "unknown".into(),
        blocks: vec![Block::Paragraph {
            runs: vec![Run::plain(text)],
        }],
    }
}
fn text(d: &Document) -> String {
    d.blocks
        .iter()
        .map(|b| match b {
            Block::Paragraph { runs } => runs.iter().map(|r| r.text.as_str()).collect::<String>(),
            _ => panic!(),
        })
        .collect()
}
#[test]
fn preserves_original_note_newline_at_article_boundary() {
    let old = doc("注記金額\n第16期決算公告\n令和八年");
    let d = [doc("注記金額"), doc("第16期決算公告\n令和八年")];
    let got = repartition(&old, &d).unwrap();
    assert_eq!(text(&got[0]), "注記金額\n");
    assert_eq!(text(&got[1]), "第16期決算公告\n令和八年");
}
#[test]
fn preserves_every_ascii_and_ideographic_space() {
    assert!(repartition(&doc("株式会社 甲　乙"), &[doc("株式会社甲乙")]).is_none());
}
#[test]
fn refuses_nonnewline_character_disagreement() {
    assert!(repartition(&doc("会社123"), &[doc("会社124")]).is_none());
}
#[test]
fn keeps_the_original_run_style_when_split() {
    let mut old = doc("公告甲公告乙");
    let Block::Paragraph { runs } = &mut old.blocks[0] else {
        panic!()
    };
    runs[0].underline = true;
    let out = repartition(&old, &[doc("公告甲"), doc("公告乙")]).unwrap();
    for d in out {
        let Block::Paragraph { runs } = &d.blocks[0] else {
            panic!()
        };
        assert!(runs[0].underline);
    }
}
#[test]
fn retains_all_original_trailing_newlines() {
    let out = repartition(&doc("甲\n乙\n\n"), &[doc("甲"), doc("乙")]).unwrap();
    assert_eq!(text(&out[0]), "甲\n");
    assert_eq!(text(&out[1]), "乙\n\n");
}
#[test]
fn preserves_original_paragraph_boundary() {
    let mut old = doc("甲");
    old.blocks.extend(doc("乙丙").blocks);
    let out = repartition(&old, &[doc("甲乙"), doc("丙")]).unwrap();
    assert_eq!(out[0].blocks.len(), 2);
    assert_eq!(out[1].blocks.len(), 1);
}
#[test]
fn refuses_source_newold_blocks() {
    let old = Document {
        format: "shinkyu".into(),
        blocks: vec![Block::Shinkyu { rows: vec![] }],
    };
    assert!(repartition(&old, &[doc("甲")]).is_none());
}
#[test]
fn does_not_synthesize_extra_desired_newlines() {
    let out = repartition(&doc("甲乙"), &[doc("甲\n\n"), doc("乙")]).unwrap();
    assert_eq!(text(&out[0]), "甲");
    assert_eq!(text(&out[1]), "乙");
}
fn table_run(n: &str) -> Run {
    Run {
        text: String::new(),
        underline: false,
        table: Some(crate::model::NestedTable {
            rows: vec![vec![vec![Run::plain(n)]]],
            merges: vec![],
        }),
    }
}
#[test]
fn complete_tables_keep_exact_values_and_original_order() {
    let t1 = table_run("123");
    let t2 = table_run("456");
    let old = Document {
        format: "unknown".into(),
        blocks: vec![Block::Paragraph {
            runs: vec![Run::plain("甲"), t1.clone(), Run::plain("乙"), t2.clone()],
        }],
    };
    let mut a = doc("甲");
    let Block::Paragraph { runs } = &mut a.blocks[0] else {
        panic!()
    };
    runs.push(t1);
    let mut b = doc("乙");
    let Block::Paragraph { runs } = &mut b.blocks[0] else {
        panic!()
    };
    runs.push(t2);
    let out = repartition(&old, &[a.clone(), b.clone()]).unwrap();
    assert_eq!(out, vec![a, b]);
}
#[test]
fn mismatched_original_table_is_refused() {
    let mut old = doc("甲");
    let Block::Paragraph { runs } = &mut old.blocks[0] else {
        panic!()
    };
    runs.push(table_run("123"));
    let mut wanted = doc("甲");
    let Block::Paragraph { runs } = &mut wanted.blocks[0] else {
        panic!()
    };
    runs.push(table_run("124"));
    assert!(repartition(&old, &[wanted]).is_none());
}
#[test]
fn table_and_text_on_same_source_run_is_ambiguous() {
    let mut r = table_run("123");
    r.text = "追加文字".into();
    let d = Document {
        format: "unknown".into(),
        blocks: vec![Block::Paragraph { runs: vec![r] }],
    };
    assert!(repartition(&d, &[doc("追加文字")]).is_none());
}

use super::*;

fn line(field: Field, start: usize, end: usize, bounds: Rect) -> SourceLine {
    SourceLine {
        field,
        start,
        end,
        bounds: Some(bounds),
    }
}
#[test]
fn source_order_is_independent_of_scale_origin_and_picture_content() {
    for scale in [0.125, 1., 32.] {
        for origin in [-256., 0., 1024.] {
            let b = |r: Rect| r.map(|v| v * scale + origin);
            assert_eq!(
                relation(b([50., 10., 60., 60.]), b([10., 10., 40., 60.])),
                Some(Relation::Before)
            );
            assert_eq!(
                relation(b([10., 10., 40., 20.]), b([10., 30., 40., 60.])),
                Some(Relation::Before)
            );
            assert_eq!(
                relation(b([10., 30., 40., 60.]), b([50., 10., 60., 20.])),
                Some(Relation::After)
            );
            assert_eq!(
                relation(b([10., 10., 40., 20.]), b([50., 30., 60., 60.])),
                None
            );
            assert_eq!(
                relation(b([10., 10., 40., 40.]), b([30., 30., 60., 60.])),
                None
            );
            let layout = PageLayout {
                starts: [17, 0, 0],
                lines: vec![line(Field::Paragraph, 17, 31, b([50., 10., 60., 60.]))],
                comparison: false,
                interrupts: true,
            };
            assert!(matches!(
                stream_target(&layout, b([10., 10., 40., 60.]), &[], false).unwrap(),
                Target::Stream {
                    field: Field::Paragraph,
                    offset: 31
                }
            ));
        }
    }
}
#[test]
fn original_closed_fields_do_not_borrow_another_role_or_unknown_source() {
    let image = [10., 20., 40., 80.];
    let frames = [[0., 0., 70., 100.], [0., 100., 70., 200.]];
    let mut layout = PageLayout {
        starts: [0, 0, 0],
        lines: vec![
            line(Field::After, 0, 12, [50., 10., 60., 90.]),
            line(Field::Before, 0, 19, [50., 110., 60., 190.]),
        ],
        comparison: true,
        interrupts: false,
    };
    assert!(matches!(
        stream_target(&layout, image, &frames, false).unwrap(),
        Target::Stream {
            field: Field::After,
            offset: 12
        }
    ));
    assert!(stream_target(&layout, image, &[[0., 0., 70., 200.]], false).is_err());
    layout.lines[1].bounds = None;
    assert!(stream_target(&layout, image, &frames, false).is_err());
    let empty = PageLayout {
        starts: [12, 0, 0],
        lines: vec![],
        comparison: false,
        interrupts: false,
    };
    assert!(stream_target(&empty, image, &[], true).is_err());
    assert!(matches!(
        stream_target(&empty, image, &[], false).unwrap(),
        Target::Stream {
            field: Field::Paragraph,
            offset: 12
        }
    ));
}
#[test]
fn figures_insert_at_original_unicode_boundaries_without_consuming_text_or_styles() {
    let mut doc = Document {
        format: "prose".into(),
        blocks: vec![Block::Paragraph {
            runs: vec![Inline::Text {
                text: "甲 乙丙".into(),
                underline: true,
            }],
        }],
    };
    assert!(model::insert_stream(&mut doc, Field::Paragraph, 2, 9));
    assert!(model::insert_stream(&mut doc, Field::Paragraph, 2, 8));
    let Block::Paragraph { runs } = &doc.blocks[0] else {
        panic!()
    };
    assert_eq!(model::text(runs), "甲乙丙");
    assert!(matches!(&runs[0],Inline::Text{text,underline:true}if text=="甲 乙"));
    assert!(matches!(runs[1], Inline::Image { asset: 8 }));
    assert!(matches!(runs[2], Inline::Image { asset: 9 }));
    assert!(matches!(&runs[3],Inline::Text{text,underline:true}if text=="丙"));
}
#[test]
fn duplicate_equal_tables_do_not_choose_an_image_cell_by_content() {
    let table = crate::model::NestedTable {
        rows: vec![vec![vec![]]],
        merges: vec![],
    };
    let mut doc = Document {
        format: "prose".into(),
        blocks: vec![Block::Paragraph {
            runs: vec![Inline::Table {
                table: Table::from(&table),
            }],
        }],
    };
    assert!(model::insert_cell(
        &mut doc,
        Field::Paragraph,
        &table,
        0,
        0,
        1
    ));
    let Block::Paragraph { runs } = &mut doc.blocks[0] else {
        panic!()
    };
    runs.push(Inline::Table {
        table: Table::from(&table),
    });
    assert!(!model::insert_cell(
        &mut doc,
        Field::Paragraph,
        &table,
        0,
        0,
        2
    ));
}

#[test]
fn figure_order_does_not_use_draw_ids_payloads_or_array_order() {
    fn asset(id: usize, page: usize, b: Rect) -> Asset {
        Asset {
            id,
            page,
            draw: 100 - id,
            document_sha256: String::new(),
            encoded_sha256: String::new(),
            encoded_bytes: vec![id as u8],
            source: serde_json::Value::Null,
            media_box: [0., 0., 100., 100.],
            crop_box: [0., 0., 100., 100.],
            rotate: 0,
            quad: None,
            bounds: Some(b),
            visibility_verified: false,
            raster_layout: None,
        }
    }
    let mut assets = vec![
        asset(0, 1, [10., 50., 40., 80.]),
        asset(1, 1, [10., 10., 40., 40.]),
        asset(2, 2, [10., 10., 40., 40.]),
    ];
    assert_eq!(figure_order(&[0, 2, 1], &assets), Some(vec![1, 0, 2]));
    assert_eq!(figure_order(&[2, 1, 0], &assets), Some(vec![1, 0, 2]));
    assets[0].bounds = Some([50., 50., 80., 80.]);
    assert!(figure_order(&[0, 1], &assets).is_none());
    assert!(figure_order(&[1, 1], &assets).is_none());
}

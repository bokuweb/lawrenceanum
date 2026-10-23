use super::*;

#[cfg(test)]
mod disjoint_envelopes {
    use super::*;
    fn side(start: usize, x: f32, y: f32, count: usize) -> Vec<Col> {
        (0..count)
            .map(|i| Col {
                id: start + i,
                x,
                x1: x + 160.0,
                y: y + i as f32 * 12.0,
                y1: y + i as f32 * 12.0 + 8.0,
                text: "任意の本文を配置した行の内容をここへ記す".into(),
            })
            .collect()
    }
    #[test]
    fn disjoint_y_envelopes_do_not_need_fixed_line_counts_or_text() {
        for n in [1, 2, 3, 8, 21] {
            for m in [1, 2, 4, 13] {
                for right_above in [false, true] {
                    let end = n as f32 * 12.0 + 4.0;
                    let (mut left, mut right) = if right_above {
                        (side(1000, 20.0, end, m), side(100, 200.0, 0.0, n))
                    } else {
                        (side(100, 20.0, 0.0, n), side(1000, 200.0, end, m))
                    };
                    for scale in [0.5, 1.0, 4.0] {
                        for ws in [&left, &right] {
                            assert!(!ws.is_empty());
                        }
                        let scaled = |ws: &[Col]| {
                            ws.iter()
                                .map(|w| Col {
                                    id: w.id,
                                    x: w.x * scale + 37.0,
                                    x1: w.x1 * scale + 37.0,
                                    y: w.y * scale + 63.0,
                                    y1: w.y1 * scale + 63.0,
                                    text: "内容を置き換えても同じ空間関係を保つ".into(),
                                })
                                .collect::<Vec<_>>()
                        };
                        assert!(
                            horizontal_dual_gutter(
                                &scaled(&left),
                                &scaled(&right),
                                &[],
                                190.0 * scale + 37.0
                            )
                            .is_some(),
                            "n={n},m={m},right_above={right_above},scale={scale}"
                        );
                    }
                    left.reverse();
                    right.reverse();
                    assert!(horizontal_dual_gutter(&left, &right, &[], 190.0).is_some());
                }
            }
        }
    }
    #[test]
    fn overlapping_staggered_columns_keep_column_order() {
        let left = side(100, 20.0, 12.0, 8);
        let right = side(1000, 200.0, 0.0, 8);
        assert!(is_column_split(&left, &right, false));
        let observed: Vec<_> =
            horizontal_lines_around(left.into_iter().chain(right).collect(), &[], &[], &[], &[])
                .into_iter()
                .flat_map(|l| l.words)
                .collect();
        assert_eq!(observed, (100..108).chain(1000..1008).collect::<Vec<_>>());
    }
    #[test]
    fn whitespace_only_bands_read_by_source_height_through_full_line_routing() {
        for n in [2, 3, 8] {
            for m in [1, 2, 4] {
                let top = side(100, 200.0, 0.0, n);
                let bottom = side(1000, 20.0, n as f32 * 12.0 + 4.0, m);
                let mut words: Vec<_> = bottom.into_iter().chain(top).collect();
                words.reverse();
                let observed: Vec<_> = horizontal_lines_around(words, &[], &[], &[], &[])
                    .into_iter()
                    .flat_map(|l| l.words)
                    .collect();
                assert_eq!(
                    observed,
                    (100..100 + n).chain(1000..1000 + m).collect::<Vec<_>>(),
                    "n={n},m={m}"
                );
            }
        }
    }
    #[test]
    fn explicit_vertical_source_divider_keeps_left_column_first() {
        let left = side(1000, 20.0, 100.0, 2);
        let right = side(100, 200.0, 0.0, 2);
        let observed: Vec<_> = horizontal_lines_around(
            left.into_iter().chain(right).collect(),
            &[],
            &[(190.0, 0.0, 160.0)],
            &[],
            &[],
        )
        .into_iter()
        .flat_map(|l| l.words)
        .collect();
        assert_eq!(observed, vec![1000, 1001, 100, 101]);
    }
    #[test]
    fn separate_boxed_groups_are_not_reclassified_as_one_horizontal_band() {
        assert!(is_column_split(
            &side(1000, 20.0, 100.0, 3),
            &side(100, 200.0, 0.0, 3),
            true
        ));
    }
}

#[cfg(test)]
mod orthogonal_alternatives {
    use super::*;
    fn word(id: usize, x: f32, y: f32, w: f32, h: f32, t: &str) -> Col {
        Col {
            id,
            x,
            x1: x + w,
            y,
            y1: y + h,
            text: t.into(),
        }
    }
    fn diagonal() -> (Vec<Col>, Vec<Col>) {
        (
            vec![word(1, 20.0, 100.0, 160.0, 8.0, "下の内容")],
            vec![word(2, 200.0, 0.0, 160.0, 8.0, "上の内容")],
        )
    }
    #[test]
    fn crossing_or_overlapping_table_bounds_prevent_an_orthogonal_proposal() {
        let (a, b) = diagonal();
        assert!(horizontal_dual_gutter(&a, &b, &[], 190.0).is_some());
        assert!(horizontal_dual_gutter(&a, &b, &[(0.0, 50.0, 400.0, 70.0)], 190.0).is_none());
        assert!(horizontal_dual_gutter(&a, &b, &[(20.0, 0.0, 180.0, 20.0)], 190.0).is_none());
    }
    #[test]
    fn unknown_or_degenerate_source_envelopes_do_not_authorize_a_dual_cut() {
        let (mut a, b) = diagonal();
        a[0].y = f32::NAN;
        assert!(horizontal_dual_gutter(&a, &b, &[], 190.0).is_none());
        a[0].y = a[0].y1;
        assert!(horizontal_dual_gutter(&a, &b, &[], 190.0).is_none());
        assert!(horizontal_dual_gutter(&[], &b, &[], 190.0).is_none());
    }
    #[test]
    fn a_source_vertical_fragment_across_the_empty_band_is_negative_evidence() {
        let (a, b) = diagonal();
        let words: Vec<_> = a.into_iter().chain(b).collect();
        assert!(clear_dual_row(
            &words,
            &Context {
                vrules: &[],
                hrules: &[],
                tables: &[],
                allow_orthogonal: true
            },
            54.0
        ));
        assert!(!clear_dual_row(
            &words,
            &Context {
                vrules: &[(190.0, 50.0, 60.0)],
                hrules: &[],
                tables: &[],
                allow_orthogonal: true
            },
            54.0
        ));
    }
    #[test]
    fn an_upper_group_is_not_interleaved_when_a_second_vertical_candidate_exists() {
        for scale in [1.0, 2.0] {
            for labels in [["甲", "乙", "い", "ろ", "は"], ["A", "B", "x", "y", "z"]] {
                let mut words = vec![
                    word(100, 146.7202, 47.0002, 14.0, 14.0, labels[0]),
                    word(101, 272.7202, 47.0002, 14.0, 14.0, labels[1]),
                    word(200, 87.8197, 79.9996, 9.0, 9.0, labels[2]),
                    word(201, 114.8197, 79.9996, 9.0, 9.0, labels[3]),
                    word(202, 141.8197, 79.9996, 9.0, 9.0, labels[4]),
                    word(
                        300,
                        317.0,
                        359.9449,
                        216.0,
                        9.0,
                        "別の記事の長い標題をこの位置へ配置する",
                    ),
                    word(400, 70.0, 385.0, 700.0, 8.0, "その本文を配置する"),
                ];
                for w in &mut words {
                    w.x = w.x * scale + 31.0;
                    w.x1 = w.x1 * scale + 31.0;
                    w.y = w.y * scale + 59.0;
                    w.y1 = w.y1 * scale + 59.0;
                }
                words.reverse();
                let observed: Vec<_> = horizontal_lines_around(words, &[], &[], &[], &[])
                    .into_iter()
                    .flat_map(|l| l.words)
                    .collect();
                assert_eq!(observed, vec![100, 101, 200, 201, 202, 300, 400]);
            }
        }
    }
}

#[cfg(test)]
mod source_wall_fences {
    use super::*;
    #[test]
    fn a_source_wall_far_from_the_candidate_still_fences_a_region_wide_row_cut() {
        let words = vec![
            Col {
                id: 0,
                x: 10.0,
                x1: 30.0,
                y: 10.0,
                y1: 20.0,
                text: "a".into(),
            },
            Col {
                id: 1,
                x: 80.0,
                x1: 100.0,
                y: 30.0,
                y1: 40.0,
                text: "b".into(),
            },
            Col {
                id: 2,
                x: 200.0,
                x1: 220.0,
                y: 50.0,
                y1: 60.0,
                text: "c".into(),
            },
        ];
        assert!(clear_dual_row(
            &words,
            &Context {
                vrules: &[],
                hrules: &[],
                tables: &[],
                allow_orthogonal: true
            },
            25.0
        ));
        assert!(!clear_dual_row(
            &words,
            &Context {
                vrules: &[(150.0, 0.0, 100.0)],
                hrules: &[],
                tables: &[],
                allow_orthogonal: true
            },
            25.0
        ));
    }
}

#[cfg(test)]
mod balanced_source_spans {
    use super::*;
    fn w(id: usize, text: String) -> Col {
        Col {
            id,
            x: 0.0,
            x1: 8.0,
            y: 0.0,
            y1: 8.0,
            text,
        }
    }
    fn lines(ids: &[usize]) -> Vec<trace::Line> {
        ids.iter()
            .map(|id| trace::Line::text("unchanged".into(), vec![*id]))
            .collect()
    }
    #[test]
    fn all_balanced_delimiter_types_preserve_arbitrary_nested_content_without_count_caps() {
        for (open, close) in [
            ('(', ')'),
            ('（', '）'),
            ('[', ']'),
            ('「', '」'),
            ('『', '』'),
            ('【', '】'),
            ('〈', '〉'),
            ('《', '》'),
        ] {
            for n in [1, 2, 7, 31] {
                let mut words = vec![w(0, format!("arbitrary{open}"))];
                for i in 1..=n {
                    words.push(w(i, "語を別の内容に置き換える".repeat(i)));
                }
                words.push(w(n + 1, format!("{close}suffix")));
                let ids: Vec<_> = (0..words.len()).collect();
                let before = lines(&ids);
                assert!(orthogonal_order_preserves_spans(&words, &before, &before));
                let mut changed = ids;
                changed.swap(n, n + 1);
                assert!(!orthogonal_order_preserves_spans(
                    &words,
                    &before,
                    &lines(&changed)
                ));
            }
        }
    }
    #[test]
    fn a_complete_delimited_span_can_move_as_one_unit_but_outside_words_cannot_enter_it() {
        let words = vec![
            w(0, "outside".into()),
            w(1, "prefix（".into()),
            w(2, "arbitrary".into()),
            w(3, "）suffix".into()),
            w(4, "outside".into()),
        ];
        let before = lines(&[0, 1, 2, 3, 4]);
        assert!(orthogonal_order_preserves_spans(
            &words,
            &before,
            &lines(&[4, 1, 2, 3, 0])
        ));
        assert!(!orthogonal_order_preserves_spans(
            &words,
            &before,
            &lines(&[0, 1, 4, 2, 3])
        ));
    }
    #[test]
    fn unknown_duplicate_or_incomplete_membership_and_unbalanced_delimiters_do_not_authorize_new_order(
    ) {
        for text in ["（open", "unmatched）", "（mismatch]", "「nested（」"] {
            let words = [w(0, text.into()), w(1, "body".into())];
            assert!(!orthogonal_order_preserves_spans(
                &words,
                &lines(&[0, 1]),
                &lines(&[1, 0])
            ));
        }
        for text in ["「nested（）」", "prefix（【arbitrary】）suffix"] {
            let words = [w(0, text.into()), w(1, "body".into())];
            assert!(orthogonal_order_preserves_spans(
                &words,
                &lines(&[0, 1]),
                &lines(&[1, 0])
            ));
        }
        let words = [w(0, "a".into()), w(1, "b".into())];
        for ids in [&[0, 0][..], &[0][..], &[0, 9][..]] {
            assert!(!orthogonal_order_preserves_spans(
                &words,
                &lines(&[0, 1]),
                &lines(ids)
            ));
        }
    }
    #[test]
    fn complete_table_cell_attributes_cannot_change_when_source_word_order_is_equal() {
        let words = [w(0, "a".into()), w(1, "b".into())];
        let table = trace::Table {
            x0: 0.0,
            y0: 0.0,
            x1: 10.0,
            y1: 10.0,
            rows: 1,
            cols: 1,
            cells: vec![trace::TableCell {
                row: 0,
                col: 0,
                rowspan: 1,
                colspan: 1,
                x0: 0.0,
                y0: 0.0,
                x1: 10.0,
                y1: 10.0,
                text: "a".into(),
                words: vec![0],
            }],
        };
        let before = vec![
            trace::Line {
                text: "a".into(),
                words: vec![],
                table: Some(table),
            },
            trace::Line::text("b".into(), vec![1]),
        ];
        let mut after = before.clone();
        assert!(orthogonal_order_preserves_spans(&words, &before, &after));
        after[0].table.as_mut().unwrap().cells[0].rowspan = 2;
        assert!(!orthogonal_order_preserves_spans(&words, &before, &after));
    }
}

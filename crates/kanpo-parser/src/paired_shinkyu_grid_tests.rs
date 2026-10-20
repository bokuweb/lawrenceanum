use super::*;
fn fixture(signature: bool) -> (Vec<Vec<Col>>, Vec<Grid>) {
    use crate::grid::GridCell;
    let grid = |xs: Vec<f32>, ys: Vec<f32>| {
        let cells = (0..xs.len() - 1)
            .flat_map(|x| {
                (0..ys.len() - 1).map(move |y| GridCell {
                    x0: x,
                    x1: x + 1,
                    y0: y,
                    y1: y + 1,
                })
            })
            .collect();
        Grid { xs, ys, cells }
    };
    let grids = vec![
        grid(vec![40., 260., 280.], vec![60., 400., 740.]),
        grid(vec![40., 80., 120., 160., 200., 240.], vec![80., 210., 350., 390.]),
        grid(vec![40., 80., 120., 160., 200., 240.], vec![420., 550., 690., 730.]),
    ];
    let c = |id, x, y, h, t: &str| Col {
        id,
        x,
        x1: x + 8.,
        y,
        y1: y + h,
        text: t.into(),
    };
    let mut a = vec![
        c(0, 266., 218., 24., "改正後"),
        c(1, 266., 558., 24., "改正前"),
        c(2, 320., 70., 80., "日付の列"),
        c(3, 50., 440., 24., "甲"),
        c(4, 90., 440., 24., "乙"),
        c(5, 130., 440., 24., "丙"),
        c(6, 50., 100., 24., "甲"),
        c(7, 90., 100., 24., "乙"),
        c(8, 130., 100., 24., "丙"),
    ];
    let mut b = vec![
        c(9, 50., 700., 24., "備考甲"),
        c(10, 90., 700., 24., "備考乙"),
        c(11, 130., 700., 24., "備考丙"),
    ];
    if signature {
        b.extend([c(12, 320., 650., 40., "担当大臣"), c(13, 320., 700., 16., "甲乙")]);
    }
    a.shrink_to_fit();
    (vec![a, b], grids)
}
#[test]
fn complete_paired_grids_and_an_existing_preamble_column_rejoin_once() {
    for signature in [false, true] {
        let (mut ts, gs) = fixture(signature);
        let before: std::collections::HashSet<_> = ts.iter().flatten().map(|c| c.id).collect();
        rejoin_split_shinkyu_grid_tiers(&mut ts, &gs);
        assert_eq!(ts.len(), 1);
        let ids: Vec<_> = ts.iter().flatten().map(|c| c.id).collect();
        assert_eq!(ids.len(), before.len());
        assert_eq!(ids.into_iter().collect::<std::collections::HashSet<_>>(), before);
        rejoin_split_shinkyu_grid_tiers(&mut ts, &gs);
        assert_eq!(ts.len(), 1);
    }
}
#[test]
fn unrelated_tiers_articles_and_incomplete_grids_remain_separate() {
    for case in 0..17 {
        let (mut ts, mut gs) = fixture(true);
        match case {
            0 => {
                ts[0][0].text = "変更後".into();
                ts[0][1].text = "変更前".into();
            }
            1 => {
                gs.remove(0);
            }
            2 => {
                gs.remove(1);
            }
            3 => {
                gs[1].xs[1] += 3.;
            }
            4 => {
                gs[1].ys[3] -= 3.;
            }
            5 => ts[1][3].text = "〇別省告示第三号".into(),
            6 => ts[1][3].text = "これは別の記事の長い本文です".into(),
            7 => {
                ts[1][3].x += 40.;
                ts[1][3].x1 += 40.;
            }
            8 => {
                ts[1][3].x = 265.;
                ts[1][3].x1 = 273.;
            }
            9 => {
                let mut c = ts[1][0].clone();
                c.id = 99;
                ts.push(vec![c]);
            }
            10 => {
                let labels = ts[0][..2].to_vec();
                ts[1].extend(labels);
            }
            11 => ts[1][3].text = "終わり。".into(),
            12 => {
                ts[1].remove(0);
            }
            13 => {
                ts[0].remove(3);
            }
            14 => gs[2].ys[0] = 380.,
            15 => {
                ts[1][3].y = 155.;
                ts[1][3].y1 = 195.;
            }
            16 => {
                gs[2].ys = vec![420., 730.];
            }
            _ => unreachable!(),
        }
        let before: Vec<Vec<_>> = ts.iter().map(|t| t.iter().map(|c| c.id).collect()).collect();
        rejoin_split_shinkyu_grid_tiers(&mut ts, &gs);
        assert_eq!(
            ts.iter().map(|t| t.iter().map(|c| c.id).collect::<Vec<_>>()).collect::<Vec<_>>(),
            before,
            "negative {case}"
        );
    }
}

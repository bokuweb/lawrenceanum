use super::*;

fn column(id: usize, x: f32, y: f32, y1: f32, text: &str) -> Col {
    Col {
        id,
        x,
        x1: x + 9.,
        y,
        y1,
        text: text.into(),
    }
}

/// 号外第 223 号 2 ページの上の段: 高い 1 段の右に前の政令の本文の長い列が並び、左に前の政令の末尾の「する。」と署名、
/// 次の政令の公布文・御名御璽・日付・署名の短い列が続く。
fn fixture() -> Vec<Col> {
    let mut cols: Vec<Col> = (0..12).map(|i| column(i, 170. + 12.5 * i as f32, 61., 405., "建築士法の一部を改正する法律附則第一条")).collect();
    cols.extend([
        column(100, 158., 61., 90., "する。"),
        column(101, 145., 309., 405., "国土交通大臣井林辰憲"),
        column(102, 133., 309., 405., "内閣総理大臣高市早苗"),
        column(103, 108., 69., 277., "建築士法施行令の一部を改正する政令をここに公布する。"),
        column(104, 83., 77., 157., "御名御璽"),
        column(105, 58., 93., 165., "令和八年十月九日"),
        column(106, 46., 309., 405., "内閣総理大臣高市早苗"),
    ]);
    cols
}

fn tiers(cols: Vec<Col>) -> Vec<Vec<usize>> {
    split_tiers(cols, &TierRules { tables: vec![], boxes: vec![], hrules: vec![], vrules: vec![], sideways: false })
        .into_iter()
        .map(|t| t.iter().map(|c| c.id).collect())
        .collect()
}

#[test]
fn short_columns_beside_crossing_text_stay_in_one_tier() {
    // 左の短い列は段の本文ではない（cut まで届かず、下は署名の 3 列だけ）。署名を公布文の後に回さない。
    let tiers = tiers(fixture());
    assert!(tiers.iter().any(|t| [100, 101, 102, 103, 104, 105, 106].iter().all(|id| t.contains(id))), "{tiers:?}");
}

#[test]
fn tiered_text_beside_crossing_text_is_still_split() {
    // 左にも段ごとの本文（上の段の下端まで組んだ列と、下の段の上端から組んだ列が 4 列以上ずつ）があれば分ける。
    let mut cols: Vec<Col> = (0..12).map(|i| column(i, 170. + 12.5 * i as f32, 61., 405., "またぐ列")).collect();
    for i in 0..6 {
        let x = 40. + 12.5 * i as f32;
        cols.push(column(100 + i, x, 61., 233., "上の段の本文の列"));
        cols.push(column(200 + i, x, 241., 405., "下の段の本文の列"));
    }
    let tiers = tiers(cols);
    assert!(!tiers.iter().any(|t| t.contains(&100) && t.contains(&200)), "{tiers:?}");
}

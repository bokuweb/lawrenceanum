#[test]
fn original_master_fences_cover_source_material_without_bridging_float_gaps() {
    use crate::native_source_scene;
    let inputs = original_inputs();
    let pages = crate::analyze_item(&inputs).pages;
    for (i, axis) in [NativeAxis::Down, NativeAxis::Down, NativeAxis::Right]
        .into_iter()
        .enumerate()
    {
        let dom = scraper::Html::parse_document(&inputs[i].xhtml);
        let source = dom
            .select(&scraper::Selector::parse("page").unwrap())
            .next()
            .unwrap();
        println!(
            "original {i} labels={:?} regions={:?}",
            pages[i].native_comparison_labels,
            pages[i]
                .regions
                .iter()
                .map(|r| (r.unit, r.role, r.lines.len()))
                .collect::<Vec<_>>()
        );
        let scene =
            native_source_scene::for_page(source, inputs[i].rules.as_ref().unwrap()).unwrap();
        assert!(scene.matches(&pages[i].words));
        let mut records = BTreeMap::new();
        for node in source.select(&scraper::Selector::parse("word").unwrap()) {
            let a = node.value();
            let b = ["xmin", "ymin", "xmax", "ymax"]
                .map(|key| a.attr(key).unwrap().parse::<f32>().unwrap());
            records
                .entry((node.text().collect::<String>(), b.map(f32::to_bits)))
                .or_insert_with(Vec::new)
                .push(node);
        }
        let cross = if axis == NativeAxis::Down { 0 } else { 1 };
        let supports = pages[i]
            .regions
            .iter()
            .filter(|r| i != 0 || matches!(r.role, Role::After | Role::Before))
            .flat_map(|r| &r.lines)
            .flat_map(|l| {
                l.words.iter().chain(
                    l.table
                        .iter()
                        .flat_map(|t| t.cells.iter().flat_map(|c| &c.words)),
                )
            })
            .map(|&id| {
                let w = &pages[i].words[id];
                let nodes = &records[&(w.raw.clone(), w.source_box.map(f32::to_bits))];
                assert_eq!(nodes.len(), 1, "unique occurrence {id}");
                let node = nodes[0];
                let NativeBounds::Glyphs(b) = pages[i].native_bounds[id] else {
                    panic!("native bounds {id}")
                };
                let total = source
                    .value()
                    .attr("data-native-glyph-count")
                    .unwrap()
                    .parse()
                    .ok();
                if axis == NativeAxis::Down {
                    if let Some(p) = crate::native_column_flow::from_word(node, total)
                        .and_then(|f| f.vertical_path(b))
                    {
                        return [p[0], p[0]];
                    }
                } else if let Some(baseline) = pages[i].native_horizontal_baselines[id] {
                    return [baseline, baseline];
                }
                if let Some(occurrence) = node.value().attr("data-native-source-occurrence") {
                    let values = occurrence
                        .split_whitespace()
                        .skip(3)
                        .map(|s| s.parse::<f64>().unwrap())
                        .collect::<Vec<_>>();
                    return [values[cross], values[cross + 2]];
                }
                [f64::from(b[cross]), f64::from(b[cross + 2])]
            })
            .collect::<Vec<_>>();
        let fences = scene.parallel_fences(axis, &supports);
        println!(
            "original {i} axis={axis:?} supports={} fences={fences:?}",
            supports.len()
        );
        for q in [69.5, 425., 780.5] {
            assert!(
                fences.contains(&q),
                "original {i}: missing original master fence {q}, got {fences:?}"
            );
        }
    }
}

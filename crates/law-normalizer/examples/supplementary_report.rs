//! 附則の抽出率をまとめて見る: e-Gov 法令 XML のディレクトリを走査し、
//! [`law_normalizer::supplementary::analyze`] の集計と未解決の附則の先頭文を出す。
//!
//! ```sh
//! cargo run --release -p law-normalizer --example supplementary_report -- <xml_dir> [unresolved.tsv] [sample.tsv]
//! ```
//!
//! XML ファイルを 1 本渡すと、その法令の supplementary.json を標準出力に出す。
//! 法令 ID は XML の親ディレクトリ名 (`.cache/revisions/{law_id}/*.xml`) を使う。
//! `sample.tsv` には施行期日の規則を 500 件に 1 件書き出す (目視の精度確認用)。

use law_normalizer::supplementary::{self, EnforcementKind, SupplementaryStats};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name();
        if name.to_string_lossy().starts_with("._") {
            continue;
        }
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().and_then(|x| x.to_str()) == Some("xml") {
            out.push(p);
        }
    }
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let dir = PathBuf::from(
        args.get(1)
            .expect("usage: supplementary_report <xml_dir|law.xml> [unresolved.tsv] [sample.tsv]"),
    );
    // XML ファイル 1 本なら supplementary.json をそのまま出す。
    if dir.is_file() {
        let law_id = dir
            .parent()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let doc = law_normalizer::parse_law_xml(&std::fs::read(&dir)?, &law_id)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&supplementary::analyze(&doc))?
        );
        return Ok(());
    }
    let mut tsv = args.get(2).map(std::fs::File::create).transpose()?;
    let mut sample = args.get(3).map(std::fs::File::create).transpose()?;
    let mut seen = 0usize;
    let mut files = Vec::new();
    walk(&dir, &mut files);
    files.sort();

    let mut total = SupplementaryStats::default();
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    let mut transitional: BTreeMap<String, usize> = BTreeMap::new();
    let (mut whole, mut whole_dated, mut partial, mut partial_dated) =
        (0usize, 0usize, 0usize, 0usize);
    let mut laws = 0usize;
    // 参照の範囲ごとの (件数, article id に解決できた件数)。
    let mut by_scope: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for f in &files {
        let law_id = f
            .parent()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let Ok(doc) = law_normalizer::parse_law_xml(&std::fs::read(f)?, &law_id) else {
            continue;
        };
        laws += 1;
        let a = supplementary::analyze(&doc);
        supplementary::add_stats(&mut total, &a.stats);
        supplementary::count_kinds(&a, &mut kinds);
        for p in &a.provisions {
            for e in &p.enforcement {
                seen += 1;
                if let Some(w) = sample.as_mut().filter(|_| seen.is_multiple_of(500)) {
                    writeln!(
                        w,
                        "{law_id}\t{}\t{:?}\t{:?}\t{:?}\tdate={:?} deadline={:?}\ttargets={:?}\t{}",
                        p.index,
                        p.promulgation_date,
                        e.scope,
                        e.kind,
                        e.date,
                        e.deadline,
                        e.targets,
                        e.text.replace('\n', " ")
                    )?;
                }
                let dated = e.date.is_some() || e.deadline.is_some();
                match e.scope {
                    supplementary::EnforcementScope::Whole => {
                        whole += 1;
                        whole_dated += dated as usize;
                    }
                    supplementary::EnforcementScope::Partial => {
                        partial += 1;
                        partial_dated += dated as usize;
                    }
                }
                if e.kind == EnforcementKind::Unknown {
                    if let Some(w) = tsv.as_mut() {
                        writeln!(
                            w,
                            "{law_id}\t{}\tunknown_when\t{}",
                            p.index,
                            e.text.replace('\n', " ")
                        )?;
                    }
                }
            }
            let refs = p
                .enforcement
                .iter()
                .flat_map(|e| &e.target_refs)
                .chain(p.transitional.iter().flat_map(|t| &t.refs))
                .chain(p.readings.iter().flat_map(|r| &r.target_refs));
            for r in refs {
                let e = by_scope.entry(format!("{:?}", r.scope)).or_default();
                e.0 += 1;
                e.1 += r.article_id.is_some() as usize;
            }
            for t in &p.transitional {
                for k in &t.kinds {
                    *transitional.entry(format!("{k:?}")).or_default() += 1;
                }
            }
            if p.enforcement_unresolved {
                if let Some(w) = tsv.as_mut() {
                    let sp = &doc.suppl_provisions[(p.index - 1) as usize];
                    let first = sp
                        .articles
                        .iter()
                        .flat_map(|a| a.paragraphs.iter())
                        .chain(sp.paragraphs.iter())
                        .map(|p| p.text.lines().next().unwrap_or_default().to_string())
                        .next()
                        .unwrap_or_default();
                    writeln!(w, "{law_id}\t{}\tno_enforcement\t{first}", p.index)?;
                }
            }
        }
    }
    let pct = |a: usize, b: usize| {
        if b == 0 {
            0.0
        } else {
            100.0 * a as f64 / b as f64
        }
    };
    println!("laws: {laws}");
    println!("provisions: {}", total.provisions);
    println!(
        "with_enforcement: {} ({:.2}%)  unresolved: {}",
        total.with_enforcement,
        pct(total.with_enforcement, total.provisions),
        total.enforcement_unresolved
    );
    println!(
        "enforcement_rules: {} (whole {whole}, partial {partial})",
        total.enforcement_rules
    );
    println!(
        "  dated (date or deadline): whole {:.2}%  partial {:.2}%",
        pct(whole_dated, whole),
        pct(partial_dated, partial)
    );
    println!("  by kind: {kinds:?}");
    println!(
        "transitional_measures: {}  by kind: {transitional:?}",
        total.transitional_measures
    );
    println!("readings: {}", total.readings);
    println!(
        "refs: {}  resolved to article id: {} ({:.2}%)",
        total.refs,
        total.refs_resolved,
        pct(total.refs_resolved, total.refs)
    );
    for (scope, (n, ok)) in &by_scope {
        println!("  {scope}: {n} refs, resolved {ok} ({:.2}%)", pct(*ok, *n));
    }
    Ok(())
}

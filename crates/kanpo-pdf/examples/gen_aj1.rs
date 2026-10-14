//! Adobe-Japan1 の CID → Unicode 表（`data/aj1-unicode.bin`）を作る。
//!
//! 入力（いずれも permissive なライセンスのデータ）:
//! - Adobe cmap-resources の `Adobe-Japan1-7/cid2code.txt`（BSD-3-Clause、`data/LICENSE-Adobe-cmap-resources.md`）
//!   <https://github.com/adobe-type-tools/cmap-resources/blob/master/Adobe-Japan1-7/cid2code.txt>
//! - Unicode IVD の `IVD_Sequences.txt`（Unicode License v3、`data/LICENSE-Unicode.txt`）
//!   <https://www.unicode.org/ivd/data/2022-09-13/IVD_Sequences.txt>
//! - Unicode の `UnicodeData.txt`（同上）<https://www.unicode.org/Public/16.0.0/ucd/UnicodeData.txt>
//!
//! cargo run -p kanpo-pdf --example gen_aj1 -- cid2code.txt IVD_Sequences.txt UnicodeData.txt crates/kanpo-pdf/data/aj1-unicode.bin
//!
//! 出力は CID 順の u32（リトルエンディアン）の並び。0 は対応無し。
//!
//! 字の選び方:
//! 1. UniJIS 系の CMap の列から、横書き用（`v` の付かない）符号を優先して採る。部首（康熙部首・
//!    CJK 部首補助）と〈〉の古い符号は、ほかに候補があれば採らない
//! 2. CMap に無い CID は、JIS78 字形（`78` 列）なら同じ JIS 符号の CID の字、異体字なら IVD で
//!    その CID を指す異体字シーケンスの基底字にする
//! 3. CJK 互換漢字は統合漢字（IVD の基底字か、正規分解）にする。本文の検索・照合のため

use std::collections::HashMap;
use std::io::Write;

fn avoided(u: u32) -> bool {
    (0x2E80..=0x2FDF).contains(&u) || u == 0x2329 || u == 0x232A
}

fn compat(u: u32) -> bool {
    (0xF900..=0xFAFF).contains(&u) || (0x2F800..=0x2FA1F).contains(&u)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [cid2code, ivd_path, ucd_path, output] = &args[..] else {
        panic!("usage: gen_aj1 cid2code.txt IVD_Sequences.txt UnicodeData.txt out.bin");
    };

    // IVD: CID → 基底字（最初に出たもの）。
    let mut ivd: HashMap<usize, u32> = HashMap::new();
    for line in std::fs::read_to_string(ivd_path).unwrap().lines() {
        let Some((seq, rest)) = line.split_once(';') else {
            continue;
        };
        let mut rest = rest.split(';').map(str::trim);
        if rest.next() != Some("Adobe-Japan1") {
            continue;
        }
        let Some(cid) = rest
            .next()
            .and_then(|c| c.strip_prefix("CID+"))
            .and_then(|c| c.parse().ok())
        else {
            continue;
        };
        let base = u32::from_str_radix(seq.split_whitespace().next().unwrap(), 16).unwrap();
        ivd.entry(cid).or_insert(base);
    }
    // 互換漢字の正規分解（1 字への分解だけ）。
    let mut decomp: HashMap<u32, u32> = HashMap::new();
    for line in std::fs::read_to_string(ucd_path).unwrap().lines() {
        let f: Vec<&str> = line.split(';').collect();
        if f.len() > 5 && !f[5].is_empty() && !f[5].starts_with('<') && !f[5].contains(' ') {
            decomp.insert(
                u32::from_str_radix(f[0], 16).unwrap(),
                u32::from_str_radix(f[5], 16).unwrap(),
            );
        }
    }

    let text = std::fs::read_to_string(cid2code).unwrap();
    let mut lines = text.lines().filter(|l| !l.starts_with('#'));
    let header: Vec<&str> = lines.next().unwrap().split('\t').collect();
    let col = |name: &str| {
        header
            .iter()
            .position(|h| *h == name)
            .unwrap_or_else(|| panic!("no column {name}"))
    };
    let order = [
        col("UniJIS-UTF32"),
        col("UniJIS2004-UTF32"),
        col("UniJISX0213-UTF32"),
        col("UniJISX02132004-UTF32"),
        col("UniJIS-UCS2-HW"),
        col("UniJIS-UCS2"),
    ];
    let (h_col, jis78_col) = (col("H"), col("78"));
    let rows: Vec<Vec<&str>> = lines.map(|l| l.split('\t').collect()).collect();
    let n = rows.iter().map(|r| r[0].parse::<usize>().unwrap()).max().unwrap() + 1;
    let mut table = vec![0u32; n];
    let mut by_jis: HashMap<&str, usize> = HashMap::new();
    for cells in &rows {
        let cid: usize = cells[0].parse().unwrap();
        if cells[h_col] != "*" {
            for code in cells[h_col].split(',') {
                by_jis.entry(code.trim_end_matches('v')).or_insert(cid);
            }
        }
        'cols: for &c in &order {
            if cells[c] == "*" {
                continue;
            }
            let codes: Vec<(u32, bool)> = cells[c]
                .split(',')
                .map(|e| {
                    (
                        u32::from_str_radix(e.trim_end_matches('v'), 16).unwrap(),
                        e.ends_with('v'),
                    )
                })
                .collect();
            for vertical in [false, true] {
                if let Some((u, _)) = codes.iter().find(|(u, v)| *v == vertical && !avoided(*u)) {
                    table[cid] = *u;
                    break 'cols;
                }
            }
            table[cid] = codes[0].0;
            break;
        }
    }
    for cells in &rows {
        let cid: usize = cells[0].parse().unwrap();
        if table[cid] == 0 && cells[jis78_col] != "*" {
            if let Some(&same) = by_jis.get(cells[jis78_col].split(',').next().unwrap().trim_end_matches('v')) {
                table[cid] = table[same];
            }
        }
        if table[cid] == 0 {
            if let Some(&base) = ivd.get(&cid) {
                table[cid] = base;
            }
        }
        if compat(table[cid]) {
            table[cid] = match ivd.get(&cid) {
                Some(&base) if !compat(base) => base,
                _ => decomp.get(&table[cid]).copied().unwrap_or(table[cid]),
            };
        }
    }
    let mut f = std::fs::File::create(output).unwrap();
    for u in &table {
        f.write_all(&u.to_le_bytes()).unwrap();
    }
    eprintln!("wrote {n} CIDs to {output}");
}

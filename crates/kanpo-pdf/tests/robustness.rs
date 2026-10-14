//! 壊れた PDF でも panic しないこと（WASM では panic がそのまま致命的なエラーになる）。
//!
//! 実官報の PDF を途中で切る・バイトを書き換える・ゴミを渡すなどして、`extract_pages` が
//! エラーか（読めたところまでの）結果を返すことを確かめる。

use std::path::Path;

fn sample() -> Vec<u8> {
    std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../kanpo-parser/tests/corpus/2026-10-05-honshi1801-p2/page.pdf"))
        .unwrap()
}

/// 決まった種から作る疑似乱数（xorshift）。
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

fn extract_without_panic(data: &[u8], label: &str) {
    let r = std::panic::catch_unwind(|| {
        if let Ok(pages) = kanpo_pdf::extract_pages(data) {
            for p in &pages {
                let _ = kanpo_pdf::page_rules(p);
                let _ = kanpo_pdf::bbox::bbox_layout(std::slice::from_ref(p));
            }
        }
    });
    assert!(r.is_ok(), "panicked: {label}");
}

#[test]
fn garbage_input_returns_error() {
    for data in [&b""[..], b"%PDF-1.7", b"not a pdf", b"%PDF-1.7\n1 0 obj << >> endobj\ntrailer << /Root 1 0 R >>"] {
        extract_without_panic(data, &String::from_utf8_lossy(data));
    }
    assert!(kanpo_pdf::extract_pages(b"not a pdf").is_err());
}

#[test]
fn truncated_pdfs_do_not_panic() {
    let pdf = sample();
    let step = pdf.len() / 41;
    for len in (0..pdf.len()).step_by(step.max(1)) {
        extract_without_panic(&pdf[..len], &format!("truncated at {len}"));
    }
}

#[test]
fn corrupted_bytes_do_not_panic() {
    let pdf = sample();
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for round in 0..60 {
        let mut data = pdf.clone();
        let n = 1 + (rng.next() % 8) as usize;
        for _ in 0..n {
            let at = (rng.next() as usize) % data.len();
            data[at] = rng.next() as u8;
        }
        extract_without_panic(&data, &format!("round {round}"));
    }
}

#[test]
fn corrupted_structure_tokens_do_not_panic() {
    // xref・ストリームの長さ・辞書の区切りなど、構造を担う語を狙って壊す。
    let pdf = sample();
    let targets: [&[u8]; 8] = [b"startxref", b"/Length", b"stream", b"endobj", b"obj", b"/W", b"<<", b"/Filter"];
    for t in targets {
        let mut at = 0;
        let mut count = 0;
        while let Some(i) = pdf[at..].windows(t.len()).position(|w| w == t) {
            let i = at + i;
            for replacement in [&b"XXXX"[..], b"-1", b"99999999999999999999"] {
                let mut data = pdf[..i].to_vec();
                data.extend_from_slice(replacement);
                data.extend_from_slice(&pdf[i + t.len()..]);
                extract_without_panic(&data, &format!("{} at {i}", String::from_utf8_lossy(t)));
            }
            at = i + t.len();
            count += 1;
            if count >= 3 {
                break;
            }
        }
    }
}

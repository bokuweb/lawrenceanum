//! `llama` 機能のときだけ、手元の llama.cpp（共有ライブラリ）に対して `csrc/llama_scorer.cpp` をビルドする。
//!
//! llama.cpp の場所は `LLAMA_CPP_PREFIX`（`include/llama.h` と `lib/libllama.*` のある所）、無ければ
//! `brew --prefix llama.cpp`。ggml のヘッダが別の所（Homebrew の ggml）にあれば `GGML_PREFIX` か
//! `brew --prefix ggml` も見る。

use std::path::PathBuf;
use std::process::Command;

fn brew_prefix(formula: &str) -> Option<PathBuf> {
    let out = Command::new("brew").args(["--prefix", formula]).output().ok()?;
    out.status.success().then(|| PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()))
}

fn main() {
    println!("cargo:rerun-if-changed=csrc/llama_scorer.cpp");
    println!("cargo:rerun-if-env-changed=LLAMA_CPP_PREFIX");
    println!("cargo:rerun-if-env-changed=GGML_PREFIX");
    if std::env::var_os("CARGO_FEATURE_LLAMA").is_none() {
        return;
    }
    let llama = std::env::var_os("LLAMA_CPP_PREFIX")
        .map(PathBuf::from)
        .or_else(|| brew_prefix("llama.cpp"))
        .expect("llama.cpp が見つからない（LLAMA_CPP_PREFIX を設定するか brew install llama.cpp）");
    let ggml = std::env::var_os("GGML_PREFIX").map(PathBuf::from).or_else(|| brew_prefix("ggml"));
    let mut build = cc::Build::new();
    build.cpp(true).std("c++17").file("csrc/llama_scorer.cpp").include(llama.join("include"));
    if let Some(ggml) = &ggml {
        build.include(ggml.join("include"));
    }
    build.compile("kanpo_lm_llama");
    let lib = llama.join("lib");
    println!("cargo:rustc-link-search=native={}", lib.display());
    println!("cargo:rustc-link-lib=dylib=llama");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib.display());
}

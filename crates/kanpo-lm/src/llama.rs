//! llama.cpp で GGUF のモデル（Gemma 4 E2B など）を動かす [`Scorer`]。
//!
//! 手元の llama.cpp の共有ライブラリにリンクする（`build.rs`）。モデルは GPU（Metal など）に全層載せる。

use crate::{Scorer, Token};
use std::ffi::{c_char, c_int, c_void, CString};

#[repr(C)]
struct KlModel {
    _private: [u8; 0],
}

type Sink = extern "C" fn(user: *mut c_void, text: usize, start: usize, end: usize, nll: f32);

extern "C" {
    fn kl_open(path: *const c_char, n_ctx: c_int, n_seq: c_int, n_gpu_layers: c_int) -> *mut KlModel;
    fn kl_close(m: *mut KlModel);
    fn kl_score(m: *mut KlModel, texts: *const *const c_char, lens: *const usize, n: usize, sink: Sink, user: *mut c_void) -> c_int;
}

/// llama.cpp のモデル。
pub struct LlamaScorer {
    model: *mut KlModel,
}

// llama.cpp の文脈は 1 つのスレッドからしか使わない（&mut self でしか触らない）。
unsafe impl Send for LlamaScorer {}

impl LlamaScorer {
    /// GGUF のモデルを読む。`n_ctx` は 1 回にまとめて流すトークン数（1 文は `n_ctx / 4` まで）、`n_seq` は 1 回に
    /// まとめる文の数。
    pub fn open(path: &std::path::Path, n_ctx: usize, n_seq: usize) -> anyhow::Result<Self> {
        let c = CString::new(path.to_string_lossy().as_bytes())?;
        let model = unsafe { kl_open(c.as_ptr(), n_ctx as c_int, n_seq as c_int, 999) };
        anyhow::ensure!(!model.is_null(), "モデルを読めない: {}", path.display());
        Ok(LlamaScorer { model })
    }
}

impl Drop for LlamaScorer {
    fn drop(&mut self) {
        unsafe { kl_close(self.model) }
    }
}

extern "C" fn collect(user: *mut c_void, text: usize, start: usize, end: usize, nll: f32) {
    let out = unsafe { &mut *(user as *mut Vec<Vec<Token>>) };
    out[text].push(Token { start, end, nll });
}

impl Scorer for LlamaScorer {
    fn score(&mut self, texts: &[&str]) -> anyhow::Result<Vec<Vec<Token>>> {
        let mut out: Vec<Vec<Token>> = vec![Vec::new(); texts.len()];
        let ptrs: Vec<*const c_char> = texts.iter().map(|t| t.as_ptr() as *const c_char).collect();
        let lens: Vec<usize> = texts.iter().map(|t| t.len()).collect();
        let rc = unsafe {
            kl_score(self.model, ptrs.as_ptr(), lens.as_ptr(), texts.len(), collect, &mut out as *mut Vec<Vec<Token>> as *mut c_void)
        };
        anyhow::ensure!(rc == 0, "llama.cpp の decode に失敗した");
        Ok(out)
    }
}

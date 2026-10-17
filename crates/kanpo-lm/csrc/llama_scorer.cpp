// llama.cpp で文のトークンごとの負の対数尤度を出す薄いラッパー（Rust から extern "C" で呼ぶ）。
// llama.h の構造体を Rust 側で定義しないため（llama.cpp の版で構造体の並びが変わる）。
#include "llama.h"

#include <cmath>
#include <cstddef>
#include <string>
#include <vector>

extern "C" {

struct kl_model {
    llama_model * model;
    llama_context * ctx;
    const llama_vocab * vocab;
    int n_ctx;
    int n_seq;
};

// トークンごとに呼ぶ。text は texts の添字、[start, end) は文の中のバイトの範囲。
typedef void (*kl_sink)(void * user, size_t text, size_t start, size_t end, float nll);

kl_model * kl_open(const char * path, int n_ctx, int n_seq, int n_gpu_layers) {
    llama_log_set([](ggml_log_level, const char *, void *) {}, nullptr);
    llama_backend_init();
    auto mparams = llama_model_default_params();
    mparams.n_gpu_layers = n_gpu_layers;
    llama_model * model = llama_model_load_from_file(path, mparams);
    if (!model) return nullptr;
    auto cparams = llama_context_default_params();
    cparams.n_ctx = n_ctx;
    cparams.n_batch = n_ctx;
    cparams.n_ubatch = n_ctx;
    cparams.n_seq_max = n_seq;
    cparams.kv_unified = true;
    llama_context * ctx = llama_init_from_model(model, cparams);
    if (!ctx) {
        llama_model_free(model);
        return nullptr;
    }
    return new kl_model{model, ctx, llama_model_get_vocab(model), n_ctx, n_seq};
}

void kl_close(kl_model * m) {
    if (!m) return;
    llama_free(m->ctx);
    llama_model_free(m->model);
    delete m;
}

// 文の先頭（BOS の後ろ）から 1 トークンずつ、前のトークンまでを見たときの確率を出す。文は
// n_ctx / 4 トークンまで（それより後ろは採点しない）。成功すれば 0。
int kl_score(kl_model * m, const char * const * texts, const size_t * lens, size_t n, kl_sink sink, void * user) {
    struct Item {
        size_t index;
        std::vector<llama_token> toks;  // 先頭は BOS
        std::vector<size_t> ends;       // 各トークン（BOS の後ろ）の終わりのバイト位置
    };
    const size_t max_tokens = m->n_ctx / 4;
    std::vector<Item> items;
    for (size_t k = 0; k < n; k++) {
        std::string s(texts[k], lens[k]);
        std::vector<llama_token> t(s.size() + 8);
        int c = llama_tokenize(m->vocab, s.c_str(), s.size(), t.data(), t.size(), true, false);
        if (c < 0) {
            t.resize(-c);
            c = llama_tokenize(m->vocab, s.c_str(), s.size(), t.data(), t.size(), true, false);
        }
        t.resize(c > 0 ? c : 0);
        if (t.size() > max_tokens) t.resize(max_tokens);
        Item it{k, t, {}};
        size_t at = 0;
        for (size_t i = 1; i < t.size(); i++) {
            char buf[256];
            int len = llama_token_to_piece(m->vocab, t[i], buf, sizeof buf, 0, false);
            at += len > 0 ? len : 0;
            it.ends.push_back(at < s.size() ? at : s.size());
        }
        items.push_back(std::move(it));
    }
    const int n_vocab = llama_vocab_n_tokens(m->vocab);
    size_t next = 0;
    while (next < items.size()) {
        // 文脈の大きさと列の数に収まるだけまとめて 1 回で流す。
        size_t used = 0, first = next;
        while (next < items.size() && (int) (next - first) < m->n_seq && used + items[next].toks.size() <= (size_t) m->n_ctx) {
            used += items[next].toks.size();
            next++;
        }
        if (next == first) next++;  // 収まらない文は飛ばす（max_tokens で切ってあるので起きない）
        llama_memory_clear(llama_get_memory(m->ctx), true);
        llama_batch batch = llama_batch_init(used, 0, 1);
        std::vector<size_t> start;
        size_t k = 0;
        for (size_t s = first; s < next; s++) {
            start.push_back(k);
            auto & toks = items[s].toks;
            for (size_t i = 0; i < toks.size(); i++, k++) {
                batch.token[k] = toks[i];
                batch.pos[k] = i;
                batch.n_seq_id[k] = 1;
                batch.seq_id[k][0] = s - first;
                batch.logits[k] = i + 1 < toks.size();
            }
        }
        batch.n_tokens = k;
        if (llama_decode(m->ctx, batch) != 0) {
            llama_batch_free(batch);
            return 1;
        }
        for (size_t s = first; s < next; s++) {
            auto & it = items[s];
            size_t prev = 0;
            for (size_t i = 1; i < it.toks.size(); i++) {
                const float * logits = llama_get_logits_ith(m->ctx, start[s - first] + i - 1);
                double mx = -1e30;
                for (int v = 0; v < n_vocab; v++) mx = std::max(mx, (double) logits[v]);
                double z = 0;
                for (int v = 0; v < n_vocab; v++) z += std::exp(logits[v] - mx);
                double nll = -(logits[it.toks[i]] - mx - std::log(z));
                sink(user, it.index, prev, it.ends[i - 1], (float) nll);
                prev = it.ends[i - 1];
            }
        }
        llama_batch_free(batch);
    }
    return 0;
}

}  // extern "C"

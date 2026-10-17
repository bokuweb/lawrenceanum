# kanpo-lm

[kanpo-parser](../kanpo-parser/) が組んだ官報の本文を、言語モデルで後から見直す後工程の crate。
パーサの中には組み込まず、パーサの出力（行のテキスト）だけを受け取る。

- `scan`: 前の行を文脈にして各行を採点し（1 字あたりの負の対数尤度）、読み順の崩れの候補を並べる
- `correct`: 怪しい行の書き直しの候補（縦中横の単位の置き直し、欄の空白で区切った片の並べ替え）を作り、
  元の行より十分に自然なものだけを補正として返す（字を足したり消したりはしない）

## 言語モデルで分かること・分からないこと

試作で確かめた範囲（Gemma 4 E2B の Q8_0、蓄積した官報の本文）:

| 使い方 | 結果 |
|---|---|
| 字を入れ替えた行と元の行を比べる（語の移動・他の行の断片の混入・行の前後の入れ替え、計 599 組） | 崩した方を低く見るのが 96〜99% |
| パーサを直す前後の行（縦中横の単位の位置、8 組） | 8 組とも直した後を高く見る |
| 1 行だけを単独で見て崩れかを決める | AUC 0.76〜0.81（表・一覧・列の途中で切れた縦書きの行も読みにくく見える） |
| ページを 600 字ほどのまとまりで文脈付きで採点し、怪しいまとまりを上から見る | 上位 30 のうち 11 が本当の読み順の崩れ（横組みのページの中の縦組みの公告の列が逆順につながる崩れを見つけた） |

比べるのは得意で、単独で決めるのは苦手なので、`scan` の結果は人が見る順番を決めるため、`correct` は書き直しの
候補どうしの比較だけに使う。ModernBERT-Ja-30m（穴埋めの擬似対数尤度）でも、比べる精度は同じくらいだった
（語の移動 99.0%・断片の混入 98.5%・行の入れ替え 95.0%）。

## 補正の確かめ方と結果

`check` を、パーサを直す前（縦中横の単位の位置が崩れていた版）の 4 ページと、今の出力から無作為に選んだ 40 ページに
掛けた（既定の設定: 見直す行は文脈付きで 1 字 2.3 nat 以上、書き直しを採るのは 8 nat 以上の改善）。

- 直す前のページ: 5 行を補正し、5 行とも正しい行に戻った（「四三三・六七　を超え四三四・一七MHz　以下…MHz」→
  「四三三・六七MHzを超え四三四・一七MHz以下…」など。改善は 24〜46 nat）。上付きの字が別の行になった行（「10　＋15）Hz」）は
  字が足りないので直せない
- 今の出力の 40 ページ: 補正は 0 行。改善の下限を 0 にしても候補が勝ったのは 11 行で、最大 5.5 nat（表の行のセルの
  入れ替え「代表取締役　愛知県豊橋市…」など、どちらの並びもあり得るもの）

## 使い方

モデルは [`Scorer`](src/lib.rs) で差し替える。`llama` 機能は手元の llama.cpp の共有ライブラリにリンクして
GGUF のモデルを動かす（`LLAMA_CPP_PREFIX`、無ければ `brew --prefix llama.cpp`）。

```bash
brew install llama.cpp
# モデル（4.97GB）
curl -L -o ~/.cache/kanpo-lm/gemma-4-E2B-Q8_0.gguf \
  https://huggingface.co/ggml-org/gemma-4-E2B-GGUF/resolve/main/gemma-4-E2B-Q8_0.gguf

# パーサの出力を書き出して（kanpo-parser の README の textdump）、補正の TSV を出す
cargo run --release -p kanpo-parser --example textdump -- .cache/kanpo-archive /tmp/after
cargo run --release -p kanpo-lm --features llama --example check -- ~/.cache/kanpo-lm/gemma-4-E2B-Q8_0.gguf /tmp/after
# 怪しい行を上から 100 行
cargo run --release -p kanpo-lm --features llama --example check -- --scan 100 ~/.cache/kanpo-lm/gemma-4-E2B-Q8_0.gguf /tmp/after
```

Apple M 系の GPU（Metal）で 1 秒に 300 字ほど。毎日の新しい号なら数分、蓄積した全ページ（約 1 万ページ）は
半日かかる。パーサを直したときの前後の比較（変わった行だけを採点して、どちらが自然か）にも使える。

### ModernBERT（`mlm` 機能）

[sbintuitions/modernbert-ja-30m](https://huggingface.co/sbintuitions/modernbert-ja-30m)（約 155MB）を candle で CPU で動かす
（llama.cpp も GPU も要らず、WASM にも載る）。各トークンを隠して前後の両側の文脈から当てる擬似対数尤度で採点する。
16 トークンおきの位置をまとめて隠す（`MlmScorer::stride`）。

```bash
mkdir -p ~/.cache/kanpo-lm/modernbert-ja-30m && cd ~/.cache/kanpo-lm/modernbert-ja-30m
for f in config.json tokenizer.json model.safetensors; do
  curl -LO https://huggingface.co/sbintuitions/modernbert-ja-30m/resolve/main/$f
done
cd -
# モデルのディレクトリを渡すと ModernBERT 向けの設定（Options::modernbert）で動く
cargo run --release -p kanpo-lm --features mlm --example check -- ~/.cache/kanpo-lm/modernbert-ja-30m /tmp/after
```

- 書き直しの候補どうしの比較は Gemma と同じくらい当たる（字を入れ替えた 599 組で 99.0%・98.0%・94.5%）
- パーサを直す前の 4 ページ: 7 行を補正し、5 行は正しい行に戻った（Gemma と同じ 5 行。改善は 33〜64 nat で Gemma より大きく
  出る）。残る 2 行も単位は数の直後に入ったが、空白や語の中に入り込んだ単位（「を超GHzえ」）が残った
- 今の出力の 40 ページ: 補正は 0 行（単位の候補が勝ったのは 1 行で 0.2 nat）。表の行の片の並べ替えは、同じ字の並びが
  隣り合う方を好んで誤る（「資産合計　13,295,943　負債・純資産合計　13,295,943」の金額を後ろにまとめる方が 38.8 nat 勝つ）ので、
  ModernBERT では試さない（`Options::modernbert`）
- 怪しい行の洗い出し（`--scan`）は Gemma より弱い。前後両側を見るので、近くの行に同じ文があると崩れた行もそこから
  当てられて読みやすく見える

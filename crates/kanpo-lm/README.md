# kanpo-lm

[kanpo-parser](../kanpo-parser/) が組んだ官報の本文を、ModernBERT（穴埋めのモデル）で後から見直す後工程の crate。
パーサの中には組み込まず、パーサの出力（行のテキスト）だけを受け取る。

- `scan`: まとまりの中の前後の行を文脈にして各行を採点し（1 字あたりの負の対数尤度）、読み順の崩れの候補を並べる
- `correct`: 怪しい行の書き直しの候補（縦中横の単位の置き直し）を作り、元の行より十分に自然なものだけを補正として
  返す（字を足したり消したりはしない）

モデルは [sbintuitions/modernbert-ja-30m](https://huggingface.co/sbintuitions/modernbert-ja-30m)（約 155MB）。candle で CPU で
動かす（GPU も外部のライブラリも要らず、WASM にも載る）。各トークンを隠して前後の両側の文脈から当てる擬似対数尤度で
採点し、16 トークンおきの位置をまとめて隠す（`MlmScorer::stride`）。採点器は `Scorer` トレイトで差し替えられる。

## 言語モデルで分かること・分からないこと

| 使い方 | 結果 |
|---|---|
| 字を入れ替えた行と元の行を比べる（語の移動・他の行の断片の混入・行の前後の入れ替え、計 599 組） | 崩した方を低く見るのが 99.0%・98.0%・94.5%（1 つずつ隠す採点とほぼ同じ） |
| 1 行だけを単独で見て崩れかを決める | AUC 0.75〜0.82（表・一覧・列の途中で切れた縦書きの行も読みにくく見える） |

比べるのは得意で、単独で決めるのは苦手なので、`scan` の結果は人が見る順番を決めるため、`correct` は書き直しの
候補どうしの比較だけに使う。前後両側を見るので、近くの行に同じ文があると崩れた行もそこから当てられて読みやすく
見える。表の行の片を並べ替える候補は、同じ字の並びが隣り合う方を好んで誤る（「資産合計　13,295,943　負債・純資産合計
　13,295,943」の金額を後ろにまとめる方が 38.8 nat 勝つ）ので、既定では試さない（`Options::segments`）。

試作では Gemma 4 E2B（llama.cpp、4.97GB）とも比べた。候補どうしの比較の精度は同じくらいで、怪しい所の洗い出しは
Gemma の方が少し強かったが、モデルの大きさと動かす手間に見合わないので使っていない。

## 補正の確かめ方と結果

`check` を、パーサを直す前（縦中横の単位の位置が崩れていた版）の 4 ページと、今の出力から無作為に選んだ 40 ページに
掛けた（既定の設定: 見直す行は文脈付きで 1 字 0.5 nat 以上、書き直しを採るのは 25 nat 以上の改善）。

- 直す前のページ: 7 行を補正し、5 行は正しい行に戻った（「四三三・六七　を超え四三四・一七MHz　以下…MHz」→
  「四三三・六七MHzを超え四三四・一七MHz以下…」など。改善は 33〜64 nat）。残る 2 行も単位は数の直後に入ったが、空白や
  語の中に入り込んだ単位（「を超GHzえ」）が残った。上付きの字が別の行になった行（「10　＋15）Hz」）は字が足りないので
  直せない
- 今の出力の 40 ページ: 補正は 0 行（単位の候補が勝ったのは 1 行で 0.2 nat）

## 使い方

```bash
mkdir -p ~/.cache/kanpo-lm/modernbert-ja-30m && cd ~/.cache/kanpo-lm/modernbert-ja-30m
for f in config.json tokenizer.json model.safetensors; do
  curl -LO https://huggingface.co/sbintuitions/modernbert-ja-30m/resolve/main/$f
done
cd -

# パーサの出力を書き出して（kanpo-parser の README の textdump）、補正の TSV を出す
cargo run --release -p kanpo-parser --example textdump -- .cache/kanpo-archive /tmp/after
cargo run --release -p kanpo-lm --example check -- ~/.cache/kanpo-lm/modernbert-ja-30m /tmp/after
# 怪しい行を上から 100 行
cargo run --release -p kanpo-lm --example check -- --scan 100 ~/.cache/kanpo-lm/modernbert-ja-30m /tmp/after
```

パーサを直したときの前後の比較（変わった行だけを採点して、どちらが自然か）にも使える。

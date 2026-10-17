# 切れ目の分類器の学習

ModernBERT-Ja-30m を、トークンごとに「直前から本文の流れが途切れているか」を当てる分類器に fine-tune する
（kanpo-lm の `tagger`）。Python（PyTorch・transformers・scikit-learn）が要る。

## 手順

```bash
# 0. 元のモデル（約 155MB）
mkdir -p ~/.cache/kanpo-lm/modernbert-ja-30m && cd ~/.cache/kanpo-lm/modernbert-ja-30m
for f in config.json tokenizer.json model.safetensors; do
  curl -LO https://huggingface.co/sbintuitions/modernbert-ja-30m/resolve/main/$f
done
cd -

# 1. 正しい本文: e-Gov の法令の全文（kanpo-parser の egov_rows / egov_check のキャッシュ .cache/egov-check）
python crates/kanpo-lm/train/extract_laws.py .cache/egov-check /tmp/ft/laws.jsonl
#    と、蓄積した官報の今の本文（評価に使うページは --exclude で除く）
cargo run --release -p kanpo-parser --example textdump -- .cache/kanpo-archive /tmp/after

# 2. 崩しを入れた学習データ（train.jsonl・dev.jsonl。dev は法令・ページ単位で分ける）
python crates/kanpo-lm/train/gen_data.py --laws /tmp/ft/laws.jsonl --kanpo /tmp/after --out /tmp/ft/data

# 3. 学習（Apple M 系の GPU で 70 分ほど）。dev の切れ目のトークンの平均適合率がいちばん高い重みを残す
python crates/kanpo-lm/train/train.py --model ~/.cache/kanpo-lm/modernbert-ja-30m --data /tmp/ft/data --out ~/.cache/kanpo-lm/breaks-30m

# 3'. 小さくする（埋め込み表だけ int8 にして 69MB。kanpo-lm はどちらの重みも読める）
python crates/kanpo-lm/train/quantize.py --embeddings-only ~/.cache/kanpo-lm/breaks-30m ~/.cache/kanpo-lm/breaks-30m-emb8

# 4. 補正の確かめ（1 行だけを崩したページと崩さないページ）
python crates/kanpo-lm/train/make_repair_eval.py make /tmp/ft/data/dev.jsonl /tmp/ft/repair --n 200
cargo run --release -p kanpo-lm --example check -- --tagger ~/.cache/kanpo-lm/breaks-30m ~/.cache/kanpo-lm/modernbert-ja-30m /tmp/ft/repair > /tmp/ft/fixes.tsv
python crates/kanpo-lm/train/make_repair_eval.py score /tmp/ft/repair /tmp/ft/fixes.tsv
```

## 補助のスクリプト

- `scan.py`: 学習した分類器で、本文のテキストの各行の切れ目の確率を GPU でまとめて出す（examples/breaks と同じ TSV。蓄積した官報の
  全ページ約 70 万行が 7 分。examples/breaks は CPU で 1 秒に 1 ページほど）。パーサを直す前後の本文を両方採点して、崩れと見る行が
  減ったかでパーサの修正を確かめるのに使う
- `evaluate.py`: 学習した分類器を、gen_data.py の形の評価用データで採点する
- `real_pairs.py`: パーサを直す前と後の本文の組（字の集合が同じページ）から、直す前の各字を直した後の位置に突き合わせて本物の崩れの
  例を作る（数字の多いまとまりは突き合わせが当てにならないので使わない）。`gen_data.py --extra` で学習に混ぜられる

## データの作り方（`gen_data.py`）

- 法令の段落は、条名・項番号・号の名称と本文の間を全角の空白にし、縦書きの列のように 18〜45 字で折り返す（句読点・括弧閉じは
  前の行にぶら下げる）。6 割の法令は全角の英数字を半角にする（官報の出力は「MHz」「10」が半角のことが多い）
- 官報の本文は 4 割のページだけ使う（今の出力にも崩れが残っていて、それを正しい本文として教えてしまうので）
- 1 例は 180〜360 字。3 割は崩さず、残りは 1〜3 回崩す（単位を含む例は半分を単位のずれにする）
- 崩し方: `reverse_merge`（2〜8 行を逆順に 1 行につなぐ）・`swap_lines`・`fragment_move`（2〜8 字を近くの行へ）・`word_move`
  （1〜4 字を 2〜8 字ずらす）・`rotate`・`unit_shift`（単位を右へずらし、元の位置に空白を残す）・`detach`（1〜3 字を別の行に）
- ラベル: 各字に元の本文の中の位置（空白・改行を除いて数えたもの）を持たせ、直前の字の位置の次でない字を切れ目とする

## 本物の崩れと官報の除外を足した学習（v2、使っていない）

v1 から続けて、本物の崩れの例（#160・#163 の直す前と後の組から 1,587 例、評価に使う 100 ページは除く）を 2 回ずつ混ぜ、今の分類器が
確率 0.9 以上と見る官報の行の前後 3 行を正しい本文から除いて（`--kanpo-flags`）1 周学習した。縦中横の単位のずれの行の取りこぼしは
減った（いちばん低い行の確率 0.59 → 0.94）が、直した後の行・今の出力の行を切れ目と見る割合が増えた（確率 0.5 以上で 0.7% → 1.3%、
今の出力の 40 ページで 12 → 22 行）。除いた官報の行に表の行（欄を「　」でつないだ行。もともと切れ目と見られやすい）が多く、表の行を
正しい本文として学ぶ例が減ったためと見て、v1 を使う。

## 結果（2026-10-17）

学習 75,755 例（法令 57,549・官報 18,206）、2 周、バッチ 32、学習率 5e-5、切れ目の重み 3。dev 2,551 例で、切れ目のトークンの
平均適合率 0.944（確率 0.5 で適合率 0.87・再現率 0.90）、例に崩れがあるかの AUC 0.991。本物の崩れでの結果は
kanpo-lm の README。

kanpo-parser のコーパス（`crates/kanpo-parser/tests/corpus/<case>/page*.pdf`）を poppler に通した出力（kanpo-pdf に切り替える前にコーパスに置いていたもの）。
kanpo-pdf の出力を poppler と突き合わせるテスト（`tests/pdf.rs`）が使う。

- `<case>.page.bbox.html` — `pdftotext -bbox-layout -enc UTF-8 page.pdf`
- `<case>.page.rules.json` — `pdftocairo -svg page.pdf` の 2 点の直線パスを縦線・横線に分けたもの

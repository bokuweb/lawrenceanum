# spaced-materials263w

`split.py OLD OUT FORMAT` は、記事の途中か末尾の行に字間を空けた区分見出し「資　料」を持つ記事を、その行で2記事に分ける。本文はその行で、Document はその見出しを含む1つの Run で分ける（見出しの直前の改行は前の記事の末尾なので落とす）。分けた後ろの記事の書式は `FORMAT` にする（その記事自身の本文で決まる。「資料」の報告は改正後・改正前の欄見出しも改め文の句も持たないので `unknown`）。頁・全文・前の記事の書式・ほかの記事は写す。

| 新しい期待値 | 旧期待値 | FORMAT |
| --- | --- | --- |
| `native-source-paragraphs/baseline-three183-materials263w.json` | `native-source-paragraphs/baseline-three183.json` | unknown |

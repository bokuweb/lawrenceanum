# company-section263v

`split.py OLD OUT` は、記事の途中か末尾の行に欄見出し「会社その他の公告」を持つ記事を、その行で2記事に分ける。本文はその行で、Document はその見出しを含む1つの Run で分ける（見出しの直前の改行は前の記事の末尾なので落とす）。`OLD` は Analysis 全体（`articles` を持つ）でも、記事の分け方の一覧（`expected-native-article-partitions`）でもよい。頁・全文・書式・ほかの記事は写す。

native263v の期待値は、次の旧期待値をこのスクリプトで分けたものだけでできている。

| 新しい期待値 | 旧期待値 |
| --- | --- |
| `native-signed-announcements/expected/20261008g002220077-company263v.json` | `native-signed-announcements/expected/20261008g002220077.json` |
| `native-unresolved-identity167/20260902h017810031-company263v.json` | `native-unresolved-identity167/20260902h017810031.json` |
| `source-comparison-labels/20260805g001760106-mixed-baseline146-company263v.json` | `source-comparison-labels/20260805g001760106-mixed-baseline146.json` |
| `../expected-native-article-partitions/20260716g001590048-company263v.json` | `../expected-native-article-partitions/20260716g001590048.json` |
| `../expected-native-article-partitions/20260729h017570029-company263v.json` | `../expected/identity_surnames/20260729h017570029-before44.json` の `articles`（`text`・`document` だけ） |

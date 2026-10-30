# agency-titles263u

`split.py OLD TITLE OUT` は、記事の途中の行に機関名だけの公示の標題（`TITLE`）を持つ記事を、その行で2記事に分ける。本文はその行で、Document はその標題を含む1つの Run で分ける。頁・全文・書式・ほかの記事は写す。

native263u の期待値は、次の旧期待値をこのスクリプトで分けたものだけでできている。

| 新しい期待値 | 旧期待値 | 標題 |
| --- | --- | --- |
| `native-section-roots/expected126-agency263u.json` | `native-section-roots/expected126.json` | 関東地方整備局公示 |
| `native-section-roots/baseline125-agency263u.json` | `native-section-roots/baseline125.json` | 関東地方整備局公示 |
| `section-context/20260730h017580006-baseline124-agency263u.json` | `section-context/20260730h017580006-baseline124.json` | 北海道開発局公示 |
| `section-context/20260730h017580006-no-rules-baseline124-agency263u.json` | `section-context/20260730h017580006-no-rules-baseline124.json` | 北海道開発局公示 |

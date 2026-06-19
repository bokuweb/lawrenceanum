# 検討メモ: 地方議会会議録 / 自治体例規 の収集対象化

> 法令・官報・パブコメ・国会会議録に続く収集対象として「①地方議会会議録」「②自治体例規」を
> 検討する。2026-06-19 の調査に基づく初期設計メモ（実装前の意思決定用）。
> 関連: [`reiki-plan.md`](reiki-plan.md) / [`public-corpus-roadmap.md`](public-corpus-roadmap.md) /
> 既存実装: [`reiki-client`](../crates/reiki-client/src/lib.rs) / [`shingikai-client`](../crates/shingikai-client/src/lib.rs)

---

## 0. 結論

- **② 例規 = 実現可能性「中〜高」。先に着手すべき（ROI高）。** ベンダー2社（ぎょうせい 61.6% + 第一法規 31.1% ≒ **92%超**）の寡占でURL/HTML構造が高度に共通。**条例・規則本文は著作権法13条で著作権が発生しない**ため再配布の法的障壁が低い。既存 `reiki-client` の路線（ぎょうせい型アダプタ）は正しく、土台に使える。
- **① 地方議会会議録 = 見送り（2026-06-19 判断）。** 商用静的サイトに載せられる「再配布可ライセンス明示」の機械可読ソースが現状存在せず（本文は著作権が議会に及ぶとの見解が一般的＝技術でなく権利の問題）、堀になりにくいため**一旦スコープから外す**。再開はライセンス取得(C)・法務確認が前提。下記「① ライセンス確定結果」「収集戦略」に調査と再開条件を記録。

---

## ① 地方議会会議録

### ① ライセンス確定結果（2026-06-19・保留）
- **local-politics.jp（政治のことば, KAKENHI 20K00576）の会議録データ = 商用再配布不可（要許諾・オープンライセンス無し）。**
  本体コーパスは「限られた条件下で研究者向け配布・要利用申込」、東京都議会データセットは「NTCIR事務局との覚書(MOU)締結が前提」。CC BY等の明示なし。HuggingFaceの`local-politics-jp/bert-*`はCC-BY-SA-4.0だが**学習済みモデルのライセンスであり原データの再配布許諾ではない**。GSK配布も該当なし。
  出典: [local-politics-BERT](http://local-politics.jp/公開物/local-politics-bert/) / [HF local-politics-jp](https://huggingface.co/local-politics-jp/bert-base-japanese-minutes-wikipedia-further) / [GSKカタログ](https://www.gsk.or.jp/catalog/)
- **代替も無し**: NDLは国会会議録のみ（地方議会本文の機械可読提供なし。[NDL API](https://www.ndl.go.jp/jp/use/api/index.html)）。各議会検索システムは robots.txt が `Disallow: /`（例: 東京都議会会議録検索）でスクレイピング不可。chiholog/yonalog（Bitlet）は公開API・再配布許諾なし。
- **判断**: 地方議会録は**スコープ保留**。どうしても進める場合は非自動・要交渉の2択（(a) local-politics.jp に商用利用可否を直接照会 kimura@res.otaru-uc.ac.jp / (b) Bitlet に商用データ提供を打診 contact@bitlet.co）。コードでの自動収集は現状しない。

### 提供基盤（寡占・統一APIなし）
- 会議録検索システムは少数ベンダー寡占。販売=会議録研究所、システム=NTT-AT系。
  **DiscussNetPremium**（クラウド型）≈ [477自治体](https://www.ntt-at.co.jp/product/discussnetpremium/)、DiscussVisionシリーズ≈234自治体。
- **公式の統一APIは存在しない**（公開方法・形式が自治体ごとに異なる）。出典: [地方議会会議録コーパスプロジェクト](http://local-politics.jp/)
- 既存民間: Bitlet [chiholog](https://chiholog.net/chiholog)/yonalog（横断検索）。学術: [local-politics.jp](http://local-politics.jp/) が文単位＋メタ（自治体コード/年月日/会議名/発言者）で構造化配布（KAKENHI 20K00576）。

### 収集戦略（A→B→C。全文の自前再配布は段階的に）
local-politics.jp が使えない以上、「全文を自前ホストするか/しないか」で分けて段階展開する。
- **A. メタ＋リンク＋引用（推奨MVP・最も堅い）**: 全文を再配布せず、事実メタ（議会名・日付・委員会・議題・発言者）を索引し、**公式ソースへ深リンク**＋必要時のみ引用(著作権法32条)。横断検索・アラートの価値は出せて再配布リスクを負わない。
- **B. 再利用可ライセンス明示分だけ全文収集**: 政府標準利用規約2.0(→2024-07 公共データ利用規約1.0, CC BY 4.0相当)/CC BY を採用する自治体のデータのみ正規取込。
- **C. 商用ライセンス取得（全文横断の本命）**: local-politics.jp 商用照会 / Bitlet(chiholog) 購入・提携。有料プロダクトなら data ライセンス費は事業コストとして妥当。
- 参考: **ポリミル(Polimill/QommonsAI)は「公開オープンデータ＋公的API＋自治体が自前文書を投入するRAG」型**で、網羅スクレイピングではなく「公開データ＋顧客投入データ」の積み上げ。我々のA/Cと整合。出典: [QommonsAI解説](https://www.nvv.genai.co.jp/2026/05/polimill/) / [データ出所](https://polimill.jp/2024031301-2/)

### 着手リスト（2026-06-19 調査・確実な再利用可を優先）
**総括: 会議録「本文」をCC BY等で商用再配布可と明示する自治体はほぼ皆無。確実に可なのはメタデータ/議会だより(CC BY)のみ。本文は robots 許可的な `ssp.kaigiroku.net/tenant/`(DiscussNet, 477自治体) に限り技術的に可だが、ライセンスは各議会で要確認。→ A方式が現実的。**

| 区分 | 自治体/基盤 | 対象 | 形式 | 利用規約 | 商用再配布 |
|---|---|---|---|---|---|
| 即可(B) | 横浜市 | 議会だより(質問要旨/賛否一覧)※本文でない | txt/CSV | **CC BY 4.0** | 可 |
| 即可(B) | 目黒区/BODIK掲載~30自治体 | 議決件数・議会統計※メタ | CSV | **CC BY 4.0** | 可 |
| 要確認(C/個別) | 神奈川県議会・大阪府議会・横浜市会 等 `ssp.kaigiroku.net/tenant/` | 会議録**本文** | HTML | サイト規約参照(要確認) | robots許可的・規約は要確認 |
| 不可寄り | 東京都議会 / `*.dbsr.jp`(京都府/福岡県) | 本文 | HTML | 「無断複製・転用不可」/robots制限 | 不可寄り |
| 不可 | 福岡市議会 / `*.gijiroku.com`(AIボット名指し拒否) | 本文 | HTML | robots `Disallow:/` 等 | 不可 |

出典: [横浜市 議会だよりCC BY](https://www.city.yokohama.lg.jp/shikai/koho/yokohama/dayori.html) / [BODIK 議会CSV](https://data.bodik.jp/dataset?q=議会&res_format=CSV) / [ssp.kaigiroku.net](https://ssp.kaigiroku.net/) / [東京都議会 著作権](https://www.gikai.metro.tokyo.lg.jp/about/copyright.html) / [デジタル庁 公共データ利用規約](https://www.digital.go.jp/)

### 着手順（会議録）
1. **A方式MVP**: まず横浜市/BODIKの**CC BYメタ・議会だより**を取り込み（即・合法）＋議会公式ページへ深リンク。`shingikai-client`の`MinistryAdapter`を`AssemblyAdapter`化して土台に。
2. 本文横断が要る段で、**`ssp.kaigiroku.net/tenant/` 系に絞り**各議会事務局/オープンデータ規約で**商用許諾を個別確認**(C)。
3. 全1,700網羅は初期スコープ外（分散メンテ破綻）。

### リスク・法的留意
- **著作権法40条**: 公開の政治上の演説等は利用可だが、**質疑応答・答弁には適用なしとの見解が有力**。会議録は編集著作物として議会に著作権帰属との見解が一般的 → **本文の商用全文再配布を著作権法のみで合法と断定できない**。B/C/Dで全文ホストする前に**法務確認推奨**（条例＝13条で確実に著作権なし＝例規が先行できる理由）。
- 網羅性の分散（高）・メンテ負荷（高）。

---

## ② 自治体例規

### 提供基盤（強い寡占・構造共通）
- ぎょうせい **1,054自治体(61.6%)** Reiki-Base（`g-reiki.net`）／第一法規 **533(31.1%)**（`d1-law.com`）／クレステック 116(6.8%)。上位2社で **約92.7%**。出典: [RILG 全国自治体例規集](https://www.rilg.or.jp/htdocs/main/zenkoku_reiki/zenkoku_link.html) / [ぎょうせい](https://gyosei.jp/business/law/super_reiki-base/) / [第一法規](https://www.daiichihoki.co.jp/jichi/reikiseibinavi/index.html)
- ぎょうせい Reiki-Base の共通URL: `https://www1.g-reiki.net/{slug}/reiki_menu.html` → 本文 `reiki_honbun/...html`。自治体独自ドメイン版（`g-reiki.city.*.lg.jp` 等）も同一エンジン。
- 全国横断リンク集（1,000自治体超）は **RILG が集約**＝テナント発見の起点に使える。

### 法的位置づけ（障壁が低い）
- **条例・規則の本文は著作権法13条二号により著作権なし**（地方公共団体の条例・規則は明示対象）。出典: [文化庁テキスト](https://www.bunka.go.jp/seisaku/chosakuken/textbook/pdf/94081601_01.pdf)
- ⚠️ ただし「例規集」という**編集物・体系目次・付加情報はベンダーの編集著作物**になり得る。**本文のみ抽出**し、DB構造の丸ごと複製は避ける（既存 reiki-client の方針コメントと一致）。

### 構造（既存資産の再利用度が高い）
- 例規も国法令と同型の**条・項・号**構成。配信形式は e-Gov=XML に対し例規=HTML。
- [`law-normalizer`](../crates/law-normalizer/src/lib.rs) は e-Gov XML 専用だが、出力型 `Article`/`Paragraph` は再利用可。**例規HTML→中間条項号→law-normalizer型** の薄いHTMLパーサを足せば、差分(`law-diff`)・表示・検索(`search-index`)の下流を共有できる。

### 推奨アプローチ（MVP）
1. ぎょうせい Reiki-Base 型1アダプタを**実テナント slug 群**（RILGリンク集から `g-reiki.net` 系を抽出）で動かし、政令市20＋特別区から本文HTML取得 → 条項号構造化 → 既存配信JSONへ。
2. 第二段で第一法規(`d1-law.com`)型アダプタ → 上位2社で約93%カバー。
3. 既存 [`reiki-client`](../crates/reiki-client/src/lib.rs) の修正3点（後述）を土台に。

### 既存 `reiki-client` の不足（要修正3点）
1. `known_municipalities()` のURLが**実在パターンと不一致**（`city.*.lg.jp/reiki` は推測値）。実テナント slug 一覧（RILG由来）へ差し替え。
2. `list_reiki` のパスが固定。実入口は `reiki_menu.html` で、本文一覧は体系/五十音目次の遷移を要する場合あり。実HTML再検証必須。
3. **条・項・号の構造化が無い**（`body_text` 平テキストのみ）。law-normalizer相当の正規化を追加。
- CI連携は**実装済み**（`reiki-fetch`/`reiki-build-json`、`update-corpus-data.yml` の `DOMAINS` に `reiki`）。配信JSON経路あり。

---

## 3. 着手順の提案

1. **② 例規 MVP**（先行）: reiki-client の slug 実体化 + `reiki_menu.html` 実HTML対応 + 条項号パーサ。政令市・特別区から。著作権リスクが本文単位で実質ゼロなのが効く。
2. **法令×例規の連携**: 国法令の委任（「○○は条例で定める」）と自治体例規を将来クロスリンク（差別化）。
3. **① 会議録**: **保留**（上記ライセンス確定結果のとおり、合法な再配布可ソースが現状なし）。再開はライセンス交渉が前提。

## 4. 未確認事項（実装前に要確認）
- ~~local-politics.jp 配布データ／chiholog・yonalog の二次利用ライセンス~~ → **確定（①ライセンス確定結果参照: 商用再配布不可・保留）**。
- 例規集ベンダー（g-reiki.net / d1-law.com）の robots.txt / ToS（条例本文は著作権なしだが、アクセス規約と編集著作物の回避を実機確認）。
- `d1-law.com` 例規の具体URL構造、ぎょうせい目次の実遷移。
- 「数アダプタで大半カバー」「メンテ負荷」は寡占シェアからの推測 — 実テナントでHTML安定性を実機検証してから本実装。

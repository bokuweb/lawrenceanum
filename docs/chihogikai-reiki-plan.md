# 検討メモ: 地方議会会議録 / 自治体例規 の収集対象化

> 法令・官報・パブコメ・国会会議録に続く収集対象として「①地方議会会議録」「②自治体例規」を
> 検討する。2026-06-19 の調査に基づく初期設計メモ（実装前の意思決定用）。
> 関連: [`reiki-plan.md`](reiki-plan.md) / [`public-corpus-roadmap.md`](public-corpus-roadmap.md) /
> 既存実装: [`reiki-client`](../crates/reiki-client/src/lib.rs) / [`shingikai-client`](../crates/shingikai-client/src/lib.rs)

---

## 0. 結論

- **② 例規 = 実現可能性「中〜高」。先に着手すべき（ROI高）。** ベンダー2社（ぎょうせい 61.6% + 第一法規 31.1% ≒ **92%超**）の寡占でURL/HTML構造が高度に共通。**条例・規則本文は著作権法13条で著作権が発生しない**ため再配布の法的障壁が低い。既存 `reiki-client` の路線（ぎょうせい型アダプタ）は正しく、土台に使える。
- **① 地方議会会議録 = 実現可能性「中」。②の後 or 慎重に並行。** ベンダー寡占（DiscussNet等）で構造は半共通だが**統一公式APIは無し**、ライセンスが本文の13条該当が条文上不明確で②よりリスク高。MVPは学術コーパス（local-politics.jp）取込が最短だが**ライセンス確認が前段ブロッカー**。

---

## ① 地方議会会議録

### 提供基盤（寡占・統一APIなし）
- 会議録検索システムは少数ベンダー寡占。販売=会議録研究所、システム=NTT-AT系。
  **DiscussNetPremium**（クラウド型）≈ [477自治体](https://www.ntt-at.co.jp/product/discussnetpremium/)、DiscussVisionシリーズ≈234自治体。
- **公式の統一APIは存在しない**（公開方法・形式が自治体ごとに異なる）。出典: [地方議会会議録コーパスプロジェクト](http://local-politics.jp/)
- 既存民間: Bitlet [chiholog](https://chiholog.net/chiholog)/yonalog（横断検索）。学術: [local-politics.jp](http://local-politics.jp/) が文単位＋メタ（自治体コード/年月日/会議名/発言者）で構造化配布（KAKENHI 20K00576）。

### 推奨アプローチ
1. **MVP = local-politics.jp 学術コーパスの取込**（自前スクレイピングより網羅性・構造化が桁違いに速い）。**ただし配布データのライセンス（CC/再配布可否）確認が前段必須**。
2. NG or 鮮度不足なら [`shingikai-client`](../crates/shingikai-client/src/lib.rs) の `MinistryAdapter` 方式を `AssemblyAdapter`（DiscussNet型/個別サイト型）に転用し、都道府県・政令市から少数スクレイピング。
3. 全1,700自治体網羅は初期スコープから外す（分散メンテが破綻）。

### リスク
- **ライセンス（中）**: 議事録本文は著作権法13条の例示（告示・訓令・通達）に**含まれず**、本文の著作権有無が条例ほどクリアでない。コーパス/民間サービスの二次利用条件も**未確認**。商用配信前に各規約/robotsを個別確認。
- 網羅性の分散（高）・メンテ負荷（高: 多テナント・ベンダーUI変更）。

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
3. **① 会議録**: まず local-politics.jp の**ライセンス確認**（前段ブロッカー）。OKなら取込MVP、NGなら AssemblyAdapter で少数スクレイピング。

## 4. 未確認事項（実装前に要確認）
- local-politics.jp 配布データ／chiholog・yonalog の二次利用ライセンス。
- 各ベンダー会議録システム・例規集の robots.txt / ToS（商用再配布可否）。
- `d1-law.com` 例規の具体URL構造、ぎょうせい目次の実遷移。
- 「数アダプタで大半カバー」「メンテ負荷」は寡占シェアからの推測 — 実テナントでHTML安定性を実機検証してから本実装。

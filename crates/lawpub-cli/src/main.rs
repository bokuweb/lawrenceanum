use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod budget;
mod build;
mod compress;
mod diffs;
mod feeds;
mod gian;
mod kanpo;
mod proceedings;
mod procurement;
mod pubcomment;
mod reiki;
mod shingikai;
mod snapshots;
mod state;
mod status;
mod validate;

#[derive(Parser)]
#[command(name = "lawpub", version, about = "e-Gov 法令データ正規化・配信 CLI")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// 単一の更新日付を取得してキャッシュに格納する。
    FetchUpdate {
        #[arg(long)]
        date: String,
        #[arg(long, default_value = ".cache")]
        cache: PathBuf,
    },
    /// 期間指定で更新を取得する。
    FetchRange {
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
        #[arg(long, default_value = ".cache")]
        cache: PathBuf,
        #[arg(long, default_value = "http", env = "LAWPUB_PROVIDER")]
        provider: String,
    },
    /// 指定カテゴリの全件バルクを取得する (e-Gov v2: 1=憲法・法律, 2=政令・勅令, ...)。
    /// 数千〜数万件になり得るため、`--limit` で件数を絞れる。
    FetchBulk {
        #[arg(long)]
        category: u32,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long, default_value = ".cache")]
        cache: PathBuf,
        #[arg(long, default_value = "http", env = "LAWPUB_PROVIDER")]
        provider: String,
    },
    /// キャッシュ済みデータから配信用JSONを生成する。
    BuildJson {
        #[arg(long, default_value = ".cache")]
        input: PathBuf,
        #[arg(long, default_value = "public")]
        output: PathBuf,
    },
    /// `public/` 配下の index/manifest を再生成する。
    BuildIndex {
        #[arg(long, default_value = "public")]
        output: PathBuf,
    },
    /// `public/manifest.json` の sha256 と実ファイルが一致するか検証する。
    Validate {
        #[arg(long, default_value = "public")]
        public: PathBuf,
    },
    /// `public/manifest.json` の files[] を現在のディスク内容から再計算する。
    /// index/laws/health は触らない。prebuilt 履歴束 (history.ndjson.zst) を
    /// 上書きした後など、ファイル実体だけ差し替えたときに validate を通すための再生成。
    RebuildManifest {
        #[arg(long, default_value = "public")]
        public: PathBuf,
    },
    /// `public` の履歴束に `prebuilt` の履歴束を法令ごとに revision_id で
    /// union (dedup) してマージする。過去版は prebuilt、新版は CI ビルド由来を
    /// 取り込めるので、全 revision キャッシュを CI に置かずに履歴を差分更新できる。
    MergeHistory {
        #[arg(long, default_value = "public")]
        public: PathBuf,
        /// マージ元の prebuilt 履歴束ツリー (laws/{id}/history.ndjson.zst を含む)。
        #[arg(long)]
        prebuilt: PathBuf,
    },
    /// fetch-update + build-json + build-index をまとめて実行する。
    Update {
        #[arg(long, default_value = "public")]
        public: PathBuf,
        #[arg(long, default_value = ".cache")]
        cache: PathBuf,
        /// `mock` (組み込みサンプル) または `http` (e-Gov 法令API)。
        /// 環境変数 `LAWPUB_PROVIDER` で上書き可能。
        #[arg(long, default_value = "http", env = "LAWPUB_PROVIDER")]
        provider: String,
        /// 取得対象の日付。指定しなければ state/latest.json から決まる。
        #[arg(long)]
        date: Option<String>,
        /// 新規revisionが無くても public/ を強制再生成する。
        #[arg(long)]
        force: bool,
    },
    /// e-Gov v2 `/law_revisions/{id}` で改正履歴メタを取得し
    /// `.cache/revisions_meta/{law_id}.json` に保存する。
    ///
    /// 指定方法は二択:
    /// - `--law-id <ID>`: 単一法令のみ取得 (テストや差分用)
    /// - `--all`: `.cache/revisions/` 以下の全法令を順に取得 (一括 backfill 用)
    /// - `--from-public <DIR>`: `public/laws/index.json` の laws[].law_id を対象に取得
    ///   (= git checkout だけある別端末で backfill する場合の入口)
    FetchRevisions {
        #[arg(long, conflicts_with_all = ["all", "from_public"])]
        law_id: Option<String>,
        /// `.cache/revisions/` に既にある全法令ぶん取得する。
        #[arg(long, conflicts_with_all = ["law_id", "from_public"])]
        all: bool,
        /// `public/laws/index.json` の laws[] から ID を読み取って取得。
        /// `.cache/revisions/` が手元に無い別端末でも回せる。
        #[arg(long, value_name = "PUBLIC_DIR", conflicts_with_all = ["law_id", "all"])]
        from_public: Option<PathBuf>,
        /// 並列度。e-Gov v2 もレートリミットがあるので控えめに (既定 2)。
        #[arg(long, default_value_t = 2)]
        concurrency: usize,
        /// スモークテスト用の件数キャップ。指定すると先頭 N 件だけ取得。
        #[arg(long)]
        limit: Option<usize>,
        /// 既に `revisions_meta/{id}.json` が存在する場合に上書きするかどうか。
        /// 未指定なら skip して resume 友好的に振る舞う。
        #[arg(long)]
        force: bool,
        #[arg(long, default_value = ".cache")]
        cache: PathBuf,
    },
    /// `.cache/revisions_meta/{law_id}.json` を元に、各 revision の本文 XML を
    /// `.cache/revisions/{law_id}/{revision_id}.xml` として e-Gov v2 から取得する。
    /// build-diffs / 任意リビジョン参照のための backfill 入口。
    FetchRevisionBodies {
        #[arg(long, conflicts_with = "all")]
        law_id: Option<String>,
        #[arg(long, conflicts_with = "law_id")]
        all: bool,
        #[arg(long, default_value_t = 2)]
        concurrency: usize,
        /// 対象法令数のキャップ (テスト用)。
        #[arg(long)]
        limit_laws: Option<usize>,
        /// 1 法令あたりの revision 数キャップ (テスト用)。
        #[arg(long)]
        limit_revs_per_law: Option<usize>,
        #[arg(long)]
        force: bool,
        #[arg(long, default_value = ".cache")]
        cache: PathBuf,
    },
    /// `.cache/revisions_meta/{law_id}.json` を 1 ファイルにまとめる/展開する。
    /// 単一の JSONL (= `{"law_id":..., "law_info":..., "revisions":[...]}` を法令毎に1行)。
    /// R2 等へのアップロードと CI 復元を 1 ファイル単位で扱うためのユーティリティ。
    BundleRevisionsMeta {
        /// pack: `.cache/revisions_meta/*.json` を読み込み 1 ファイルにまとめる。
        /// unpack: 1 ファイルを読み 法令毎の JSON を `--in-dir` に書き出す。
        #[arg(long, value_parser = ["pack", "unpack"])]
        mode: String,
        #[arg(long, default_value = ".cache/revisions_meta")]
        dir: PathBuf,
        #[arg(long, default_value = ".cache/revisions_meta.jsonl")]
        file: PathBuf,
    },
    /// 全法令の隣接 revision 間 diff を生成し `laws/{id}/diff/` と `laws/{id}/diffs.json` を書き出す。
    BuildDiffs {
        #[arg(long, default_value = "public")]
        public: PathBuf,
    },
    /// 単発の 2 revision diff を stdout に出す。
    Diff {
        #[arg(long)]
        law: String,
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
        #[arg(long, default_value = "public")]
        public: PathBuf,
    },
    /// 任意日付スナップショット `laws/{id}/at/{yyyy-mm-dd}.json` を生成する。
    BuildSnapshots {
        /// カンマ区切りの日付リスト (例: 2018-04-01,2020-04-01)。
        #[arg(long, value_delimiter = ',')]
        dates: Vec<String>,
        /// 公布済み・未施行も含めて解決する。
        #[arg(long)]
        include_unenforced: bool,
        #[arg(long, default_value = "public")]
        public: PathBuf,
    },
    /// 配信用 JSON を gzip 事前圧縮する (`*.json` / `*.ndjson` → `*.gz`)。
    /// SPA は `VITE_COMPRESSED` 時に `.gz` を取得し展開する。
    /// `search.db*` は Range アクセスのため対象外。
    Compress {
        #[arg(long, default_value = "public")]
        public: PathBuf,
        /// 元の非圧縮ファイルを削除する (容量削減)。未指定なら `.gz` を併置。
        #[arg(long)]
        remove_original: bool,
    },
    /// 指定日の官報を取得し、各項目の改め文を抽出して `.cache/kanpo/{date}.json` に保存。
    KanpoFetch {
        #[arg(long)]
        date: String,
        /// `http` (デジタル官報) または `mock`。
        #[arg(long, default_value = "http", env = "LAWPUB_PROVIDER")]
        provider: String,
        /// 1日あたりの PDF ダウンロード上限 (負荷ガード)。
        #[arg(long, default_value_t = 200)]
        limit: usize,
        /// 取得した生 PDF を `{cache}/kanpo-pdf/{date}/` に保持する (後の再抽出用)。
        /// 既定オフ (CI のキャッシュ肥大回避)。ローカルでのアーカイブ時に指定。
        #[arg(long)]
        save_pdf: bool,
        #[arg(long, default_value = ".cache")]
        cache: PathBuf,
    },
    /// 官報リンクを `public/kanpo/` に書き出す (Phase 3 placeholder)。
    KanpoLink {
        #[arg(long, default_value = "public")]
        output: PathBuf,
    },
    /// 官報の項目別 PDF から改め文を抽出する PoC。`.cache/kanpo-poc/{date}/` に
    /// 整形済みテキストと目次 JSON を書き出し、抽出精度を目視検証する。
    KanpoPoc {
        /// 対象日 (YYYY-MM-DD)。
        #[arg(long)]
        date: String,
        /// 改正・廃止系の項目だけに絞る (標題に「改正」「廃止」を含む)。
        #[arg(long)]
        amend_only: bool,
        /// ダウンロードする項目数の上限 (負荷確認用)。
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long, default_value = ".cache")]
        cache: PathBuf,
    },
    /// cache / public / state を要約して JSON で stdout に出す。
    Status {
        #[arg(long, default_value = "public")]
        public: PathBuf,
        #[arg(long, default_value = ".cache")]
        cache: PathBuf,
    },

    /// 国会会議録: 指定会期の全会議を取得し `.cache/proceedings/{session}/` に保存する。
    ProceedingsFetch {
        /// 国会回次 (例: 215)。
        #[arg(long)]
        session: u32,
        #[arg(long, default_value = ".cache")]
        cache: PathBuf,
        /// `mock` (組み込みサンプル) または `http` (国会会議録API)。
        #[arg(long, default_value = "http", env = "LAWPUB_PROVIDER")]
        provider: String,
    },

    /// 国会会議録: `.cache/proceedings/` のキャッシュから配信用 JSON を生成する。
    ProceedingsBuildJson {
        #[arg(long, default_value = ".cache")]
        cache: PathBuf,
        #[arg(long, default_value = "public")]
        public: PathBuf,
    },

    /// 法令 ↔ 国会会議録 クロスリンクを生成する。
    /// `public/links/law-to-proceedings/{law_id}.json` を書き出す。
    LinkLawsAndProceedings {
        #[arg(long, default_value = "public")]
        public: PathBuf,
    },

    /// パブコメ: 結果公示済み案件を取得し `.cache/pubcomment/` に保存する。
    PubcommentFetch {
        #[arg(long, default_value = ".cache")]
        cache: PathBuf,
        #[arg(long, default_value = "http", env = "LAWPUB_PROVIDER")]
        provider: String,
        /// 最大取得ページ数（1 ページ = 最大 20 件程度）。
        #[arg(long, default_value_t = 100)]
        max_pages: u32,
    },

    /// パブコメ: キャッシュから配信用 JSON を生成する。
    PubcommentBuildJson {
        #[arg(long, default_value = ".cache")]
        cache: PathBuf,
        #[arg(long, default_value = "public")]
        public: PathBuf,
    },

    /// 調達情報: 官公需ポータル (kkj.go.jp) から公告日範囲で取得する。
    ProcurementFetch {
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
        #[arg(long, default_value = ".cache")]
        cache: PathBuf,
        #[arg(long, default_value = "http", env = "LAWPUB_PROVIDER")]
        provider: String,
    },

    /// 調達情報: キャッシュから配信用 JSON を生成する。
    ProcurementBuildJson {
        #[arg(long, default_value = ".cache")]
        cache: PathBuf,
        #[arg(long, default_value = "public")]
        public: PathBuf,
    },

    /// 例規: 自治体例規集を取得する（初期: 3 自治体）。
    ReikiFetch {
        /// 対象自治体コード（省略時: 全登録自治体）。
        #[arg(long, value_delimiter = ',')]
        municipalities: Vec<String>,
        #[arg(long, default_value = ".cache")]
        cache: PathBuf,
        #[arg(long, default_value = "http", env = "LAWPUB_PROVIDER")]
        provider: String,
    },

    /// 例規: キャッシュから配信用 JSON を生成する。
    ReikiBuildJson {
        #[arg(long, default_value = ".cache")]
        cache: PathBuf,
        #[arg(long, default_value = "public")]
        public: PathBuf,
    },

    /// 審議会: 府省の審議会・委員会議事録を取得する。
    ShingiakaiFetch {
        /// 府省 ID (moj, cao, ...)。
        #[arg(long, default_value = "moj")]
        ministry: String,
        #[arg(long, default_value = ".cache")]
        cache: PathBuf,
        #[arg(long, default_value = "http", env = "LAWPUB_PROVIDER")]
        provider: String,
    },

    /// 審議会: キャッシュから配信用 JSON を生成する。
    ShingiakaiBuildJson {
        #[arg(long, default_value = ".cache")]
        cache: PathBuf,
        #[arg(long, default_value = "public")]
        public: PathBuf,
    },

    /// 予算: e-Stat API から財政統計データを取得する（LAWPUB_ESTAT_APP_ID 必須）。
    BudgetFetch {
        #[arg(long, default_value = ".cache")]
        cache: PathBuf,
        #[arg(long, default_value = "http", env = "LAWPUB_PROVIDER")]
        provider: String,
    },

    /// 予算: キャッシュから配信用 JSON を生成する。
    BudgetBuildJson {
        #[arg(long, default_value = ".cache")]
        cache: PathBuf,
        #[arg(long, default_value = "public")]
        public: PathBuf,
    },

    /// 法令 ↔ パブコメ クロスリンクを生成する。
    /// `public/links/law-to-pubcomment/{law_id}.json` を書き出す。
    LinkLawsAndPubcomment {
        #[arg(long, default_value = "public")]
        public: PathBuf,
    },

    /// 法令 ↔ 調達情報 クロスリンクを生成する。
    /// `public/links/law-to-procurement/{law_id}.json` を書き出す。
    LinkLawsAndProcurement {
        #[arg(long, default_value = "public")]
        public: PathBuf,
    },

    /// 規制変化フィードを生成する (法令改正・パブコメ・官報の新着)。
    /// `public/feeds/recent.json` と RSS `recent.xml` を書き出す。
    BuildFeeds {
        #[arg(long, default_value = "public")]
        public: PathBuf,
    },

    /// 国会 議案情報 (法案審議トラッキング) を取得し `.cache/gian/` に保存する。
    GianFetch {
        #[arg(long, default_value = ".cache")]
        cache: PathBuf,
        #[arg(long, default_value = "http")]
        provider: String,
        /// 国会回次。0 で最新回。
        #[arg(long, default_value_t = 0)]
        session: u32,
    },

    /// `.cache/gian/` → `public/gian/{session}/*.json` + index を書き出す。
    GianBuildJson {
        #[arg(long, default_value = ".cache")]
        cache: PathBuf,
        #[arg(long, default_value = "public")]
        public: PathBuf,
    },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Update {
            public,
            cache,
            provider,
            date,
            force,
        } => build::run_update(&public, &cache, &provider, date.as_deref(), force),
        Cmd::BuildJson { input, output } => build::run_build_json(&input, &output),
        Cmd::BuildIndex { output } => build::run_build_index(&output),
        Cmd::Validate { public } => validate::run_validate(&public),
        Cmd::RebuildManifest { public } => build::run_rebuild_manifest(&public),
        Cmd::MergeHistory { public, prebuilt } => build::run_merge_history(&public, &prebuilt),
        Cmd::FetchUpdate { date, cache } => {
            build::run_fetch_update(&date, &cache, "mock").map(|_| ())
        }
        Cmd::FetchRange {
            from,
            to,
            cache,
            provider,
        } => build::run_fetch_range(&from, &to, &cache, &provider),
        Cmd::FetchBulk {
            category,
            limit,
            cache,
            provider,
        } => build::run_fetch_bulk(category, limit, &cache, &provider),
        Cmd::FetchRevisions {
            law_id,
            all,
            from_public,
            concurrency,
            limit,
            force,
            cache,
        } => build::run_fetch_revisions(
            law_id.as_deref(),
            all,
            from_public.as_deref(),
            concurrency,
            limit,
            force,
            &cache,
        ),
        Cmd::FetchRevisionBodies {
            law_id,
            all,
            concurrency,
            limit_laws,
            limit_revs_per_law,
            force,
            cache,
        } => build::run_fetch_revision_bodies(
            law_id.as_deref(),
            all,
            concurrency,
            limit_laws,
            limit_revs_per_law,
            force,
            &cache,
        ),
        Cmd::BundleRevisionsMeta { mode, dir, file } => {
            build::run_bundle_revisions_meta(&mode, &dir, &file)
        }
        Cmd::BuildDiffs { public } => {
            diffs::run_build_diffs(&public)?;
            // 単独実行でも manifest を整合させる。
            build::rebuild_manifest(&public)
        }
        Cmd::Diff {
            law,
            from,
            to,
            public,
        } => diffs::run_diff_pair(&public, &law, &from, &to),
        Cmd::BuildSnapshots {
            dates,
            include_unenforced,
            public,
        } => {
            snapshots::run_build_snapshots(&public, &dates, include_unenforced)?;
            build::rebuild_manifest(&public)
        }
        Cmd::Compress {
            public,
            remove_original,
        } => {
            let s = compress::run_compress(&public, remove_original)?;
            let pct = if s.bytes_in > 0 {
                s.bytes_out as f64 * 100.0 / s.bytes_in as f64
            } else {
                0.0
            };
            tracing::info!(
                "compress: {} files, {:.1} MB -> {:.1} MB ({:.1}%)",
                s.files,
                s.bytes_in as f64 / 1_048_576.0,
                s.bytes_out as f64 / 1_048_576.0,
                pct
            );
            Ok(())
        }
        Cmd::KanpoFetch {
            date,
            provider,
            limit,
            save_pdf,
            cache,
        } => kanpo::run_fetch(&date, &provider, limit, save_pdf, &cache),
        Cmd::KanpoLink { output } => kanpo::run_link(&output),
        Cmd::KanpoPoc {
            date,
            amend_only,
            limit,
            cache,
        } => kanpo::run_poc(&date, amend_only, limit, &cache),
        Cmd::Status { public, cache } => status::run(&public, &cache),
        Cmd::ProceedingsFetch { session, cache, provider } => {
            proceedings::run_fetch(session, &cache, &provider)
        }
        Cmd::ProceedingsBuildJson { cache, public } => {
            proceedings::run_build_json(&cache, &public)
        }
        Cmd::LinkLawsAndProceedings { public } => linking::run_link(&public),
        Cmd::PubcommentFetch { cache, provider, max_pages } => {
            pubcomment::run_fetch(&cache, &provider, max_pages)
        }
        Cmd::PubcommentBuildJson { cache, public } => {
            pubcomment::run_build_json(&cache, &public)
        }
        Cmd::ProcurementFetch { from, to, cache, provider } => {
            procurement::run_fetch(&from, &to, &cache, &provider)
        }
        Cmd::ProcurementBuildJson { cache, public } => {
            procurement::run_build_json(&cache, &public)
        }
        Cmd::ReikiFetch { municipalities, cache, provider } => {
            reiki::run_fetch(&municipalities, &cache, &provider)
        }
        Cmd::ReikiBuildJson { cache, public } => reiki::run_build_json(&cache, &public),
        Cmd::ShingiakaiFetch { ministry, cache, provider } => {
            shingikai::run_fetch(&ministry, &cache, &provider)
        }
        Cmd::ShingiakaiBuildJson { cache, public } => {
            shingikai::run_build_json(&cache, &public)
        }
        Cmd::BudgetFetch { cache, provider } => budget::run_fetch(&cache, &provider),
        Cmd::BudgetBuildJson { cache, public } => budget::run_build_json(&cache, &public),
        Cmd::LinkLawsAndPubcomment { public } => linking::run_link_pubcomment(&public),
        Cmd::BuildFeeds { public } => feeds::run_build_feeds(&public),
        Cmd::GianFetch { cache, provider, session } => gian::run_fetch(&cache, &provider, session),
        Cmd::GianBuildJson { cache, public } => gian::run_build_json(&cache, &public),
        Cmd::LinkLawsAndProcurement { public } => linking::run_link_procurement(&public),
    }
}

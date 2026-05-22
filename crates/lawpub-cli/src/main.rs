use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod build;
mod kanpo;
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
    /// 官報の日付ページを取得する (Phase 3 placeholder)。
    KanpoFetch {
        #[arg(long)]
        date: String,
        #[arg(long, default_value = ".cache")]
        cache: PathBuf,
    },
    /// 官報リンクを `public/kanpo/` に書き出す (Phase 3 placeholder)。
    KanpoLink {
        #[arg(long, default_value = "public")]
        output: PathBuf,
    },
    /// cache / public / state を要約して JSON で stdout に出す。
    Status {
        #[arg(long, default_value = "public")]
        public: PathBuf,
        #[arg(long, default_value = ".cache")]
        cache: PathBuf,
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
        Cmd::Update { public, cache, provider, date, force } => {
            build::run_update(&public, &cache, &provider, date.as_deref(), force)
        }
        Cmd::BuildJson { input, output } => build::run_build_json(&input, &output),
        Cmd::BuildIndex { output } => build::run_build_index(&output),
        Cmd::Validate { public } => validate::run_validate(&public),
        Cmd::FetchUpdate { date, cache } => build::run_fetch_update(&date, &cache, "mock").map(|_| ()),
        Cmd::FetchRange { from, to, cache, provider } => build::run_fetch_range(&from, &to, &cache, &provider),
        Cmd::FetchBulk { category, limit, cache, provider } => {
            build::run_fetch_bulk(category, limit, &cache, &provider)
        }
        Cmd::FetchRevisions { law_id, all, from_public, concurrency, limit, force, cache } => {
            build::run_fetch_revisions(
                law_id.as_deref(),
                all,
                from_public.as_deref(),
                concurrency,
                limit,
                force,
                &cache,
            )
        }
        Cmd::BundleRevisionsMeta { mode, dir, file } => {
            build::run_bundle_revisions_meta(&mode, &dir, &file)
        }
        Cmd::KanpoFetch { date, cache } => kanpo::run_fetch(&date, &cache),
        Cmd::KanpoLink { output } => kanpo::run_link(&output),
        Cmd::Status { public, cache } => status::run(&public, &cache),
    }
}

// 官報パーサ playground（`/kanpo-playground`）のデータ層。
//
// - kanpo-parser の WASM（`pnpm build:wasm` で src/wasm/kanpo-parser/ に生成。gitignore 済み）
// - 入力ソース: リポジトリの回帰テスト用コーパス / R2 のアーカイブ / ローカルファイル
import type { AmendDocument } from "./api";

// ---------------------------------------------------------------------------
// analyze() の結果（crates/kanpo-parser/src/trace.rs・lib.rs の serde 表現）
// ---------------------------------------------------------------------------

export type KanpoRole = "body" | "preamble" | "after" | "before" | "outside";
export type KanpoWordStatus = "used" | "margin" | "empty" | "header_label" | "noise";

/** ページ上の 1 語。座標は pt、左上原点（pdf.js の scale=1 viewport と同じ）。 */
export type KanpoWord = {
  id: number;
  x0: number;
  y0: number;
  x1: number;
  y1: number;
  raw: string;
  text: string;
  status: KanpoWordStatus;
};
export type KanpoLine = { text: string; words: number[] };
export type KanpoRegion = { role: KanpoRole; unit: number; part: number; lines: KanpoLine[] };
export type KanpoPageTrace = {
  width: number;
  height: number;
  layout: "tiers" | "headerless_shinkyu" | string;
  words: KanpoWord[];
  regions: KanpoRegion[];
  text: string;
};
export type KanpoAnalysis = {
  pages: KanpoPageTrace[];
  text: string;
  format: "prose" | "shinkyu" | "unknown" | string;
  articles: { text: string; document: AmendDocument }[];
};

export const ROLE_LABEL: Record<KanpoRole, string> = {
  body: "本文",
  preamble: "前文",
  after: "改正後",
  before: "改正前",
  outside: "表外",
};

/**
 * 役割ごとの色。オーバーレイは PDF の白い紙面の上に描くのでテーマに依らず紙面上で
 * 見分けやすい中間調にし、右パネルのチップも同じ色を使う（ダークでも沈まない明度）。
 */
export const ROLE_COLOR: Record<KanpoRole, string> = {
  after: "#2563eb",
  before: "#e11d48",
  preamble: "#7c3aed",
  outside: "#059669",
  body: "#d97706",
};

export const STATUS_LABEL: Record<KanpoWordStatus, string> = {
  used: "採用",
  margin: "余白（柱）",
  empty: "正規化後に空",
  header_label: "欄見出し",
  noise: "柱の行",
};

/** 捨てた語の理由（ホバー時の説明）。 */
export const STATUS_REASON: Record<KanpoWordStatus, string> = {
  used: "どこかの行に組まれた",
  margin: "上下の余白帯（柱）にあるため捨てた",
  empty: "正規化後に空になった（傍線などの私用領域グリフだけの語）",
  header_label: "新旧対照表の欄見出し（改/正/後/前）",
  noise: "柱と判定した行（発行日・号数など）に含まれていた",
};

// ---------------------------------------------------------------------------
// WASM
// ---------------------------------------------------------------------------

export type KanpoParser = { analyze: (xhtml: string) => KanpoAnalysis; version: string };

/** wasm-bindgen（--target web）が出力するモジュールのうち使うもの。 */
type WasmModule = { default: () => Promise<unknown>; analyze: (xhtml: string) => string; version: () => string };

// WASM を作っていない環境（他の CI ジョブ等）でもビルドが通るよう、静的 import ではなく
// glob にする。未生成なら空のオブジェクトになり、playground は案内を出す。
const wasmModules = import.meta.glob<WasmModule>("../../wasm/kanpo-parser/kanpo_parser_wasm.js");

let parserPromise: Promise<KanpoParser | null> | null = null;

/** kanpo-parser の WASM を読み込む。未ビルドなら null。 */
export function loadKanpoParser(): Promise<KanpoParser | null> {
  if (!parserPromise) {
    const load = Object.values(wasmModules)[0];
    parserPromise = load
      ? load().then(async (m) => {
          await m.default();
          return {
            analyze: (xhtml: string) => {
              const v = JSON.parse(m.analyze(xhtml));
              if (v && typeof v.error === "string") throw new Error(v.error);
              return v as KanpoAnalysis;
            },
            version: m.version(),
          };
        })
      : Promise.resolve(null);
  }
  return parserPromise;
}

// ---------------------------------------------------------------------------
// ソース: コーパス（crates/kanpo-parser/tests/corpus/<case-id>/）
// ---------------------------------------------------------------------------

export type CorpusCaseMeta = {
  title: string;
  date?: string;
  issue?: string;
  page?: number;
  source?: string;
  tags?: string[];
  notes?: string;
};
export type CorpusCase = { id: string; meta: CorpusCaseMeta; bboxUrl: string; pdfUrl: string | null };

// コピーのスクリプトを挟まず、リポジトリのコーパスをそのままビルドに取り込む。
// case.json は中身ごと、pdf / bbox は URL（ビルド時に assets へ出力）として。
const corpusMeta = import.meta.glob<CorpusCaseMeta>("../../../../crates/kanpo-parser/tests/corpus/*/case.json", {
  eager: true,
  import: "default",
});
const corpusPdf = import.meta.glob<string>("../../../../crates/kanpo-parser/tests/corpus/*/page.pdf", {
  eager: true,
  query: "?url",
  import: "default",
});
const corpusBbox = import.meta.glob<string>("../../../../crates/kanpo-parser/tests/corpus/*/page.bbox.html", {
  eager: true,
  query: "?url",
  import: "default",
});

/** glob のキー（相対パス）からケース ID（ディレクトリ名）を取り出す。 */
const caseIdOf = (key: string) => key.split("/").slice(-2, -1)[0];

export const CORPUS_CASES: CorpusCase[] = Object.entries(corpusMeta)
  .flatMap(([key, meta]) => {
    const dir = key.slice(0, key.lastIndexOf("/"));
    const bboxUrl = corpusBbox[`${dir}/page.bbox.html`];
    if (!bboxUrl) return [];
    return [{ id: caseIdOf(key), meta, bboxUrl, pdfUrl: corpusPdf[`${dir}/page.pdf`] ?? null }];
  })
  .sort((a, b) => a.id.localeCompare(b.id));

// ---------------------------------------------------------------------------
// ソース: R2 のアーカイブ（`lawpub kanpo-archive` の出力）
// ---------------------------------------------------------------------------

/** アーカイブのベース URL（未設定ならアーカイブは選べない）。 */
export const ARCHIVE_BASE: string | null = (import.meta.env.VITE_KANPO_ARCHIVE_URL || "").replace(/\/$/, "") || null;

export type ArchivePage = { page: number; file: string; sha256: string; bytes: number; bbox: boolean };
export type ArchiveIssue = {
  issue_type: string;
  issue_no: string;
  dir: string;
  url: string;
  page_count: number;
  pages: ArchivePage[];
  items: { title: string; page: number }[];
};
export type ArchiveManifest = { date: string; issues: ArchiveIssue[] };

async function fetchJson<T>(url: string): Promise<T> {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${res.status} ${res.statusText}: ${url}`);
  return res.json() as Promise<T>;
}

/** アーカイブ済みの日付（新しい順）。 */
export async function fetchArchiveDates(): Promise<string[]> {
  if (!ARCHIVE_BASE) return [];
  const v = await fetchJson<{ dates: string[] }>(`${ARCHIVE_BASE}/index.json`);
  return [...(v.dates ?? [])].sort().reverse();
}

export function fetchArchiveManifest(date: string): Promise<ArchiveManifest> {
  return fetchJson<ArchiveManifest>(`${ARCHIVE_BASE}/${date}/manifest.json`);
}

export function archivePdfUrl(date: string, dir: string, file: string): string {
  return `${ARCHIVE_BASE}/${date}/${dir}/${file}`;
}

export function archiveBboxUrl(date: string, dir: string, file: string): string {
  return archivePdfUrl(date, dir, file.replace(/\.pdf$/, ".bbox.html"));
}

/** 号の種類の表示名。 */
export function issueTypeLabel(t: string): string {
  return (
    { regular: "本紙", extra: "号外", special_extra: "特別号外" } as Record<string, string>
  )[t] ?? t;
}

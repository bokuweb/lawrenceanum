// 官報パーサ playground（`/kanpo-playground`）のデータ層。
//
// - kanpo-parser の WASM（`pnpm build:wasm` で src/wasm/kanpo-parser/ に生成。gitignore 済み）
// - 入力ソース: リポジトリの回帰テスト用コーパス / R2 のアーカイブ / ローカルファイル
// - 入力は項目（告示など）単位の複数ページ。各ページは bbox の XHTML と、あれば罫線（.rules.json）
import type { AmendDocument } from "./api";

// ---------------------------------------------------------------------------
// analyze() の結果（crates/kanpo-parser/src/trace.rs・lib.rs の serde 表現）
// ---------------------------------------------------------------------------

export type KanpoRole = "body" | "preamble" | "after" | "before" | "outside";
export type KanpoWordStatus = "used" | "margin" | "empty" | "header_label" | "noise" | "ruby";

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
/** 罫線から復元した表のセル。結合セルは rowspan/colspan が 2 以上。x0..y1 はセルの枠（pt）。 */
export type KanpoTableCell = {
  row: number;
  col: number;
  rowspan: number;
  colspan: number;
  x0: number;
  y0: number;
  x1: number;
  y1: number;
  text: string;
  words: number[];
};
/** 罫線から復元した表。縦組みの表は転置済み（右の縦の列が 1 行目、上の欄が 1 列目）。 */
export type KanpoTable = {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
  rows: number;
  cols: number;
  cells: KanpoTableCell[];
};
/** 復元した 1 行。新旧対照表の帯の中の罫線の表は 1 行として持ち、table にセル構造が入る。 */
export type KanpoLine = { text: string; words: number[]; table?: KanpoTable };
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

/** 1 ページ分の罫線（`.rules.json`）。座標は語と同じ（pt、左上原点）。 */
export type KanpoPageRules = {
  vertical: { x: number; y0: number; y1: number }[];
  horizontal: { y: number; x0: number; x1: number }[];
};

/** analyze_item() に渡す 1 ページ分の入力。 */
export type KanpoPageInput = { xhtml: string; rules: KanpoPageRules | null };

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
  ruby: "振り仮名",
};

/** 捨てた語の理由（ホバー時の説明）。 */
export const STATUS_REASON: Record<KanpoWordStatus, string> = {
  used: "どこかの行に組まれた",
  margin: "上下の余白帯（柱）にあるため捨てた",
  empty: "正規化後に空になった（傍線などの私用領域グリフだけの語）",
  header_label: "新旧対照表の欄見出し（改/正/後/前）",
  noise: "柱と判定した行（発行日・号数など）に含まれていた",
  ruby: "振り仮名（親字の脇に小さく組んだ読み）",
};

// ---------------------------------------------------------------------------
// WASM
// ---------------------------------------------------------------------------

export type KanpoParser = {
  analyze: (xhtml: string) => KanpoAnalysis;
  /** 複数ページの項目をまとめて解析する（ページで切れた新旧対照表をつなぎ、罫線から表を復元する）。 */
  analyzeItem: (pages: KanpoPageInput[]) => KanpoAnalysis;
  version: string;
};

/** wasm-bindgen（--target web）が出力するモジュールのうち使うもの。 */
type WasmModule = {
  default: () => Promise<unknown>;
  analyze: (xhtml: string) => string;
  /** 古い WASM には無い（その場合は 1 ページ・罫線なしの analyze で代用する）。 */
  analyze_item?: (pagesJson: string) => string;
  version: () => string;
};

/** analyze() / analyze_item() の JSON を読む（エラーは `{"error": ...}` で返る）。 */
function parseAnalysis(json: string): KanpoAnalysis {
  const v = JSON.parse(json);
  if (v && typeof v.error === "string") throw new Error(v.error);
  return v as KanpoAnalysis;
}

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
            analyze: (xhtml: string) => parseAnalysis(m.analyze(xhtml)),
            analyzeItem: (pages: KanpoPageInput[]) => {
              if (m.analyze_item) return parseAnalysis(m.analyze_item(JSON.stringify(pages)));
              if (pages.length === 1) return parseAnalysis(m.analyze(pages[0].xhtml));
              throw new Error("この WASM は複数ページの解析（analyze_item）に対応していません。pnpm build:wasm で作り直してください。");
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
  /** 複数ページのケースの紙面のページ番号（page.* が先頭、page-2.* … が続き）。 */
  pages?: number[];
  source?: string;
  tags?: string[];
  notes?: string;
};
/** 項目の 1 ページ分の入力ファイル（URL）。 */
export type PageFiles = { bboxUrl: string; pdfUrl: string | null; rulesUrl: string | null };
export type CorpusCase = { id: string; meta: CorpusCaseMeta; pages: PageFiles[] };

// コピーのスクリプトを挟まず、リポジトリのコーパスをそのままビルドに取り込む。
// case.json は中身ごと、pdf / bbox / rules は URL（ビルド時に assets へ出力）として。
// 複数ページのケースは page.* の後に page-2.*, page-3.* … が続く。
const corpusMeta = import.meta.glob<CorpusCaseMeta>("../../../../crates/kanpo-parser/tests/corpus/*/case.json", {
  eager: true,
  import: "default",
});
const corpusPdf = import.meta.glob<string>("../../../../crates/kanpo-parser/tests/corpus/*/page*.pdf", {
  eager: true,
  query: "?url",
  import: "default",
});
const corpusBbox = import.meta.glob<string>("../../../../crates/kanpo-parser/tests/corpus/*/page*.bbox.html", {
  eager: true,
  query: "?url",
  import: "default",
});
const corpusRules = import.meta.glob<string>("../../../../crates/kanpo-parser/tests/corpus/*/page*.rules.json", {
  eager: true,
  query: "?url",
  import: "default",
});

/** glob のキー（相対パス）からケース ID（ディレクトリ名）を取り出す。 */
const caseIdOf = (key: string) => key.split("/").slice(-2, -1)[0];

/** ページの stem（page / page-2 / …）の並び順。page が 1 ページ目。 */
const stemOrder = (stem: string) => (stem === "page" ? 1 : Number(stem.slice("page-".length)) || Infinity);

export const CORPUS_CASES: CorpusCase[] = Object.entries(corpusMeta)
  .flatMap(([key, meta]) => {
    const dir = key.slice(0, key.lastIndexOf("/"));
    const stems = Object.keys(corpusBbox)
      .filter((k) => k.startsWith(`${dir}/`))
      .map((k) => k.slice(dir.length + 1, -".bbox.html".length))
      .filter((stem) => /^page(-\d+)?$/.test(stem))
      .sort((a, b) => stemOrder(a) - stemOrder(b));
    if (!stems.length) return [];
    const pages = stems.map((stem) => ({
      bboxUrl: corpusBbox[`${dir}/${stem}.bbox.html`],
      pdfUrl: corpusPdf[`${dir}/${stem}.pdf`] ?? null,
      rulesUrl: corpusRules[`${dir}/${stem}.rules.json`] ?? null,
    }));
    return [{ id: caseIdOf(key), meta, pages }];
  })
  .sort((a, b) => a.id.localeCompare(b.id));

// ---------------------------------------------------------------------------
// ソース: R2 のアーカイブ（`lawpub kanpo-archive` の出力）
// ---------------------------------------------------------------------------

/** アーカイブのベース URL（未設定ならアーカイブは選べない）。 */
export const ARCHIVE_BASE: string | null = (import.meta.env.VITE_KANPO_ARCHIVE_URL || "").replace(/\/$/, "") || null;

/** `rules` は罫線（.rules.json）があるか。古い manifest には無い（その場合は取得を試す）。 */
export type ArchivePage = { page: number; file: string; sha256: string; bytes: number; bbox: boolean; rules?: boolean };
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

export function archiveRulesUrl(date: string, dir: string, file: string): string {
  return archivePdfUrl(date, dir, file.replace(/\.pdf$/, ".rules.json"));
}

/**
 * 項目（manifest の items[i]）のページ範囲。項目の開始ページから次の項目の開始ページまで
 * （記事が次の項目の先頭ページへはみ出すので次の項目の開始ページも含める）。最後の項目は号の
 * 最終ページまで。長すぎる範囲は ITEM_MAX_PAGES ページで切る。
 */
export const ITEM_MAX_PAGES = 25;
export function itemPageRange(issue: ArchiveIssue, index: number): { first: number; last: number } {
  const item = issue.items[index];
  const lastPage = Math.max(issue.page_count, ...issue.pages.map((p) => p.page));
  const next = issue.items[index + 1]?.page ?? lastPage;
  const first = item.page;
  return { first, last: Math.min(Math.max(first, next), first + ITEM_MAX_PAGES - 1) };
}

/** 号の種類の表示名。 */
export function issueTypeLabel(t: string): string {
  return (
    { regular: "本紙", extra: "号外", special_extra: "特別号外" } as Record<string, string>
  )[t] ?? t;
}

// ---------------------------------------------------------------------------
// 入力の読み込み
// ---------------------------------------------------------------------------

export async function fetchText(url: string): Promise<string> {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${res.status} ${res.statusText}: ${url}`);
  return res.text();
}

/** 罫線（.rules.json）を取得する。無い（404 など）なら null。 */
export async function fetchRules(url: string): Promise<KanpoPageRules | null> {
  try {
    const res = await fetch(url);
    if (!res.ok) return null;
    const v = await res.json();
    return v && Array.isArray(v.vertical) && Array.isArray(v.horizontal) ? (v as KanpoPageRules) : null;
  } catch {
    return null;
  }
}

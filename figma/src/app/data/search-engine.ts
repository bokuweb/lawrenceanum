/**
 * sql.js-httpvfs (WASM SQLite + HTTP Range) によるブラウザ内検索。
 *
 * - search.db は Cloudflare R2 に置く想定 (`VITE_SEARCH_DB_URL` で URL 指定)。
 * - sql.js-httpvfs が SQLite ページ (4KB) を Range fetch するので、1.5GB DB
 *   でも 1 query ≒ 100〜300KB しか DL しない。
 * - VITE_SEARCH_DB_URL 未設定時は同 origin の `./search.db` を使う (小規模時 fallback)。
 *
 * Rust 側の `crates/search-index::tokenize` と完全一致した bigram 分割を
 * クエリにも適用する。
 */

import { createDbWorker, type WorkerHttpvfs } from "sql.js-httpvfs";
// 法令シソーラス (ellisii-toolkit jp-law-thesaurus)。クエリ同義語展開に使う。
import thesaurusData from "./jp-law-thesaurus.json";

export type SearchHit = {
  law_id: string;
  law_num: string | null;
  title: string;
  article_id: string;
  article_no: string;
  caption: string;
  snippet: string;
};

export type ArticleRef = {
  from_law_id: string;
  from_article_id: string;
  to_law_id: string;
  to_article_id: string | null;
  ref_text: string;
  ref_type: string;
};

function isCjk(c: string): boolean {
  const code = c.codePointAt(0);
  if (code === undefined) return false;
  return (
    (code >= 0x3040 && code <= 0x309f) ||
    (code >= 0x30a0 && code <= 0x30ff) ||
    (code >= 0x31f0 && code <= 0x31ff) ||
    (code >= 0x3400 && code <= 0x4dbf) ||
    (code >= 0x4e00 && code <= 0x9fff) ||
    (code >= 0xf900 && code <= 0xfaff) ||
    (code >= 0xff66 && code <= 0xff9d)
  );
}

function isWordChar(c: string): boolean {
  return /\p{L}|\p{N}/u.test(c) || isCjk(c);
}

export function tokenize(text: string): string[] {
  const out: string[] = [];
  let buf = "";
  let bufIsCjk = false;
  const flush = () => {
    if (!buf) return;
    if (bufIsCjk) {
      const chars = Array.from(buf);
      if (chars.length === 1) {
        out.push(chars[0]);
      } else {
        for (let i = 0; i < chars.length - 1; i++) out.push(chars[i] + chars[i + 1]);
      }
    } else {
      out.push(buf.toLowerCase());
    }
    buf = "";
  };
  for (const c of text) {
    if (!isWordChar(c)) {
      flush();
      continue;
    }
    const curIsCjk = isCjk(c);
    if (buf && curIsCjk !== bufIsCjk) flush();
    buf += c;
    bufIsCjk = curIsCjk;
  }
  flush();
  return out;
}

export function tokenizeForFts(text: string): string {
  return tokenize(text).join(" ");
}

/**
 * クエリ文字列を FTS5 の MATCH 式に変換する。
 *
 * content は文字 bigram で索引されているため、1 文字トークンは検索に使えない:
 * - exact (`あ`) → bigram トークンに当たらず ほぼ 0 件
 * - prefix (`あ*`) → prefix index が無いので httpvfs 上で FTS index を
 *   ほぼ全スキャンし 30s 超でハングする
 *
 * よって 1 文字トークンは捨て、2 文字以上のトークン (= bigram) だけ残す。
 * 全トークンが 1 文字なら空文字を返す → 呼び出し側で「2文字以上」を促す。
 */
export function buildFtsMatch(text: string): string {
  return tokenize(text)
    .filter(t => Array.from(t).length >= 2)
    .join(" ");
}

// ── 法令シソーラスによるクエリ同義語展開 ──────────────────────────
// 索引側 (search.db build 時) の同義語追記は法令本文のみ・再ビルドが必要なのに対し、
// クエリ側展開は全 FTS 面 (法令/官報/会議録) に即時効く。入力語に法律 term が含まれて
// いれば、その別表記を OR で足して取りこぼしを減らす。
const THESAURUS_ENTRIES: string[][] = (() => {
  const out: string[][] = [];
  const entries = (thesaurusData as any)?.entries ?? {};
  for (const [k, v] of Object.entries(entries)) {
    if (k.startsWith("_") || typeof v !== "object" || v === null) continue;
    const syns = (v as any).synonyms;
    if (!Array.isArray(syns) || syns.length === 0) continue;
    const forms = [k, ...syns].filter(s => typeof s === "string" && Array.from(s).length >= 2);
    if (forms.length >= 2) out.push(forms);
  }
  return out;
})();

/** クエリに出現した法律 term の「別表記」(原文に無いもの) を返す。UI 表示にも使う。 */
export function synonymExpansions(query: string): string[] {
  const q = query.trim();
  if (!q) return [];
  const extra = new Set<string>();
  for (const forms of THESAURUS_ENTRIES) {
    if (forms.some(f => q.includes(f))) {
      for (const f of forms) if (!q.includes(f)) extra.add(f);
    }
  }
  return [...extra].slice(0, 12); // 暴発防止に上限。
}

/** 原クエリ + 同義語を OR 連結した FTS5 MATCH 式。同義語が無ければ buildFtsMatch と同じ。 */
export function buildFtsMatchExpanded(query: string): string {
  const base = buildFtsMatch(query);
  if (!base) return base;
  const groups = [base, ...synonymExpansions(query).map(buildFtsMatch).filter(Boolean)];
  if (groups.length === 1) return base;
  return groups.map(g => `(${g})`).join(" OR ");
}

/**
 * FTS5 snippet() の出力は事前 bigram トークン化されたテキスト
 * (例: `第三 三十 十一 一条 <mark>民法</mark> 法施 施行 ...`) で読みづらいため、
 * 隣接する CJK bigram のオーバーラップ (= 末尾 1 文字 = 先頭 1 文字) を畳んで
 * `第三十一条<mark>民法</mark>施行...` に復元する。
 *
 * - `<mark>` / `</mark>` は通過させる。直前の bigram と直後の bigram に
 *   挟まれていても overlap 判定は維持するので、mark 跨ぎでも崩れない。
 * - ASCII 単語同士は半角空白で区切り直す (元のスペースは separator として捨てる)。
 * - `...` (snippet の省略マーカ) は contiguity を切る。
 */
export function unbigramSnippet(s: string): string {
  type Tok = { kind: "text" | "mark"; v: string };
  const toks: Tok[] = [];
  let i = 0;
  while (i < s.length) {
    if (s.startsWith("<mark>", i)) { toks.push({ kind: "mark", v: "<mark>" }); i += 6; continue; }
    if (s.startsWith("</mark>", i)) { toks.push({ kind: "mark", v: "</mark>" }); i += 7; continue; }
    if (s[i] === " ") { i++; continue; }
    // ellipsis marker "..." は前後にスペースが入らないので、bigram にくっついた
     // `タル...` のような塊を `タル` + `...` に切る。
    if (s.startsWith("...", i)) { toks.push({ kind: "text", v: "..." }); i += 3; continue; }
    let j = i;
    while (
      j < s.length &&
      s[j] !== " " &&
      !s.startsWith("<mark>", j) &&
      !s.startsWith("</mark>", j) &&
      !s.startsWith("...", j)
    ) j++;
    toks.push({ kind: "text", v: s.slice(i, j) });
    i = j;
  }
  const isCjkBigram = (t: string): boolean => {
    const chars = Array.from(t);
    return chars.length === 2 && isCjk(chars[0]) && isCjk(chars[1]);
  };
  let out = "";
  let prev = "";
  for (const t of toks) {
    if (t.kind === "mark") { out += t.v; continue; }
    if (t.v === "...") { out += t.v; prev = ""; continue; }
    if (isCjkBigram(t.v) && isCjkBigram(prev) && prev[1] === t.v[0]) {
      out += t.v[1];
    } else {
      const lastCh = out.length > 0 ? out[out.length - 1] : "";
      if (lastCh && /[A-Za-z0-9]/.test(lastCh) && /[A-Za-z0-9]/.test(t.v[0])) out += " ";
      out += t.v;
    }
    prev = t.v;
  }
  return out;
}

/** sql.js-httpvfs のワーカーを 1 DB につき 1 つ遅延生成する。 */
function workerLoader(envUrl: string | undefined, fallbackPath: string, label: string) {
  let promise: Promise<WorkerHttpvfs | null> | null = null;
  return (): Promise<WorkerHttpvfs | null> => {
    if (!promise) {
      promise = (async () => {
        try {
          // wasm/worker は sql.js-httpvfs の dist を Vite が ?url で解決 → 同一 host bundle に含める。
          // DB 本体は環境変数の URL (R2 等) を優先、未設定なら同 origin の相対パス。
          const wasmUrl = (await import("sql.js-httpvfs/dist/sql-wasm.wasm?url")).default;
          const workerUrl = (await import("sql.js-httpvfs/dist/sqlite.worker.js?url")).default;
          const dbUrl = envUrl || new URL(fallbackPath, document.baseURI).toString();
          const worker = await createDbWorker(
            [
              {
                from: "inline",
                config: {
                  serverMode: "full",
                  // DB の 64KB page_size に合わせ、1 回の Range で
                  // 必要な SQLite ページを過不足なく取得する。
                  requestChunkSize: 65536,
                  url: dbUrl,
                },
              },
            ],
            workerUrl,
            wasmUrl,
          );
          // Pre-warm: prime the SQLite page cache with a lightweight query so the
          // first real search doesn't pay cold-start cost.
          await (worker.db.query as any)("SELECT 1");
          return worker;
        } catch (e) {
          console.warn(`[search] httpvfs init failed (${label})`, e);
          return null;
        }
      })();
    }
    return promise;
  };
}

const loadWorker = workerLoader((import.meta as any).env?.VITE_SEARCH_DB_URL, "./search.db", "search.db");
// 自治体例規は件数が法令の 100 倍規模なので別 DB (reiki-search.db) に分けている。
const loadReikiWorker = workerLoader(
  (import.meta as any).env?.VITE_REIKI_SEARCH_DB_URL,
  "./reiki-search.db",
  "reiki-search.db",
);

export async function isAvailable(): Promise<boolean> {
  return (await loadWorker()) !== null;
}

// 静的DBの同一検索は再利用する。進行中の検索も共有し、戻る操作や再入力で
// Worker の待ち行列と HTTP Range 読みを増やさない。DB ごとに直近32検索だけ保持。
const queryCaches = new WeakMap<WorkerHttpvfs, Map<string, Promise<unknown[]>>>();
async function queryWorker<T>(w: WorkerHttpvfs, sql: string, params: unknown[]): Promise<T[]> {
  if (!sql.includes(" MATCH ")) return (await (w.db.query as any)(sql, params)) as T[];
  let cache = queryCaches.get(w);
  if (!cache) { cache = new Map(); queryCaches.set(w, cache); }
  const key = JSON.stringify([sql, params]);
  const existing = cache.get(key);
  if (existing) {
    cache.delete(key);
    cache.set(key, existing);
    return existing as Promise<T[]>;
  }
  const result = Promise.resolve((w.db.query as any)(sql, params)).catch(error => {
    if (cache.get(key) === result) cache.delete(key);
    throw error;
  });
  cache.set(key, result);
  if (cache.size > 32) cache.delete(cache.keys().next().value!);
  return result as Promise<T[]>;
}

async function exec<T = Record<string, unknown>>(sql: string, params: unknown[] = []): Promise<T[]> {
  const w = await loadWorker();
  if (!w) return [];
  // sql.js-httpvfs の db.query は `(sql, params[]) => row[]`。配列で渡す
  // (spread すると bind が効かず fts5 が空文字 MATCH を見て syntax error)。
  return queryWorker<T>(w, sql, params);
}

export type DocumentKind = "pubcomment" | "gian" | "shingikai";
export type DocumentHit = {
  kind: DocumentKind;
  document_id: string;
  title: string;
  subtitle: string;
  route: string;
  snippet: string;
};

/** 収集済みのパブコメ・議案・審議会（添付の抽出本文を含む）。 */
export async function searchDocuments(q: string, kind: DocumentKind, limit = 10, strict = false, offset = 0): Promise<DocumentHit[]> {
  const match = buildFtsMatchExpanded(q.trim());
  if (!match) return [];
  try {
    return await exec<DocumentHit>(
      `SELECT kind, document_id, title, subtitle, route,
              snippet(documents_fts, 6, '<mark>', '</mark>', '...', 12) AS snippet
         FROM documents_fts
        WHERE documents_fts MATCH ? AND kind = ?
        ORDER BY rank, rowid LIMIT ? OFFSET ?`,
      [match, kind, limit, offset],
    );
  } catch (error) {
    if (strict) throw error;
    // 再ビルド前のDBでも既存の検索対象を利用できる。
    if (String(error).includes("no such table")) return [];
    throw error;
  }
}

/** search.db の `laws.category` に存在する e-Gov 法令分類を昇順で返す。 */
export async function getCategories(): Promise<string[]> {
  const rows = await exec<{ category: string }>(
    `SELECT DISTINCT category FROM laws
      WHERE category IS NOT NULL AND category <> ''
      ORDER BY category`,
  );
  return rows.map(r => String(r.category));
}

export async function search(
  q: string,
  limit = 50,
  categories: string[] = [],
  offset = 0,
): Promise<SearchHit[]> {
  // bigram index に合わせ、2文字未満の検索語は対象外。
  const match = buildFtsMatchExpanded(q.trim());
  if (!match) return [];
  // カテゴリ絞り込み: 選択があれば l.category IN (?, ?, ...) を足す。
  const catFilter =
    categories.length > 0
      ? ` AND l.category IN (${categories.map(() => "?").join(",")})`
      : "";
  const rows = await exec<{
    law_id: string; article_id: string; article_no: string; caption: string;
    title: string; law_num: string | null; snippet: string;
  }>(
    `SELECT s.law_id, s.article_id, s.article_no, s.caption,
            l.title, l.law_num,
            snippet(search_fts, 5, '<mark>', '</mark>', '...', 8) AS snippet
       FROM search_fts s
       JOIN laws l ON l.law_id = s.law_id
      WHERE search_fts MATCH ?${catFilter}
      ORDER BY rank, s.rowid
      LIMIT ? OFFSET ?`,
    [match, ...categories, limit, offset],
  );
  return rows.map(r => ({
    law_id: String(r.law_id ?? ""),
    law_num: r.law_num ?? null,
    title: String(r.title ?? ""),
    article_id: String(r.article_id ?? ""),
    article_no: String(r.article_no ?? ""),
    caption: String(r.caption ?? ""),
    snippet: String(r.snippet ?? ""),
  }));
}

export type SpeechHit = {
  meeting_id: string;
  speech_id: string;
  speaker: string | null;
  speaker_group: string | null;
  snippet: string;
  house: string;
  committee: string | null;
  date: string;
  session: number;
};

export async function searchSpeeches(q: string, limit = 20, offset = 0): Promise<SpeechHit[]> {
  const match = buildFtsMatchExpanded(q.trim());
  if (!match) return [];
  const rows = await exec<{
    meeting_id: string; speech_id: string; speaker: string | null;
    speaker_group: string | null; snippet: string;
    house: string; committee: string | null; date: string; session: number;
  }>(
    `SELECT s.meeting_id, s.speech_id, s.speaker, s.speaker_group,
            snippet(speeches_fts, 4, '<mark>', '</mark>', '...', 10) AS snippet,
            m.house, m.committee, m.date, m.session
       FROM speeches_fts s
       JOIN meetings m ON m.meeting_id = s.meeting_id
      WHERE speeches_fts MATCH ?
      ORDER BY rank, s.rowid
      LIMIT ? OFFSET ?`,
    [match, limit, offset],
  );
  return rows.map(r => ({
    meeting_id: String(r.meeting_id ?? ""),
    speech_id: String(r.speech_id ?? ""),
    speaker: r.speaker ?? null,
    speaker_group: r.speaker_group ?? null,
    snippet: String(r.snippet ?? ""),
    house: String(r.house ?? ""),
    committee: r.committee ?? null,
    date: String(r.date ?? ""),
    session: Number(r.session ?? 0),
  }));
}

export type KanpoHit = {
  date: string;
  issue_no: string;
  title: string;
  page: number;
  pdf_url: string;
  agency: string | null;
  snippet: string;
  /** 逆引き: この改め文が改正する対象法令 (あれば)。 */
  law_id: string | null;
  law_title: string | null;
};

/**
 * 官報記事の全文検索 (kanpo_fts)。改め文 (amend_text) と記事タイトルを横断する。
 * 旧 search.db（kanpo_fts 未作成）では "no such table" になるため空配列にフォールバックする。
 */
export async function searchKanpo(q: string, limit = 10, strict = false, offset = 0): Promise<KanpoHit[]> {
  const match = buildFtsMatchExpanded(q.trim());
  if (!match) return [];
  try {
    const rows = await exec<{
      date: string; issue_no: string; title: string; page: number;
      pdf_url: string; agency: string | null; snippet: string;
      law_id: string | null; law_title: string | null;
    }>(
      `SELECT date, issue_no, title, page, pdf_url, agency, law_id, law_title,
              snippet(kanpo_fts, 7, '<mark>', '</mark>', '...', 10) AS snippet
         FROM kanpo_fts
        WHERE kanpo_fts MATCH ?
        ORDER BY rank, rowid
        LIMIT ? OFFSET ?`,
      [match, limit, offset],
    );
    return rows.map(r => ({
      date: String(r.date ?? ""),
      issue_no: String(r.issue_no ?? ""),
      title: String(r.title ?? ""),
      page: Number(r.page ?? 0),
      pdf_url: String(r.pdf_url ?? ""),
      agency: r.agency ?? null,
      snippet: String(r.snippet ?? ""),
      law_id: r.law_id ? String(r.law_id) : null,
      law_title: r.law_title ? String(r.law_title) : null,
    }));
  } catch (error) {
    if (strict) throw error;
    return [];
  }
}

export type TsutatsuHit = {
  tax: string;
  number: string;
  caption: string | null;
  set_name: string | null;
  source_url: string;
  snippet: string;
};

/**
 * 通達 (soft law) の全文検索 (tsutatsu_fts)。番号・見出し・本文を横断。
 * 旧 search.db（tsutatsu_fts 未作成）では空配列にフォールバック。
 */
export async function searchTsutatsu(q: string, limit = 10, strict = false, offset = 0): Promise<TsutatsuHit[]> {
  const match = buildFtsMatchExpanded(q.trim());
  if (!match) return [];
  try {
    const rows = await exec<{
      tax: string; number: string; caption: string | null;
      set_name: string | null; source_url: string; snippet: string;
    }>(
      `SELECT tax, number, caption, set_name, source_url,
              snippet(tsutatsu_fts, 6, '<mark>', '</mark>', '...', 10) AS snippet
         FROM tsutatsu_fts
        WHERE tsutatsu_fts MATCH ?
        ORDER BY rank, rowid
        LIMIT ? OFFSET ?`,
      [match, limit, offset],
    );
    return rows.map(r => ({
      tax: String(r.tax ?? ""),
      number: String(r.number ?? ""),
      caption: r.caption ?? null,
      set_name: r.set_name ?? null,
      source_url: String(r.source_url ?? ""),
      snippet: String(r.snippet ?? ""),
    }));
  } catch (error) {
    if (strict) throw error;
    return [];
  }
}

export type ReikiHit = {
  municipality_code: string;
  municipality_name: string;
  prefecture: string;
  reiki_id: string;
  title: string;
  reiki_number: string | null;
  /** 題名で当たった行は空文字 */
  article_id: string;
  article_no: string;
  caption: string | null;
  excerpt: string;
};

export type ReikiScope =
  | { kind: "all" }
  | { kind: "prefecture"; prefecture: string }
  | { kind: "municipality"; code: string };

/**
 * 自治体例規の全文検索 (reiki-search.db)。
 *
 * - 行は自治体コード順に並んでいるので、県・自治体の絞り込みは `reiki_municipalities` の
 *   rowid 範囲を FTS5 に渡して Range 読みの量を抑える。
 * - 全国で数十万件に当たる語でも重くならないよう、MATCH の候補を先頭 `scan` 件に
 *   限ってから rank で並べる（候補の外側は「さらに絞り込む」で辿る想定）。
 * - `titleOnly` は題名だけを対象にする（他自治体の同種例規を探す用途）。
 */
export async function searchReiki(
  q: string,
  opts: { scope?: ReikiScope; titleOnly?: boolean; limit?: number; scan?: number; strict?: boolean; offset?: number } = {},
): Promise<ReikiHit[]> {
  const base = buildFtsMatchExpanded(q.trim());
  if (!base) return [];
  const w = await loadReikiWorker();
  if (!w) {
    if (opts.strict) throw new Error("自治体例規の検索 DB を利用できません。");
    return [];
  }
  const query = async <T,>(sql: string, params: unknown[]): Promise<T[]> =>
    queryWorker<T>(w, sql, params);
  const { scope = { kind: "all" }, titleOnly = false, limit = 30, scan = 2000, offset = 0 } = opts;
  try {
    let lo = 0;
    let hi = Number.MAX_SAFE_INTEGER;
    if (scope.kind !== "all") {
      const where = scope.kind === "prefecture" ? "prefecture = ?" : "municipality_code = ?";
      const arg = scope.kind === "prefecture" ? scope.prefecture : scope.code;
      const r = await query<{ lo: number | null; hi: number | null }>(
        `SELECT min(min_rowid) AS lo, max(max_rowid) AS hi FROM reiki_municipalities WHERE ${where}`,
        [arg],
      );
      if (!r[0] || r[0].lo == null || r[0].hi == null) return [];
      lo = r[0].lo;
      hi = r[0].hi;
    }
    const match = titleOnly ? `title_tokens : (${base})` : base;
    const rows = await query<ReikiHit>(
      `SELECT d.municipality_code, d.municipality_name, d.prefecture, d.reiki_id, d.title,
              d.reiki_number, m.article_id, m.article_no, m.caption, m.excerpt
         FROM (SELECT rowid, rank FROM reiki_fts
                WHERE reiki_fts MATCH ? AND rowid BETWEEN ? AND ?
                LIMIT ?) f
         JOIN reiki_fts_meta m ON m.rowid = f.rowid
         JOIN reiki_docs d ON d.id = m.doc_id
        ORDER BY f.rank, f.rowid
        LIMIT ? OFFSET ?`,
      [match, lo, hi, scan, limit, offset],
    );
    return rows.map(r => ({
      municipality_code: String(r.municipality_code ?? ""),
      municipality_name: String(r.municipality_name ?? ""),
      prefecture: String(r.prefecture ?? ""),
      reiki_id: String(r.reiki_id ?? ""),
      title: String(r.title ?? ""),
      reiki_number: r.reiki_number ?? null,
      article_id: String(r.article_id ?? ""),
      article_no: String(r.article_no ?? ""),
      caption: r.caption ?? null,
      excerpt: String(r.excerpt ?? ""),
    }));
  } catch (e) {
    if (opts.strict) throw e;
    console.warn("[search] reiki query failed", e);
    return [];
  }
}

/** 例規題名から先頭の自治体名を外した「種類名」（他自治体の同種例規を探す検索語）。 */
export function reikiGenericTitle(title: string, municipalityName: string): string {
  const t = title.trim();
  if (municipalityName && t.startsWith(municipalityName)) return t.slice(municipalityName.length).trim() || t;
  return t;
}

export async function getOutgoingRefs(lawId: string, articleId?: string): Promise<ArticleRef[]> {
  return exec<ArticleRef>(
    `SELECT from_law_id, from_article_id, to_law_id, to_article_id, ref_text, ref_type
       FROM refs WHERE from_law_id = ?${articleId === undefined ? '' : ' AND from_article_id = ?'} ORDER BY id`,
    articleId === undefined ? [lawId] : [lawId, articleId],
  );
}

export async function getIncomingRefs(lawId: string, articleId?: string): Promise<ArticleRef[]> {
  return exec<ArticleRef>(
    `SELECT from_law_id, from_article_id, to_law_id, to_article_id, ref_text, ref_type
       FROM refs WHERE to_law_id = ?${articleId === undefined ? '' : ' AND to_article_id = ?'} ORDER BY id`,
    articleId === undefined ? [lawId] : [lawId, articleId],
  );
}

export async function getRefsForLaw(lawId: string): Promise<ArticleRef[]> {
  return exec<ArticleRef>(
    `SELECT from_law_id, from_article_id, to_law_id, to_article_id, ref_text, ref_type
       FROM refs WHERE from_law_id = ? OR to_law_id = ? ORDER BY id`,
    [lawId, lawId],
  );
}

export async function getMeta(): Promise<Record<string, string> | null> {
  const w = await loadWorker();
  if (!w) return null;
  const rows = await (w.db.query as any)(`SELECT key, value FROM meta`);
  const meta: Record<string, string> = {};
  for (const r of rows as { key: string; value: string }[]) meta[r.key] = r.value;
  return meta;
}

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

let workerPromise: Promise<WorkerHttpvfs | null> | null = null;

async function loadWorker(): Promise<WorkerHttpvfs | null> {
  if (!workerPromise) {
    workerPromise = (async () => {
      try {
        // wasm/worker は sql.js-httpvfs の dist を Vite が ?url で解決 → 同一 host bundle に含める。
        // search.db は VITE_SEARCH_DB_URL (R2 等) を優先、未設定なら同 origin の ./search.db。
        const wasmUrl = (await import("sql.js-httpvfs/dist/sql-wasm.wasm?url")).default;
        const workerUrl = (await import("sql.js-httpvfs/dist/sqlite.worker.js?url")).default;
        const dbUrl =
          (import.meta as any).env?.VITE_SEARCH_DB_URL ||
          new URL("./search.db", document.baseURI).toString();
        const worker = await createDbWorker(
          [
            {
              from: "inline",
              config: {
                serverMode: "full",
                // 64KB/chunk: FTS5 B-tree traversal typically needs 10-30 pages.
                // At 4KB that's 10-30 HTTP round trips (~50ms each on R2).
                // At 64KB each request covers 16 SQLite pages, cutting trips by ~16x.
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
        console.warn("[search] httpvfs init failed", e);
        return null;
      }
    })();
  }
  return workerPromise;
}

export async function isAvailable(): Promise<boolean> {
  return (await loadWorker()) !== null;
}

async function exec<T = Record<string, unknown>>(sql: string, params: unknown[] = []): Promise<T[]> {
  const w = await loadWorker();
  if (!w) return [];
  // sql.js-httpvfs の db.query は `(sql, params[]) => row[]`。配列で渡す
  // (spread すると bind が効かず fts5 が空文字 MATCH を見て syntax error)。
  return (await (w.db.query as any)(sql, params)) as T[];
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
): Promise<SearchHit[]> {
  // 1 文字クエリは prefix (`あ*`) になる。exact だと bigram index に当たらない。
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
      ORDER BY rank
      LIMIT ?`,
    [match, ...categories, limit],
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

export async function searchSpeeches(q: string, limit = 20): Promise<SpeechHit[]> {
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
      ORDER BY rank
      LIMIT ?`,
    [match, limit],
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
export async function searchKanpo(q: string, limit = 10): Promise<KanpoHit[]> {
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
        ORDER BY rank
        LIMIT ?`,
      [match, limit],
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
  } catch {
    return [];
  }
}

export async function getOutgoingRefs(lawId: string, articleId: string): Promise<ArticleRef[]> {
  return exec<ArticleRef>(
    `SELECT from_law_id, from_article_id, to_law_id, to_article_id, ref_text, ref_type
       FROM refs WHERE from_law_id = ? AND from_article_id = ? ORDER BY id`,
    [lawId, articleId],
  );
}

export async function getIncomingRefs(lawId: string, articleId: string): Promise<ArticleRef[]> {
  return exec<ArticleRef>(
    `SELECT from_law_id, from_article_id, to_law_id, to_article_id, ref_text, ref_type
       FROM refs WHERE to_law_id = ? AND to_article_id = ? ORDER BY id`,
    [lawId, articleId],
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

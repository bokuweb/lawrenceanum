/**
 * sql.js-httpvfs (WASM SQLite + HTTP Range) によるブラウザ内検索。
 *
 * - search.db は Cloudflare R2 に置く想定 (`VITE_SEARCH_DB_URL` で URL 指定)。
 * - sql.js-httpvfs が必要な SQLite ページを HTTP Range で取得する。
 * - 全ヒットの採点を避け、索引順で必要件数だけ読み出す。
 * - VITE_SEARCH_DB_URL 未設定時は同 origin の `./search.db` を使う (小規模時 fallback)。
 *
 * Rust 側の `crates/search-index::tokenize` と完全一致した bigram 分割を
 * クエリにも適用する。
 */

import { SearchWorker, type SearchRequest } from "./search-worker";
import { buildFtsMatchExpanded } from "./search-query";
export { tokenize, tokenizeForFts, buildFtsMatch, buildFtsMatchExpanded, synonymExpansions, unbigramSnippet } from "./search-query";

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

// 各対象を独立したWorkerで検索する。実行中の古い検索はAbortSignalで停止できる。
const workers = new Map<string, SearchWorker>();
function client(source: string): SearchWorker {
  let worker = workers.get(source);
  if (!worker) {
    const reiki = source === "reiki";
    const url = reiki ? (import.meta as any).env?.VITE_REIKI_SEARCH_DB_URL : (import.meta as any).env?.VITE_SEARCH_DB_URL;
    worker = new SearchWorker(url || new URL(reiki ? "./reiki-search.db" : "./search.db", document.baseURI).toString());
    workers.set(source, worker);
  }
  return worker;
}

export async function isAvailable(): Promise<boolean> {
  try { await client("laws").query("SELECT 1"); return true; } catch { return false; }
}

async function exec<T = Record<string, unknown>>(sql: string, params: unknown[] = [], request?: SearchRequest): Promise<T[]> {
  const source = sql.includes("documents_fts") ? String(params[1])
    : sql.includes("speeches_fts") ? "speeches"
    : sql.includes("kanpo_fts") ? "kanpo"
    : sql.includes("tsutatsu_fts") ? "tsutatsu"
    : sql.includes("laws_en_fts") ? "english"
    : sql.includes("hanrei_fts") ? "hanrei" : "laws";
  return client(source).query<T>(sql, params, request);
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
export async function searchDocuments(q: string, kind: DocumentKind, limit = 10, strict = false, offset = 0, request?: SearchRequest & { includeSnippet?: boolean }): Promise<DocumentHit[]> {
  const match = buildFtsMatchExpanded(q.trim());
  if (!match) return [];
  // build-search-db inserts each kind contiguously and records its row count.
  // Constrain the FTS cursor before reading content: kind is UNINDEXED, so
  // filtering it alone would fetch large PDF bodies from unrelated kinds.
  const kinds = ["pubcomment", "gian", "shingikai"];
  const position = kinds.indexOf(kind);
  const keys = (end: number) => kinds.slice(0, end).map(k => `'${k}_count'`).join(",") || "NULL";
  const bound = (end: number, fallback: string) => `(SELECT COALESCE(SUM(CAST(value AS INTEGER)), ${fallback}) FROM meta WHERE key IN (${keys(end)}))`;
  const range = `rowid BETWEEN ${bound(position, "0")} + 1 AND ${bound(position + 1, "9007199254740991")}`;
  try {
    if (request?.includeSnippet === false) {
      // FTS xColumn loads all indexed text, including multi-megabyte PDFs.
      // Read only the compact leading columns of its backing content record.
      return await client(kind).query<DocumentHit>(
        `SELECT c.c0 AS kind, c.c1 AS document_id, c.c2 AS title,
                c.c3 AS subtitle, c.c4 AS route, '' AS snippet
           FROM (SELECT rowid FROM documents_fts
                  WHERE documents_fts MATCH ? AND ${range}
                  ORDER BY rowid LIMIT ? OFFSET ?) f
           JOIN documents_fts_content c ON c.id = f.rowid
          WHERE c.c0 = ? ORDER BY f.rowid`,
        [match, limit, offset, kind], request,
      );
    }
    return await client(kind).query<DocumentHit>(
      `SELECT kind, document_id, title, subtitle, route,
              snippet(documents_fts, 6, '<mark>', '</mark>', '...', 12) AS snippet
         FROM documents_fts
        WHERE documents_fts MATCH ? AND kind = ?
          AND ${range}
        ORDER BY rowid LIMIT ? OFFSET ?`,
      [match, kind, limit, offset], request,
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
  request?: SearchRequest,
): Promise<SearchHit[]> {
  // bigram index に合わせ、2文字未満の検索語は対象外。
  const match = buildFtsMatchExpanded(q.trim());
  if (!match) return [];
  // カテゴリ絞り込み: 選択があれば l.category IN (?, ?, ...) を足す。
  const catFilter =
    categories.length > 0
      ? ` AND l.category IN (${categories.map(() => "?").join(",")})`
      : "";
  // Reading metadata via FTS xColumn also loads the full body.
  // The UI reads leading columns from the backing record instead; MATCH still
  // covers all text. API callers keep snippets unless explicitly disabled.
  const compact = request?.includeSnippet === false;
  const rows = await exec<{
    law_id: string; article_id: string; article_no: string; caption: string;
    title: string; law_num: string | null; snippet: string;
  }>(
    `SELECT ${compact ? "c.c0 AS law_id, c.c1 AS article_id, c.c2 AS article_no, c.c3 AS caption" : "s.law_id, s.article_id, s.article_no, s.caption"},
            l.title, l.law_num,
            ${compact ? "''" : "snippet(search_fts, 5, '<mark>', '</mark>', '...', 8)"} AS snippet
       FROM search_fts s
       ${compact ? "JOIN search_fts_content c ON c.id = s.rowid" : ""}
       JOIN laws l ON l.law_id = ${compact ? "c.c0" : "s.law_id"}
      WHERE search_fts MATCH ?${catFilter}
      ORDER BY s.rowid
      LIMIT ? OFFSET ?`,
    [match, ...categories, limit, offset], request,
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

export async function searchSpeeches(q: string, limit = 20, offset = 0, request?: SearchRequest): Promise<SpeechHit[]> {
  const match = buildFtsMatchExpanded(q.trim());
  if (!match) return [];
  const compact = request?.includeSnippet === false;
  const rows = await exec<{
    meeting_id: string; speech_id: string; speaker: string | null;
    speaker_group: string | null; snippet: string;
    house: string; committee: string | null; date: string; session: number;
  }>(
    `SELECT ${compact ? "c.c0 AS meeting_id, c.c1 AS speech_id, c.c2 AS speaker, c.c3 AS speaker_group" : "s.meeting_id, s.speech_id, s.speaker, s.speaker_group"},
            ${compact ? "''" : "snippet(speeches_fts, 4, '<mark>', '</mark>', '...', 10)"} AS snippet,
            m.house, m.committee, m.date, m.session
       FROM speeches_fts s
       ${compact ? "JOIN speeches_fts_content c ON c.id = s.rowid" : ""}
       JOIN meetings m ON m.meeting_id = ${compact ? "c.c0" : "s.meeting_id"}
      WHERE speeches_fts MATCH ?
      ORDER BY s.rowid
      LIMIT ? OFFSET ?`,
    [match, limit, offset], request,
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
export async function searchKanpo(q: string, limit = 10, strict = false, offset = 0, request?: SearchRequest): Promise<KanpoHit[]> {
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
        ORDER BY rowid
        LIMIT ? OFFSET ?`,
      [match, limit, offset], request,
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
export async function searchTsutatsu(q: string, limit = 10, strict = false, offset = 0, request?: SearchRequest): Promise<TsutatsuHit[]> {
  const match = buildFtsMatchExpanded(q.trim());
  if (!match) return [];
  const compact = request?.includeSnippet === false;
  try {
    const rows = await exec<{
      tax: string; number: string; caption: string | null;
      set_name: string | null; source_url: string; snippet: string;
    }>(
      `SELECT ${compact ? "c.c0 AS tax, c.c1 AS number, c.c2 AS caption, c.c3 AS set_name, c.c4 AS source_url" : "tax, number, caption, set_name, source_url"},
              ${compact ? "''" : "snippet(tsutatsu_fts, 6, '<mark>', '</mark>', '...', 10)"} AS snippet
         FROM tsutatsu_fts
         ${compact ? "JOIN tsutatsu_fts_content c ON c.id = tsutatsu_fts.rowid" : ""}
        WHERE tsutatsu_fts MATCH ?
        ORDER BY tsutatsu_fts.rowid
        LIMIT ? OFFSET ?`,
      [match, limit, offset], request,
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

export type EnglishHit = {
  law_id: string;
  article_id: string;
  article_no: string;
  /** 英訳の法令名 (例: "Civil Code")。 */
  law_title: string;
  /** JLT の該当条 URL。 */
  url: string;
  title: string;
  caption: string;
  /** HTML エスケープ済み。一致箇所だけ <mark> で囲む。 */
  snippet: string;
};

/** 英語クエリの AND から外す機能語 ("guardian of minors" → guardian AND minors)。 */
const ENGLISH_STOPWORDS = new Set([
  "a", "an", "the", "of", "to", "in", "on", "for", "by", "with", "as", "at", "from",
  "and", "or", "is", "are", "be", "that", "this", "which", "its",
]);

/** 英語クエリを FTS5 の MATCH 式にする (英単語だけを拾って AND)。英単語が無ければ空文字。 */
export function buildEnglishMatch(q: string): string {
  const words: string[] = Array.from(q.toLowerCase().match(/[a-z0-9]+(?:[-'][a-z0-9]+)*/g) ?? []);
  const useful = words.filter(w => /[a-z]/.test(w) && w.length >= 2 && !ENGLISH_STOPWORDS.has(w));
  return useful.map(w => `"${w.replace(/"/g, "")}"`).join(" ");
}

const escapeHtml = (s: string) =>
  s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

/**
 * 法令英訳 (日本法令外国語訳データベース) の条単位英語検索 (laws_en_fts, porter 語幹化)。
 * 旧 search.db（laws_en_fts 未作成）や英単語を含まないクエリでは空配列。
 */
export async function searchEnglish(q: string, limit = 10, strict = false, offset = 0, request?: SearchRequest): Promise<EnglishHit[]> {
  const match = buildEnglishMatch(q);
  if (!match) return [];
  try {
    const rows = await exec<{
      law_id: string; article_id: string; article_no: string | null; law_title: string | null;
      url: string | null; title_en: string | null; caption_en: string | null; snippet: string | null;
    }>(
      `SELECT law_id, article_id, article_no, law_title, url, title_en, caption_en,
              snippet(laws_en_fts, 7, char(1), char(2), '...', 16) AS snippet
         FROM laws_en_fts
        WHERE laws_en_fts MATCH ?
        ORDER BY rank
        LIMIT ? OFFSET ?`,
      [match, limit, offset], request,
    );
    return rows.map(r => {
      const lawTitle = String(r.law_title ?? "");
      const title = String(r.title_en ?? "");
      return {
        law_id: String(r.law_id ?? ""),
        article_id: String(r.article_id ?? ""),
        article_no: String(r.article_no ?? ""),
        law_title: lawTitle,
        url: String(r.url ?? ""),
        // title_en 列は「法令名 + 条名」で索引しているので、表示は条名だけにする。
        title: lawTitle && title.startsWith(lawTitle) ? title.slice(lawTitle.length).trim() : title,
        caption: String(r.caption_en ?? ""),
        snippet: escapeHtml(String(r.snippet ?? "")).replace(/\u0001/g, "<mark>").replace(/\u0002/g, "</mark>"),
      };
    });
  } catch (error) {
    if (strict) throw error;
    return [];
  }
}

export type HanreiHit = {
  case_id: number;
  case_number: string;
  case_name: string;
  /** 和暦のまま (例: 令和8年8月28日)。 */
  judged_on: string;
  court: string;
  result: string;
  /** 判示事項 (原文のまま)。 */
  holding: string;
  /** 出典: 裁判所 裁判例検索の詳細ページ。 */
  detail_url: string;
  pdf_url: string | null;
  snippet: string;
};

/**
 * 判例の全文検索 (hanrei_fts)。事件名・事件番号・判示事項・裁判要旨・参照法条・
 * 全文 PDF 本文と引用を横断する。索引は新しい事件順 (case_id 降順) に並ぶ。
 * 旧 search.db（hanrei_fts 未作成）では strict でも空配列にフォールバック。
 */
export async function searchHanrei(q: string, limit = 10, strict = false, offset = 0, request?: SearchRequest): Promise<HanreiHit[]> {
  const match = buildFtsMatchExpanded(q.trim());
  if (!match) return [];
  const compact = request?.includeSnippet === false;
  try {
    const rows = await exec<{
      case_id: number; case_number: string; case_name: string; judged_on: string;
      court: string; result: string; holding: string; detail_url: string;
      pdf_url: string | null; snippet: string;
    }>(
      `SELECT ${compact
        ? "c.c0 AS case_id, c.c1 AS case_number, c.c2 AS case_name, c.c3 AS judged_on, c.c4 AS court, c.c5 AS result, c.c6 AS holding, c.c7 AS detail_url, c.c8 AS pdf_url"
        : "case_id, case_number, case_name, judged_on, court, result, holding, detail_url, pdf_url"},
              ${compact ? "''" : "snippet(hanrei_fts, -1, '<mark>', '</mark>', '...', 10)"} AS snippet
         FROM hanrei_fts
         ${compact ? "JOIN hanrei_fts_content c ON c.id = hanrei_fts.rowid" : ""}
        WHERE hanrei_fts MATCH ?
        ORDER BY hanrei_fts.rowid
        LIMIT ? OFFSET ?`,
      [match, limit, offset], request,
    );
    return rows.map(r => ({
      case_id: Number(r.case_id ?? 0),
      case_number: String(r.case_number ?? ""),
      case_name: String(r.case_name ?? ""),
      judged_on: String(r.judged_on ?? ""),
      court: String(r.court ?? ""),
      result: String(r.result ?? ""),
      holding: String(r.holding ?? ""),
      detail_url: String(r.detail_url ?? ""),
      pdf_url: r.pdf_url ? String(r.pdf_url) : null,
      snippet: String(r.snippet ?? ""),
    }));
  } catch (error) {
    // hanrei_fts 追加前に作られた search.db では判例タブを空にするだけでエラーにしない。
    if (String(error).includes("no such table")) return [];
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
 *   限る（候補の外側は「さらに絞り込む」で辿る想定）。索引順で採点を避ける。
 * - `titleOnly` は題名だけを対象にする（他自治体の同種例規を探す用途）。
 */
export async function searchReiki(
  q: string,
  opts: { scope?: ReikiScope; titleOnly?: boolean; limit?: number; scan?: number; strict?: boolean; offset?: number; signal?: AbortSignal; timeoutMs?: number } = {},
): Promise<ReikiHit[]> {
  const base = buildFtsMatchExpanded(q.trim());
  if (!base) return [];
  const query = async <T,>(sql: string, params: unknown[]): Promise<T[]> =>
    client("reiki").query<T>(sql, params, opts);
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
         FROM (SELECT rowid FROM reiki_fts
                WHERE reiki_fts MATCH ? AND rowid BETWEEN ? AND ?
                LIMIT ?) f
         JOIN reiki_fts_meta m ON m.rowid = f.rowid
         JOIN reiki_docs d ON d.id = m.doc_id
        ORDER BY f.rowid
        LIMIT ? OFFSET ?`,
      [match, lo, hi, Math.min(scan, offset + limit), limit, offset],
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
    if (opts.strict) throw Object.assign(new Error(`自治体例規の検索 DB: ${e instanceof Error ? e.message : String(e)}`), { name: e instanceof Error ? e.name : "Error" });
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
  const rows = await exec<{ key: string; value: string }>("SELECT key, value FROM meta");
  const meta: Record<string, string> = {};
  for (const r of rows as { key: string; value: string }[]) meta[r.key] = r.value;
  return meta;
}

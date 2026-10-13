// 官報パーサ playground（`/kanpo-playground`）。官報の項目（告示など。複数ページ）の
// `pdftotext -bbox-layout` と罫線を kanpo-parser（WASM）で解析し、各語がどの領域・行に組まれたか
// （捨てられたならなぜか）や罫線から復元した表を紙面の上に重ねて見せる。パーサの回帰ケースを
// 調べたり、新しいケースを足すときに使う。
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useSearchParams } from "react-router";
import { AlertTriangle, ChevronDown, ChevronRight, Copy, ExternalLink, FileUp, Minus, Plus, Upload, X } from "lucide-react";
import { Badge } from "../ui/badge";
import { Button } from "../ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../ui/select";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "../ui/tabs";
import { cn } from "../ui/utils";
import { AmendDocumentView } from "../amend-document-view";
import { KanpoPageView, wordKey, type CellRef, type LineInfo, type PdfSource, type SheetSource, type Zoom } from "./kanpo-page-view";
import type { AmendDocument, AmendRun } from "../../data/api";
import {
  ARCHIVE_BASE,
  CORPUS_CASES,
  ITEM_MAX_PAGES,
  ROLE_COLOR,
  ROLE_LABEL,
  STATUS_LABEL,
  archiveBboxUrl,
  archivePdfUrl,
  archiveRulesUrl,
  fetchArchiveDates,
  fetchArchiveManifest,
  fetchRules,
  fetchText,
  issueTypeLabel,
  itemPageRange,
  loadKanpoParser,
  type ArchiveIssue,
  type ArchiveManifest,
  type CorpusCase,
  type KanpoAnalysis,
  type KanpoPageRules,
  type KanpoParser,
  type KanpoTable,
  type KanpoWordStatus,
} from "../../data/kanpo-playground";

type SourceKind = "corpus" | "archive" | "local";
type ArchiveMode = "item" | "page";

/** 解析対象の 1 ページ（bbox の XHTML・罫線・あれば PDF）。label は紙面の見出し。 */
type InputPage = { bbox: string; rules: KanpoPageRules | null; pdf: PdfSource | null; label: string };
/** 解析対象（項目の全ページ）。 */
type Input = { pages: InputPage[] };
/** 取得する 1 ページ分の URL。 */
type PageUrls = { bbox: string; pdf: string | null; rules: string | null; label: string };

const BBOX_COMMAND = "pdftotext -bbox-layout -enc UTF-8 page.pdf page.bbox.html";
const PREPARE_COMMAND = "cargo run -p kanpo-parser --example prepare -- page.pdf";

/** container だけをスクロールして el を見える位置に入れる（ページ全体は動かさない）。 */
function scrollWithin(container: HTMLElement, el: HTMLElement) {
  const c = container.getBoundingClientRect();
  const r = el.getBoundingClientRect();
  if (r.top < c.top) container.scrollTop += r.top - c.top - 8;
  else if (r.bottom > c.bottom) container.scrollTop += r.bottom - c.bottom + 8;
}

/** bbox の XHTML に含まれる <page> の数（1 ファイルに複数ページのローカル入力用）。 */
const countPages = (xhtml: string) => Math.max(1, xhtml.match(/<page[\s>]/g)?.length ?? 0);

/** 構造化結果の中の表（Run の table と旧形式の after_table/before_table）の数。 */
function countDocTables(doc: AmendDocument): number {
  const runs = (rs: AmendRun[]): number =>
    rs.reduce((n, r) => n + (r.table ? 1 + r.table.rows.flat().reduce((m, c) => m + runs(c), 0) : 0), 0);
  return doc.blocks.reduce(
    (n, b) =>
      n +
      (b.kind === "paragraph"
        ? runs(b.runs)
        : b.rows.reduce((m, r) => m + runs(r.after) + runs(r.before) + (r.after_table ? 1 : 0) + (r.before_table ? 1 : 0), 0)),
    0,
  );
}

/** コーパスのケースのページの見出し（紙面のページ番号が分かればそれ）。 */
function corpusPageLabel(c: CorpusCase, i: number): string {
  const n = c.meta.pages?.[i] ?? (c.meta.page != null ? c.meta.page + i : null);
  return n != null ? `p.${n}` : `${i + 1} ページ目`;
}

function CopyCommand({ command }: { command: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <div className="mt-1 flex items-center gap-1 min-w-0">
      <code className="flex-1 min-w-0 overflow-x-auto whitespace-nowrap rounded bg-muted px-2 py-1 font-mono">{command}</code>
      <Button
        variant="ghost"
        size="icon"
        className="size-7 shrink-0"
        title="コピー"
        onClick={() => {
          navigator.clipboard?.writeText(command).then(() => {
            setCopied(true);
            setTimeout(() => setCopied(false), 1200);
          });
        }}
      >
        <Copy className="size-3.5" />
      </Button>
      {copied && <span className="shrink-0">コピーしました</span>}
    </div>
  );
}

// ---------------------------------------------------------------------------
// ソースの選択
// ---------------------------------------------------------------------------

function CorpusPicker({ caseId, onSelect }: { caseId: string | null; onSelect: (id: string) => void }) {
  const c = CORPUS_CASES.find((x) => x.id === caseId);
  if (!CORPUS_CASES.length) {
    return <p className="text-sm text-muted-foreground">crates/kanpo-parser/tests/corpus にケースがありません。</p>;
  }
  return (
    <div className="space-y-2 min-w-0">
      <Select value={caseId ?? undefined} onValueChange={onSelect}>
        <SelectTrigger className="w-full max-w-xl">
          <SelectValue placeholder="ケースを選択" />
        </SelectTrigger>
        <SelectContent>
          {CORPUS_CASES.map((x) => (
            <SelectItem key={x.id} value={x.id}>
              <span className="font-mono text-xs">{x.id}</span>
              {x.pages.length > 1 && <span className="text-xs text-muted-foreground">（{x.pages.length} ページ）</span>}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      {c && (
        <div className="text-sm space-y-1">
          <div className="font-medium break-words">{c.meta.title}</div>
          <div className="flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
            {[
              c.meta.date,
              c.meta.issue,
              c.meta.pages && c.meta.pages.length > 1
                ? `${c.meta.pages[0]}–${c.meta.pages[c.meta.pages.length - 1]} ページ`
                : c.meta.page != null
                  ? `${c.meta.page} ページ`
                  : null,
            ]
              .filter(Boolean)
              .join(" · ")}
            {c.meta.source && (
              <a href={c.meta.source} target="_blank" rel="noreferrer" className="inline-flex items-center gap-0.5 hover:text-primary">
                原本 <ExternalLink className="size-3" />
              </a>
            )}
            {c.meta.tags?.map((t) => (
              <Badge key={t} variant="secondary" className="font-mono text-[11px] max-w-full whitespace-normal break-all">{t}</Badge>
            ))}
          </div>
          {c.meta.notes && <p className="text-xs text-muted-foreground leading-relaxed break-words">{c.meta.notes}</p>}
        </div>
      )}
    </div>
  );
}

/** アーカイブの選択（URL のクエリに載る）。 */
type ArchiveSel = {
  mode: ArchiveMode;
  date: string | null;
  dir: string | null;
  /** 1 ページモードのページ。 */
  page: number | null;
  /** 項目モードの項目（manifest の items の添字）。 */
  item: number | null;
  /** 項目モードで手で変えた最終ページ（無ければ既定の範囲）。 */
  last: number | null;
};
type ArchiveChange = { mode: ArchiveMode; date: string; dir?: string; page?: number; item?: number; last?: number };

/** ページの罫線の URL。manifest に rules の印があればそれに従い、古い manifest（印が無い）なら取得を試す。 */
function rulesUrlOf(date: string, issue: ArchiveIssue, entry: ArchiveIssue["pages"][number]): string | null {
  if (entry.rules === true) return archiveRulesUrl(date, issue.dir, entry.file);
  if (entry.rules === false || issue.pages.some((p) => p.rules !== undefined)) return null;
  return archiveRulesUrl(date, issue.dir, entry.file);
}

function ArchivePicker({
  sel,
  onChange,
  onInput,
}: {
  sel: ArchiveSel;
  onChange: (v: ArchiveChange) => void;
  onInput: (pages: PageUrls[] | null, error?: string) => void;
}) {
  const { mode, date, dir, page, item } = sel;
  const [dates, setDates] = useState<string[] | null>(null);
  // 読み込んだ manifest は日付と組で持ち、日付を切り替えた直後に前の日の号で選び直さないようにする。
  const [loaded, setLoaded] = useState<{ date: string; manifest: ArchiveManifest } | null>(null);
  const manifest = loaded && loaded.date === date ? loaded.manifest : null;
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetchArchiveDates().then(setDates, (e) => setError(`index.json を読めませんでした: ${e.message}`));
  }, []);

  // 日付が未指定なら最新日を選ぶ。
  useEffect(() => {
    if (!date && dates?.length) onChange({ mode, date: dates[0] });
  }, [mode, date, dates, onChange]);

  useEffect(() => {
    if (!date) return;
    let cancelled = false;
    setError(null);
    fetchArchiveManifest(date).then(
      (m) => { if (!cancelled) setLoaded({ date, manifest: m }); },
      (e) => { if (!cancelled) setError(`manifest.json を読めませんでした: ${e.message}`); },
    );
    return () => { cancelled = true; };
  }, [date]);

  const issue = manifest?.issues.find((i) => i.dir === dir) ?? null;
  const itemIndex = issue && item != null && item >= 0 && item < issue.items.length ? item : null;
  // 号・ページ（項目）が未指定なら先頭を選ぶ。
  useEffect(() => {
    if (!manifest || !date) return;
    const first = manifest.issues[0];
    const target = issue ?? first;
    if (!target) return;
    if (mode === "page") {
      if (!issue || page == null) onChange({ mode, date, dir: target.dir, page: target.pages[0]?.page });
    } else if (!issue || (itemIndex == null && target.items.length)) {
      onChange({ mode, date, dir: target.dir, item: target.items.length ? 0 : undefined });
    }
  }, [manifest, issue, date, mode, page, itemIndex, onChange]);

  // 対象のページ範囲。項目モードは項目の開始ページから（手で変えた）最終ページまで。
  const maxPage = issue ? Math.max(issue.page_count, ...issue.pages.map((p) => p.page)) : 0;
  const range = useMemo(() => {
    if (!issue) return null;
    if (mode === "page") return page != null ? { first: page, last: page, defaultLast: page } : null;
    if (itemIndex == null) return null;
    const r = itemPageRange(issue, itemIndex);
    const cap = Math.min(maxPage, r.first + ITEM_MAX_PAGES - 1);
    const last = Math.min(Math.max(sel.last ?? r.last, r.first), cap);
    return { first: r.first, last, defaultLast: r.last };
  }, [issue, mode, page, itemIndex, sel.last, maxPage]);

  const entries = useMemo(
    () => (issue && range ? issue.pages.filter((p) => p.page >= range.first && p.page <= range.last).sort((a, b) => a.page - b.page) : []),
    [issue, range],
  );
  const missingBbox = entries.filter((e) => !e.bbox).map((e) => e.page);
  const entryKey = entries.map((e) => `${e.page}:${e.file}:${e.bbox}`).join(",");
  useEffect(() => {
    if (!date || !issue || !range) return onInput(null);
    const usable = entries.filter((e) => e.bbox);
    if (!entries.length) return onInput(null, `${range.first}–${range.last} ページがアーカイブにありません。`);
    if (!usable.length) return onInput(null, "このページは座標付きテキスト（.bbox.html）が作られていません。");
    onInput(
      usable.map((e) => ({
        bbox: archiveBboxUrl(date, issue.dir, e.file),
        pdf: archivePdfUrl(date, issue.dir, e.file),
        rules: rulesUrlOf(date, issue, e),
        label: `p.${e.page}`,
      })),
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [date, issue?.dir, entryKey]);

  const itemsOnPage = mode === "page" ? issue?.items.filter((it) => it.page === page) ?? [] : [];
  const switchMode = (m: ArchiveMode) => {
    if (m === mode || !date) return;
    if (!issue) return onChange({ mode: m, date });
    if (m === "page") return onChange({ mode: m, date, dir: issue.dir, page: range?.first ?? issue.pages[0]?.page });
    // 今のページを含む（その前で最後に始まる）項目を選ぶ。
    let idx = issue.items.length ? 0 : undefined;
    issue.items.forEach((it, i) => { if (page != null && it.page <= page) idx = i; });
    onChange({ mode: m, date, dir: issue.dir, item: idx });
  };
  const setLast = (last: number) => {
    if (!date || !issue || itemIndex == null || !range) return;
    onChange({ mode, date, dir: issue.dir, item: itemIndex, last: last === range.defaultLast ? undefined : last });
  };

  return (
    <div className="space-y-2 min-w-0">
      <div className="flex flex-wrap items-center gap-2">
        <div className="inline-flex rounded-md border border-border p-0.5 text-sm">
          {(
            [
              ["item", "項目"],
              ["page", "1 ページ"],
            ] as [ArchiveMode, string][]
          ).map(([m, label]) => (
            <button
              key={m}
              type="button"
              className={cn("px-2.5 h-7 rounded", mode === m ? "bg-accent text-accent-foreground font-medium" : "text-muted-foreground hover:bg-accent/60")}
              onClick={() => switchMode(m)}
            >
              {label}
            </button>
          ))}
        </div>
        <Select value={date ?? undefined} onValueChange={(d) => onChange({ mode, date: d })}>
          <SelectTrigger className="w-40"><SelectValue placeholder={dates ? "日付" : "読み込み中…"} /></SelectTrigger>
          <SelectContent>
            {dates?.map((d) => <SelectItem key={d} value={d}>{d}</SelectItem>)}
          </SelectContent>
        </Select>
        <Select
          value={issue?.dir}
          onValueChange={(v) => {
            const i = manifest?.issues.find((x) => x.dir === v);
            if (!date || !i) return;
            if (mode === "page") onChange({ mode, date, dir: v, page: i.pages[0]?.page });
            else onChange({ mode, date, dir: v, item: i.items.length ? 0 : undefined });
          }}
          disabled={!manifest}
        >
          <SelectTrigger className="w-56 max-w-full"><SelectValue placeholder="号" /></SelectTrigger>
          <SelectContent>
            {manifest?.issues.map((i) => (
              <SelectItem key={i.dir} value={i.dir}>
                {issueTypeLabel(i.issue_type)} {i.issue_no}（{i.page_count} ページ）
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        {mode === "page" && (
          <Select
            value={page != null ? String(page) : undefined}
            onValueChange={(v) => date && issue && onChange({ mode, date, dir: issue.dir, page: Number(v) })}
            disabled={!issue}
          >
            <SelectTrigger className="w-28"><SelectValue placeholder="ページ" /></SelectTrigger>
            <SelectContent>
              {issue?.pages.map((p) => (
                <SelectItem key={p.page} value={String(p.page)}>
                  {p.page} ページ{p.bbox ? "" : "（bbox なし）"}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        )}
        {issue && (
          <a href={issue.url} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-primary">
            デジタル官報 <ExternalLink className="size-3" />
          </a>
        )}
      </div>
      {mode === "item" && issue && (
        <div className="flex flex-wrap items-center gap-2">
          {issue.items.length ? (
            <Select
              value={itemIndex != null ? String(itemIndex) : undefined}
              onValueChange={(v) => date && onChange({ mode, date, dir: issue.dir, item: Number(v) })}
            >
              <SelectTrigger className="w-full max-w-xl min-w-0">
                <SelectValue placeholder="項目" />
              </SelectTrigger>
              <SelectContent className="max-w-[min(42rem,90vw)]">
                {issue.items.map((it, i) => (
                  <SelectItem key={i} value={String(i)}>
                    <span className="font-mono text-xs text-muted-foreground shrink-0">p.{it.page}</span>
                    <span className="truncate">{it.title}</span>
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          ) : (
            <span className="text-xs text-muted-foreground">この号には項目（目次）がありません。1 ページモードで選んでください。</span>
          )}
          {range && (
            <div className="inline-flex items-center gap-1 text-xs">
              <span className="text-muted-foreground">ページ</span>
              <span className="font-mono">{range.first}–{range.last}</span>
              <Button
                variant="outline"
                size="icon"
                className="size-6"
                title="最終ページを 1 つ前に"
                disabled={range.last <= range.first}
                onClick={() => setLast(range.last - 1)}
              >
                <Minus className="size-3" />
              </Button>
              <Button
                variant="outline"
                size="icon"
                className="size-6"
                title="最終ページを 1 つ後ろに"
                disabled={range.last >= Math.min(maxPage, range.first + ITEM_MAX_PAGES - 1)}
                onClick={() => setLast(range.last + 1)}
              >
                <Plus className="size-3" />
              </Button>
              {range.last !== range.defaultLast && (
                <button type="button" className="text-muted-foreground hover:text-primary underline" onClick={() => setLast(range.defaultLast)}>
                  既定に戻す
                </button>
              )}
            </div>
          )}
        </div>
      )}
      {itemsOnPage.length > 0 && (
        <ul className="text-xs text-muted-foreground space-y-0.5">
          {itemsOnPage.map((it, i) => <li key={i} className="break-words">・{it.title}</li>)}
        </ul>
      )}
      {missingBbox.length > 0 && entries.length > missingBbox.length && (
        <p className="text-xs text-amber-700 dark:text-amber-400">
          座標付きテキストの無いページを飛ばしました: {missingBbox.join(", ")}
        </p>
      )}
      {error && <p className="text-xs text-destructive break-words">{error}</p>}
    </div>
  );
}

/** ローカルで選んだファイル（stem ごと）。 */
type LocalPage = {
  bbox?: { name: string; text: string };
  pdf?: { name: string; data: ArrayBuffer };
  rules?: { name: string; rules: KanpoPageRules | null };
};

/** ファイル名の stem（`x.pdf`・`x.bbox.html`・`x.rules.json` → `x`）。 */
const stemOf = (name: string) => name.replace(/\.(bbox\.html|rules\.json|x?html?|json|pdf)$/i, "");

function LocalPicker({
  pages,
  onFiles,
  onClear,
}: {
  pages: [string, LocalPage][];
  onFiles: (files: File[]) => void;
  onClear: () => void;
}) {
  const [dragging, setDragging] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  return (
    <div className="space-y-2 min-w-0">
      <div
        className={cn(
          "rounded-md border border-dashed border-border p-4 text-sm text-center transition-colors",
          dragging ? "bg-accent border-primary" : "bg-muted/30",
        )}
        onDragOver={(e) => { e.preventDefault(); setDragging(true); }}
        onDragLeave={() => setDragging(false)}
        onDrop={(e) => {
          e.preventDefault();
          setDragging(false);
          onFiles(Array.from(e.dataTransfer.files));
        }}
      >
        <Upload className="size-5 mx-auto mb-1 text-muted-foreground" />
        <div>
          ページごとの .bbox.html（必須）と、あれば同じ名前の .pdf・.rules.json をドロップ（複数ページ可。名前順に 1 つの項目として解析）
        </div>
        <div className="mt-2 flex justify-center gap-2">
          <Button variant="outline" size="sm" onClick={() => inputRef.current?.click()}>
            <FileUp className="size-4" /> ファイルを選ぶ
          </Button>
          {pages.length > 0 && (
            <Button variant="ghost" size="sm" onClick={onClear}>
              <X className="size-4" /> クリア
            </Button>
          )}
        </div>
        <input
          ref={inputRef}
          type="file"
          multiple
          accept=".html,.xhtml,.htm,.pdf,.json"
          className="hidden"
          onChange={(e) => {
            onFiles(Array.from(e.target.files ?? []));
            e.target.value = "";
          }}
        />
        {pages.length > 0 && (
          <ul className="mt-2 text-xs text-muted-foreground break-all text-left inline-block">
            {pages.map(([stem, p]) => (
              <li key={stem}>
                <span className="font-mono text-foreground">{stem}</span>: bbox {p.bbox ? "✓" : "なし（解析しない）"} / PDF{" "}
                {p.pdf ? "✓" : "なし"} / 罫線 {p.rules ? (p.rules.rules ? "✓" : "読めない") : "なし"}
              </li>
            ))}
          </ul>
        )}
      </div>
      <div className="text-xs text-muted-foreground space-y-2">
        <div>
          bbox と罫線の作り方（リポジトリで。poppler が必要）:
          <CopyCommand command={PREPARE_COMMAND} />
        </div>
        <div>
          bbox だけなら（poppler の pdftotext。罫線が無いと表は復元しない）:
          <CopyCommand command={BBOX_COMMAND} />
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// 結果パネル
// ---------------------------------------------------------------------------

/** JSON を折りたためる木で表示する。大きい配列（語など）は既定で閉じる。 */
function JsonNode({ name, value, depth }: { name: string; value: unknown; depth: number }) {
  const isObj = value !== null && typeof value === "object";
  const size = isObj ? (Array.isArray(value) ? value.length : Object.keys(value as object).length) : 0;
  const [open, setOpen] = useState(depth < 2 && size <= 20);
  if (!isObj) {
    // 長い文字列（本文など）は先頭だけ出し、クリックで全体を開く。
    const s = JSON.stringify(value);
    const long = s.length > 160;
    return (
      <div className="pl-4 break-all">
        <span className="text-muted-foreground">{name}: </span>
        <span
          className={cn(
            typeof value === "string" ? "text-emerald-700 dark:text-emerald-400" : "text-blue-700 dark:text-blue-400",
            long && "cursor-pointer",
          )}
          onClick={long ? () => setOpen(!open) : undefined}
        >
          {long && !open ? `${s.slice(0, 160)}…（${s.length} 文字）` : s}
        </span>
      </div>
    );
  }
  const entries = Array.isArray(value) ? value.map((v, i) => [String(i), v] as const) : Object.entries(value as object);
  return (
    <div className={depth > 0 ? "pl-4" : ""}>
      <button type="button" className="inline-flex items-center gap-0.5 hover:text-primary" onClick={() => setOpen(!open)}>
        {open ? <ChevronDown className="size-3" /> : <ChevronRight className="size-3" />}
        <span className="text-muted-foreground">{name}</span>
        <span className="text-muted-foreground/70">{Array.isArray(value) ? ` [${size}]` : ` {${size}}`}</span>
      </button>
      {open && entries.map(([k, v]) => <JsonNode key={k} name={k} value={v} depth={depth + 1} />)}
    </div>
  );
}

function Stat({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex items-baseline gap-1">
      <span className="text-muted-foreground">{label}</span>
      <span className="font-medium">{children}</span>
    </div>
  );
}

// ---------------------------------------------------------------------------

/**
 * 行パネルで罫線の表の行を小さな表として描く。セルにカーソルを合わせると紙面のセルと語を強調する。
 */
function LineTable({
  table,
  lineKey,
  activeCell,
  onHoverCell,
}: {
  table: KanpoTable;
  lineKey: string;
  activeCell: CellRef | null;
  onHoverCell: (cell: CellRef | null) => void;
}) {
  const rows = Array.from({ length: table.rows }, (_, r) =>
    table.cells.map((c, i) => ({ c, i })).filter(({ c }) => c.row === r).sort((a, b) => a.c.col - b.c.col),
  );
  return (
    <table className="border-collapse text-xs my-0.5">
      <tbody>
        {rows.map((cells, r) => (
          <tr key={r} className="align-top">
            {cells.map(({ c, i }) => {
              const active = activeCell?.line === lineKey && activeCell.cell === i;
              return (
                <td
                  key={i}
                  rowSpan={c.rowspan > 1 ? c.rowspan : undefined}
                  colSpan={c.colspan > 1 ? c.colspan : undefined}
                  className={cn(
                    "border border-teal-700/40 px-1 py-0.5 max-w-64 break-all",
                    active && "bg-teal-500/20",
                  )}
                  onMouseEnter={() => onHoverCell({ line: lineKey, cell: i })}
                  onMouseLeave={() => onHoverCell(null)}
                >
                  {c.text || <span className="text-muted-foreground">（空）</span>}
                </td>
              );
            })}
          </tr>
        ))}
      </tbody>
    </table>
  );
}

// ---------------------------------------------------------------------------
// 本体
// ---------------------------------------------------------------------------

export function KanpoPlaygroundView() {
  const [params, setParams] = useSearchParams();
  const defaultSrc: SourceKind = CORPUS_CASES.length ? "corpus" : "local";
  const srcParam = params.get("src") as SourceKind | null;
  const src: SourceKind =
    srcParam === "archive" && ARCHIVE_BASE ? "archive" : srcParam === "corpus" || srcParam === "local" ? srcParam : defaultSrc;

  const updateParams = useCallback(
    (next: Record<string, string | number | undefined | null>) => {
      const p = new URLSearchParams();
      for (const [k, v] of Object.entries(next)) if (v != null && v !== "") p.set(k, String(v));
      setParams(p, { replace: true });
    },
    [setParams],
  );

  // WASM（未ビルドなら null）。
  const [parser, setParser] = useState<KanpoParser | null | undefined>(undefined);
  const [parserError, setParserError] = useState<string | null>(null);
  useEffect(() => {
    loadKanpoParser().then(setParser, (e) => {
      setParser(null);
      setParserError(String(e?.message ?? e));
    });
  }, []);

  // ---- 入力の取得 ----
  const [input, setInput] = useState<Input | null>(null);
  const [inputError, setInputError] = useState<string | null>(null);
  const [loadingInput, setLoadingInput] = useState(false);
  // 選択を素早く切り替えたとき、古い取得の結果で上書きしない。
  const loadSeq = useRef(0);

  const loadFromUrls = useCallback((pages: PageUrls[] | null, error?: string) => {
    const seq = ++loadSeq.current;
    setInput(null);
    setInputError(error ?? null);
    setLoadingInput(false);
    if (!pages?.length) return;
    setLoadingInput(true);
    Promise.all(
      pages.map(async (p) => {
        const [bbox, rules] = await Promise.all([fetchText(p.bbox), p.rules ? fetchRules(p.rules) : Promise.resolve(null)]);
        return { bbox, rules, pdf: p.pdf ? { url: p.pdf } : null, label: p.label } satisfies InputPage;
      }),
    ).then(
      (loaded) => {
        if (seq !== loadSeq.current) return;
        setInput({ pages: loaded });
        setLoadingInput(false);
      },
      (e) => {
        if (seq !== loadSeq.current) return;
        setInputError(`bbox を読めませんでした: ${e.message}`);
        setLoadingInput(false);
      },
    );
  }, []);

  // コーパス
  const caseId = src === "corpus" ? params.get("case") ?? CORPUS_CASES[0]?.id ?? null : null;
  useEffect(() => {
    if (src !== "corpus") return;
    const c = CORPUS_CASES.find((x) => x.id === caseId);
    loadFromUrls(
      c ? c.pages.map((p, i) => ({ bbox: p.bboxUrl, pdf: p.pdfUrl, rules: p.rulesUrl, label: corpusPageLabel(c, i) })) : null,
      caseId && !c ? `ケース ${caseId} が見つかりません。` : undefined,
    );
  }, [src, caseId, loadFromUrls]);

  // ローカル（URL には載せない）。stem ごとに bbox・PDF・罫線をまとめ、名前順に 1 つの項目にする。
  const [local, setLocal] = useState<Map<string, LocalPage>>(() => new Map());
  const onLocalFiles = useCallback(async (files: File[]) => {
    const updates: [string, Partial<LocalPage>][] = [];
    for (const f of files) {
      const stem = stemOf(f.name);
      if (/\.pdf$/i.test(f.name)) {
        updates.push([stem, { pdf: { name: f.name, data: await f.arrayBuffer() } }]);
      } else if (/\.json$/i.test(f.name)) {
        let rules: KanpoPageRules | null = null;
        try {
          const v = JSON.parse(await f.text());
          if (v && Array.isArray(v.vertical) && Array.isArray(v.horizontal)) rules = v;
        } catch {
          // 読めない JSON は「読めない」と表示するだけにする。
        }
        updates.push([stem, { rules: { name: f.name, rules } }]);
      } else if (/\.(x?html?)$/i.test(f.name)) {
        updates.push([stem, { bbox: { name: f.name, text: await f.text() } }]);
      }
    }
    setLocal((prev) => {
      const next = new Map(prev);
      for (const [stem, u] of updates) next.set(stem, { ...next.get(stem), ...u });
      return next;
    });
  }, []);
  const localPages = useMemo(
    () => [...local.entries()].sort(([a], [b]) => a.localeCompare(b, undefined, { numeric: true })),
    [local],
  );
  useEffect(() => {
    if (src !== "local") return;
    loadSeq.current++;
    setInputError(null);
    setLoadingInput(false);
    const pages = localPages.flatMap(([stem, p]) =>
      p.bbox
        ? [{ bbox: p.bbox.text, rules: p.rules?.rules ?? null, pdf: p.pdf ? { data: p.pdf.data } : null, label: stem }]
        : [],
    );
    setInput(pages.length ? { pages } : null);
  }, [src, localPages]);

  // アーカイブ
  const archiveSel: ArchiveSel = {
    mode: params.get("mode") === "page" ? "page" : "item",
    date: params.get("date"),
    dir: params.get("dir"),
    page: params.get("page") ? Number(params.get("page")) : null,
    item: params.get("item") ? Number(params.get("item")) : null,
    last: params.get("last") ? Number(params.get("last")) : null,
  };
  const onArchiveChange = useCallback(
    (v: ArchiveChange) => updateParams({ src: "archive", ...v, mode: v.mode === "page" ? "page" : undefined }),
    [updateParams],
  );

  // ---- 解析 ----
  const { analysis, analyzeError, elapsed } = useMemo(() => {
    if (!parser || !input) return { analysis: null, analyzeError: null, elapsed: 0 };
    const t0 = performance.now();
    try {
      const a: KanpoAnalysis = parser.analyzeItem(input.pages.map((p) => ({ xhtml: p.bbox, rules: p.rules })));
      return { analysis: a, analyzeError: null, elapsed: performance.now() - t0 };
    } catch (e: any) {
      return { analysis: null, analyzeError: String(e?.message ?? e), elapsed: 0 };
    }
  }, [parser, input]);

  // 解析結果のページ → 紙面（PDF・罫線）。ふつうは入力 1 ページ = 解析 1 ページだが、1 つの bbox に
  // 複数の <page> がある入力（ローカル）では同じ PDF のページを順に当てる。
  const sheets = useMemo<SheetSource[]>(() => {
    if (!analysis || !input) return [];
    const out: SheetSource[] = [];
    for (const p of input.pages) {
      const n = countPages(p.bbox);
      for (let k = 0; k < n; k++) {
        out.push({ pdf: p.pdf, pdfPage: k + 1, rules: n === 1 ? p.rules : null, label: n === 1 ? p.label : `${p.label} (${k + 1}/${n})` });
      }
    }
    return analysis.pages.map((_, i) => out[i] ?? { pdf: null, pdfPage: 1, rules: null, label: `${i + 1} ページ目` });
  }, [analysis, input]);

  // 行の通し番号（項目全体の読み順）と、語 → 行・表のセルの対応。
  const { lines, wordLine, wordCell, lineOrder } = useMemo(() => {
    const lines: LineInfo[] = [];
    const wordLine = new Map<string, LineInfo>();
    const wordCell = new Map<string, CellRef>();
    analysis?.pages.forEach((page, pi) =>
      page.regions.forEach((r, ri) =>
        r.lines.forEach((l, li) => {
          const key = `${pi}:${ri}:${li}`;
          const info: LineInfo = { key, page: pi, order: lines.length, role: r.role, words: l.words, text: l.text, table: l.table };
          lines.push(info);
          for (const w of l.words) wordLine.set(wordKey(pi, w), info);
          l.table?.cells.forEach((c, ci) => c.words.forEach((w) => wordCell.set(wordKey(pi, w), { line: key, cell: ci })));
        }),
      ),
    );
    return { lines, wordLine, wordCell, lineOrder: new Map(lines.map((l) => [l.key, l.order])) };
  }, [analysis]);

  // 項目全体の集計。
  const stats = useMemo(() => {
    const status: Partial<Record<KanpoWordStatus, number>> = {};
    let words = 0;
    for (const p of analysis?.pages ?? []) {
      words += p.words.length;
      for (const w of p.words) status[w.status] = (status[w.status] ?? 0) + 1;
    }
    return {
      words,
      status,
      tables: lines.filter((l) => l.table).length,
      rulesPages: sheets.filter((s) => s.rules).length,
      layouts: [...new Set(analysis?.pages.map((p) => p.layout) ?? [])].join(", "),
      docTables: analysis?.articles.reduce((n, a) => n + countDocTables(a.document), 0) ?? 0,
    };
  }, [analysis, lines, sheets]);

  // ---- 表示状態と連動 ----
  const [zoom, setZoom] = useState<Zoom>("fit");
  const [showDropped, setShowDropped] = useState(true);
  const [showOrder, setShowOrder] = useState(false);
  const [showRules, setShowRules] = useState(false);
  const [tab, setTab] = useState("lines");
  const [hoverWord, setHoverWord] = useState<string | null>(null);
  const [hoverCell, setHoverCell] = useState<CellRef | null>(null);
  const [hoverPanelLine, setHoverPanelLine] = useState<string | null>(null);
  const [selectedLine, setSelectedLine] = useState<string | null>(null);
  const [scrollToLine, setScrollToLine] = useState<{ key: string; nonce: number } | null>(null);
  useEffect(() => {
    setSelectedLine(null);
    setHoverWord(null);
    setHoverCell(null);
    setHoverPanelLine(null);
  }, [analysis]);

  const hoverWordLine = hoverWord != null ? wordLine.get(hoverWord)?.key ?? null : null;
  const activeLine = hoverWordLine ?? hoverPanelLine ?? selectedLine;
  const activeCell = hoverCell ?? (hoverWord ? wordCell.get(hoverWord) ?? null : null);

  // 紙面でホバーした語の行を、行パネルの中だけスクロールして見せる。
  const linesPanelRef = useRef<HTMLDivElement>(null);
  const scrollPanelTo = useCallback((key: string | null) => {
    const panel = linesPanelRef.current;
    const el = key ? panel?.querySelector<HTMLElement>(`[data-line="${key}"]`) : null;
    if (panel && el) scrollWithin(panel, el);
  }, []);
  useEffect(() => {
    if (tab === "lines" && hoverWordLine) scrollPanelTo(hoverWordLine);
  }, [tab, hoverWordLine, scrollPanelTo]);

  const selectLineInPanel = useCallback(
    (key: string) => {
      setSelectedLine(key);
      setTab("lines");
      // タブ切替後に描画されてからスクロールする。
      requestAnimationFrame(() => scrollPanelTo(key));
    },
    [scrollPanelTo],
  );
  const onClickWord = useCallback(
    (wk: string) => {
      const key = wordLine.get(wk)?.key;
      if (key) selectLineInPanel(key);
    },
    [wordLine, selectLineInPanel],
  );
  const onPanelLineClick = (key: string) => {
    setSelectedLine(key);
    setScrollToLine({ key, nonce: Date.now() });
  };

  const json = useMemo(() => (analysis ? JSON.stringify(analysis, null, 2) : ""), [analysis]);
  const multi = (analysis?.pages.length ?? 0) > 1;

  return (
    <div className="flex flex-col gap-4 p-4 md:p-6 min-w-0 xl:h-full">
      {/* 見出しとソース */}
      <div className="space-y-3 min-w-0">
        <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
          <h1 className="text-xl font-semibold">官報パーサ</h1>
          <span className="text-xs text-muted-foreground">
            kanpo-parser {parser ? `v${parser.version}` : parser === null ? "（WASM 未ビルド）" : "読み込み中…"}
          </span>
        </div>
        {parser === null && (
          <div className="flex gap-2 rounded-md border border-amber-500/40 bg-amber-500/10 p-3 text-sm">
            <AlertTriangle className="size-4 shrink-0 mt-0.5 text-amber-600" />
            <div className="min-w-0">
              {parserError ? (
                <>WASM を読み込めませんでした: <span className="break-all">{parserError}</span></>
              ) : (
                <>
                  kanpo-parser の WASM がビルドされていません。<code className="font-mono bg-muted px-1 rounded">figma/</code> で{" "}
                  <code className="font-mono bg-muted px-1 rounded">pnpm build:wasm</code> を実行してから開き直してください（Rust と
                  wasm32-unknown-unknown ターゲットが必要です）。
                </>
              )}
            </div>
          </div>
        )}
        <div className="flex flex-col md:flex-row gap-3 md:items-start min-w-0">
          <div className="inline-flex flex-wrap max-w-full rounded-md border border-border p-0.5 self-start">
            {(
              [
                ["corpus", "コーパス"],
                ...(ARCHIVE_BASE ? [["archive", "アーカイブ (R2)"]] : []),
                ["local", "ローカル"],
              ] as [SourceKind, string][]
            ).map(([k, label]) => (
              <button
                key={k}
                type="button"
                className={cn(
                  "px-3 h-8 rounded text-sm whitespace-nowrap transition-colors",
                  src === k ? "bg-primary text-primary-foreground" : "hover:bg-accent text-foreground/80",
                )}
                onClick={() => {
                  if (k === src) return;
                  loadSeq.current++;
                  updateParams({ src: k });
                  setInput(null);
                  setInputError(null);
                  setLoadingInput(false);
                }}
              >
                {label}
              </button>
            ))}
          </div>
          <div className="flex-1 min-w-0">
            {src === "corpus" && <CorpusPicker caseId={caseId} onSelect={(id) => updateParams({ src: "corpus", case: id })} />}
            {src === "archive" && <ArchivePicker sel={archiveSel} onChange={onArchiveChange} onInput={loadFromUrls} />}
            {src === "local" && <LocalPicker pages={localPages} onFiles={onLocalFiles} onClear={() => setLocal(new Map())} />}
          </div>
        </div>
        {(inputError || analyzeError) && (
          <div className="text-sm text-destructive space-y-0.5 break-words">
            {inputError && <div>{inputError}</div>}
            {analyzeError && <div>解析に失敗しました: {analyzeError}</div>}
          </div>
        )}
      </div>

      {loadingInput && <div className="text-sm text-muted-foreground">読み込み中…</div>}
      {analysis && analysis.pages.length === 0 && (
        <div className="text-sm text-muted-foreground">
          ページが見つかりません。<code className="font-mono">pdftotext -bbox-layout</code> の出力（&lt;page&gt; と &lt;word&gt; を含む XHTML）か確認してください。
        </div>
      )}

      {analysis && analysis.pages.length > 0 && (
        <div className="grid grid-cols-1 xl:grid-cols-2 gap-4 min-w-0 xl:flex-1 xl:min-h-0">
          {/* 紙面 */}
          <section className="flex flex-col gap-2 min-w-0 h-[75vh] xl:h-auto xl:min-h-0">
            <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5 text-xs">
              <div className="inline-flex rounded-md border border-border p-0.5">
                {(
                  [
                    ["fit", "幅に合わせる"],
                    [1, "100%"],
                    [1.5, "150%"],
                  ] as [Zoom, string][]
                ).map(([z, label]) => (
                  <button
                    key={String(z)}
                    type="button"
                    className={cn("px-2 h-7 rounded", zoom === z ? "bg-accent text-accent-foreground font-medium" : "text-muted-foreground hover:bg-accent/60")}
                    onClick={() => setZoom(z)}
                  >
                    {label}
                  </button>
                ))}
              </div>
              <label className="inline-flex items-center gap-1 cursor-pointer select-none">
                <input type="checkbox" checked={showDropped} onChange={(e) => setShowDropped(e.target.checked)} />
                捨てた語
              </label>
              <label className="inline-flex items-center gap-1 cursor-pointer select-none">
                <input type="checkbox" checked={showOrder} onChange={(e) => setShowOrder(e.target.checked)} />
                読み順
              </label>
              <label
                className={cn("inline-flex items-center gap-1 select-none", stats.rulesPages ? "cursor-pointer" : "text-muted-foreground")}
                title={stats.rulesPages ? undefined : "罫線（.rules.json）がありません"}
              >
                <input type="checkbox" checked={showRules} disabled={!stats.rulesPages} onChange={(e) => setShowRules(e.target.checked)} />
                罫線
              </label>
              {input && input.pages.every((p) => !p.pdf) && <span className="text-muted-foreground">PDF なし（語のテキストを白紙に描画）</span>}
            </div>
            {/* 凡例 */}
            <div className="flex flex-wrap gap-x-3 gap-y-1 text-xs">
              {(Object.keys(ROLE_LABEL) as (keyof typeof ROLE_LABEL)[]).map((r) => (
                <span key={r} className="inline-flex items-center gap-1">
                  <span className="size-2.5 rounded-sm border" style={{ background: `${ROLE_COLOR[r]}33`, borderColor: ROLE_COLOR[r] }} />
                  {ROLE_LABEL[r]}
                </span>
              ))}
              <span className="inline-flex items-center gap-1 text-muted-foreground">
                <span className="size-2.5 rounded-sm border border-dashed border-gray-500" />
                捨てた語
              </span>
              <span className="inline-flex items-center gap-1">
                <span className="size-2.5 rounded-sm border-2 border-teal-700" />
                表（点線はセル）
              </span>
              {showRules && (
                <span className="inline-flex items-center gap-1">
                  <span className="w-3 h-px bg-cyan-600" />
                  罫線
                </span>
              )}
            </div>
            <div className="flex-1 min-h-0">
              <KanpoPageView
                pages={analysis.pages}
                sources={sheets}
                lines={lines}
                wordLine={wordLine}
                zoom={zoom}
                showDropped={showDropped}
                showOrder={showOrder}
                showRules={showRules}
                activeLine={activeLine}
                activeCell={activeCell}
                hoverWord={hoverWord}
                scrollToLine={scrollToLine}
                onHoverWord={setHoverWord}
                onHoverCell={setHoverCell}
                onClickWord={onClickWord}
                onClickLine={selectLineInPanel}
              />
            </div>
          </section>

          {/* 結果 */}
          <section className="flex flex-col gap-2 min-w-0 xl:min-h-0">
            <div className="flex flex-wrap gap-x-4 gap-y-1 text-xs">
              <Stat label="ページ">{analysis.pages.length}</Stat>
              <Stat label="layout">{stats.layouts}</Stat>
              <Stat label="format">{analysis.format}</Stat>
              <Stat label="語">{stats.words}</Stat>
              {(Object.keys(STATUS_LABEL) as KanpoWordStatus[]).map((s) =>
                stats.status[s] ? <Stat key={s} label={STATUS_LABEL[s]}>{stats.status[s]}</Stat> : null,
              )}
              <Stat label="行">{lines.length}</Stat>
              <Stat label="表">{stats.tables}</Stat>
              <Stat label="罫線">{stats.rulesPages ? `${stats.rulesPages} / ${analysis.pages.length} ページ` : "なし"}</Stat>
              <Stat label="記事">{analysis.articles.length}</Stat>
              <Stat label="解析">{elapsed.toFixed(1)} ms</Stat>
            </div>
            <Tabs value={tab} onValueChange={setTab} className="flex-1 min-h-0">
              <TabsList className="w-full max-w-md">
                <TabsTrigger value="lines">行</TabsTrigger>
                <TabsTrigger value="text">本文</TabsTrigger>
                <TabsTrigger value="html">HTML 再現</TabsTrigger>
                <TabsTrigger value="json">JSON</TabsTrigger>
              </TabsList>
              <TabsContent value="lines" className="min-h-0">
                <div ref={linesPanelRef} className="max-h-[70vh] xl:max-h-none xl:h-full overflow-auto rounded-md border border-border">
                  {analysis.pages.map((page, pi) => (
                    <div key={pi}>
                      {multi && (
                        <div className="flex items-center gap-2 bg-muted px-2 py-1 border-b border-border text-xs">
                          <span className="font-medium font-mono">{sheets[pi]?.label}</span>
                          <span className="text-muted-foreground">
                            {page.layout} · 語 {page.words.length} · {page.regions.length} 領域
                          </span>
                        </div>
                      )}
                      {page.regions.map((r, ri) => (
                        <div key={ri}>
                          <div className="sticky top-0 z-[1] flex items-center gap-2 bg-background/95 backdrop-blur px-2 py-1 border-b border-border text-xs">
                            <span className="size-2.5 rounded-sm" style={{ background: ROLE_COLOR[r.role] }} />
                            <span className="font-medium">{ROLE_LABEL[r.role]}</span>
                            <span className="text-muted-foreground">
                              {multi ? `${sheets[pi]?.label} · ` : ""}段 {r.unit + 1}
                              {r.part > 0 ? ` · 続き ${r.part}` : ""} · {r.lines.length} 行
                            </span>
                          </div>
                          {r.lines.map((l, li) => {
                            const key = `${pi}:${ri}:${li}`;
                            const order = lineOrder.get(key) ?? 0;
                            const active = activeLine === key;
                            return (
                              <div
                                key={key}
                                role="button"
                                tabIndex={0}
                                data-line={key}
                                className={cn(
                                  "w-full text-left flex gap-2 px-2 py-0.5 text-sm border-l-2 transition-colors cursor-pointer",
                                  active ? "bg-accent" : "hover:bg-accent/50",
                                  selectedLine === key ? "font-medium" : "",
                                )}
                                style={{ borderLeftColor: active || selectedLine === key ? ROLE_COLOR[r.role] : "transparent" }}
                                onMouseEnter={() => setHoverPanelLine(key)}
                                onMouseLeave={() => setHoverPanelLine(null)}
                                onClick={() => onPanelLineClick(key)}
                                onKeyDown={(e) => {
                                  if (e.key === "Enter" || e.key === " ") {
                                    e.preventDefault();
                                    onPanelLineClick(key);
                                  }
                                }}
                              >
                                <span className="w-8 shrink-0 text-right font-mono text-xs text-muted-foreground pt-0.5">{order + 1}</span>
                                {l.table ? (
                                  <div className="min-w-0 overflow-x-auto">
                                    <div className="text-[11px] text-teal-700 dark:text-teal-400">
                                      表 {l.table.rows}×{l.table.cols}（クリックで紙面の表へ）
                                    </div>
                                    <LineTable table={l.table} lineKey={key} activeCell={activeCell} onHoverCell={setHoverCell} />
                                  </div>
                                ) : (
                                  <span className="min-w-0 break-all">{l.text || <span className="text-muted-foreground">（空）</span>}</span>
                                )}
                              </div>
                            );
                          })}
                        </div>
                      ))}
                    </div>
                  ))}
                  {lines.length === 0 && <p className="p-3 text-sm text-muted-foreground">行がありません。</p>}
                </div>
              </TabsContent>
              <TabsContent value="text" className="min-h-0">
                <pre className="max-h-[70vh] xl:max-h-none xl:h-full overflow-auto rounded-md border border-border p-3 text-sm whitespace-pre-wrap break-all font-sans leading-relaxed">
                  {analysis.text || "（空）"}
                </pre>
              </TabsContent>
              <TabsContent value="html" className="min-h-0">
                <div className="max-h-[70vh] xl:max-h-none xl:h-full overflow-auto space-y-3">
                  {analysis.articles.map((a, i) => {
                    const tables = countDocTables(a.document);
                    return (
                      <div key={i} className="rounded-md border border-border p-2 space-y-1.5">
                        <div className="flex items-center gap-2 text-xs">
                          <Badge variant="secondary">記事 {i + 1}</Badge>
                          <span className="text-muted-foreground">{a.document.format}</span>
                          <span className="text-muted-foreground">{a.document.blocks.length} ブロック</span>
                          {tables > 0 && <span className="text-muted-foreground">表 {tables}</span>}
                        </div>
                        <AmendDocumentView doc={a.document} className="text-xs" />
                        <details>
                          <summary className="text-xs text-muted-foreground cursor-pointer select-none">本文テキスト</summary>
                          <pre className="mt-1 text-xs whitespace-pre-wrap break-all font-sans leading-relaxed">{a.text}</pre>
                        </details>
                      </div>
                    );
                  })}
                  {analysis.articles.length === 0 && <p className="p-3 text-sm text-muted-foreground">記事がありません。</p>}
                </div>
              </TabsContent>
              <TabsContent value="json" className="min-h-0">
                <div className="max-h-[70vh] xl:max-h-none xl:h-full overflow-auto rounded-md border border-border p-2 font-mono text-xs">
                  <div className="flex justify-end">
                    <Button variant="ghost" size="sm" className="h-7" onClick={() => navigator.clipboard?.writeText(json)}>
                      <Copy className="size-3.5" /> コピー
                    </Button>
                  </div>
                  <JsonNode name="analysis" value={analysis} depth={0} />
                </div>
              </TabsContent>
            </Tabs>
          </section>
        </div>
      )}
    </div>
  );
}

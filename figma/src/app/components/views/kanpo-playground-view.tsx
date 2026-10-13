// 官報パーサ playground（`/kanpo-playground`）。官報 1 ページの `pdftotext -bbox-layout` を
// kanpo-parser（WASM）で解析し、各語がどの領域・行に組まれたか（捨てられたならなぜか）を
// 紙面の上に重ねて見せる。パーサの回帰ケースを調べたり、新しいケースを足すときに使う。
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useSearchParams } from "react-router";
import { AlertTriangle, ChevronDown, ChevronRight, Copy, ExternalLink, FileUp, Upload } from "lucide-react";
import { Badge } from "../ui/badge";
import { Button } from "../ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../ui/select";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "../ui/tabs";
import { cn } from "../ui/utils";
import { AmendDocumentView } from "../amend-document-view";
import { KanpoPageView, usePdfDocument, type LineInfo, type PdfSource, type Zoom } from "./kanpo-page-view";
import {
  ARCHIVE_BASE,
  CORPUS_CASES,
  ROLE_COLOR,
  ROLE_LABEL,
  STATUS_LABEL,
  archiveBboxUrl,
  archivePdfUrl,
  fetchArchiveDates,
  fetchArchiveManifest,
  issueTypeLabel,
  loadKanpoParser,
  type ArchiveManifest,
  type KanpoAnalysis,
  type KanpoParser,
  type KanpoWordStatus,
} from "../../data/kanpo-playground";

type SourceKind = "corpus" | "archive" | "local";

/** 解析対象（bbox の XHTML と、あれば PDF）。 */
type Input = { bbox: string; pdf: PdfSource | null; pdfPage: number };

const BBOX_COMMAND = "pdftotext -bbox-layout -enc UTF-8 page.pdf page.bbox.html";

async function fetchText(url: string): Promise<string> {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${res.status} ${res.statusText}: ${url}`);
  return res.text();
}

/** container だけをスクロールして el を見える位置に入れる（ページ全体は動かさない）。 */
function scrollWithin(container: HTMLElement, el: HTMLElement) {
  const c = container.getBoundingClientRect();
  const r = el.getBoundingClientRect();
  if (r.top < c.top) container.scrollTop += r.top - c.top - 8;
  else if (r.bottom > c.bottom) container.scrollTop += r.bottom - c.bottom + 8;
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
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      {c && (
        <div className="text-sm space-y-1">
          <div className="font-medium break-words">{c.meta.title}</div>
          <div className="flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
            {[c.meta.date, c.meta.issue, c.meta.page != null ? `${c.meta.page} ページ` : null].filter(Boolean).join(" · ")}
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

function ArchivePicker({
  date,
  dir,
  page,
  onChange,
  onInput,
}: {
  date: string | null;
  dir: string | null;
  page: number | null;
  onChange: (v: { date: string; dir?: string; page?: number }) => void;
  onInput: (urls: { bbox: string; pdf: string } | null, error?: string) => void;
}) {
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
    if (!date && dates?.length) onChange({ date: dates[0] });
  }, [date, dates, onChange]);

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
  // 号・ページが未指定なら先頭を選ぶ。
  useEffect(() => {
    if (!manifest || !date) return;
    const first = manifest.issues[0];
    if (!issue && first) onChange({ date, dir: first.dir, page: first.pages[0]?.page });
    else if (issue && page == null && issue.pages[0]) onChange({ date, dir: issue.dir, page: issue.pages[0].page });
  }, [manifest, issue, date, page, onChange]);

  const entry = issue?.pages.find((p) => p.page === page) ?? null;
  useEffect(() => {
    if (!date || !issue || !entry) return onInput(null);
    if (!entry.bbox) return onInput(null, "このページは座標付きテキスト（.bbox.html）が作られていません。");
    onInput({ bbox: archiveBboxUrl(date, issue.dir, entry.file), pdf: archivePdfUrl(date, issue.dir, entry.file) });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [date, issue?.dir, entry?.file]);

  const itemsOnPage = issue?.items.filter((it) => it.page === page) ?? [];
  return (
    <div className="space-y-2 min-w-0">
      <div className="flex flex-wrap gap-2">
        <Select value={date ?? undefined} onValueChange={(d) => onChange({ date: d })}>
          <SelectTrigger className="w-40"><SelectValue placeholder={dates ? "日付" : "読み込み中…"} /></SelectTrigger>
          <SelectContent>
            {dates?.map((d) => <SelectItem key={d} value={d}>{d}</SelectItem>)}
          </SelectContent>
        </Select>
        <Select
          value={issue?.dir}
          onValueChange={(v) => {
            const i = manifest?.issues.find((x) => x.dir === v);
            if (date && i) onChange({ date, dir: v, page: i.pages[0]?.page });
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
        <Select
          value={page != null ? String(page) : undefined}
          onValueChange={(v) => date && issue && onChange({ date, dir: issue.dir, page: Number(v) })}
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
        {issue && (
          <a href={issue.url} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-primary">
            デジタル官報 <ExternalLink className="size-3" />
          </a>
        )}
      </div>
      {itemsOnPage.length > 0 && (
        <ul className="text-xs text-muted-foreground space-y-0.5">
          {itemsOnPage.map((it, i) => <li key={i} className="break-words">・{it.title}</li>)}
        </ul>
      )}
      {error && <p className="text-xs text-destructive break-words">{error}</p>}
    </div>
  );
}

function LocalPicker({ names, onFiles }: { names: { bbox: string | null; pdf: string | null }; onFiles: (files: File[]) => void }) {
  const [dragging, setDragging] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const [copied, setCopied] = useState(false);
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
        <div>.bbox.html（必須）と、あれば同じページの .pdf をドロップ</div>
        <Button variant="outline" size="sm" className="mt-2" onClick={() => inputRef.current?.click()}>
          <FileUp className="size-4" /> ファイルを選ぶ
        </Button>
        <input
          ref={inputRef}
          type="file"
          multiple
          accept=".html,.xhtml,.htm,.pdf"
          className="hidden"
          onChange={(e) => {
            onFiles(Array.from(e.target.files ?? []));
            e.target.value = "";
          }}
        />
        {(names.bbox || names.pdf) && (
          <div className="mt-2 text-xs text-muted-foreground break-all">
            bbox: {names.bbox ?? "未選択"} / PDF: {names.pdf ?? "なし"}
          </div>
        )}
      </div>
      <div className="text-xs text-muted-foreground">
        bbox の作り方（poppler の pdftotext）:
        <div className="mt-1 flex items-center gap-1 min-w-0">
          <code className="flex-1 min-w-0 overflow-x-auto whitespace-nowrap rounded bg-muted px-2 py-1 font-mono">{BBOX_COMMAND}</code>
          <Button
            variant="ghost"
            size="icon"
            className="size-7 shrink-0"
            title="コピー"
            onClick={() => {
              navigator.clipboard?.writeText(BBOX_COMMAND).then(() => {
                setCopied(true);
                setTimeout(() => setCopied(false), 1200);
              });
            }}
          >
            <Copy className="size-3.5" />
          </Button>
          {copied && <span>コピーしました</span>}
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

  const loadFromUrls = useCallback((urls: { bbox: string; pdf: string | null } | null, error?: string) => {
    setInput(null);
    setInputError(error ?? null);
    if (!urls) return;
    setLoadingInput(true);
    fetchText(urls.bbox).then(
      (bbox) => {
        setInput({ bbox, pdf: urls.pdf ? { url: urls.pdf } : null, pdfPage: 1 });
        setLoadingInput(false);
      },
      (e) => {
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
    loadFromUrls(c ? { bbox: c.bboxUrl, pdf: c.pdfUrl } : null, caseId && !c ? `ケース ${caseId} が見つかりません。` : undefined);
  }, [src, caseId, loadFromUrls]);

  // ローカル（URL には載せない）
  const [local, setLocal] = useState<{ bbox: string | null; bboxName: string | null; pdf: ArrayBuffer | null; pdfName: string | null }>({
    bbox: null,
    bboxName: null,
    pdf: null,
    pdfName: null,
  });
  const onLocalFiles = useCallback(async (files: File[]) => {
    const next = { ...local };
    for (const f of files) {
      if (/\.pdf$/i.test(f.name)) {
        next.pdf = await f.arrayBuffer();
        next.pdfName = f.name;
      } else if (/\.(x?html?)$/i.test(f.name)) {
        next.bbox = await f.text();
        next.bboxName = f.name;
      }
    }
    setLocal(next);
  }, [local]);
  const localPdf = useMemo<PdfSource | null>(() => (local.pdf ? { data: local.pdf } : null), [local.pdf]);
  useEffect(() => {
    if (src !== "local") return;
    setInputError(null);
    setLoadingInput(false);
    setInput(local.bbox ? { bbox: local.bbox, pdf: localPdf, pdfPage: 1 } : null);
  }, [src, local.bbox, localPdf]);

  // アーカイブ
  const archiveSel = {
    date: params.get("date"),
    dir: params.get("dir"),
    page: params.get("page") ? Number(params.get("page")) : null,
  };
  const onArchiveChange = useCallback(
    (v: { date: string; dir?: string; page?: number }) => updateParams({ src: "archive", ...v }),
    [updateParams],
  );

  // ---- 解析 ----
  const { analysis, analyzeError, elapsed } = useMemo(() => {
    if (!parser || !input) return { analysis: null, analyzeError: null, elapsed: 0 };
    const t0 = performance.now();
    try {
      const a: KanpoAnalysis = parser.analyze(input.bbox);
      return { analysis: a, analyzeError: null, elapsed: performance.now() - t0 };
    } catch (e: any) {
      return { analysis: null, analyzeError: String(e?.message ?? e), elapsed: 0 };
    }
  }, [parser, input]);

  const [pageIndex, setPageIndex] = useState(0);
  useEffect(() => setPageIndex(0), [analysis]);
  const page = analysis?.pages[pageIndex] ?? null;
  const { doc: pdf, error: pdfError } = usePdfDocument(input?.pdf ?? null);

  // 行の通し番号（読み順）と、語 → 行の対応。
  const { lines, wordLine } = useMemo(() => {
    const lines: LineInfo[] = [];
    const wordLine = new Map<number, LineInfo>();
    page?.regions.forEach((r, ri) =>
      r.lines.forEach((l, li) => {
        const info: LineInfo = { key: `${ri}:${li}`, order: lines.length, role: r.role, words: l.words, text: l.text };
        lines.push(info);
        for (const w of l.words) wordLine.set(w, info);
      }),
    );
    return { lines, wordLine };
  }, [page]);

  const statusCounts = useMemo(() => {
    const c: Partial<Record<KanpoWordStatus, number>> = {};
    for (const w of page?.words ?? []) c[w.status] = (c[w.status] ?? 0) + 1;
    return c;
  }, [page]);

  // ---- 表示状態と連動 ----
  const [zoom, setZoom] = useState<Zoom>("fit");
  const [showDropped, setShowDropped] = useState(true);
  const [showOrder, setShowOrder] = useState(false);
  const [tab, setTab] = useState("lines");
  const [hoverWord, setHoverWord] = useState<number | null>(null);
  const [hoverPanelLine, setHoverPanelLine] = useState<string | null>(null);
  const [selectedLine, setSelectedLine] = useState<string | null>(null);
  const [scrollToLine, setScrollToLine] = useState<{ key: string; nonce: number } | null>(null);
  useEffect(() => {
    setSelectedLine(null);
    setHoverWord(null);
    setHoverPanelLine(null);
  }, [page]);

  const hoverWordLine = hoverWord != null ? wordLine.get(hoverWord)?.key ?? null : null;
  const activeLine = hoverWordLine ?? hoverPanelLine ?? selectedLine;

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

  const onClickWord = useCallback(
    (id: number) => {
      const key = wordLine.get(id)?.key;
      if (!key) return;
      setSelectedLine(key);
      setTab("lines");
      // タブ切替後に描画されてからスクロールする。
      requestAnimationFrame(() => scrollPanelTo(key));
    },
    [wordLine, scrollPanelTo],
  );

  const json = useMemo(() => (analysis ? JSON.stringify(analysis, null, 2) : ""), [analysis]);

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
                  updateParams({ src: k });
                  setInput(null);
                  setInputError(null);
                }}
              >
                {label}
              </button>
            ))}
          </div>
          <div className="flex-1 min-w-0">
            {src === "corpus" && <CorpusPicker caseId={caseId} onSelect={(id) => updateParams({ src: "corpus", case: id })} />}
            {src === "archive" && (
              <ArchivePicker
                date={archiveSel.date}
                dir={archiveSel.dir}
                page={archiveSel.page}
                onChange={onArchiveChange}
                onInput={loadFromUrls}
              />
            )}
            {src === "local" && <LocalPicker names={{ bbox: local.bboxName, pdf: local.pdfName }} onFiles={onLocalFiles} />}
          </div>
        </div>
        {(inputError || analyzeError || pdfError) && (
          <div className="text-sm text-destructive space-y-0.5 break-words">
            {inputError && <div>{inputError}</div>}
            {analyzeError && <div>解析に失敗しました: {analyzeError}</div>}
            {pdfError && <div>PDF を開けませんでした（白紙に語を描きます）: {pdfError}</div>}
          </div>
        )}
      </div>

      {loadingInput && <div className="text-sm text-muted-foreground">読み込み中…</div>}
      {analysis && !page && (
        <div className="text-sm text-muted-foreground">
          ページが見つかりません。<code className="font-mono">pdftotext -bbox-layout</code> の出力（&lt;page&gt; と &lt;word&gt; を含む XHTML）か確認してください。
        </div>
      )}

      {analysis && page && (
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
              {analysis.pages.length > 1 && (
                <Select value={String(pageIndex)} onValueChange={(v) => setPageIndex(Number(v))}>
                  <SelectTrigger size="sm" className="w-28 h-7"><SelectValue /></SelectTrigger>
                  <SelectContent>
                    {analysis.pages.map((_, i) => <SelectItem key={i} value={String(i)}>{i + 1} / {analysis.pages.length}</SelectItem>)}
                  </SelectContent>
                </Select>
              )}
              {!input?.pdf && <span className="text-muted-foreground">PDF なし（語のテキストを白紙に描画）</span>}
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
            </div>
            <div className="flex-1 min-h-0">
              <KanpoPageView
                page={page}
                pdf={pdf}
                pdfPage={(input?.pdfPage ?? 1) + pageIndex}
                lines={lines}
                wordLine={wordLine}
                zoom={zoom}
                showDropped={showDropped}
                showOrder={showOrder}
                activeLine={activeLine}
                hoverWord={hoverWord}
                scrollToLine={scrollToLine}
                onHoverWord={setHoverWord}
                onClickWord={onClickWord}
              />
            </div>
          </section>

          {/* 結果 */}
          <section className="flex flex-col gap-2 min-w-0 xl:min-h-0">
            <div className="flex flex-wrap gap-x-4 gap-y-1 text-xs">
              <Stat label="layout">{page.layout}</Stat>
              <Stat label="format">{analysis.format}</Stat>
              <Stat label="語">{page.words.length}</Stat>
              {(Object.keys(STATUS_LABEL) as KanpoWordStatus[]).map((s) =>
                statusCounts[s] ? <Stat key={s} label={STATUS_LABEL[s]}>{statusCounts[s]}</Stat> : null,
              )}
              <Stat label="行">{lines.length}</Stat>
              <Stat label="記事">{analysis.articles.length}</Stat>
              <Stat label="解析">{elapsed.toFixed(1)} ms</Stat>
            </div>
            <Tabs value={tab} onValueChange={setTab} className="flex-1 min-h-0">
              <TabsList className="w-full max-w-md">
                <TabsTrigger value="lines">行</TabsTrigger>
                <TabsTrigger value="text">本文</TabsTrigger>
                <TabsTrigger value="articles">記事・構造</TabsTrigger>
                <TabsTrigger value="json">JSON</TabsTrigger>
              </TabsList>
              <TabsContent value="lines" className="min-h-0">
                <div ref={linesPanelRef} className="max-h-[70vh] xl:max-h-none xl:h-full overflow-auto rounded-md border border-border">
                  {page.regions.map((r, ri) => (
                    <div key={ri}>
                      <div className="sticky top-0 z-[1] flex items-center gap-2 bg-background/95 backdrop-blur px-2 py-1 border-b border-border text-xs">
                        <span className="size-2.5 rounded-sm" style={{ background: ROLE_COLOR[r.role] }} />
                        <span className="font-medium">{ROLE_LABEL[r.role]}</span>
                        <span className="text-muted-foreground">
                          段 {r.unit + 1}
                          {r.part > 0 ? ` · 続き ${r.part}` : ""} · {r.lines.length} 行
                        </span>
                      </div>
                      {r.lines.map((l, li) => {
                        const key = `${ri}:${li}`;
                        const info = wordLine.get(l.words[0]);
                        const active = activeLine === key;
                        return (
                          <button
                            key={key}
                            type="button"
                            data-line={key}
                            className={cn(
                              "w-full text-left flex gap-2 px-2 py-0.5 text-sm border-l-2 transition-colors",
                              active ? "bg-accent" : "hover:bg-accent/50",
                              selectedLine === key ? "font-medium" : "",
                            )}
                            style={{ borderLeftColor: active || selectedLine === key ? ROLE_COLOR[r.role] : "transparent" }}
                            onMouseEnter={() => setHoverPanelLine(key)}
                            onMouseLeave={() => setHoverPanelLine(null)}
                            onClick={() => {
                              setSelectedLine(key);
                              setScrollToLine({ key, nonce: Date.now() });
                            }}
                          >
                            <span className="w-8 shrink-0 text-right font-mono text-xs text-muted-foreground pt-0.5">
                              {(info?.order ?? 0) + 1}
                            </span>
                            <span className="min-w-0 break-all">{l.text || <span className="text-muted-foreground">（空）</span>}</span>
                          </button>
                        );
                      })}
                    </div>
                  ))}
                  {page.regions.length === 0 && <p className="p-3 text-sm text-muted-foreground">行がありません。</p>}
                </div>
              </TabsContent>
              <TabsContent value="text" className="min-h-0">
                <pre className="max-h-[70vh] xl:max-h-none xl:h-full overflow-auto rounded-md border border-border p-3 text-sm whitespace-pre-wrap break-all font-sans leading-relaxed">
                  {analysis.text || "（空）"}
                </pre>
              </TabsContent>
              <TabsContent value="articles" className="min-h-0">
                <div className="max-h-[70vh] xl:max-h-none xl:h-full overflow-auto space-y-3">
                  {analysis.articles.map((a, i) => (
                    <div key={i} className="rounded-md border border-border p-2 space-y-1.5">
                      <div className="flex items-center gap-2 text-xs">
                        <Badge variant="secondary">記事 {i + 1}</Badge>
                        <span className="text-muted-foreground">{a.document.format}</span>
                        <span className="text-muted-foreground">{a.document.blocks.length} ブロック</span>
                      </div>
                      <AmendDocumentView doc={a.document} className="text-xs" />
                      <details>
                        <summary className="text-xs text-muted-foreground cursor-pointer select-none">本文テキスト</summary>
                        <pre className="mt-1 text-xs whitespace-pre-wrap break-all font-sans leading-relaxed">{a.text}</pre>
                      </details>
                    </div>
                  ))}
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

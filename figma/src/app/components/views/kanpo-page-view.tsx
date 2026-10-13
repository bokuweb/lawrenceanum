// 官報パーサ playground の紙面ビュー。pdf.js で PDF を描き、その上に analyze() の語の
// 矩形を PDF の pt 座標の SVG で重ねる。PDF が無ければ白紙に語のテキストを描く。
// 項目の全ページを縦に並べ、画面から離れたページは描かない（大きさだけの枠にする）。
import { memo, useEffect, useMemo, useRef, useState, type MouseEvent as ReactMouseEvent } from "react";
import { getDocument, GlobalWorkerOptions, type PDFDocumentProxy, type RenderTask } from "pdfjs-dist";
import pdfWorkerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";
import {
  ROLE_COLOR,
  STATUS_LABEL,
  STATUS_REASON,
  type KanpoPageRules,
  type KanpoPageTrace,
  type KanpoRole,
  type KanpoTable,
  type KanpoWord,
} from "../../data/kanpo-playground";

GlobalWorkerOptions.workerSrc = pdfWorkerUrl;

// 官報の和文フォント（CID の Identity-H/V）は pdf.js の CMap が無いとグリフを描けない。
// ビルド後のアセットはハッシュ付きの名前で 1 ディレクトリに並ばず cMapUrl を使えないので、
// CMap 名 → URL の表を glob で作り、それを引く CMapReaderFactory を渡す（必要な分だけ取得）。
const cmapUrls = import.meta.glob<string>("/node_modules/pdfjs-dist/cmaps/*.bcmap", {
  eager: true,
  query: "?url&no-inline",
  import: "default",
});
const cmapUrlByName = new Map(
  Object.entries(cmapUrls).map(([path, url]) => [path.slice(path.lastIndexOf("/") + 1, -".bcmap".length), url]),
);

class GlobCMapReaderFactory {
  async fetch({ name }: { name: string }) {
    const url = cmapUrlByName.get(name);
    if (!url) throw new Error(`CMap ${name} がありません`);
    const res = await fetch(url);
    if (!res.ok) throw new Error(`CMap ${name} を取得できません: ${res.status}`);
    return { cMapData: new Uint8Array(await res.arrayBuffer()), isCompressed: true };
  }
}

/** PDF の入力。URL（コーパス・アーカイブ）かバイト列（ローカル）。 */
export type PdfSource = { url: string } | { data: ArrayBuffer };

/** PDF を開く。失敗しても playground は白紙モードで続けるので、エラーは文字列で返す。 */
export function usePdfDocument(source: PdfSource | null): { doc: PDFDocumentProxy | null; error: string | null } {
  const [state, setState] = useState<{ doc: PDFDocumentProxy | null; error: string | null }>({ doc: null, error: null });
  useEffect(() => {
    setState({ doc: null, error: null });
    if (!source) return;
    // pdf.js はバイト列を worker へ移譲（detach）するので複製して渡す。
    const task = getDocument({
      ...("url" in source ? { url: source.url } : { data: new Uint8Array(source.data.slice(0)) }),
      CMapReaderFactory: GlobCMapReaderFactory,
    });
    let cancelled = false;
    task.promise.then(
      (doc) => { if (!cancelled) setState({ doc, error: null }); },
      (e) => { if (!cancelled) setState({ doc: null, error: String(e?.message ?? e) }); },
    );
    return () => {
      cancelled = true;
      task.destroy();
    };
  }, [source]);
  return state;
}


/**
 * 行 1 本分の描画情報（項目全体の読み順に並ぶ）。key は `${page}:${region}:${line}`。
 * 罫線の表の行は table にセル構造を持つ。
 */
export type LineInfo = {
  key: string;
  page: number;
  order: number;
  role: KanpoRole;
  words: number[];
  text: string;
  table?: KanpoTable;
};

/** 表のセルの参照（行の key と table.cells の添字）。 */
export type CellRef = { line: string; cell: number };

/** 語の参照キー（ページをまたいで一意）。 */
export const wordKey = (page: number, id: number) => `${page}:${id}`;

/** 1 ページ分の紙面の入力（PDF・そのページ番号・罫線・見出し）。 */
export type SheetSource = { pdf: PdfSource | null; pdfPage: number; rules: KanpoPageRules | null; label: string };

export type Zoom = "fit" | 1 | 1.5;

/** 縦書きの語か（縦長の矩形）。 */
const isVertical = (w: KanpoWord) => w.y1 - w.y0 > (w.x1 - w.x0) * 1.2;

/** 罫線と復元した表の色（役割の色と紛れない色）。 */
const RULE_COLOR = "#0891b2";
const TABLE_COLOR = "#0f766e";

type SheetProps = {
  index: number;
  page: KanpoPageTrace;
  source: SheetSource;
  /** このページの行。 */
  lines: LineInfo[];
  wordLine: Map<string, LineInfo>;
  scale: number;
  root: HTMLElement | null;
  showDropped: boolean;
  showOrder: boolean;
  showRules: boolean;
  /** このページで強調する語（行・セルのホバーや選択）。 */
  activeWords: Set<number>;
  activeCell: CellRef | null;
  hoverWord: number | null;
};

/** 1 ページ分の紙面。画面の近くに来たときだけ PDF とオーバーレイを描く。 */
const PageSheet = memo(function PageSheet({
  index,
  page,
  source,
  lines,
  wordLine,
  scale,
  root,
  showDropped,
  showOrder,
  showRules,
  activeWords,
  activeCell,
  hoverWord,
}: SheetProps) {
  const boxRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [visible, setVisible] = useState(false);
  // 一度見えたページだけ PDF を開く（多ページの項目で全部を先に開かない）。
  const [seen, setSeen] = useState(false);
  const { doc: pdf, error: pdfOpenError } = usePdfDocument(seen ? source.pdf : null);
  const [pdfError, setPdfError] = useState<string | null>(null);
  const cssW = page.width * scale;
  const cssH = page.height * scale;

  useEffect(() => {
    const el = boxRef.current;
    if (!el || !root) return;
    const io = new IntersectionObserver(
      ([e]) => {
        setVisible(e.isIntersecting);
        if (e.isIntersecting) setSeen(true);
      },
      { root, rootMargin: "600px 0px" },
    );
    io.observe(el);
    return () => io.disconnect();
  }, [root]);

  // PDF をキャンバスへ描く。解析結果の幅（pdftotext の pt）に合わせて拡縮する。
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!pdf || !visible || !canvas || !cssW) return;
    let task: RenderTask | null = null;
    let cancelled = false;
    setPdfError(null);
    pdf.getPage(source.pdfPage).then(
      (p) => {
        if (cancelled) return;
        const base = p.getViewport({ scale: 1 });
        const dpr = window.devicePixelRatio || 1;
        const vp = p.getViewport({ scale: (cssW / base.width) * dpr });
        canvas.width = Math.floor(vp.width);
        canvas.height = Math.floor(vp.height);
        task = p.render({ canvasContext: canvas.getContext("2d")!, viewport: vp });
        task.promise.catch((e) => {
          if (!cancelled && e?.name !== "RenderingCancelledException") setPdfError(String(e?.message ?? e));
        });
      },
      (e) => { if (!cancelled) setPdfError(String(e?.message ?? e)); },
    );
    return () => {
      cancelled = true;
      task?.cancel();
    };
  }, [pdf, visible, source.pdfPage, cssW]);

  // 読み順: 行の順に語の中心を結ぶ折れ線と、各行の先頭語の行番号。
  const order = useMemo(() => {
    if (!showOrder || !visible) return null;
    const pts: string[] = [];
    const labels: { x: number; y: number; n: number; color: string }[] = [];
    for (const l of lines) {
      l.words.forEach((id, i) => {
        const w = page.words[id];
        if (!w) return;
        pts.push(`${((w.x0 + w.x1) / 2).toFixed(1)},${((w.y0 + w.y1) / 2).toFixed(1)}`);
        if (i === 0) labels.push({ x: w.x1, y: w.y0, n: l.order + 1, color: ROLE_COLOR[l.role] });
      });
    }
    return { pts: pts.join(" "), labels };
  }, [showOrder, visible, lines, page.words]);

  const tables = useMemo(() => lines.filter((l) => l.table), [lines]);
  const drawText = !source.pdf || !!pdfOpenError;
  const error = pdfOpenError ?? pdfError;

  return (
    <div className="flex flex-col items-center gap-1">
      <div className="self-stretch text-[11px] text-muted-foreground font-mono px-1">{source.label}</div>
      <div ref={boxRef} data-sheet={index} className="relative bg-white shadow-sm" style={{ width: cssW, height: cssH }}>
        {visible && (
          <>
            {source.pdf && !pdfOpenError && (
              <canvas ref={canvasRef} className="absolute inset-0" style={{ width: cssW, height: cssH }} />
            )}
            <svg
              className="absolute inset-0"
              width={cssW}
              height={cssH}
              viewBox={`0 0 ${page.width} ${page.height}`}
              data-page={index}
            >
              {showRules && source.rules && (
                <g pointerEvents="none" stroke={RULE_COLOR} strokeWidth={0.6} strokeOpacity={0.9}>
                  {source.rules.vertical.map((r, i) => <line key={`v${i}`} x1={r.x} x2={r.x} y1={r.y0} y2={r.y1} />)}
                  {source.rules.horizontal.map((r, i) => <line key={`h${i}`} x1={r.x0} x2={r.x1} y1={r.y} y2={r.y} />)}
                </g>
              )}
              {/* 復元した表: 外枠と各セルの枠（セルは語の下に置き、語の無い所でホバーできる）。 */}
              {tables.map((l) => {
                const t = l.table!;
                return (
                  <g key={l.key}>
                    {t.cells.map((c, ci) => {
                      const active = activeCell?.line === l.key && activeCell.cell === ci;
                      return (
                        <rect
                          key={ci}
                          data-cell={`${l.key}|${ci}`}
                          x={c.x0}
                          y={c.y0}
                          width={Math.max(0.5, c.x1 - c.x0)}
                          height={Math.max(0.5, c.y1 - c.y0)}
                          fill={TABLE_COLOR}
                          fillOpacity={active ? 0.16 : 0.001}
                          stroke={TABLE_COLOR}
                          strokeWidth={active ? 1 : 0.5}
                          strokeDasharray={active ? undefined : "2 1"}
                          className="cursor-pointer"
                        >
                          <title>{`行 ${l.order + 1} の表のセル (${c.row + 1}, ${c.col + 1})${c.rowspan > 1 || c.colspan > 1 ? ` ${c.rowspan}×${c.colspan} 結合` : ""} ${c.text}`}</title>
                        </rect>
                      );
                    })}
                    <rect
                      x={t.x0}
                      y={t.y0}
                      width={t.x1 - t.x0}
                      height={t.y1 - t.y0}
                      fill="none"
                      stroke={TABLE_COLOR}
                      strokeWidth={1.4}
                      pointerEvents="none"
                    />
                  </g>
                );
              })}
              {page.words.map((w) => {
                const line = wordLine.get(wordKey(index, w.id));
                const dropped = w.status !== "used";
                if (dropped && !showDropped) return null;
                const color = line ? ROLE_COLOR[line.role] : "#6b7280";
                const active = activeWords.has(w.id) || w.id === hoverWord;
                const width = Math.max(0.5, w.x1 - w.x0);
                const height = Math.max(0.5, w.y1 - w.y0);
                const vertical = isVertical(w);
                return (
                  <g key={w.id}>
                    {drawText && w.text && (
                      <text
                        x={vertical ? (w.x0 + w.x1) / 2 : w.x0}
                        y={vertical ? w.y0 : w.y1 - height * 0.15}
                        fontSize={vertical ? width : height * 0.85}
                        writingMode={vertical ? "vertical-rl" : undefined}
                        fill={dropped ? "#9ca3af" : "#111827"}
                        pointerEvents="none"
                      >
                        {w.text}
                      </text>
                    )}
                    <rect
                      data-w={w.id}
                      x={w.x0}
                      y={w.y0}
                      width={width}
                      height={height}
                      fill={dropped ? "transparent" : color}
                      fillOpacity={dropped ? 0 : active ? 0.4 : 0.12}
                      stroke={color}
                      strokeOpacity={dropped ? 0.7 : active ? 1 : 0.55}
                      strokeWidth={active ? 1.2 : 0.4}
                      strokeDasharray={dropped ? "1.5 1" : undefined}
                      className="cursor-pointer"
                    >
                      <title>
                        {dropped
                          ? `${w.text || w.raw}（${STATUS_LABEL[w.status]}: ${STATUS_REASON[w.status]}）`
                          : `${w.text}（行 ${(line?.order ?? 0) + 1}）`}
                      </title>
                    </rect>
                  </g>
                );
              })}
              {order && (
                <g pointerEvents="none">
                  <polyline points={order.pts} fill="none" stroke="#0f172a" strokeOpacity={0.45} strokeWidth={0.5} />
                  {order.labels.map((l) => (
                    <g key={l.n}>
                      <rect x={l.x + 0.3} y={l.y - 5} width={l.n >= 100 ? 9 : l.n >= 10 ? 6.5 : 4.5} height={5} rx={1} fill={l.color} />
                      <text x={l.x + 0.9} y={l.y - 1} fontSize={4} fill="#fff" fontFamily="ui-monospace, monospace">
                        {l.n}
                      </text>
                    </g>
                  ))}
                </g>
              )}
            </svg>
            {error && (
              <div className="absolute left-1 top-1 right-1 rounded bg-white/90 px-1.5 py-0.5 text-[11px] text-red-600 break-all">
                PDF を描画できませんでした（語のテキストを描きます）: {error}
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
});

const EMPTY = new Set<number>();

export function KanpoPageView({
  pages,
  sources,
  lines,
  wordLine,
  zoom,
  showDropped,
  showOrder,
  showRules,
  activeLine,
  activeCell,
  hoverWord,
  scrollToLine,
  onHoverWord,
  onHoverCell,
  onClickWord,
  onClickLine,
}: {
  pages: KanpoPageTrace[];
  /** pages と同じ長さ。 */
  sources: SheetSource[];
  lines: LineInfo[];
  /** `wordKey(page, id)` → 行。 */
  wordLine: Map<string, LineInfo>;
  zoom: Zoom;
  showDropped: boolean;
  showOrder: boolean;
  showRules: boolean;
  /** 強調する行（パネルのホバー・選択、または紙面のホバー）。 */
  activeLine: string | null;
  /** 強調する表のセル（紙面・パネルのホバー）。 */
  activeCell: CellRef | null;
  /** ホバー中の語（`wordKey(page, id)`）。 */
  hoverWord: string | null;
  /** この行の語（表なら表の枠）が見えるよう紙面をスクロールする（値が変わるたびに）。 */
  scrollToLine: { key: string; nonce: number } | null;
  onHoverWord: (key: string | null) => void;
  onHoverCell: (cell: CellRef | null) => void;
  onClickWord: (key: string) => void;
  onClickLine: (key: string) => void;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [container, setContainer] = useState<HTMLDivElement | null>(null);
  const [containerWidth, setContainerWidth] = useState(0);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    setContainer(el);
    const ro = new ResizeObserver(() => setContainerWidth(el.clientWidth));
    ro.observe(el);
    setContainerWidth(el.clientWidth);
    return () => ro.disconnect();
  }, []);

  // 1pt を何 px で描くか。幅合わせは左右の余白 16px を引く。
  const scaleOf = (p: KanpoPageTrace) => (zoom === "fit" ? Math.max(0.2, (containerWidth - 16) / p.width) : zoom);

  const lineByKey = useMemo(() => new Map(lines.map((l) => [l.key, l])), [lines]);
  const linesByPage = useMemo(() => {
    const m = pages.map(() => [] as LineInfo[]);
    for (const l of lines) m[l.page]?.push(l);
    return m;
  }, [pages, lines]);

  // パネルで行を選んだら、その行の語（表なら表の枠）が紙面の中央付近に来るようにスクロールする。
  useEffect(() => {
    if (!scrollToLine) return;
    const c = containerRef.current;
    const info = lineByKey.get(scrollToLine.key);
    const page = info ? pages[info.page] : null;
    const box = info ? c?.querySelector<HTMLElement>(`[data-sheet="${info.page}"]`) : null;
    if (!c || !info || !page || !box) return;
    let rect: { x0: number; y0: number; x1: number; y1: number } | null = info.table ?? null;
    if (!rect) {
      const ws = info.words.map((id) => page.words[id]).filter(Boolean);
      if (!ws.length) return;
      rect = {
        x0: Math.min(...ws.map((w) => w.x0)),
        y0: Math.min(...ws.map((w) => w.y0)),
        x1: Math.max(...ws.map((w) => w.x1)),
        y1: Math.max(...ws.map((w) => w.y1)),
      };
    }
    const scale = scaleOf(page);
    const cr = c.getBoundingClientRect();
    const br = box.getBoundingClientRect();
    const left = br.left - cr.left + c.scrollLeft + ((rect.x0 + rect.x1) / 2) * scale;
    const top = br.top - cr.top + c.scrollTop + ((rect.y0 + rect.y1) / 2) * scale;
    c.scrollTo({ left: left - c.clientWidth / 2, top: top - c.clientHeight / 2, behavior: "smooth" });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scrollToLine]);

  // ページごとの強調する語（行・セルの語）。
  const activeWordsByPage = useMemo(() => {
    const m = new Map<number, Set<number>>();
    const add = (page: number, ids: number[]) => {
      const s = m.get(page) ?? new Set<number>();
      ids.forEach((id) => s.add(id));
      m.set(page, s);
    };
    const line = activeLine ? lineByKey.get(activeLine) : null;
    if (line) add(line.page, line.words);
    const cellLine = activeCell ? lineByKey.get(activeCell.line) : null;
    const cell = cellLine?.table?.cells[activeCell!.cell];
    if (cellLine && cell) add(cellLine.page, cell.words);
    return m;
  }, [activeLine, activeCell, lineByKey]);

  const hover = useMemo(() => {
    if (!hoverWord) return null;
    const [p, id] = hoverWord.split(":").map(Number);
    return { page: p, id };
  }, [hoverWord]);
  const hovered = hover ? pages[hover.page]?.words[hover.id] : null;
  const hoveredLine = hoverWord ? wordLine.get(hoverWord) : null;
  const cellLine = activeCell ? lineByKey.get(activeCell.line) : null;
  const hoveredCell = cellLine?.table?.cells[activeCell!.cell] ?? null;

  /** SVG 上のイベントの対象（語かセル）。 */
  const targetOf = (e: ReactMouseEvent) => {
    const el = e.target as Element;
    const page = el.closest?.("svg[data-page]")?.getAttribute("data-page");
    const w = el.getAttribute?.("data-w");
    const cell = el.getAttribute?.("data-cell");
    if (page != null && w != null) return { word: wordKey(Number(page), Number(w)) };
    if (cell != null) {
      const [line, ci] = cell.split("|");
      return { cell: { line, cell: Number(ci) } };
    }
    return null;
  };

  return (
    <div className="flex flex-col min-h-0 h-full gap-2">
      <div
        ref={containerRef}
        className="relative flex-1 min-h-[320px] overflow-auto rounded-md border border-border bg-muted/40 p-2"
        onMouseOver={(e) => {
          const t = targetOf(e);
          onHoverWord(t?.word ?? null);
          onHoverCell(t?.cell ?? null);
        }}
        onMouseLeave={() => {
          onHoverWord(null);
          onHoverCell(null);
        }}
        onClick={(e) => {
          const t = targetOf(e);
          if (t?.word) onClickWord(t.word);
          else if (t?.cell) onClickLine(t.cell.line);
        }}
      >
        <div className="flex flex-col items-center gap-4 w-max min-w-full">
          {pages.map((p, i) => (
            <PageSheet
              key={i}
              index={i}
              page={p}
              source={sources[i]}
              lines={linesByPage[i] ?? []}
              wordLine={wordLine}
              scale={scaleOf(p)}
              root={container}
              showDropped={showDropped}
              showOrder={showOrder}
              showRules={showRules}
              activeWords={activeWordsByPage.get(i) ?? EMPTY}
              activeCell={cellLine?.page === i ? activeCell : null}
              hoverWord={hover?.page === i ? hover.id : null}
            />
          ))}
        </div>
      </div>
      {/* ホバー中の語（生の文字列と正規化後、扱い、所属行）やセルの詳細。 */}
      <div className="min-h-10 text-xs text-muted-foreground rounded-md border border-border px-2 py-1.5 break-all">
        {hovered && hover ? (
          <>
            {pages.length > 1 && <span className="font-mono">{sources[hover.page]?.label} · </span>}
            <span className="font-mono">#{hovered.id}</span>{" "}
            <span className="text-foreground">{hovered.text || "（空）"}</span>
            {hovered.raw !== hovered.text && <span>（生: {hovered.raw}）</span>}
            {" · "}
            {STATUS_LABEL[hovered.status]}
            {hovered.status !== "used" && <span>: {STATUS_REASON[hovered.status]}</span>}
            {hoveredLine && <span> · 行 {hoveredLine.order + 1}</span>}
            {hoveredCell && (
              <span> · 表のセル ({hoveredCell.row + 1}, {hoveredCell.col + 1})</span>
            )}
            <span className="font-mono">
              {" "}· ({hovered.x0.toFixed(1)}, {hovered.y0.toFixed(1)})–({hovered.x1.toFixed(1)}, {hovered.y1.toFixed(1)})
            </span>
          </>
        ) : hoveredCell && cellLine ? (
          <>
            表（行 {cellLine.order + 1}）のセル ({hoveredCell.row + 1}, {hoveredCell.col + 1})
            {(hoveredCell.rowspan > 1 || hoveredCell.colspan > 1) && ` · ${hoveredCell.rowspan}×${hoveredCell.colspan} 結合`}
            {" · "}語 {hoveredCell.words.length}
            {" · "}
            <span className="text-foreground">{hoveredCell.text || "（空）"}</span>
          </>
        ) : (
          "語にカーソルを合わせると詳細を表示します。クリックで行をパネルに表示します。"
        )}
      </div>
    </div>
  );
}

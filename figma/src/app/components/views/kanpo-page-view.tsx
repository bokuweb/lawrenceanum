// 官報パーサ playground の紙面ビュー。pdf.js で PDF を描き、その上に analyze() の語の
// 矩形を PDF の pt 座標の SVG で重ねる。PDF が無ければ白紙に語のテキストを描く。
import { useEffect, useMemo, useRef, useState } from "react";
import { getDocument, GlobalWorkerOptions, type PDFDocumentProxy, type RenderTask } from "pdfjs-dist";
import pdfWorkerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";
import { ROLE_COLOR, STATUS_LABEL, STATUS_REASON, type KanpoPageTrace, type KanpoRole, type KanpoWord } from "../../data/kanpo-playground";

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

/** 行 1 本分の描画情報（読み順に並ぶ）。key は `${region}:${line}`。 */
export type LineInfo = { key: string; order: number; role: KanpoRole; words: number[]; text: string };

export type Zoom = "fit" | 1 | 1.5;

/** 縦書きの語か（縦長の矩形）。 */
const isVertical = (w: KanpoWord) => w.y1 - w.y0 > (w.x1 - w.x0) * 1.2;

export function KanpoPageView({
  page,
  pdf,
  pdfPage,
  lines,
  wordLine,
  zoom,
  showDropped,
  showOrder,
  activeLine,
  hoverWord,
  scrollToLine,
  onHoverWord,
  onClickWord,
}: {
  page: KanpoPageTrace;
  pdf: PDFDocumentProxy | null;
  /** 描く PDF のページ番号（1 始まり）。 */
  pdfPage: number;
  lines: LineInfo[];
  wordLine: Map<number, LineInfo>;
  zoom: Zoom;
  showDropped: boolean;
  showOrder: boolean;
  /** 強調する行（パネルのホバー・選択、または紙面のホバー）。 */
  activeLine: string | null;
  hoverWord: number | null;
  /** この行の語が見えるよう紙面をスクロールする（値が変わるたびに）。 */
  scrollToLine: { key: string; nonce: number } | null;
  onHoverWord: (id: number | null) => void;
  onClickWord: (id: number) => void;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const svgRef = useRef<SVGSVGElement>(null);
  const [containerWidth, setContainerWidth] = useState(0);
  const [pdfError, setPdfError] = useState<string | null>(null);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setContainerWidth(el.clientWidth));
    ro.observe(el);
    setContainerWidth(el.clientWidth);
    return () => ro.disconnect();
  }, []);

  // 1pt を何 px で描くか。幅合わせは左右の余白 16px を引く。
  const scale = zoom === "fit" ? Math.max(0.2, (containerWidth - 16) / page.width) : zoom;
  const cssW = page.width * scale;
  const cssH = page.height * scale;

  // PDF をキャンバスへ描く。解析結果の幅（pdftotext の pt）に合わせて拡縮する。
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!pdf || !canvas || !cssW) return;
    let task: RenderTask | null = null;
    let cancelled = false;
    setPdfError(null);
    pdf.getPage(pdfPage).then(
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
  }, [pdf, pdfPage, cssW]);

  // パネルで行を選んだら、その行の語が紙面の中央付近に来るようにスクロールする。
  useEffect(() => {
    if (!scrollToLine) return;
    const container = containerRef.current;
    const info = lines.find((l) => l.key === scrollToLine.key);
    if (!container || !info?.words.length) return;
    const ws = info.words.map((id) => page.words[id]).filter(Boolean);
    const cx = ((Math.min(...ws.map((w) => w.x0)) + Math.max(...ws.map((w) => w.x1))) / 2) * scale;
    const cy = ((Math.min(...ws.map((w) => w.y0)) + Math.max(...ws.map((w) => w.y1))) / 2) * scale;
    container.scrollTo({
      left: cx - container.clientWidth / 2,
      top: cy - container.clientHeight / 2,
      behavior: "smooth",
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scrollToLine]);

  const activeWords = useMemo(() => {
    const info = activeLine ? lines.find((l) => l.key === activeLine) : null;
    return new Set(info?.words ?? []);
  }, [activeLine, lines]);

  // 読み順: 行の順に語の中心を結ぶ折れ線と、各行の先頭語の行番号。
  const order = useMemo(() => {
    if (!showOrder) return null;
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
  }, [showOrder, lines, page.words]);

  const hovered = hoverWord != null ? page.words[hoverWord] : null;
  const hoveredLine = hovered ? wordLine.get(hovered.id) : null;
  const drawText = !pdf;

  return (
    <div className="flex flex-col min-h-0 h-full gap-2">
      <div
        ref={containerRef}
        className="relative flex-1 min-h-[320px] overflow-auto rounded-md border border-border bg-muted/40 p-2"
      >
        <div className="relative mx-auto bg-white shadow-sm" style={{ width: cssW, height: cssH }}>
          {pdf && <canvas ref={canvasRef} className="absolute inset-0" style={{ width: cssW, height: cssH }} />}
          <svg
            ref={svgRef}
            className="absolute inset-0"
            width={cssW}
            height={cssH}
            viewBox={`0 0 ${page.width} ${page.height}`}
            onMouseOver={(e) => {
              const id = (e.target as Element).getAttribute?.("data-w");
              onHoverWord(id != null ? Number(id) : null);
            }}
            onMouseLeave={() => onHoverWord(null)}
            onClick={(e) => {
              const id = (e.target as Element).getAttribute?.("data-w");
              if (id != null) onClickWord(Number(id));
            }}
          >
            {page.words.map((w) => {
              const line = wordLine.get(w.id);
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
        </div>
      </div>
      {/* ホバー中の語の詳細（生の文字列と正規化後、扱い、所属行）。 */}
      <div className="min-h-10 text-xs text-muted-foreground rounded-md border border-border px-2 py-1.5 break-all">
        {hovered ? (
          <>
            <span className="font-mono">#{hovered.id}</span>{" "}
            <span className="text-foreground">{hovered.text || "（空）"}</span>
            {hovered.raw !== hovered.text && <span>（生: {hovered.raw}）</span>}
            {" · "}
            {STATUS_LABEL[hovered.status]}
            {hovered.status !== "used" && <span>: {STATUS_REASON[hovered.status]}</span>}
            {hoveredLine && <span> · 行 {hoveredLine.order + 1}</span>}
            <span className="font-mono">
              {" "}· ({hovered.x0.toFixed(1)}, {hovered.y0.toFixed(1)})–({hovered.x1.toFixed(1)}, {hovered.y1.toFixed(1)})
            </span>
          </>
        ) : pdfError ? (
          <span className="text-destructive">PDF を描画できませんでした: {pdfError}</span>
        ) : (
          "語にカーソルを合わせると詳細を表示します。クリックで行をパネルに表示します。"
        )}
      </div>
    </div>
  );
}

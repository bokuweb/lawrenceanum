import { useEffect, useMemo, useRef, useState } from "react";
import ForceGraph2D, { type ForceGraphMethods } from "react-force-graph-2d";
import type { WikiGraph } from "../../data/api";
import { WIKI_TYPES } from "./wiki-types";

// OKF ページ間リンクのナレッジグラフ。ノードの大きさは次数、色は type。
// react-force-graph は canvas 描画で重いので、このファイルごと遅延ロードする。
//
// wiki は 1,800 ページ・7,000 リンクを超えるので、全部を一度に力学配置すると毛玉になって
// 読めず、描画も止まる。そこで (1) 既定ではつながりの多い順に一定数だけ置き、
// (2) ラベルは重なりを避けて上位だけ描く。全体を見たいときは「表示範囲」で広げられる。

/** 「表示範囲」の選択肢。既定はつながりの多い 250 件。 */
export const GRAPH_BUDGETS = [100, 250, 500, 1000, Infinity] as const;
const DEFAULT_BUDGET = 250;
/** 1 フレームに描くラベルの上限 (重なりを避けた結果これより少なくなることもある)。
 *  ズームすると文字が小さくなって重なりが減るので、寄ったときは多めに出す。 */
const MAX_LABELS = 48;
const MAX_LABELS_ZOOMED = 120;
/** ズームしていないときにラベルを出す候補 (つながりの多い順の順位)。 */
const LABEL_CANDIDATES = MAX_LABELS * 4;
const ZOOMED_IN = 1.6;

type Node = WikiGraph["nodes"][number] & { degree: number; x?: number; y?: number };

export function WikiGraphCanvas({
  graph,
  visibleTypes,
  showStubs,
  focus,
  onOpen,
}: {
  graph: WikiGraph;
  visibleTypes: Set<string>;
  showStubs: boolean;
  /** 中心に据えるページ (そのページと直接つながるページだけを描く)。 */
  focus?: string | null;
  onOpen: (path: string) => void;
}) {
  const box = useRef<HTMLDivElement>(null);
  const fg = useRef<ForceGraphMethods | undefined>(undefined);
  const [size, setSize] = useState({ width: 800, height: 560 });
  const [budget, setBudget] = useState<number>(DEFAULT_BUDGET);
  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const ro = new ResizeObserver(([e]) => {
      setSize({ width: Math.max(320, e.contentRect.width), height: Math.max(360, e.contentRect.height) });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // force-graph はノード/リンクを書き換えるため、毎回コピーを渡す。
  const { data, total } = useMemo(() => {
    const shown = graph.nodes.filter(n => visibleTypes.has(n.type) && (showStubs || !n.stub));
    const shownIds = new Set(shown.map(n => n.id));
    const links = graph.links.filter(l => shownIds.has(l.source) && shownIds.has(l.target));
    // 次数は絞り込む前に数える (「つながりの多い順」の基準にするため)。
    const degree = new Map<string, number>();
    const neighbors = new Map<string, Set<string>>();
    for (const l of links) {
      degree.set(l.source, (degree.get(l.source) ?? 0) + 1);
      degree.set(l.target, (degree.get(l.target) ?? 0) + 1);
      if (!neighbors.has(l.source)) neighbors.set(l.source, new Set());
      if (!neighbors.has(l.target)) neighbors.set(l.target, new Set());
      neighbors.get(l.source)!.add(l.target);
      neighbors.get(l.target)!.add(l.source);
    }

    let picked: typeof shown;
    if (focus && shownIds.has(focus)) {
      // 中心が指定されたら、その隣接だけにする (何とつながっているかを読める大きさにする)。
      const keep = new Set<string>([focus, ...(neighbors.get(focus) ?? [])]);
      picked = shown.filter(n => keep.has(n.id));
    } else {
      picked = [...shown].sort(
        (a, b) => (degree.get(b.id) ?? 0) - (degree.get(a.id) ?? 0) || a.title.localeCompare(b.title, "ja"),
      );
      if (Number.isFinite(budget)) picked = picked.slice(0, budget);
    }
    const ids = new Set(picked.map(n => n.id));
    return {
      total: shown.length,
      data: {
        nodes: picked.map(n => ({ ...n, degree: degree.get(n.id) ?? 0 })) as Node[],
        links: links.filter(l => ids.has(l.source) && ids.has(l.target)).map(l => ({ ...l })),
      },
    };
  }, [graph, visibleTypes, showStubs, budget, focus]);

  const labelColor = useMemo(() => {
    const v = getComputedStyle(document.documentElement).getPropertyValue("--foreground").trim();
    return v || "#888";
  }, []);

  // ラベルを描く順 (つながりの多い順)。重なったものは落とすので、大事なものから描く。
  const labelRank = useMemo(() => {
    const rank = new Map<string, number>();
    [...data.nodes]
      .sort((a, b) => b.degree - a.degree || a.title.localeCompare(b.title, "ja"))
      .forEach((n, i) => rank.set(n.id, i));
    return rank;
  }, [data]);

  // 1 フレームぶんのラベルの占有領域。onRenderFramePre で空にする。
  const taken = useRef<{ x0: number; y0: number; x1: number; y1: number }[]>([]);
  const labelsDrawn = useRef(0);

  const hidden = Number.isFinite(budget) ? Math.max(0, total - data.nodes.length) : 0;

  return (
    <div className="space-y-2">
      <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
        <label className="flex items-center gap-1.5">
          表示範囲
          <select
            className="border border-border rounded px-1.5 py-1 bg-background text-foreground"
            value={String(budget)}
            disabled={!!focus}
            onChange={e => setBudget(Number(e.target.value))}
            data-testid="wiki-graph-budget"
          >
            {GRAPH_BUDGETS.map(b => (
              <option key={String(b)} value={String(b)}>
                {Number.isFinite(b) ? `つながりの多い ${b} 件` : `すべて（${total} 件）`}
              </option>
            ))}
          </select>
        </label>
        {focus ? (
          <span>選んだページとつながる {data.nodes.length} 件を表示しています。</span>
        ) : (
          <span>
            {data.nodes.length} 件を表示
            {hidden > 0 && `（つながりの少ない ${hidden} 件は省略。一覧タブから探せます）`}
          </span>
        )}
      </div>

      <div
        ref={box}
        className="w-full h-[min(70vh,640px)] rounded-lg border border-border overflow-hidden bg-card"
        data-testid="wiki-graph"
      >
        <ForceGraph2D
          ref={fg}
          graphData={data}
          width={size.width}
          height={size.height}
          backgroundColor="rgba(0,0,0,0)"
          // ライト/ダークどちらの背景でも見える中間色 (slate-400)。
          linkColor={() => "rgba(148,163,184,0.75)"}
          linkWidth={1.2}
          nodeLabel={(n: any) => `${escapeHtml(n.title)}${n.description ? `<br/><small>${escapeHtml(n.description)}</small>` : ""}`}
          onNodeClick={(n: any) => onOpen(n.id)}
          cooldownTicks={100}
          // 配置が落ち着いたら全ノードが収まるように合わせる。
          onEngineStop={() => fg.current?.zoomToFit(400, 40)}
          onRenderFramePre={(ctx, scale) => {
            taken.current = [];
            labelsDrawn.current = 0;
            ctx.font = `${Math.max(10 / scale, 2)}px sans-serif`;
            ctx.textAlign = "center";
            ctx.textBaseline = "top";
          }}
          nodeCanvasObject={(n: any, ctx, scale) => {
            const r = 3 + Math.sqrt(n.degree) * 1.6;
            ctx.beginPath();
            ctx.arc(n.x, n.y, r, 0, 2 * Math.PI);
            // 要約前のページは中抜きの輪で描き、要約済みと見分けられるようにする。
            const color = WIKI_TYPES[n.type]?.color ?? "#94a3b8";
            if (n.stub) {
              ctx.strokeStyle = color;
              ctx.lineWidth = 1;
              ctx.stroke();
            } else {
              ctx.fillStyle = color;
              ctx.fill();
            }
            // ラベルは上位から詰めていき、既に描いたラベルと重なるものは落とす。
            // (全ノードに描くと字が重なって読めなくなる。ズームすると落ちた分が出てくる。)
            const zoomed = scale > ZOOMED_IN;
            if (labelsDrawn.current >= (zoomed ? MAX_LABELS_ZOOMED : MAX_LABELS)) return;
            if (!zoomed && (labelRank.get(n.id) ?? Infinity) >= LABEL_CANDIDATES) return;
            const t = n.title ?? "";
            if (!t) return;
            const text = t.length > 18 ? t.slice(0, 18) + "…" : t;
            const fontSize = Math.max(10 / scale, 2);
            const w = ctx.measureText(text).width;
            const rect = {
              x0: n.x - w / 2,
              y0: n.y + r + 1,
              x1: n.x + w / 2,
              y1: n.y + r + 1 + fontSize,
            };
            if (taken.current.some(o => rect.x0 < o.x1 && o.x0 < rect.x1 && rect.y0 < o.y1 && o.y0 < rect.y1)) return;
            taken.current.push(rect);
            labelsDrawn.current += 1;
            ctx.fillStyle = labelColor;
            ctx.fillText(text, n.x, rect.y0);
          }}
          nodePointerAreaPaint={(n: any, color, ctx) => {
            ctx.fillStyle = color;
            ctx.beginPath();
            ctx.arc(n.x, n.y, 4 + Math.sqrt(n.degree) * 1.6, 0, 2 * Math.PI);
            ctx.fill();
          }}
        />
      </div>
    </div>
  );
}

function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]!);
}

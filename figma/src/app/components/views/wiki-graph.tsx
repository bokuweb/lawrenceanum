import { useEffect, useMemo, useRef, useState } from "react";
import ForceGraph2D, { type ForceGraphMethods } from "react-force-graph-2d";
import type { WikiGraph } from "../../data/api";
import { WIKI_TYPES } from "./wiki-types";

// OKF ページ間リンクのナレッジグラフ。ノードの大きさは次数、色は type。
// react-force-graph は canvas 描画で重いので、このファイルごと遅延ロードする。
export function WikiGraphCanvas({
  graph,
  visibleTypes,
  onOpen,
}: {
  graph: WikiGraph;
  visibleTypes: Set<string>;
  onOpen: (path: string) => void;
}) {
  const box = useRef<HTMLDivElement>(null);
  const fg = useRef<ForceGraphMethods | undefined>(undefined);
  const [size, setSize] = useState({ width: 800, height: 560 });
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
  const data = useMemo(() => {
    const nodes = graph.nodes.filter(n => visibleTypes.has(n.type)).map(n => ({ ...n }));
    const ids = new Set(nodes.map(n => n.id));
    const links = graph.links.filter(l => ids.has(l.source) && ids.has(l.target)).map(l => ({ ...l }));
    const degree = new Map<string, number>();
    for (const l of links) {
      degree.set(l.source, (degree.get(l.source) ?? 0) + 1);
      degree.set(l.target, (degree.get(l.target) ?? 0) + 1);
    }
    return { nodes: nodes.map(n => ({ ...n, degree: degree.get(n.id) ?? 0 })), links };
  }, [graph, visibleTypes]);

  const labelColor = useMemo(() => {
    const v = getComputedStyle(document.documentElement).getPropertyValue("--foreground").trim();
    return v || "#888";
  }, []);

  return (
    <div ref={box} className="w-full h-[min(70vh,640px)] rounded-lg border border-border overflow-hidden bg-card" data-testid="wiki-graph">
      <ForceGraph2D
        ref={fg}
        graphData={data}
        width={size.width}
        height={size.height}
        backgroundColor="rgba(0,0,0,0)"
        linkColor={() => "rgba(127,127,127,0.35)"}
        nodeLabel={(n: any) => `${escapeHtml(n.title)}${n.description ? `<br/><small>${escapeHtml(n.description)}</small>` : ""}`}
        onNodeClick={(n: any) => onOpen(n.id)}
        cooldownTicks={120}
        // 配置が落ち着いたら全ノードが収まるように合わせる。
        onEngineStop={() => fg.current?.zoomToFit(400, 40)}
        nodeCanvasObject={(n: any, ctx, scale) => {
          const r = 3 + Math.sqrt(n.degree) * 1.6;
          ctx.beginPath();
          ctx.arc(n.x, n.y, r, 0, 2 * Math.PI);
          ctx.fillStyle = WIKI_TYPES[n.type]?.color ?? "#94a3b8";
          ctx.fill();
          // ズームしたとき、または結節点 (法令・論点) は常にラベルを出す。
          if (scale > 1.6 || n.type === "law" || n.type === "topic") {
            const fontSize = Math.max(10 / scale, 2);
            ctx.font = `${fontSize}px sans-serif`;
            ctx.textAlign = "center";
            ctx.textBaseline = "top";
            ctx.fillStyle = labelColor;
            const t: string = n.title ?? "";
            ctx.fillText(t.length > 18 ? t.slice(0, 18) + "…" : t, n.x, n.y + r + 1);
          }
        }}
        nodePointerAreaPaint={(n: any, color, ctx) => {
          ctx.fillStyle = color;
          ctx.beginPath();
          ctx.arc(n.x, n.y, 4 + Math.sqrt(n.degree) * 1.6, 0, 2 * Math.PI);
          ctx.fill();
        }}
      />
    </div>
  );
}

function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]!);
}

import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { ChevronLeft, ChevronRight, Loader2 } from "lucide-react";
import { Button } from "../ui/button";
import { usePdfDocument } from "./kanpo-page-view";
import {
  JADE_AVAILABLE,
  JADE_NOTICES,
  JadeDocxClient,
  type JadeDocumentInfo,
} from "../../data/jade-docx";

function useWidth(ref: React.RefObject<HTMLDivElement | null>) {
  const [width, setWidth] = useState(500);
  useEffect(() => {
    const element = ref.current;
    if (!element) return;
    const observer = new ResizeObserver(([entry]) =>
      setWidth(entry.contentRect.width),
    );
    observer.observe(element);
    return () => observer.disconnect();
  }, [ref]);
  return width;
}
function Controls({
  kind,
  page,
  count,
  onPage,
  zoom,
  onZoom,
}: {
  kind: string;
  page: number;
  count: number;
  onPage: (page: number) => void;
  zoom: string;
  onZoom: (zoom: string) => void;
}) {
  return (
    <div className="flex flex-wrap items-center gap-2 border-b px-3 py-2 text-xs">
      <Button
        variant="outline"
        size="icon"
        className="size-7"
        aria-label={`${kind}の前のページ`}
        disabled={page <= 0}
        onClick={() => onPage(page - 1)}
      >
        <ChevronLeft className="size-4" />
      </Button>
      <span className="tabular-nums" aria-live="polite">
        {count ? page + 1 : 0} / {count} ページ
      </span>
      <Button
        variant="outline"
        size="icon"
        className="size-7"
        aria-label={`${kind}の次のページ`}
        disabled={page >= count - 1}
        onClick={() => onPage(page + 1)}
      >
        <ChevronRight className="size-4" />
      </Button>
      <label className="ml-auto flex items-center gap-2">
        倍率
        <select
          aria-label={`${kind}の表示倍率`}
          className="rounded border bg-background px-2 py-1"
          value={zoom}
          onChange={(e) => onZoom(e.target.value)}
        >
          <option value="fit">幅に合わせる</option>
          {[0.5, 0.75, 1, 1.25, 1.5, 2].map((value) => (
            <option key={value} value={value}>
              {value * 100}%
            </option>
          ))}
        </select>
      </label>
    </div>
  );
}
function Frame({
  title,
  testId,
  children,
  controls,
}: {
  title: string;
  testId: string;
  children: ReactNode;
  controls: ReactNode;
}) {
  return (
    <section
      data-testid={testId}
      aria-label={title}
      className="flex min-h-0 min-w-0 flex-col overflow-hidden rounded-xl border bg-background shadow-sm"
    >
      <h2 className="border-b px-4 py-3 text-sm font-semibold">{title}</h2>
      {controls}
      {children}
    </section>
  );
}
function Loading({ children }: { children: ReactNode }) {
  return (
    <div className="flex items-center justify-center gap-2 p-8 text-sm text-muted-foreground">
      <Loader2 className="size-4 animate-spin" />
      {children}
    </div>
  );
}

export function KanpoOriginalPreview({ bytes }: { bytes: Uint8Array | null }) {
  const source = useMemo(
    () => (bytes ? { data: bytes.slice().buffer as ArrayBuffer } : null),
    [bytes],
  );
  const { doc, error } = usePdfDocument(source);
  const [page, setPage] = useState(0),
    [zoom, setZoom] = useState("fit");
  const [renderError, setRenderError] = useState<string | null>(null),
    [painting, setPainting] = useState(false);
  const container = useRef<HTMLDivElement>(null),
    canvas = useRef<HTMLCanvasElement>(null);
  const width = useWidth(container);
  useEffect(() => {
    setPage(0);
    setRenderError(null);
    setPainting(false);
  }, [bytes]);
  useEffect(() => {
    if (!doc || !canvas.current) return;
    let cancelled = false;
    let task: { cancel(): void } | undefined;
    setRenderError(null);
    setPainting(true);
    doc
      .getPage(Math.min(page + 1, doc.numPages))
      .then(async (pdf) => {
        if (cancelled || !canvas.current) return;
        const base = pdf.getViewport({ scale: 96 / 72 });
        const scale =
          zoom === "fit"
            ? Math.max(0.1, (width - 32) / base.width)
            : Number(zoom);
        const viewport = pdf.getViewport({ scale: (96 / 72) * scale });
        const ratio = Math.min(window.devicePixelRatio || 1, 2),
          element = canvas.current;
        element.width = Math.ceil(viewport.width * ratio);
        element.height = Math.ceil(viewport.height * ratio);
        element.style.width = `${viewport.width}px`;
        element.style.height = `${viewport.height}px`;
        const render = pdf.render({
          canvasContext: element.getContext("2d")!,
          viewport,
          transform: [ratio, 0, 0, ratio, 0, 0],
        });
        task = render;
        await render.promise;
        if (!cancelled) setPainting(false);
      })
      .catch((e) => {
        if (!cancelled) {
          setRenderError(String(e?.message ?? e));
          setPainting(false);
        }
      });
    return () => {
      cancelled = true;
      task?.cancel();
    };
  }, [doc, page, zoom, width]);
  const failure = error || renderError;
  return (
    <Frame
      title="原本 PDF"
      testId="kanpo-original-panel"
      controls={
        <Controls
          kind="PDF"
          page={page}
          count={doc?.numPages ?? 0}
          onPage={setPage}
          zoom={zoom}
          onZoom={setZoom}
        />
      }
    >
      <div
        ref={container}
        className="relative min-h-0 flex-1 overflow-auto bg-muted/40 p-4"
      >
        {!bytes ? (
          <p className="p-8 text-center text-sm text-muted-foreground">
            官報を選ぶと、ここに原本が表示されます。
          </p>
        ) : failure ? (
          <p role="alert" className="p-4 text-sm text-destructive">
            PDFを表示できませんでした: {failure}
          </p>
        ) : (
          <>
            {!doc && <Loading>PDFを読み込んでいます</Loading>}
            <canvas
              ref={canvas}
              data-testid="kanpo-original-canvas"
              aria-label={`原本PDF ${page + 1}ページ`}
              className="mx-auto bg-white shadow-sm"
            />
            {painting && (
              <span className="absolute right-4 top-4 rounded bg-background/90 px-2 py-1 text-xs">
                描画中…
              </span>
            )}
          </>
        )}
      </div>
    </Frame>
  );
}

export function JadeDocxPreview({ bytes }: { bytes: Uint8Array | null }) {
  const [info, setInfo] = useState<JadeDocumentInfo | null>(null),
    [error, setError] = useState<string | null>(null);
  const [page, setPage] = useState(0),
    [zoom, setZoom] = useState("fit"),
    [painting, setPainting] = useState(false);
  const [client, setClient] = useState<JadeDocxClient | null>(null);
  const container = useRef<HTMLDivElement>(null),
    canvas = useRef<HTMLCanvasElement>(null);
  const width = useWidth(container);
  useEffect(() => {
    setInfo(null);
    setError(null);
    setPage(0);
    setClient(null);
    if (!bytes) return;
    if (!JADE_AVAILABLE) {
      setError("DOCXプレビューを読み込めませんでした。");
      return;
    }
    let active = true;
    const editor = new JadeDocxClient();
    editor.load(bytes).then(
      (value) => {
        if (active) {
          setInfo(value);
          setClient(editor);
        }
      },
      (e) => {
        if (active) setError(e.message);
      },
    );
    return () => {
      active = false;
      editor.dispose();
    };
  }, [bytes]);
  useEffect(() => {
    if (!client || !info || !canvas.current) return;
    let active = true;
    setPainting(true);
    client
      .render(page)
      .then(({ bitmap, width: pageWidth, height: pageHeight }) => {
        try {
          if (!active || !canvas.current) return;
          const element = canvas.current,
            scale =
              zoom === "fit"
                ? Math.max(0.1, (width - 32) / pageWidth)
                : Number(zoom);
          element.width = bitmap.width;
          element.height = bitmap.height;
          element.style.width = `${pageWidth * scale}px`;
          element.style.height = `${pageHeight * scale}px`;
          element.getContext("2d")!.drawImage(bitmap, 0, 0);
          setPainting(false);
        } finally {
          bitmap.close();
        }
      })
      .catch((e) => {
        if (active) {
          setError(e.message);
          setPainting(false);
        }
      });
    return () => {
      active = false;
    };
  }, [client, info, page, zoom, width]);
  return (
    <Frame
      title="変換後 DOCX"
      testId="kanpo-docx-panel"
      controls={
        <Controls
          kind="DOCX"
          page={page}
          count={info?.pages ?? 0}
          onPage={setPage}
          zoom={zoom}
          onZoom={setZoom}
        />
      }
    >
      <div
        ref={container}
        data-testid="jade-docx-preview"
        data-engine="jade-core"
        className="relative min-h-0 flex-1 overflow-auto bg-muted/40 p-4"
      >
        {!bytes ? (
          <p className="p-8 text-center text-sm text-muted-foreground">
            DOCXに変換すると、ここにプレビューが表示されます。
          </p>
        ) : error ? (
          <p role="alert" className="p-4 text-sm text-destructive">
            DOCXを表示できませんでした: {error}
          </p>
        ) : (
          <>
            {!info && <Loading>DOCXを読み込んでいます</Loading>}
            {info && (
              <canvas
                ref={canvas}
                data-testid="jade-docx-canvas"
                aria-label={`DOCX ${page + 1}ページ`}
                className="mx-auto bg-white shadow-sm"
              />
            )}
            {painting && (
              <span className="absolute right-4 top-4 rounded bg-background/90 px-2 py-1 text-xs">
                描画中…
              </span>
            )}
          </>
        )}
      </div>
      {JADE_NOTICES.length > 0 && (
        <details className="border-t px-3 py-1 text-[10px] text-muted-foreground">
          <summary>JADE Core・フォントのライセンス</summary>
          {JADE_NOTICES.map((notice) => (
            <a
              className="mr-3 underline"
              key={notice.name}
              href={notice.url}
              target="_blank"
              rel="noreferrer"
            >
              {notice.name}
            </a>
          ))}
        </details>
      )}
    </Frame>
  );
}

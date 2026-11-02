// 官報の PDF を DOCX にする（`/kanpo-docx`）。PDF の字はそのまま読み、ページの画像（スキャンした本文）は
// ブラウザで OCR（ndlocr-lite-rs の WASM 版）して、記事ごとの段落・表・新旧対照表の DOCX を書く。
// 処理はすべてブラウザの中で行い、PDF はどこにも送らない。
import { useCallback, useEffect, useState } from "react";
import { AlertTriangle, Download, FileUp, Loader2 } from "lucide-react";
import { Button } from "../ui/button";
import { ensureCrossOriginIsolated, loadKanpoDocx, loadOcr, OCR_AVAILABLE, pdfToDocx, type KanpoDocx } from "../../data/kanpo-docx";

type Result = { url: string; name: string; images: number; recognized: number; seconds: number };

export function KanpoDocxView() {
  const [docx, setDocx] = useState<KanpoDocx | null | undefined>(undefined);
  const [useOcr, setUseOcr] = useState(OCR_AVAILABLE);
  const [busy, setBusy] = useState(false);
  const [log, setLog] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<Result | null>(null);

  useEffect(() => {
    loadKanpoDocx().then(setDocx, (e) => setError(String(e)));
  }, []);
  // ブラウザの OCR は cross-origin isolated なページでしか動かない。OCR を使うなら、先に service worker を入れて読み直す。
  useEffect(() => {
    if (useOcr && OCR_AVAILABLE) void ensureCrossOriginIsolated();
  }, [useOcr]);
  useEffect(() => () => {
    if (result) URL.revokeObjectURL(result.url);
  }, [result]);

  const progress = useCallback((message: string) => setLog((l) => [...l.slice(-200), message]), []);

  const run = useCallback(
    async (file: File) => {
      if (!docx) return;
      setBusy(true);
      setError(null);
      setResult(null);
      setLog([]);
      const started = performance.now();
      try {
        const pdf = new Uint8Array(await file.arrayBuffer());
        let ocr = null;
        if (useOcr) {
          progress("OCR のモデルを読み込んでいます（初回は約 150MB）");
          ocr = await loadOcr(progress);
          if (!ocr) progress("OCR が使えないので、PDF の字だけで書きます");
        }
        const out = await pdfToDocx(docx, pdf, ocr, progress);
        const blob = new Blob([out.docx], { type: "application/vnd.openxmlformats-officedocument.wordprocessingml.document" });
        const name = file.name.replace(/\.pdf$/i, "") + ".docx";
        setResult({ url: URL.createObjectURL(blob), name, images: out.images, recognized: out.recognized, seconds: (performance.now() - started) / 1000 });
        progress("できました");
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setBusy(false);
      }
    },
    [docx, useOcr, progress],
  );

  return (
    <div className="mx-auto max-w-3xl space-y-6 p-6">
      <div>
        <h1 className="text-xl font-semibold">官報 PDF → DOCX</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          官報の PDF を Word（DOCX）にします。PDF の字はそのまま、ページの画像の本文は OCR（ndlocr-lite-rs）で読みます。処理はすべてこのブラウザの中で行い、PDF はどこにも送りません。
        </p>
      </div>

      {docx === null && (
        <div className="flex items-start gap-2 rounded-md border border-amber-300 bg-amber-50 p-3 text-sm dark:bg-amber-950/30">
          <AlertTriangle className="mt-0.5 size-4 shrink-0" />
          <span>
            DOCX 変換の WASM がビルドされていません。<code>pnpm build:wasm</code> を実行してください。
          </span>
        </div>
      )}

      <div className="space-y-3 rounded-md border p-4">
        <label className="flex items-center gap-2 text-sm">
          <input type="checkbox" checked={useOcr} disabled={!OCR_AVAILABLE || busy} onChange={(e) => setUseOcr(e.target.checked)} />
          画像の本文を OCR する
          {!OCR_AVAILABLE && <span className="text-muted-foreground">（OCR がこのビルドに含まれていません）</span>}
        </label>
        <label className="inline-flex">
          <input
            type="file"
            accept="application/pdf,.pdf"
            className="hidden"
            disabled={!docx || busy}
            onChange={(e) => {
              const f = e.target.files?.[0];
              if (f) void run(f);
              e.target.value = "";
            }}
          />
          <Button asChild disabled={!docx || busy}>
            <span>
              {busy ? <Loader2 className="mr-2 size-4 animate-spin" /> : <FileUp className="mr-2 size-4" />}
              PDF を選ぶ
            </span>
          </Button>
        </label>
      </div>

      {error && <div className="rounded-md border border-red-300 bg-red-50 p-3 text-sm text-red-800 dark:bg-red-950/30 dark:text-red-200">{error}</div>}

      {result && (
        <div className="flex items-center justify-between rounded-md border p-4 text-sm">
          <span>
            {result.name}（画像 {result.images} 枚、OCR {result.recognized} 枚、{result.seconds.toFixed(1)} 秒）
          </span>
          <Button asChild>
            <a href={result.url} download={result.name}>
              <Download className="mr-2 size-4" />
              ダウンロード
            </a>
          </Button>
        </div>
      )}

      {log.length > 0 && <pre className="max-h-64 overflow-auto rounded-md bg-muted p-3 text-xs">{log.join("\n")}</pre>}
    </div>
  );
}

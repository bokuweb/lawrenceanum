import { useCallback, useEffect, useRef, useState } from "react";
import { Download, FileUp, Loader2, RefreshCw } from "lucide-react";
import { Button } from "../ui/button";
import {
  KanpoOriginalPreview,
  JadeDocxPreview,
} from "./kanpo-docx-preview-panels";
import {
  ARCHIVE_BASE,
  CORPUS_CASES,
  archivePdfUrl,
  fetchArchiveDates,
  fetchArchiveManifest,
  issueTypeLabel,
  type ArchiveManifest,
} from "../../data/kanpo-playground";
import {
  ensureCrossOriginIsolated,
  loadKanpoDocx,
  loadOcr,
  OCR_AVAILABLE,
  pdfToDocx,
  type KanpoDocx,
} from "../../data/kanpo-docx";

type Mode = "archive" | "corpus" | "local";
type Input = { name: string; bytes: Uint8Array };
type Result = { bytes: Uint8Array; name: string; seconds: number };
const corpus = CORPUS_CASES.flatMap((item) =>
  item.pages.flatMap((page, index) =>
    page.pdfUrl
      ? [
          {
            id: `${item.id}:${index}`,
            url: page.pdfUrl,
            name: `${item.id}${index ? `-p${index + 1}` : ""}.pdf`,
            label: `${item.meta.date ?? ""} ${item.meta.issue ?? item.id}・${item.meta.title}（${item.meta.pages?.[index] ?? (item.meta.page != null ? item.meta.page + index : index + 1)}ページ）`,
          },
        ]
      : [],
  ),
);
const selectClass =
  "min-w-0 w-full rounded-md border bg-background px-3 py-2 text-sm disabled:opacity-50";

export function KanpoDocxPreviewView() {
  const [mode, setMode] = useState<Mode>(ARCHIVE_BASE ? "archive" : "corpus");
  const [corpusId, setCorpusId] = useState(
    corpus.find((c) => c.id === "2026-07-28-honshi1756-p4:0")?.id ??
      corpus[0]?.id ??
      "",
  );
  const [dates, setDates] = useState<string[]>([]),
    [date, setDate] = useState("");
  const [loaded, setLoaded] = useState<{
    date: string;
    manifest: ArchiveManifest;
  } | null>(null);
  const [archiveSelection, setArchiveSelection] = useState({
    dir: "",
    file: "",
  });
  const [sourceError, setSourceError] = useState<string | null>(null);
  const [input, setInput] = useState<Input | null>(null),
    [result, setResult] = useState<Result | null>(null);
  const [converter, setConverter] = useState<KanpoDocx | null>(null),
    [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false),
    [converting, setConverting] = useState(false),
    [useOcr, setUseOcr] = useState(true);
  const [progress, setProgress] = useState(""),
    [downloadUrl, setDownloadUrl] = useState<string | null>(null);
  const operation = useRef(0);
  const manifest = loaded?.date === date ? loaded.manifest : null;
  const issue = manifest?.issues.find(
    (issue) => issue.dir === archiveSelection.dir,
  );
  const archivePage = issue?.pages.find(
    (page) => page.file === archiveSelection.file,
  );
  const chosenCorpus = corpus.find((item) => item.id === corpusId);
  const remote =
    mode === "corpus" && chosenCorpus
      ? chosenCorpus
      : mode === "archive" && issue && archivePage
        ? {
            url: archivePdfUrl(date, issue.dir, archivePage.file),
            name: `${date}-${issue.dir}-p${archivePage.page}.pdf`,
          }
        : null;
  const remoteUrl = remote?.url,
    remoteName = remote?.name;

  useEffect(() => {
    let active = true;
    loadKanpoDocx().then(
      (value) => {
        if (active) {
          setConverter(value);
          if (!value) setError("変換機能を読み込めませんでした。");
        }
      },
      (e) => {
        if (active) setError(String(e));
      },
    );
    if (OCR_AVAILABLE) void ensureCrossOriginIsolated().catch(() => {});
    return () => {
      active = false;
      operation.current++;
    };
  }, []);
  useEffect(() => {
    if (mode !== "archive" || !ARCHIVE_BASE) return;
    let active = true;
    setSourceError(null);
    fetchArchiveDates().then(
      (values) => {
        if (active) {
          setDates(values);
          setDate((current) => current || values[0] || "");
        }
      },
      (e) => {
        if (active)
          setSourceError(`官報の一覧を取得できませんでした: ${e.message}`);
      },
    );
    return () => {
      active = false;
    };
  }, [mode]);
  useEffect(() => {
    if (mode !== "archive" || !date) return;
    let active = true;
    setLoaded(null);
    setSourceError(null);
    fetchArchiveManifest(date).then(
      (value) => {
        if (!active) return;
        setLoaded({ date, manifest: value });
        const first = value.issues.find((issue) => issue.pages.length > 0);
        setArchiveSelection({
          dir: first?.dir ?? "",
          file: first?.pages[0]?.file ?? "",
        });
      },
      (e) => {
        if (active) setSourceError(`官報を取得できませんでした: ${e.message}`);
      },
    );
    return () => {
      active = false;
    };
  }, [date, mode]);

  const clear = useCallback(() => {
    operation.current++;
    setInput(null);
    setResult(null);
    setError(null);
    setSourceError(null);
    setProgress("");
    setConverting(false);
    return operation.current;
  }, []);
  const accept = useCallback((bytes: Uint8Array, name: string, key: number) => {
    if (operation.current !== key) return;
    const header = new TextDecoder("latin1").decode(bytes.subarray(0, 1024));
    if (!header.includes("%PDF-"))
      throw new Error(
        "PDFファイルを読み込めませんでした。PDFを選び直してください。",
      );
    setInput({ bytes, name });
  }, []);
  useEffect(() => {
    const key = clear();
    if (!remoteUrl || !remoteName) {
      setLoading(false);
      return;
    }
    const abort = new AbortController();
    setLoading(true);
    fetch(remoteUrl, { signal: abort.signal })
      .then(async (response) => {
        if (!response.ok)
          throw new Error(`PDFを取得できませんでした（${response.status}）。`);
        accept(new Uint8Array(await response.arrayBuffer()), remoteName, key);
      })
      .catch((e) => {
        if (!abort.signal.aborted && operation.current === key)
          setError(e.message);
      })
      .finally(() => {
        if (operation.current === key) setLoading(false);
      });
    return () => abort.abort();
  }, [mode, remoteUrl, remoteName, clear, accept]);
  useEffect(() => {
    if (!result) {
      setDownloadUrl(null);
      return;
    }
    const url = URL.createObjectURL(
      new Blob([result.bytes.slice().buffer], {
        type: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
      }),
    );
    setDownloadUrl(url);
    return () => URL.revokeObjectURL(url);
  }, [result]);

  const pickFile = async (file: File) => {
    const key = clear();
    setLoading(true);
    try {
      accept(new Uint8Array(await file.arrayBuffer()), file.name, key);
    } catch (e) {
      if (operation.current === key)
        setError(e instanceof Error ? e.message : String(e));
    } finally {
      if (operation.current === key) setLoading(false);
    }
  };
  const convert = async () => {
    if (!input || !converter) return;
    const key = ++operation.current,
      started = performance.now();
    setResult(null);
    setError(null);
    setConverting(true);
    setProgress("変換しています");
    const update = (message: string) => {
      if (operation.current === key) setProgress(message);
    };
    try {
      let engine = null;
      if (useOcr && converter.pageImages(input.bytes).length) {
        if (!OCR_AVAILABLE)
          throw new Error(
            "画像の文字認識を利用できません。文字情報のあるPDFを選ぶか、「画像の本文もOCRする」を外してください。",
          );
        if (!window.crossOriginIsolated)
          throw new Error(
            "画像の文字認識を準備できませんでした。ページを再読み込みしてください。",
          );
        update("画像の文字認識を準備しています（初回はモデルを読み込みます）");
        engine = await loadOcr();
        if (!engine) throw new Error("画像の文字認識を読み込めませんでした。");
      }
      if (operation.current !== key) return;
      const output = await pdfToDocx(converter, input.bytes, engine, update);
      if (operation.current !== key) return;
      setResult({
        bytes: output.docx,
        name: input.name.replace(/\.pdf$/i, "") + ".docx",
        seconds: (performance.now() - started) / 1000,
      });
      setProgress("変換が完了しました");
    } catch (e) {
      if (operation.current === key)
        setError(e instanceof Error ? e.message : String(e));
    } finally {
      if (operation.current === key) setConverting(false);
    }
  };

  return (
    <div className="flex h-full min-h-[650px] flex-col gap-4 p-4 lg:p-6">
      <header className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h1 className="text-xl font-semibold">官報 PDF・DOCX 比較</h1>
          <p className="mt-1 text-sm text-muted-foreground">
            官報を選び、原本と変換後の文書を左右で確認できます。ファイルの処理はこのブラウザ内で行います。
          </p>
        </div>
        {downloadUrl && result && (
          <Button asChild size="sm">
            <a href={downloadUrl} download={result.name}>
              <Download className="mr-2 size-4" />
              DOCXをダウンロード
            </a>
          </Button>
        )}
      </header>
      <div className="space-y-3 rounded-xl border bg-background p-4">
        <div className="grid items-end gap-3 md:grid-cols-[180px_minmax(0,1fr)]">
          <label className="space-y-1 text-xs font-medium">
            入力元
            <select
              aria-label="入力元"
              value={mode}
              onChange={(e) => setMode(e.target.value as Mode)}
              className={selectClass}
            >
              {ARCHIVE_BASE && <option value="archive">官報アーカイブ</option>}
              <option value="corpus">収録済みの官報</option>
              <option value="local">PDFファイル</option>
            </select>
          </label>
          {mode === "corpus" && (
            <label className="space-y-1 text-xs font-medium">
              収録済みの官報
              <select
                aria-label="収録済みの官報"
                className={selectClass}
                value={corpusId}
                onChange={(e) => setCorpusId(e.target.value)}
              >
                {corpus.map((item) => (
                  <option key={item.id} value={item.id}>
                    {item.label}
                  </option>
                ))}
              </select>
            </label>
          )}
          {mode === "archive" && (
            <div className="grid gap-2 sm:grid-cols-[150px_minmax(0,1fr)_140px]">
              <label className="space-y-1 text-xs font-medium">
                発行日
                <select
                  aria-label="発行日"
                  className={selectClass}
                  value={date}
                  onChange={(e) => setDate(e.target.value)}
                  disabled={!dates.length}
                >
                  {dates.map((value) => (
                    <option key={value}>{value}</option>
                  ))}
                </select>
              </label>
              <label className="space-y-1 text-xs font-medium">
                号
                <select
                  aria-label="官報の号"
                  className={selectClass}
                  value={archiveSelection.dir}
                  disabled={!manifest}
                  onChange={(e) => {
                    const next = manifest?.issues.find(
                      (issue) => issue.dir === e.target.value,
                    );
                    setArchiveSelection({
                      dir: next?.dir ?? "",
                      file: next?.pages[0]?.file ?? "",
                    });
                  }}
                >
                  {manifest?.issues
                    .filter((issue) => issue.pages.length)
                    .map((issue) => (
                      <option key={issue.dir} value={issue.dir}>
                        {issueTypeLabel(issue.issue_type)} 第{issue.issue_no}号
                      </option>
                    ))}
                </select>
              </label>
              <label className="space-y-1 text-xs font-medium">
                ページ
                <select
                  aria-label="官報のページ"
                  className={selectClass}
                  value={archiveSelection.file}
                  disabled={!issue}
                  onChange={(e) =>
                    setArchiveSelection((current) => ({
                      ...current,
                      file: e.target.value,
                    }))
                  }
                >
                  {issue?.pages.map((page) => (
                    <option key={page.file} value={page.file}>
                      {page.page} ページ
                    </option>
                  ))}
                </select>
              </label>
            </div>
          )}
          {mode === "local" && (
            <label className="flex min-w-0 items-center gap-3 text-sm">
              <FileUp className="size-4 shrink-0 text-muted-foreground" />
              <input
                aria-label="PDFファイル"
                type="file"
                accept="application/pdf,.pdf"
                className="min-w-0 text-sm file:mr-3 file:rounded file:border file:bg-background file:px-3 file:py-2"
                onChange={(e) => {
                  const file = e.target.files?.[0];
                  if (file) void pickFile(file);
                  e.target.value = "";
                }}
              />
            </label>
          )}
        </div>
        {sourceError && (
          <p role="alert" className="text-sm text-destructive">
            {sourceError}
          </p>
        )}
        <div className="flex flex-wrap items-center gap-3 border-t pt-3">
          <label className="flex items-center gap-2 text-xs">
            <input
              type="checkbox"
              checked={useOcr}
              onChange={(e) => setUseOcr(e.target.checked)}
              disabled={converting}
            />
            画像の本文もOCRする
          </label>
          <Button
            size="sm"
            onClick={() => void convert()}
            disabled={!input || !converter || loading || converting}
          >
            {converting ? (
              <Loader2 className="mr-2 size-4 animate-spin" />
            ) : (
              <RefreshCw className="mr-2 size-4" />
            )}
            DOCXに変換
          </Button>
          <span
            role="status"
            className="min-w-0 truncate text-xs text-muted-foreground"
          >
            {loading
              ? "PDFを読み込んでいます"
              : converting
                ? progress
                : result
                  ? `${result.name}・${result.seconds.toFixed(1)}秒`
                  : (input?.name ?? "官報を選択してください")}
          </span>
        </div>
        {error && (
          <p role="alert" className="text-sm text-destructive">
            {error}
          </p>
        )}
      </div>
      <div
        className="grid min-h-0 flex-1 grid-cols-2 gap-4"
        style={{ minHeight: 420 }}
      >
        <KanpoOriginalPreview bytes={input?.bytes ?? null} />
        <JadeDocxPreview bytes={result?.bytes ?? null} />
      </div>
    </div>
  );
}

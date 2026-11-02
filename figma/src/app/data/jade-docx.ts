/** Hosts the real jade-rs/jade-core editor runtime in a dedicated worker. */
export const JADE_AVAILABLE =
  Object.keys(import.meta.glob("../../wasm/jade-core/jade_docx.js")).length > 0;
const notices = import.meta.glob<string>(
  "../../wasm/jade-core/*{LICENSES.html,LICENSE.txt,OFL.txt}",
  { eager: true, query: "?url&no-inline", import: "default" },
);
export const JADE_NOTICES = Object.entries(notices).map(([path, url]) => ({
  name: path.split("/").pop()!,
  url,
}));

export type JadeDocumentInfo = { pages: number; firstParagraph: string | null };
export type JadePage = { bitmap: ImageBitmap; width: number; height: number };
export class JadeDocxClient {
  private worker: Worker;
  private sequence = 0;
  private stopped = false;
  private pending = new Map<
    number,
    {
      resolve: (result: any) => void;
      reject: (error: Error) => void;
      timer: ReturnType<typeof setTimeout>;
    }
  >();
  constructor() {
    this.worker = new Worker(
      new URL("./jade-docx.worker.ts", import.meta.url),
      { type: "module" },
    );
    this.worker.onmessage = ({ data }) => {
      const entry = this.pending.get(data.id);
      if (!entry) {
        data.result?.bitmap?.close();
        return;
      }
      this.pending.delete(data.id);
      clearTimeout(entry.timer);
      if (data.error) entry.reject(new Error(data.error));
      else entry.resolve(data.result);
    };
    this.worker.onerror = (event) =>
      this.dispose(
        new Error(event.message || "DOCXのプレビューを準備できませんでした。"),
      );
    this.worker.onmessageerror = () =>
      this.dispose(new Error("DOCXの描画結果を受け取れませんでした。"));
  }
  private request<T>(
    message: Record<string, unknown>,
    transfer: Transferable[] = [],
  ): Promise<T> {
    if (this.stopped)
      return Promise.reject(new Error("プレビューは終了しました。"));
    const id = ++this.sequence;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(
        () =>
          this.dispose(
            new Error("DOCXのプレビューに時間がかかりすぎています。"),
          ),
        120_000,
      );
      this.pending.set(id, { resolve, reject, timer });
      try {
        this.worker.postMessage({ ...message, id }, transfer);
      } catch (error) {
        this.pending.delete(id);
        clearTimeout(timer);
        reject(error);
      }
    });
  }
  load(bytes: Uint8Array): Promise<JadeDocumentInfo> {
    const copy = bytes.slice();
    return this.request(
      {
        type: "load",
        bytes: copy,
        ratio: Math.min(window.devicePixelRatio || 1, 2),
      },
      [copy.buffer],
    );
  }
  render(page: number): Promise<JadePage> {
    return this.request({ type: "render", page });
  }
  dispose(error = new Error("プレビューは終了しました。")) {
    if (this.stopped) return;
    this.stopped = true;
    this.worker.terminate();
    for (const entry of this.pending.values()) {
      clearTimeout(entry.timer);
      entry.reject(error);
    }
    this.pending.clear();
  }
}

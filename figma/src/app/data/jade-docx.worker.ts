/// <reference lib="webworker" />
declare const self: DedicatedWorkerGlobalScope;
type JadeView = {
  free(): void;
  load_docx(bytes: Uint8Array): Promise<unknown>;
  set_preview_mode(value: boolean): void;
  page_count(): number;
  first_paragraph_text(): string | null;
  page_dimensions(index: number): { width: number; height: number };
  paged_images():
    | { bitmap: ImageBitmap; index?: number; pageIndex?: number }[]
    | null;
  rerenderWithoutSnapshots(): void;
  create_page_bitmap(index: number): Promise<ImageBitmap>;
};
type JadeModule = {
  default(): Promise<unknown>;
  Options: new () => {
    set_preview_mode(value: boolean): void;
    set_pageless(value: boolean): void;
  };
  OffscreenDocumentView: new (
    canvas: OffscreenCanvas,
    options: unknown,
    ratio: number,
  ) => JadeView;
  setWordFontFallbacks?(
    entries: { family: string; substitutes: string[] }[],
  ): void;
};
const modules = import.meta.glob<JadeModule>(
  "../../wasm/jade-core/jade_docx.js",
);
const fonts = import.meta.glob<string>(
  "../../wasm/jade-core/NotoSansJP-Regular.ttf",
  { eager: true, query: "?url&no-inline", import: "default" },
);
let initialized: Promise<JadeModule> | null = null;
let view: JadeView | null = null;
function initialize(): Promise<JadeModule> {
  return (initialized ??= (async () => {
    const load = Object.values(modules)[0];
    if (!load) throw new Error("DOCXプレビューを読み込めませんでした。");
    const module = await load();
    await module.default();
    const fontUrl = Object.values(fonts)[0];
    if (!fontUrl) throw new Error("日本語フォントを読み込めませんでした。");
    const face = await new FontFace(
      "Noto Sans JP",
      `url("${new URL(fontUrl, self.location.href).href}")`,
    ).load();
    (self as unknown as { fonts: { add(face: FontFace): unknown } }).fonts.add(
      face,
    );
    module.setWordFontFallbacks?.(
      [
        "ＭＳ 明朝",
        "MS Mincho",
        "ＭＳ ゴシック",
        "MS Gothic",
        "Yu Mincho",
        "游明朝",
      ].map((family) => ({ family, substitutes: ["Noto Sans JP"] })),
    );
    return module;
  })());
}
let queue = Promise.resolve();
self.onmessage = ({ data }) => {
  queue = queue.then(async () => {
    try {
      if (data.type === "load") {
        const module = await initialize();
        view?.free();
        view = null;
        const options = new module.Options();
        options.set_preview_mode(true);
        options.set_pageless(false);
        view = new module.OffscreenDocumentView(
          new OffscreenCanvas(800, 1120),
          options,
          data.ratio,
        );
        await view.load_docx(data.bytes);
        view.set_preview_mode(true);
        // The engine's initial snapshots may transfer/clear backing canvases. Release
        // those bitmaps and repaint using the engine before taking on-demand pages.
        for (const page of view.paged_images() ?? []) page.bitmap?.close();
        view.rerenderWithoutSnapshots();
        self.postMessage({
          id: data.id,
          result: {
            pages: view.page_count(),
            firstParagraph: view.first_paragraph_text(),
          },
        });
      } else if (data.type === "render") {
        if (!view) throw new Error("DOCXを先に変換してください。");
        const dimensions = view.page_dimensions(data.page);
        if (!dimensions) throw new Error("DOCXのページが見つかりません。");
        const bitmap = await view.create_page_bitmap(data.page);
        self.postMessage({ id: data.id, result: { ...dimensions, bitmap } }, [
          bitmap,
        ]);
      }
    } catch (error) {
      self.postMessage({
        id: data.id,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  });
};

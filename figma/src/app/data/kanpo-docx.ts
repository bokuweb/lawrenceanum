// 官報の PDF を DOCX にする（`/kanpo-docx`）。
// - kanpo-docx の WASM（`pnpm build:wasm` で src/wasm/kanpo-docx/ に生成。gitignore 済み）が、PDF からページの画像を
//   取り出し（pageImages）、OCR の結果と PDF の字を合わせて DOCX を書く（convert）。
// - OCR は ndlocr-lite-rs の WASM 版（src/wasm/ndlocr/ に生成）。無ければ PDF の字だけで書く。

/** ページの画像（表示の向きに起こした 8 bit グレースケール、0 が黒）。位置・大きさは pt（左上原点）。 */
export type KanpoPageImage = {
  page: number;
  width: number;
  height: number;
  left: number;
  top: number;
  scale: number;
  pageWidth: number;
  pageHeight: number;
  pixels: Uint8Array;
};

export type KanpoDocx = {
  pageImages: (pdf: Uint8Array) => KanpoPageImage[];
  /** `traces` は pageImages と同じ並びの NDLOCR の trace（OCR しない画像は null）。 */
  convert: (pdf: Uint8Array, traces: (unknown | null)[]) => Uint8Array;
};

/** 1 枚の画像を認識して NDLOCR の trace（`recognize-page --trace-json` と同じ形）を返す OCR。 */
export type OcrEngine = {
  name: string;
  recognize: (gray: Uint8Array, width: number, height: number, onProgress?: (message: string) => void) => Promise<unknown>;
};

type DocxModule = {
  default: () => Promise<unknown>;
  pageImages: (pdf: Uint8Array) => KanpoPageImage[];
  convert: (pdf: Uint8Array, tracesJson: string) => Uint8Array;
};

// WASM を作っていない環境でもビルドが通るよう、静的 import ではなく glob にする。
const docxModules = import.meta.glob<DocxModule>("../../wasm/kanpo-docx/kanpo_docx_wasm.js");

let docxPromise: Promise<KanpoDocx | null> | null = null;

/** kanpo-docx の WASM を読み込む。未ビルドなら null。 */
export function loadKanpoDocx(): Promise<KanpoDocx | null> {
  if (!docxPromise) {
    const load = Object.values(docxModules)[0];
    docxPromise = load
      ? load().then(async (m) => {
          await m.default();
          return {
            pageImages: (pdf: Uint8Array) => m.pageImages(pdf),
            convert: (pdf: Uint8Array, traces: (unknown | null)[]) => m.convert(pdf, JSON.stringify(traces)),
          };
        })
      : Promise.resolve(null);
  }
  return docxPromise;
}

/** ndlocr-lite-rs の WASM 版のモジュール（`src/wasm/ndlocr/` の `ndlocr.js`）が持つ関数。 */
type NdlocrModule = {
  createRecognizer: (options: { modelBaseUrl: string; onProgress?: (message: string) => void }) => Promise<{
    recognizePage: (gray: Uint8Array, width: number, height: number) => Promise<unknown>;
  }>;
};

const ndlocrModules = import.meta.glob<NdlocrModule>("../../wasm/ndlocr/ndlocr.js");

/** OCR のモデルの置き場所（ビルド時に `VITE_NDLOCR_MODEL_BASE` で変えられる）。 */
export const NDLOCR_MODEL_BASE: string = import.meta.env.VITE_NDLOCR_MODEL_BASE || `${import.meta.env.BASE_URL}ndlocr-models/`;

let ocrPromise: Promise<OcrEngine | null> | null = null;

/** ブラウザで動く OCR（ndlocr-lite-rs の WASM 版）を読み込む。未ビルドなら null。 */
export function loadOcr(onProgress?: (message: string) => void): Promise<OcrEngine | null> {
  if (!ocrPromise) {
    const load = Object.values(ndlocrModules)[0];
    ocrPromise = load
      ? load().then(async (m) => {
          const recognizer = await m.createRecognizer({ modelBaseUrl: NDLOCR_MODEL_BASE, onProgress });
          return { name: "ndlocr-lite-rs", recognize: (gray: Uint8Array, width: number, height: number) => recognizer.recognizePage(gray, width, height) };
        }).catch((error) => {
          ocrPromise = null;
          throw error;
        })
      : Promise.resolve(null);
  }
  return ocrPromise;
}

/**
 * ページを cross-origin isolated にする（ブラウザの OCR が SharedArrayBuffer を使うため）。まだなら service worker
 * （`coi-serviceworker.js`）を登録してページを読み直し、false を返す（呼出元はそのまま待てばよい）。読み直しても
 * isolated にならない（service worker が使えない）ときは、1 度だけ試して false を返し続ける。
 */
export async function ensureCrossOriginIsolated(): Promise<boolean> {
  if (window.crossOriginIsolated) return true;
  if (!("serviceWorker" in navigator)) return false;
  const KEY = "coi-serviceworker-reloaded";
  if (sessionStorage.getItem(KEY)) return false;
  await navigator.serviceWorker.register(`${import.meta.env.BASE_URL}coi-serviceworker.js`);
  await navigator.serviceWorker.ready;
  sessionStorage.setItem(KEY, "1");
  window.location.reload();
  return false;
}

/** OCR のモジュールがビルドに含まれているか（モデルの読み込み前に分かる）。 */
export const OCR_AVAILABLE = Object.keys(ndlocrModules).length > 0;

/** PDF を DOCX にする。`ocr` が null なら PDF の字だけで書く。 */
export async function pdfToDocx(
  docx: KanpoDocx,
  pdf: Uint8Array,
  ocr: OcrEngine | null,
  onProgress: (message: string) => void,
): Promise<{ docx: Uint8Array; images: number; recognized: number }> {
  const images = ocr ? docx.pageImages(pdf) : [];
  const traces: (unknown | null)[] = [];
  for (const [i, img] of images.entries()) {
    onProgress(`OCR ${i + 1}/${images.length}（${img.page + 1} ページ、${img.width}×${img.height}）`);
    traces.push(await ocr!.recognize(img.pixels, img.width, img.height, onProgress));
  }
  onProgress("DOCX を書いています");
  return { docx: docx.convert(pdf, traces), images: images.length, recognized: traces.length };
}

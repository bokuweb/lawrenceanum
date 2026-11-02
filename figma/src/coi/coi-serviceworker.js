// サイトを cross-origin isolated にする service worker（官報 PDF → DOCX のブラウザ OCR 用）。
//
// ブラウザの OCR（ndlocr-lite-rs の WASM 版）は、同期の推論を SharedArrayBuffer と Atomics で onnxruntime-web の
// ワーカーに渡すので、ページが cross-origin isolated（COOP: same-origin・COEP）である必要がある。GitHub Pages は
// 応答のヘッダを変えられないので、この service worker が同じ origin の応答にヘッダを足す。COEP は credentialless
// にして、ほかの origin（R2 の配信データなど）の読み込みは CORP ヘッダが無くても通す。
// /kanpo-docx で OCR を使うときだけ登録する（figma/src/app/data/kanpo-docx.ts の ensureCrossOriginIsolated）。

self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (event) => event.waitUntil(self.clients.claim()));

self.addEventListener("fetch", (event) => {
  const request = event.request;
  // only-if-cached は同じ origin でしか使えない（Chrome の既知の制約）。
  if (request.cache === "only-if-cached" && request.mode !== "same-origin") return;
  if (new URL(request.url).origin !== self.location.origin) return;
  event.respondWith(
    fetch(request).then((response) => {
      if (response.status === 0) return response;
      const headers = new Headers(response.headers);
      headers.set("Cross-Origin-Opener-Policy", "same-origin");
      headers.set("Cross-Origin-Embedder-Policy", "credentialless");
      headers.set("Cross-Origin-Resource-Policy", "cross-origin");
      return new Response(response.body, { status: response.status, statusText: response.statusText, headers });
    }),
  );
});

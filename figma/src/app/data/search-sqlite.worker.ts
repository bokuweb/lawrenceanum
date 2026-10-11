// httpvfs uses synchronous XHR inside this worker. Report completed reads so
// the caller can distinguish a slow, progressing query from a stalled one.
const send = XMLHttpRequest.prototype.send;
XMLHttpRequest.prototype.send = function (body) {
  send.call(this, body);
  self.postMessage({ type: "search-progress" });
};

// The bundled SQLite worker is a classic script (it uses top-level `this`).
// Load it synchronously before processing the following initialization RPC.
self.addEventListener("message", function bootstrap(event) {
  if (event.data?.type !== "search-init") return;
  self.removeEventListener("message", bootstrap);
  importScripts(event.data.url);
});

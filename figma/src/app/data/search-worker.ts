export const SEARCH_TIMEOUT_MS = 12_000;
export type SearchRequest = { signal?: AbortSignal; timeoutMs?: number };

type Pending = { reject: (error: Error) => void; cleanup: () => void };
type State = {
  native?: Worker;
  db?: MessagePort;
  ready: Promise<MessagePort>;
  pending: Set<Pending>;
  calls: Set<(error: Error) => void>;
  closed: boolean;
};

// sql.js-httpvfs 0.8.12 bundles the numeric Comlink wire protocol. Only these
// two read-only RPCs are needed; owning the native Worker lets us terminate a
// synchronous SQLite/XHR call instead of merely timing out its Promise.
function call<T>(state: State, endpoint: Worker | MessagePort, method: string, args: unknown[]): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const id = crypto.randomUUID();
    const cleanup = () => { endpoint.removeEventListener("message", receive); state.calls.delete(cancel); };
    const cancel = (error: Error) => { cleanup(); reject(error); };
    const receive = (event: MessageEvent) => {
      const reply = event.data;
      if (reply?.id !== id) return;
      cleanup();
      if (reply.type === 0 || (reply.type === 3 && reply.name === "WORKERSQLPROXIES")) {
        resolve(reply.value);
      } else if (reply.type === 3 && reply.name === "throw") {
        const value = reply.value;
        reject(value.isError ? Object.assign(new Error(value.value.message), { name: value.value.name }) : new Error(String(value.value)));
      } else { reject(new Error("Unsupported search worker response")); }
    };
    state.calls.add(cancel);
    endpoint.addEventListener("message", receive);
    try {
      endpoint.postMessage({ id, type: 2, path: [method], argumentList: args.map(value => ({ type: 0, value })) });
    } catch (error) { cancel(error instanceof Error ? error : new Error(String(error))); }
  });
}

/** One interruptible worker per search source. A slow query never holds another source. */
export class SearchWorker {
  private state?: State;
  private cache = new Map<string, unknown[]>();

  constructor(private readonly url: string) {}

  private stop(state: State, error: Error) {
    if (state.closed) return;
    state.closed = true;
    state.native?.terminate();
    state.db?.close();
    if (this.state === state) this.state = undefined;
    for (const request of state.pending) {
      request.cleanup();
      request.reject(error);
    }
    state.pending.clear();
    for (const cancel of [...state.calls]) cancel(error);
  }

  private getState(): State {
    if (this.state) return this.state;
    const state: State = { ready: null!, pending: new Set(), calls: new Set(), closed: false };
    this.state = state;
    state.ready = (async () => {
      const [wasm, workerModule] = await Promise.all([
        import("sql.js-httpvfs/dist/sql-wasm.wasm?url"),
        import("sql.js-httpvfs/dist/sqlite.worker.js?url"),
      ]);
      if (state.closed) throw new DOMException("Search cancelled", "AbortError");
      const native = new Worker(workerModule.default);
      state.native = native;
      native.addEventListener("error", event => this.stop(state, new Error(event.message || "Search worker failed")));
      const db = await call<MessagePort>(state, native, "SplitFileHttpDatabase", [wasm.default, [{ from: "inline", config: {
        serverMode: "full", requestChunkSize: 65536, url: this.url,
      } }]]);
      db.start();
      state.db = db;
      return db;
    })();
    state.ready.catch(error => this.stop(state, error));
    return state;
  }

  query<T>(sql: string, params: unknown[] = [], { signal, timeoutMs = SEARCH_TIMEOUT_MS }: SearchRequest = {}): Promise<T[]> {
    if (signal?.aborted) return Promise.reject(new DOMException("Search cancelled", "AbortError"));
    const key = sql.includes(" MATCH ") ? JSON.stringify([sql, params]) : undefined;
    if (key && this.cache.has(key)) {
      const rows = this.cache.get(key)!;
      this.cache.delete(key);
      this.cache.set(key, rows);
      return Promise.resolve(rows as T[]);
    }
    const state = this.getState();
    return new Promise<T[]>((resolve, reject) => {
      const abort = () => this.stop(state, new DOMException("Search cancelled", "AbortError"));
      const timer = setTimeout(() => this.stop(state, new DOMException("Search timed out", "TimeoutError")), timeoutMs);
      const request: Pending = { reject, cleanup: () => {
        clearTimeout(timer);
        signal?.removeEventListener("abort", abort);
      } };
      state.pending.add(request);
      signal?.addEventListener("abort", abort, { once: true });
      state.ready.then(db => {
        if (state.closed) return;
        return call<T[]>(state, db, "query", [sql, params]);
      }).then(rows => {
        if (state.closed || !rows) return;
        if (key) {
          this.cache.set(key, rows);
          if (this.cache.size > 32) this.cache.delete(this.cache.keys().next().value!);
        }
        request.cleanup();
        state.pending.delete(request);
        resolve(rows);
      }, error => {
        request.cleanup();
        state.pending.delete(request);
        reject(error);
      });
    });
  }
}

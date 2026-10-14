// Tasks handle their own results and errors. Give the visible source a head start.
export function createSearchQueue<T>(tasks: Map<T, () => Promise<void>>, initial: T | undefined) {
  const queued = new Map(tasks);
  let cancelled = false;

  // A stalled foreground query must not hold other sources until its timeout.
  const fallback = setTimeout(startRemaining, 1_000);

  function start(target: T) {
    const task = queued.get(target);
    if (!task) return;
    queued.delete(target);
    void Promise.resolve().then(() => { if (!cancelled) return task(); }).then(startRemaining, startRemaining);
  }

  function startRemaining() {
    clearTimeout(fallback);
    if (cancelled) return;
    for (const target of queued.keys()) start(target);
  }

  if (initial !== undefined && queued.has(initial)) start(initial);
  else startRemaining();

  return {
    prioritize(target: T) { if (!cancelled) start(target); },
    cancel() { cancelled = true; clearTimeout(fallback); queued.clear(); },
  };
}

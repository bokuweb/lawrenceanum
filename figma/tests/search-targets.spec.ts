import { test, expect } from "@playwright/test";
import { createSearchQueue } from "../src/app/data/search-queue";

const BASE = process.env.HISTORY_BASE ?? "http://127.0.0.1:8799/";

test("法令と会議録は本文だけの一致でも条番号・話者を表示する", async ({ page }) => {
  await page.goto(new URL("#/search", BASE).toString());
  await page.getByRole("button", { name: "解除", exact: true }).click();
  await page.getByRole("checkbox", { name: "法令・条文", exact: true }).check();
  await page.getByRole("checkbox", { name: "国会会議録", exact: true }).check();
  await page.getByPlaceholder(/検索/).fill("索引照合");
  await expect(page.getByText("試験法", { exact: true })).toBeVisible();
  await expect(page.getByText("第一条", { exact: true })).toBeVisible();
  await expect(page.getByRole("tab", { name: "法令・条文 1件", exact: true })).toHaveAttribute("aria-selected", "true");
  await expect(page.getByText("試験法", { exact: true })).toBeInViewport();
  await expect(page.getByText("試験太郎", { exact: true })).toHaveCount(0);
  await page.getByRole("tab", { name: /^国会会議録/ }).click();
  await expect(page.getByText("試験太郎", { exact: true })).toBeVisible();
  await expect(page.getByText("試験法", { exact: true })).toHaveCount(0);
  await expect(page.getByRole("alert")).toHaveCount(0);
});

for (const [query, label, title, path] of [
  ["添付原文", "パブコメ", "検索テストの意見募集案件", "/pubcomment/search-case"],
  ["提出理由", "議案", "検索テストの法律案", "/gian/221/search-bill"],
  ["配布原文", "審議会資料", "検索テストの審議会", "/shingikai/moj/search-meeting"],
]) {
  test(`${label}の本文・添付から検索して詳細へ移動できる`, async ({ page }) => {
    let reikiRequests = 0;
    page.on("request", req => { if (req.url().includes("reiki-search.db")) reikiRequests++; });
    await page.goto(new URL("#/search", BASE).toString());
    await page.getByRole("button", { name: "解除", exact: true }).click();
    await page.getByRole("checkbox", { name: label, exact: true }).check();
    await page.getByPlaceholder(/検索/).fill(query);
    await expect(page.getByText(`${label} (1件)`, { exact: true })).toBeVisible({ timeout: 20_000 });
    expect(reikiRequests).toBe(0);
    await expect(page.getByText("該当する条文がありません")).toHaveCount(0);
    await page.getByText(title, { exact: true }).click();
    await expect(page).toHaveURL(new URL(`#${path}`, BASE).toString());
    await expect(page.getByRole("heading", { name: title, exact: true })).toBeVisible();
  });
}

test("例規DBの読み込みが遅くても次の検索と対象切り替えを止めない", async ({ page }) => {
  let release = () => {};
  const gate = new Promise<void>(resolve => { release = resolve; });
  let reikiStarted = () => {};
  const started = new Promise<void>(resolve => { reikiStarted = resolve; });
  await page.route("**/reiki-search.db", async route => {
    reikiStarted();
    await gate;
    await route.continue();
  });
  try {
    await page.goto(new URL("#/search?q=郵便法施行規則", BASE).toString());
    await started;
    await expect(page.getByRole("tab", { name: "官報 1件", exact: true })).toBeVisible({ timeout: 20_000 });
    await page.getByRole("tab", { name: /^官報/ }).click();
    await expect(page.getByText("官報 (1件)", { exact: true })).toBeVisible();
    await page.getByPlaceholder(/検索/).fill("添付原文");
    await page.getByRole("tab", { name: /^パブコメ/ }).click();
    await expect(page.getByText("パブコメ (1件)", { exact: true })).toBeVisible({ timeout: 10_000 });
    await expect(page.getByText("官報 (1件)", { exact: true })).toHaveCount(0);
    await page.getByRole("checkbox", { name: "パブコメ", exact: true }).uncheck();
    await expect(page.getByText("パブコメ (1件)", { exact: true })).toHaveCount(0);
    // 再選択しても同じ検索結果を再利用できる。
    await page.getByRole("checkbox", { name: "パブコメ", exact: true }).check();
    await page.getByRole("tab", { name: /^パブコメ/ }).click();
    await expect(page.getByText("パブコメ (1件)", { exact: true })).toBeVisible({ timeout: 10_000 });
  } finally { release(); }
});

test("検索対象の解除と短い検索語を案内する", async ({ page }) => {
  await page.goto(new URL("#/search", BASE).toString());
  await page.getByRole("button", { name: "解除", exact: true }).click();
  await expect(page.getByText("検索対象を選んでください")).toBeVisible();
  await page.getByRole("checkbox", { name: "パブコメ", exact: true }).check();
  await page.getByPlaceholder(/検索/).fill("法");
  await expect(page.getByText("2 文字以上で検索してください")).toBeVisible();
});

test("同じDBへの古い検索が停止していても入力を変えると検索し直せる", async ({ page }) => {
  let blocked = true;
  let release = () => {};
  const gate = new Promise<void>(resolve => { release = resolve; });
  let started = () => {};
  const firstRequest = new Promise<void>(resolve => { started = resolve; });
  await page.route("**/search.db", async route => {
    if (blocked) { started(); await gate; }
    await route.continue().catch(() => {});
  });
  try {
    await page.goto(new URL("#/search?q=郵便法施行規則", BASE).toString());
    await firstRequest;
    blocked = false;
    await page.getByPlaceholder(/検索/).fill("添付原文");
    await page.getByRole("tab", { name: /^パブコメ/ }).click();
    await expect(page.getByText("パブコメ (1件)", { exact: true })).toBeVisible({ timeout: 5000 });
    await expect(page.getByText("官報 (1件)", { exact: true })).toHaveCount(0);
    await expect(page.getByRole("alert")).toHaveCount(0);
  } finally { release(); }
});

test("1つの検索が停止しても他の結果を出し、期限後は終了して再試行できる", async ({ page }) => {
  let blockOne = true;
  let release = () => {};
  const gate = new Promise<void>(resolve => { release = resolve; });
  await page.route("**/search.db", async route => {
    if (blockOne) { blockOne = false; await gate; }
    await route.continue().catch(() => {});
  });
  try {
    await page.goto(new URL("#/search?q=検索テスト", BASE).toString());
    // いずれか1対象のWorkerが停止しても、別の同一DBの対象は検索できる。
    const completedTab = page.getByRole("tab", { name: /^(パブコメ|議案|審議会資料) 1件$/ }).first();
    await expect(completedTab).toBeVisible({ timeout: 5000 });
    await completedTab.click();
    await expect(page.getByText(/^(パブコメ|議案|審議会資料) \(1件\)$/).first()).toBeVisible();
    const failedTab = page.getByRole("tab", { name: /エラー$/ });
    await expect(failedTab).toBeVisible({ timeout: 15000 });
    await failedTab.click();
    await expect(page.getByRole("alert")).toContainText("時間がかかっているため中断しました", { timeout: 15000 });
    await expect(page.getByRole("status")).toHaveCount(0);
    release();
    await page.getByRole("button", { name: "再試行", exact: true }).click();
    for (const label of ["パブコメ", "議案", "審議会資料"]) {
      await expect(page.getByRole("tab", { name: `${label} 1件`, exact: true })).toBeVisible();
      await page.getByRole("tab", { name: `${label} 1件`, exact: true }).click();
      await expect(page.getByText(`${label} (1件)`, { exact: true })).toBeVisible();
    }
    await expect(page.getByRole("status")).toHaveCount(0);
    await expect(page.getByRole("alert")).toHaveCount(0);
  } finally { release(); }
});

test("読み込みが進んでいる検索は12秒を超えても結果を表示する", async ({ page }) => {
  // A slow SQLite worker that keeps completing HTTP reads. The old absolute
  // 12-second timer rejected this valid response before it could arrive.
  await page.route(/\/sqlite\.worker-[^/]+\.js$/, async route => {
    const response = await route.fetch();
    // Vite also emits a tiny ?url module; keep that module intact.
    if ((await response.body()).length < 10_000) { await route.fulfill({ response }); return; }
    await route.fulfill({
      contentType: "text/javascript",
      body: `self.addEventListener('message', event => {
      const channel = new MessageChannel();
      channel.port1.onmessage = ({data}) => {
        const sql = data.argumentList[0].value;
        if (!sql.includes(' MATCH ')) {
          channel.port1.postMessage({id:data.id,type:0,value:[]}); return;
        }
        const progress = setInterval(() => self.postMessage({type:'search-progress'}), 500);
        setTimeout(() => {
          clearInterval(progress);
          channel.port1.postMessage({id:data.id,type:0,value:[{
            law_id:'129AC0000000089',title:'通信が進む検索結果',article_id:'1',
            article_no:'第一条',caption:'労働',law_num:'',snippet:''
          }]});
        }, 13000);
      };
      self.postMessage({id:event.data.id,type:3,name:'WORKERSQLPROXIES',value:channel.port2},[channel.port2]);
    });`,
    });
  });
  await page.goto(new URL("#/search", BASE).toString());
  await page.getByRole("button", { name: "解除", exact: true }).click();
  await page.getByRole("checkbox", { name: "法令・条文", exact: true }).check();
  await page.getByPlaceholder(/検索/).fill("労働");
  await expect(page.getByText("通信が進む検索結果", { exact: true })).toBeVisible({ timeout: 20_000 });
  await expect(page.getByRole("alert")).toHaveCount(0);
  await expect(page.getByRole("status")).toHaveCount(0);
});

test("タブはキーボードで切り替えられ、切り替えても検索をやり直さない", async ({ page }) => {
  await page.addInitScript(() => {
    const OriginalWorker = window.Worker;
    (window as any).searchWorkersCreated = 0;
    window.Worker = class extends OriginalWorker {
      constructor(url: string | URL, options?: WorkerOptions) {
        super(url, options);
        (window as any).searchWorkersCreated++;
      }
    };
  });
  await page.goto(new URL("#/search?q=索引照合", BASE).toString());
  await expect(page.getByText("試験法", { exact: true })).toBeInViewport();
  await expect(page.getByRole("status")).toHaveCount(0);
  await expect(page.getByRole("tab")).toHaveCount(9);
  await expect(page.getByRole("tabpanel")).toHaveCount(1);
  const workersBefore = await page.evaluate(() => (window as any).searchWorkersCreated);
  const lawTab = page.getByRole("tab", { name: "法令・条文 1件", exact: true });
  await lawTab.focus();
  await page.keyboard.press("ArrowRight");
  await expect(page.getByRole("tab", { name: "国会会議録 1件", exact: true })).toHaveAttribute("aria-selected", "true");
  await expect(page.getByText("試験太郎", { exact: true })).toBeVisible();
  await page.getByRole("tab", { name: "官報 0件", exact: true }).click();
  await expect(page.getByText("該当する資料がありません", { exact: true })).toBeVisible();
  await lawTab.click();
  await expect(page.getByText("試験法", { exact: true })).toBeVisible();
  await expect(page.getByRole("status")).toHaveCount(0);
  expect(await page.evaluate(() => (window as any).searchWorkersCreated)).toBe(workersBefore);
  await page.getByRole("checkbox", { name: "法令・条文", exact: true }).uncheck();
  await expect(page.getByRole("tab", { name: /^法令・条文/ })).toHaveCount(0);
  await expect(page.getByRole("tab", { name: /^国会会議録/ })).toHaveAttribute("aria-selected", "true");
  await expect(page.getByText("試験太郎", { exact: true })).toBeVisible();
});

test("待機中のタブへ切り替えると、最初の検索の完了を待たずに検索する", async ({ page }) => {
  let release = () => {};
  const gate = new Promise<void>(resolve => { release = resolve; });
  let first = true;
  let started = () => {};
  const firstRequest = new Promise<void>(resolve => { started = resolve; });
  await page.route("**/search.db", async route => {
    if (first) { first = false; started(); await gate; }
    await route.continue().catch(() => {});
  });
  await page.addInitScript(() => {
    const OriginalWorker = window.Worker;
    (window as any).searchWorkersCreated = 0;
    window.Worker = class extends OriginalWorker {
      constructor(url: string | URL, options?: WorkerOptions) {
        super(url, options);
        (window as any).searchWorkersCreated++;
      }
    };
  });
  try {
    await page.goto(new URL("#/search?q=検索テスト", BASE).toString());
    await firstRequest;
    await expect(page.getByRole("tab", { name: "法令・条文 検索中", exact: true })).toBeVisible();
    await expect(page.getByRole("tab", { name: /待機中$/ })).toHaveCount(8);
    expect(await page.evaluate(() => (window as any).searchWorkersCreated)).toBe(1);
    await page.getByRole("tab", { name: "議案 待機中", exact: true }).click();
    await expect(page.getByText("検索テストの法律案", { exact: true })).toBeVisible({ timeout: 2500 });
    await expect(page.getByRole("tab", { name: "法令・条文 検索中", exact: true })).toBeVisible();
    release();
    await expect(page.getByRole("status")).toHaveCount(0);
    await expect(page.getByRole("alert")).toHaveCount(0);
  } finally { release(); }
});

function controlledSearches() {
  const started: string[] = [];
  const finish = new Map<string, () => void>();
  const tasks = new Map(["laws", "speeches", "kanpo", "tsutatsu", "hanrei", "reiki", "pubcomment", "gian", "shingikai"].map(key => [key, () => new Promise<void>(resolve => {
    started.push(key);
    finish.set(key, resolve);
  })] as const));
  return { started, finish, tasks };
}
const flushQueue = () => new Promise<void>(resolve => setImmediate(resolve));

test("検索キューは表示タブを先行し、選択したタブを重複せずに優先する", async () => {
  const { started, finish, tasks } = controlledSearches();
  const queue = createSearchQueue(tasks, "laws");
  try {
    await flushQueue();
    expect(started).toEqual(["laws"]);
    queue.prioritize("gian");
    queue.prioritize("gian");
    await flushQueue();
    expect(started).toEqual(["laws", "gian"]);
    finish.get("gian")!();
    await flushQueue();
    expect(started).toHaveLength(9);
    expect(new Set(started).size).toBe(9);
  } finally { queue.cancel(); finish.forEach(resolve => resolve()); }
});

test("表示タブの検索が停止しても1秒後には残りの検索を開始する", async () => {
  const { started, finish, tasks } = controlledSearches();
  const queue = createSearchQueue(tasks, "laws");
  try {
    await flushQueue();
    expect(started).toEqual(["laws"]);
    await expect.poll(() => started.length, { timeout: 2500 }).toBe(9);
    expect(new Set(started).size).toBe(9);
  } finally { queue.cancel(); finish.forEach(resolve => resolve()); }
});

test("検索キューを中断すると、完了通知やタブ変更で待機中の検索を開始しない", async () => {
  const { started, finish, tasks } = controlledSearches();
  const queue = createSearchQueue(tasks, "laws");
  await flushQueue();
  queue.cancel();
  queue.prioritize("gian");
  finish.get("laws")!();
  await flushQueue();
  expect(started).toEqual(["laws"]);
});

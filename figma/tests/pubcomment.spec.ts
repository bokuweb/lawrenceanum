import { test, expect } from "@playwright/test";

// パブコメ (PubcommentView) の e2e。pubcomment 系 JSON は fixture を置かず
// page.route で都度モックする (views.spec と同じ方式)。
const BASE = process.env.HISTORY_BASE ?? "http://127.0.0.1:8799/";

const INDEX = {
  schema_version: 1,
  count: 1,
  cases: [
    {
      case_id: "2023-00001",
      title: "テスト民法改正パブリックコメント",
      ministry: "法務省",
      result_published: "2026-06-01",
      related_law_name: "民法",
      category: "民事",
      responsible_office: "法務省民事局",
      opinion_count: 1,
    },
  ],
};

const DETAIL = {
  schema_version: 1,
  case_id: "2023-00001",
  title: "テスト民法改正パブリックコメント",
  ministry: "法務省",
  reception_start: "2026-04-01",
  reception_end: "2026-04-30",
  result_published: "2026-06-01",
  related_law_name: "民法",
  category: "民事",
  opinion_count: 1,
  opinions: [
    {
      item: "第1条関係",
      opinion: "基本原則をより明確にすべきである。",
      ministry_response: "ご意見を踏まえ条文を検討します。",
    },
  ],
  attachments: [
    { name: "結果公示PDF", url: "https://example.com/result.pdf" },
  ],
  source: {
    provider: "egov_pubcomment",
    fetched_at: "2026-06-01T00:00:00Z",
    detail_url: "https://example.com/detail",
  },
};

async function mockPubcomment(page: import("@playwright/test").Page, detail = DETAIL) {
  await page.route("**/pubcomment/index.json", (route) =>
    route.fulfill({ json: INDEX }),
  );
  await page.route("**/pubcomment/2023-00001.json", (route) =>
    route.fulfill({ json: detail }),
  );
}

test("パブコメ一覧に案件と関連法令チップが出る", async ({ page }) => {
  await mockPubcomment(page);
  await page.goto(new URL("#/pubcomment", BASE).toString());

  await expect(page.getByRole("heading", { name: "パブリックコメント" })).toBeVisible({ timeout: 15_000 });
  // 一覧アイテム: 案件名・所管省庁・関連法令名。
  await expect(page.getByText("テスト民法改正パブリックコメント").first()).toBeVisible({ timeout: 15_000 });
  await expect(page.getByText("法務省").first()).toBeVisible();
  await expect(page.getByText("民事").first()).toBeVisible();
  await expect(page.getByText("提出意見 1件").first()).toBeVisible();
});

test("案件を選ぶと意見と府省の考え方・添付PDF・関連法令が出る", async ({ page }) => {
  await mockPubcomment(page);
  // 直接ディープリンクで詳細を開く (HashRouter)。
  await page.goto(new URL("#/pubcomment/2023-00001", BASE).toString());

  // 意見と府省の考え方 (2 カラム)。
  await expect(page.getByText("寄せられた意見")).toBeVisible({ timeout: 15_000 });
  await expect(page.getByText("府省の考え方")).toBeVisible();
  await expect(page.getByText("基本原則をより明確にすべきである。")).toBeVisible();
  await expect(page.getByText("ご意見を踏まえ条文を検討します。")).toBeVisible();

  // 添付 PDF リンク (実体は e-Gov の結果公示 PDF)。
  const pdf = page.getByRole("link", { name: /結果公示PDF/ });
  await expect(pdf).toBeVisible();
  await expect(pdf).toHaveAttribute("href", "https://example.com/result.pdf");

  // 関連法令ボタン (クリックで検索へ誘導)。一覧アイテムも「民法」を含むので exact 指定。
  await expect(page.getByRole("button", { name: "民法", exact: true })).toBeVisible();
});

test("意見が未構造化でも添付PDFの抽出本文を表示・検索できる", async ({ page }) => {
  await mockPubcomment(page, {
    ...DETAIL,
    opinions: [],
    attachments: [{
      name: "提出意見及び府省の考え方",
      url: "https://example.com/result.pdf",
      extracted_text: "長期的なモニタリングを実施すべきである。 府省回答：今後の参考とします。",
    }],
  });
  await page.goto(new URL("#/pubcomment/2023-00001", BASE).toString());

  await expect(page.getByText("添付資料の抽出本文（未構造化）")).toBeVisible({ timeout: 15_000 });
  await expect(page.getByText(/長期的なモニタリングを実施すべき/)).toBeVisible();

  await page.getByPlaceholder("意見・考え方を検索…").fill("モニタリング");
  await expect(page.getByText("1 / 1 件の抽出資料")).toBeVisible();
});

test("募集中の案件では結果公示前であることを表示する", async ({ page }) => {
  await mockPubcomment(page, {
    ...DETAIL,
    status: "open",
    result_published: null,
    opinion_count: null,
    opinions: [],
    attachments: [],
  });
  await page.goto(new URL("#/pubcomment/2023-00001", BASE).toString());

  await expect(page.getByText("意見募集中のため、意見の概要・府省の考え方はまだ公開されていません"))
    .toBeVisible({ timeout: 15_000 });
});

test("結果公示済みで提出意見0件なら件数を明示する", async ({ page }) => {
  await mockPubcomment(page, {
    ...DETAIL,
    status: "closed",
    opinion_count: 0,
    opinions: [],
    attachments: [],
  });
  await page.goto(new URL("#/pubcomment/2023-00001", BASE).toString());

  await expect(page.getByText("提出意見は0件です")).toBeVisible({ timeout: 15_000 });
});

test("一覧から案件をクリックして詳細へ遷移できる", async ({ page }) => {
  await mockPubcomment(page);
  await page.goto(new URL("#/pubcomment", BASE).toString());

  await page.getByText("テスト民法改正パブリックコメント").first().click();
  // URL が詳細へ。
  await expect(page).toHaveURL(/#\/pubcomment\/2023-00001/, { timeout: 15_000 });
  await expect(page.getByText("府省の考え方")).toBeVisible({ timeout: 15_000 });
});

const MEETING = {
  schema_version: 1, meeting_id: "meeting-one", session: 221, house: "衆議院", committee: "法務委員会",
  date: "2026-10-20", issue: "第1号",
  speeches: [{ speech_id: "speech-one", order: 1, speaker: "山田太郎", speaker_id: null,
    speaker_group: null, speaker_position: null, speech: "基本原則について審議します。" }],
  source: { provider: "kokkai", fetched_at: "2026-10-20" },
};

for (const config of [
  { label: "パブコメ", path: "pubcomment", id: "2023-00001", next: "2023-00002",
    index: { ...INDEX, cases: [...INDEX.cases, { ...INDEX.cases[0], case_id: "2023-00002", title: "次の案件" }] },
    detail: DETAIL, nextDetail: { ...DETAIL, case_id: "2023-00002", title: "次の案件" },
    title: DETAIL.title, nextTitle: "次の案件", query: "テスト民法", placeholder: "案件名・法令名…",
    detailQuery: "意見・考え方を検索…", text: DETAIL.opinions[0].opinion },
  { label: "国会会議録", path: "proceedings", id: "meeting-one", next: "meeting-two",
    index: { schema_version: 1, count: 2, meetings: [
      { ...MEETING, speech_count: 1 }, { ...MEETING, meeting_id: "meeting-two", committee: "予算委員会", speech_count: 1 },
    ] },
    detail: MEETING, nextDetail: { ...MEETING, meeting_id: "meeting-two", committee: "予算委員会" },
    title: "法務委員会", nextTitle: "予算委員会", query: "法務", placeholder: "委員会・日付…",
    detailQuery: "発言を検索…", text: MEETING.speeches[0].speech },
]) {
  test(`${config.label}: 取得失敗を表示し一覧の絞り込みを保って再試行できる`, async ({ page }) => {
    let fail = true;
    await page.route(`**/${config.path}/index.json`, route => fail
      ? route.fulfill({ status: 503, body: "unavailable" }) : route.fulfill({ json: config.index }));
    await page.goto(new URL(`#/${config.path}`, BASE).toString());
    await expect(page.getByRole("alert")).toContainText("読み込めませんでした");
    await page.getByPlaceholder(config.placeholder).fill(config.query);
    fail = false;
    await page.getByRole("button", { name: "再試行", exact: true }).click();
    await expect(page.getByRole("button", { name: new RegExp(config.title) })).toBeVisible();
    await expect(page.getByRole("button", { name: new RegExp(config.nextTitle) })).toHaveCount(0);
    await expect(page.getByPlaceholder(config.placeholder)).toHaveValue(config.query);
  });

  test(`${config.label}: 詳細を同じURLで再試行できる`, async ({ page }) => {
    let fail = true;
    await page.route(`**/${config.path}/index.json`, route => route.fulfill({ json: config.index }));
    await page.route(`**/${config.path}/${config.id}.json`, route => fail
      ? route.fulfill({ status: 503, body: "unavailable" }) : route.fulfill({ json: config.detail }));
    await page.goto(new URL(`#/${config.path}/${config.id}`, BASE).toString());
    await expect(page.getByRole("alert")).toContainText("読み込めませんでした");
    fail = false;
    await page.getByRole("button", { name: "再試行", exact: true }).click();
    await expect(page.getByText(config.text, { exact: true })).toBeVisible();
    await expect(page).toHaveURL(new URL(`#/${config.path}/${config.id}`, BASE).toString());
  });

  test(`${config.label}: 次の詳細が取得失敗したとき前の本文を表示しない`, async ({ page }) => {
    await page.route(`**/${config.path}/index.json`, route => route.fulfill({ json: config.index }));
    await page.route(`**/${config.path}/${config.id}.json`, route => route.fulfill({ json: config.detail }));
    await page.route(`**/${config.path}/${config.next}.json`, route => route.fulfill({ status: 503, body: "unavailable" }));
    await page.goto(new URL(`#/${config.path}/${config.id}`, BASE).toString());
    await expect(page.getByText(config.text, { exact: true })).toBeVisible();
    await page.getByRole("button", { name: new RegExp(config.nextTitle) }).click();
    await expect(page.getByRole("alert")).toContainText("読み込めませんでした");
    await expect(page.getByText(config.text, { exact: true })).toHaveCount(0);
  });

  test(`${config.label}: 詳細を切り替えると本文の検索語をリセットする`, async ({ page }) => {
    await page.route(`**/${config.path}/index.json`, route => route.fulfill({ json: config.index }));
    await page.route(`**/${config.path}/${config.id}.json`, route => route.fulfill({ json: config.detail }));
    await page.route(`**/${config.path}/${config.next}.json`, route => route.fulfill({ json: config.nextDetail }));
    await page.goto(new URL(`#/${config.path}/${config.id}`, BASE).toString());
    await expect(page.getByText(config.text, { exact: true })).toBeVisible();
    await page.getByPlaceholder(config.detailQuery).fill("一致しない検索語");
    await page.getByRole("button", { name: new RegExp(config.nextTitle) }).click();
    await expect(page.getByPlaceholder(config.detailQuery)).toHaveValue("");
    await expect(page.getByText(config.text, { exact: true })).toBeVisible();
  });

  test(`${config.label}: スマートフォンで一覧と本文を画面内で操作できる`, async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await page.route(`**/${config.path}/index.json`, route => route.fulfill({ json: config.index }));
    await page.route(`**/${config.path}/${config.id}.json`, route => route.fulfill({ json: config.detail }));
    await page.goto(new URL(`#/${config.path}`, BASE).toString());
    await page.getByRole("button", { name: new RegExp(config.title) }).click();
    const search = page.getByPlaceholder(config.detailQuery);
    await expect(search).toBeVisible();
    const bounds = await search.boundingBox();
    expect(bounds!.x).toBeGreaterThanOrEqual(0);
    expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(390);
    expect(bounds!.y + bounds!.height).toBeLessThanOrEqual(844);
    await expect(page.getByText(config.text, { exact: true })).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
  });
}

test("国会会議録: 次の会議で関連法令を取得できなくても前の会議の法令を残さない", async ({ page }) => {
  const next = { ...MEETING, meeting_id: "meeting-two", committee: "予算委員会" };
  await page.route("**/proceedings/index.json", route => route.fulfill({ json: {
    schema_version: 1, count: 2, meetings: [MEETING, next].map(meeting => ({ ...meeting, speech_count: 1 })),
  } }));
  await page.route("**/proceedings/meeting-one.json", route => route.fulfill({ json: MEETING }));
  await page.route("**/proceedings/meeting-two.json", route => route.fulfill({ json: next }));
  await page.route("**/links/meeting-to-laws/meeting-one.json", route => route.fulfill({ json: {
    schema_version: 1, meeting_id: "meeting-one", linked_laws: [{
      law_id: "old-law", title: "前の会議だけの関連法令", relevance: "reference_only", confidence: 1, match_reasons: [],
    }],
  } }));
  await page.route("**/links/meeting-to-laws/meeting-two.json", route => route.fulfill({ status: 503, body: "unavailable" }));
  await page.goto(new URL("#/proceedings/meeting-one", BASE).toString());
  await expect(page.getByRole("button", { name: "前の会議だけの関連法令" })).toBeVisible();
  await page.getByRole("button", { name: /予算委員会/ }).click();
  await expect(page.getByText("予算委員会", { exact: true }).last()).toBeVisible();
  await expect(page.getByRole("button", { name: "前の会議だけの関連法令" })).toHaveCount(0);
});

test("国会会議録: 一覧を委員会名で絞っても発言本文は絞り込まない", async ({ page }) => {
  await page.route("**/proceedings/index.json", route => route.fulfill({ json: {
    schema_version: 1, count: 1, meetings: [{ ...MEETING, speech_count: 1 }],
  } }));
  await page.route("**/proceedings/meeting-one.json", route => route.fulfill({ json: MEETING }));
  await page.goto(new URL("#/proceedings", BASE).toString());
  await page.getByPlaceholder("委員会・日付…").fill("法務");
  await page.getByRole("button", { name: /法務委員会/ }).click();
  await expect(page.getByText(MEETING.speeches[0].speech, { exact: true })).toBeVisible();
  await expect(page.getByPlaceholder("委員会・日付…")).toHaveValue("法務");
  await expect(page.getByPlaceholder("発言を検索…")).toHaveValue("");
  await page.getByPlaceholder("発言を検索…").fill("一致しない検索語");
  await expect(page.getByText("該当する発言がありません")).toBeVisible();
  await page.getByPlaceholder("発言を検索…").fill("");
  await expect(page.getByText(MEETING.speeches[0].speech, { exact: true })).toBeVisible();
});

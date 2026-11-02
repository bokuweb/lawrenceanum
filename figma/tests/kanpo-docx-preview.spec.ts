import { test, expect } from "@playwright/test";
import { readFileSync } from "node:fs";

async function painted(locator: import("@playwright/test").Locator) {
  await expect(locator).toBeVisible({ timeout: 120_000 });
  await expect
    .poll(
      () =>
        locator.evaluate((canvas: HTMLCanvasElement) => {
          if (!canvas.width || !canvas.height) return false;
          const { data } = canvas
            .getContext("2d")!
            .getImageData(0, 0, canvas.width, canvas.height);
          let ink = 0;
          for (let i = 0; i < data.length; i += 4)
            if (data[i + 3] > 0 && data[i] + data[i + 1] + data[i + 2] < 600)
              ink++;
          return ink > 100;
        }),
      { timeout: 120_000 },
    )
    .toBe(true);
}

test("hidden comparison page converts a selected kanpo and renders actual PDF.js / JADE pages", async ({
  page,
}) => {
  test.setTimeout(180_000);
  await page.goto("#/kanpo-docx-preview");
  await expect(
    page.getByRole("heading", { name: "官報 PDF・DOCX 比較" }),
  ).toBeVisible();
  await expect(page.locator('a[href*="kanpo-docx-preview"]')).toHaveCount(0);
  await page.getByLabel("入力元").selectOption("corpus");
  await page
    .getByLabel("収録済みの官報")
    .selectOption("2026-07-28-honshi1756-p4:0");
  await painted(page.getByTestId("kanpo-original-canvas"));
  await page.getByRole("button", { name: "DOCXに変換" }).click();
  await painted(page.getByTestId("jade-docx-canvas"));
  await expect(page.getByTestId("jade-docx-preview")).toHaveAttribute(
    "data-engine",
    "jade-core",
  );
  await expect(
    page.getByRole("link", { name: "DOCXをダウンロード" }),
  ).toBeVisible();
  const pdfBox = await page.getByTestId("kanpo-original-panel").boundingBox();
  const docxBox = await page.getByTestId("kanpo-docx-panel").boundingBox();
  expect(pdfBox!.x + pdfBox!.width).toBeLessThanOrEqual(docxBox!.x + 2);
  const originalWidth = await page
    .getByTestId("kanpo-original-canvas")
    .evaluate((canvas) => canvas.getBoundingClientRect().width);
  const docxWidth = await page
    .getByTestId("jade-docx-canvas")
    .evaluate((canvas) => canvas.getBoundingClientRect().width);
  await page.getByLabel("DOCXの表示倍率").selectOption("1.25");
  await expect
    .poll(() =>
      page
        .getByTestId("jade-docx-canvas")
        .evaluate((canvas) => canvas.getBoundingClientRect().width),
    )
    .toBeGreaterThan(docxWidth);
  expect(
    await page
      .getByTestId("kanpo-original-canvas")
      .evaluate((canvas) => canvas.getBoundingClientRect().width),
  ).toBe(originalWidth);
  await page.getByLabel("PDFの表示倍率").selectOption("1.25");
  await expect
    .poll(() =>
      page
        .getByTestId("kanpo-original-canvas")
        .evaluate((canvas) => canvas.getBoundingClientRect().width),
    )
    .toBeGreaterThan(originalWidth);
});

test("local PDF can replace a previous conversion; invalid input does not leave stale DOCX", async ({
  page,
}) => {
  test.setTimeout(180_000);
  await page.goto("#/kanpo-docx-preview");
  await page.getByLabel("入力元").selectOption("local");
  const pdf = new URL(
    "../../crates/kanpo-parser/tests/corpus/2026-07-28-honshi1756-p4/page.pdf",
    import.meta.url,
  );
  await page.getByLabel("PDFファイル").setInputFiles({
    name: "官報.pdf",
    mimeType: "application/pdf",
    buffer: readFileSync(pdf),
  });
  await painted(page.getByTestId("kanpo-original-canvas"));
  await page.getByRole("button", { name: "DOCXに変換" }).click();
  await painted(page.getByTestId("jade-docx-canvas"));
  await page.getByLabel("PDFファイル").setInputFiles({
    name: "invalid.pdf",
    mimeType: "application/pdf",
    buffer: Buffer.from("not a pdf"),
  });
  await expect(
    page.getByRole("link", { name: "DOCXをダウンロード" }),
  ).toHaveCount(0);
  await expect(page.getByTestId("jade-docx-canvas")).toHaveCount(0);
  await expect(page.getByRole("alert")).toContainText(/PDF/);
});

test("archive Selects load the selected issue and page, and keep the conversion input intact", async ({
  page,
}) => {
  test.setTimeout(180_000);
  const pdf = readFileSync(
    new URL(
      "../../crates/kanpo-parser/tests/corpus/2026-07-28-honshi1756-p4/page.pdf",
      import.meta.url,
    ),
  );
  const requests: string[] = [];
  await page.route("**/__test_kanpo_archive/**", async (route) => {
    const path = new URL(route.request().url()).pathname;
    requests.push(path);
    if (path.endsWith("/index.json"))
      return route.fulfill({ json: { dates: ["2026-07-28", "2026-07-29"] } });
    if (path.endsWith("/manifest.json"))
      return route.fulfill({
        json: {
          date: path.split("/")[2],
          issues: [
            {
              issue_type: "regular",
              issue_no: "1756",
              dir: "honshi1756",
              url: "",
              page_count: 2,
              pages: [1, 2].map((page) => ({
                page,
                file: `page${page}.pdf`,
                sha256: "",
                bytes: pdf.length,
                bbox: false,
              })),
              items: [],
            },
          ],
        },
      });
    return route.fulfill({ body: pdf, contentType: "application/pdf" });
  });
  await page.goto("#/kanpo-docx-preview");
  await page.getByLabel("入力元").selectOption("archive");
  await expect(page.getByLabel("発行日")).toHaveValue("2026-07-29");
  await page.getByLabel("発行日").selectOption("2026-07-28");
  await expect(page.getByLabel("官報の号")).toHaveValue("honshi1756");
  await page.getByLabel("官報のページ").selectOption("page2.pdf");
  await painted(page.getByTestId("kanpo-original-canvas"));
  await page.getByRole("button", { name: "DOCXに変換" }).click();
  await painted(page.getByTestId("jade-docx-canvas"));
  expect(requests).toContain(
    "/__test_kanpo_archive/2026-07-28/honshi1756/page2.pdf",
  );
  await page.getByLabel("官報のページ").selectOption("page1.pdf");
  await expect(
    page.getByRole("link", { name: "DOCXをダウンロード" }),
  ).toHaveCount(0);
  await expect(page.getByTestId("jade-docx-canvas")).toHaveCount(0);
});

import { test, expect, type Locator } from "@playwright/test";
import { readdirSync } from "node:fs";

async function expectGeometry(sheet: Locator, width: number, height: number, scale?: number) {
  await expect(sheet).toHaveAttribute("data-aligned", "true");
  const svg = sheet.locator("svg[data-page]");
  await expect(svg).toHaveAttribute("viewBox", `0 0 ${width} ${height}`);
  await expect.poll(async () => sheet.locator("canvas").evaluate((canvas: HTMLCanvasElement) => {
    // サイズ設定だけでなく PDF の描画完了まで待つ。
    const pixels = canvas.getContext("2d")!.getImageData(0, 0, canvas.width, canvas.height).data;
    return pixels.some((v, i) => i % 4 === 3 && v > 0);
  })).toBe(true);
  const geometry = await svg.evaluate((el: SVGSVGElement) => {
    const rect = el.getBoundingClientRect();
    const canvas = el.parentElement!.querySelector("canvas")!.getBoundingClientRect();
    const matrix = el.getScreenCTM()!;
    return { width: rect.width, height: rect.height, canvasWidth: canvas.width, canvasHeight: canvas.height,
      scaleX: matrix.a, scaleY: matrix.d };
  });
  expect(geometry.width / geometry.height).toBeCloseTo(width / height, 2);
  expect(geometry.canvasWidth).toBeCloseTo(geometry.width, 2);
  expect(geometry.canvasHeight).toBeCloseTo(geometry.height, 2);
  expect(geometry.scaleX).toBeCloseTo(geometry.scaleY, 5);
  if (scale !== undefined) {
    expect(geometry.width).toBeCloseTo(width * scale, 1);
    expect(geometry.height).toBeCloseTo(height * scale, 1);
  }
}

async function expectWordBox(sheet: Locator, text: string, box: number[]) {
  const rect = sheet.locator("rect[data-w]").filter({ hasText: new RegExp(`^${text}（`) });
  await expect(rect).toHaveCount(1);
  const actual = await rect.evaluate((r: SVGRectElement) => [r.x.baseVal.value, r.y.baseVal.value,
    r.x.baseVal.value + r.width.baseVal.value, r.y.baseVal.value + r.height.baseVal.value]);
  actual.forEach((v, i) => expect(v).toBeCloseTo(box[i], 2));
}

for (const [id, width, height] of [
  ["2026-10-13-gougai224-p69", 842, 595],
  ["2026-10-15-honshi1808-p8", 842, 595],
  ["2026-07-24-honshi1754-p2", 595, 842],
] as const) {
  test(`${id}: PDF とハイライトが同じ縦横比・倍率で表示される`, async ({ page }) => {
    await page.goto(`#/kanpo-playground?src=corpus&case=${id}`);
    const sheet = page.locator('[data-sheet="0"]');
    await expectGeometry(sheet, width, height);
    await page.getByRole("button", { name: "100%", exact: true }).click();
    await expectGeometry(sheet, width, height, 1);
    await page.getByRole("button", { name: "150%", exact: true }).click();
    await expectGeometry(sheet, width, height, 1.5);
    await page.getByRole("button", { name: "幅に合わせる", exact: true }).click();
    await page.setViewportSize({ width: 1000, height: 900 });
    await expect.poll(async () => (await sheet.boundingBox())!.width).toBeLessThan(760);
    await expectGeometry(sheet, width, height);
    // 回転前の幅 595pt を超える語も、PDF の右側に収まる。
    if (id === "2026-10-15-honshi1808-p8") {
      // /Rotate=270 の縦書きフォント。ページの縦横比だけでは検出できなかったずれ。
      await expectWordBox(sheet, "皇室事項", [299.7202, 208.0002, 369.7202, 222.0002]);
      await expectWordBox(sheet, "御祝電", [244.84, 233, 268.84, 241]);
      const rightmost = await sheet.locator("rect[data-w]").evaluateAll((rects: SVGRectElement[]) =>
        Math.max(...rects.map((r) => r.x.baseVal.value + r.width.baseVal.value)));
      expect(rightmost).toBeGreaterThan(595);
      expect(rightmost).toBeLessThan(width);
    }
  });
}

const corpusRoot = new URL("../../crates/kanpo-parser/tests/corpus/", import.meta.url);
for (const { name: id } of readdirSync(corpusRoot, { withFileTypes: true }).filter((entry) => entry.isDirectory())) {
  const pdfs = readdirSync(new URL(`${id}/`, corpusRoot)).filter((name) => /^page(?:-\d+)?\.pdf$/.test(name));
  if (!pdfs.length) continue;
  test(`${id}: 全ページの語の枠が PDF の描画上に載る`, async ({ page }) => {
    await page.goto(`#/kanpo-playground?src=corpus&case=${id}`);
    // 細い傍線・記号も画素として検査できる大きさで描画する。
    await page.getByRole("button", { name: "150%", exact: true }).click();
    for (let index = 0; index < pdfs.length; index++) {
      const sheet = page.locator(`[data-sheet="${index}"]`);
      await sheet.scrollIntoViewIfNeeded();
      await expect(sheet).toHaveAttribute("data-aligned", "true");
      await expect.poll(async () => sheet.evaluate((el) => {
        const canvas = el.querySelector("canvas")!;
        const svg = el.querySelector("svg[data-page]") as SVGSVGElement;
        const ctx = canvas.getContext("2d")!;
        const sx = canvas.width / svg.viewBox.baseVal.width;
        const sy = canvas.height / svg.viewBox.baseVal.height;
        const rects = [...svg.querySelectorAll<SVGRectElement>("rect[data-w]")];
        const missing: string[] = [];
        for (const r of rects) {
          const x = Math.max(0, Math.floor(r.x.baseVal.value * sx));
          const y = Math.max(0, Math.floor(r.y.baseVal.value * sy));
          const w = Math.min(canvas.width - x, Math.ceil(r.width.baseVal.value * sx));
          const h = Math.min(canvas.height - y, Math.ceil(r.height.baseVal.value * sy));
          if (w <= 0 || h <= 0) { missing.push(r.textContent ?? "outside canvas"); continue; }
          const data = ctx.getImageData(x, y, w, h).data;
          if (!data.some((v, i) => i % 4 === 0 && v < 245 && data[i + 3] > 0)) missing.push(r.textContent ?? "");
        }
        return rects.length ? missing : ["no word boxes"];
      })).toEqual([]);
    }
  });
}

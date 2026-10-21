import { test, expect, type Locator } from "@playwright/test";

async function expectGeometry(sheet: Locator, width: number, height: number, scale?: number) {
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
      const rightmost = await sheet.locator("rect[data-w]").evaluateAll((rects: SVGRectElement[]) =>
        Math.max(...rects.map((r) => r.x.baseVal.value + r.width.baseVal.value)));
      expect(rightmost).toBeGreaterThan(595);
      expect(rightmost).toBeLessThan(width);
    }
  });
}

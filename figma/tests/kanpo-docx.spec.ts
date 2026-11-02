import { test, expect } from "@playwright/test";
import { readFileSync } from "node:fs";
import { inflateRawSync } from "node:zlib";

// 官報 PDF → DOCX（/kanpo-docx）。コーパスの PDF を選び、ダウンロードした DOCX の本文を確かめる。
// OCR（ndlocr-lite-rs の WASM 版）はこのビルドに含めないので、PDF の字だけで書く。

/** DOCX（zip）から 1 つの部品を取り出す（中央ディレクトリを読む）。 */
function zipEntry(zip: Buffer, name: string): string {
  const end = zip.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
  let p = zip.readUInt32LE(end + 16);
  for (let n = zip.readUInt16LE(end + 10); n > 0; n--) {
    const method = zip.readUInt16LE(p + 10);
    const size = zip.readUInt32LE(p + 20);
    const nameLen = zip.readUInt16LE(p + 28);
    const extra = zip.readUInt16LE(p + 30);
    const comment = zip.readUInt16LE(p + 32);
    const local = zip.readUInt32LE(p + 42);
    if (zip.toString("utf8", p + 46, p + 46 + nameLen) === name) {
      const start = local + 30 + zip.readUInt16LE(local + 26) + zip.readUInt16LE(local + 28);
      const data = zip.subarray(start, start + size);
      return (method === 8 ? inflateRawSync(data) : data).toString("utf8");
    }
    p += 46 + nameLen + extra + comment;
  }
  throw new Error(`${name} not found`);
}

test("コーパスの PDF を DOCX にしてダウンロードできる", async ({ page }) => {
  test.setTimeout(120_000);
  await page.goto("#/kanpo-docx");
  await expect(page.getByRole("heading", { name: "官報 PDF → DOCX" })).toBeVisible();
  // WASM を読み込むまで、ファイルの入力は無効。
  await expect(page.locator('input[type="file"]')).toBeEnabled({ timeout: 60_000 });
  const pdf = new URL("../../crates/kanpo-parser/tests/corpus/2026-07-28-honshi1756-p4/page.pdf", import.meta.url);
  await page.locator('input[type="file"]').setInputFiles({ name: "honshi1756-p4.pdf", mimeType: "application/pdf", buffer: readFileSync(pdf) });
  const link = page.getByRole("link", { name: "ダウンロード" });
  await expect(link).toBeVisible({ timeout: 60_000 });
  const [download] = await Promise.all([page.waitForEvent("download"), link.click()]);
  expect(download.suggestedFilename()).toBe("honshi1756-p4.docx");
  const xml = zipEntry(readFileSync(await download.path()), "word/document.xml");
  const text = xml.replace(/<[^>]+>/g, "");
  expect(text).toContain("経済産業省告示第九十号");
  expect(text).toContain("川重ファシリテック株式会社");
  expect(xml).toContain("<w:tbl>");
});

import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { writeFile } from "node:fs/promises";
import { chromium } from "playwright-core";

const baseUrl = process.env.REALTIME_MANIM_SITE_URL ?? "http://localhost:3001";
const browser = await chromium.launch({
  headless: true,
  executablePath: "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  args: ["--enable-unsafe-webgpu", "--enable-features=Vulkan,UseSkiaRenderer"],
});
const page = await browser.newPage({ viewport: { width: 1440, height: 1000 }, deviceScaleFactor: 1 });
const errors = [];
page.on("response", (response) => {
  if (response.status() >= 400) errors.push(`http ${response.status()}: ${response.url()}`);
});
page.on("console", (message) => {
  if (message.type() === "error" || message.type() === "warning") {
    const source = message.location().url;
    errors.push(`${message.type()}: ${message.text()}${source ? ` (${source})` : ""}`);
  }
});
page.on("pageerror", (error) => errors.push(`pageerror: ${error.message}`));

const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");

try {
  const response = await page.goto(baseUrl, { waitUntil: "networkidle" });
  assert.equal(response?.status(), 200);
  await page.locator("#demos").scrollIntoViewIfNeeded();
  await page.waitForFunction(() => document.querySelector(".live-indicator")?.textContent?.startsWith("Live"), null, { timeout: 30_000 });
  assert.equal(await page.locator(".canvas-error").count(), 0);
  assert.equal(await page.locator(".parameter").count(), 3);

  const favicon = await page.request.get(`${baseUrl}/favicon.svg`);
  assert.equal(favicon.status(), 200);
  assert.match(await favicon.text(), /<svg/);

  const sceneHashes = [];
  for (const title of ["Möbius light field", "Torus-knot reactor", "Gravitational wave grid"]) {
    await page.getByRole("tab", { name: new RegExp(title) }).click();
    await page.waitForFunction(() => document.querySelector(".live-indicator")?.textContent?.startsWith("Live"), null, { timeout: 30_000 });
    await page.getByRole("button", { name: "Pause" }).click();
    const canvasPng = await page.locator(".canvas-frame canvas").screenshot();
    assert.ok(canvasPng.byteLength > 10_000, `${title} canvas should be nonblank`);
    sceneHashes.push(hash(canvasPng));
  }
  assert.equal(new Set(sceneHashes).size, 3, "All three demos should render distinct frames");

  const slider = page.locator(".parameter input").first();
  const before = await page.locator(".canvas-frame canvas").screenshot();
  await slider.fill(await slider.getAttribute("max") ?? "2");
  await page.waitForTimeout(250);
  const after = await page.locator(".canvas-frame canvas").screenshot();
  assert.notEqual(hash(before), hash(after), "Dragging a parameter should update the WebGPU frame");

  const editor = page.getByLabel("Editable realtime-manim scene code");
  const source = await editor.inputValue();
  await editor.fill(source.replace("#030712", "#160510"));
  await page.getByRole("button", { name: "Render code" }).click();
  await page.waitForFunction(() => document.querySelector(".live-indicator")?.textContent?.startsWith("Live"), null, { timeout: 30_000 });
  assert.equal(await page.locator(".canvas-error").count(), 0);

  await page.screenshot({ path: "/tmp/realtime-manim-docs-playground-desktop.png", fullPage: true });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.reload({ waitUntil: "networkidle" });
  await page.locator("#demos").scrollIntoViewIfNeeded();
  await page.waitForFunction(() => document.querySelector(".live-indicator")?.textContent?.startsWith("Live"), null, { timeout: 30_000 });
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
  assert.ok(overflow <= 1, `Mobile layout overflows horizontally by ${overflow}px`);
  await page.screenshot({ path: "/tmp/realtime-manim-docs-playground-mobile.png", fullPage: true });

  assert.deepEqual(errors, []);
  await writeFile("/tmp/realtime-manim-docs-playground-qa.json", JSON.stringify({ ok: true, sceneHashes }, null, 2));
  console.log(JSON.stringify({ ok: true, sceneHashes, desktop: "/tmp/realtime-manim-docs-playground-desktop.png", mobile: "/tmp/realtime-manim-docs-playground-mobile.png" }));
} finally {
  await browser.close();
}

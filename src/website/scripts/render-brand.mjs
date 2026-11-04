import { readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { chromium } from "@playwright/test";

const images = new URL("../src/assets/images/", import.meta.url);
const logo = (await readFile(new URL("logo.png", images))).toString("base64");
const browser = await chromium.launch();
try {
  const page = await browser.newPage({ viewport: { width: 1200, height: 630 }, deviceScaleFactor: 1 });
  await page.setContent(`<!doctype html><html lang="en"><meta charset="utf-8">
    <style>
      * { box-sizing: border-box; }
      body { margin: 0; background: #0c1325; color: #dce1fb; font-family: Geist, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }
      main { height: 630px; padding: 64px; display: flex; align-items: center; gap: 32px; }
      section { flex: 1; }
      h1 { margin: 0 0 28px; font-size: 88px; letter-spacing: -4px; font-weight: 600; }
      p { margin: 0 0 22px; font-size: 34px; line-height: 1.3; }
      .accent { color: #77d7f4; }
      .detail { color: #bdc8cd; font-size: 23px; line-height: 1.5; }
      .address { margin-top: 44px; color: #bdeeff; font-size: 22px; }
      img { width: 440px; height: 440px; object-fit: contain; }
    </style><main><section><h1>Osprey</h1>
      <p>Effects in <span class="accent">your hands.</span></p>
      <p class="detail">Functional programming.<br>Typed effects. Native compilation.</p>
      <p class="address">ospreylang.dev</p>
    </section><img src="data:image/png;base64,${logo}" alt="Osprey in flight"></main></html>`);
  await page.locator("img").evaluate(image => image.decode());
  const card = await page.screenshot({ type: "png" });
  for (const name of ["og-image.png", "twitter-card.png"]) {
    await writeFile(new URL(name, images), card);
  }
  console.log(`Brand cards rendered at 1200×630 from ${fileURLToPath(new URL("logo.png", images))}`);
} finally {
  await browser.close();
}

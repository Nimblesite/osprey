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
  const icons = new Map();
  for (const size of [16, 32, 48, 180]) icons.set(size, await renderIcon(browser, size));
  await writeFile(new URL("favicon-32.png", images), icons.get(32));
  await writeFile(new URL("apple-touch-icon.png", images), icons.get(180));
  await writeFile(new URL("../../favicon.ico", images), ico([16, 32, 48].map(size => [size, icons.get(size)])));
  console.log("Icons rendered: favicon.ico (16, 32, 48), favicon-32.png, apple-touch-icon.png (180)");
} finally {
  await browser.close();
}

// The bird on the site's own navy, square: browsers draw a tab icon at 16 to 32
// pixels and Safari ignores a 1024-pixel PNG there, so each size is its own file.
async function renderIcon(browser, size) {
  const page = await browser.newPage({ viewport: { width: size, height: size }, deviceScaleFactor: 1 });
  const radius = size >= 180 ? 0 : Math.round(size / 5);
  await page.setContent(`<!doctype html><html lang="en"><style>
      body { margin: 0; background: transparent; }
      div { width: ${size}px; height: ${size}px; border-radius: ${radius}px; background: #0c1325; display: flex; }
      img { width: 100%; height: 100%; object-fit: contain; }
    </style><div><img src="data:image/png;base64,${logo}" alt=""></div></html>`);
  await page.locator("img").evaluate(image => image.decode());
  const shot = await page.screenshot({ type: "png", omitBackground: true });
  await page.close();
  return shot;
}

// An ICO file holding PNG images, one directory entry per size.
function ico(images) {
  const header = Buffer.alloc(6 + 16 * images.length);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(images.length, 4);
  let offset = header.length;
  images.forEach(([size, png], i) => {
    const entry = 6 + 16 * i;
    header.writeUInt8(size % 256, entry);
    header.writeUInt8(size % 256, entry + 1);
    header.writeUInt16LE(1, entry + 4);
    header.writeUInt16LE(32, entry + 6);
    header.writeUInt32LE(png.length, entry + 8);
    header.writeUInt32LE(offset, entry + 12);
    offset += png.length;
  });
  return Buffer.concat([header, ...images.map(([, png]) => png)]);
}

// Reproduce the homepage film: run node scripts/render-flight.mjs from src/toolchain/website/.
import { chromium } from '@playwright/test';
import { mkdir, writeFile } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';

const browser = await chromium.launch({ headless: true });
try {
  const page = await browser.newPage({ viewport: { width: 1440, height: 960 } });
  await page.setContent('<canvas width="1440" height="960"></canvas>');
  await page.addScriptTag({ path: 'scripts/flight-scene.js' });
  const poster = await page.locator('canvas').screenshot();
  const film = await page.evaluate(() => window.recordFlight());
  await mkdir('src/assets/motion', { recursive: true });
  await writeFile('src/assets/motion/osprey-flight.png', poster);
  await writeFile('src/assets/motion/osprey-flight.webm', Buffer.from(film));
  execFileSync('ffmpeg', ['-y', '-v', 'error', '-i', 'src/assets/motion/osprey-flight.webm', '-c:v', 'libx264', '-crf', '23', '-pix_fmt', 'yuv420p', '-an', '-movflags', '+faststart', 'src/assets/motion/osprey-flight.mp4']);
  console.log(`Flight film: ${film.length} bytes; poster: ${poster.length} bytes`);
} finally {
  await browser.close();
}

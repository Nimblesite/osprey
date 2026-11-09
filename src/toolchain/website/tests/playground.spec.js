const { test, expect } = require('@playwright/test');
const { execFileSync } = require('node:child_process');
const { writeFile } = require('node:fs/promises');
const { readFileSync } = require('node:fs');
const path = require('node:path');

const sample = path.resolve(__dirname, '../../../osprey/tests/effects/install/install.test');
const pythonOutput = readFileSync(`${sample}.osp.expectedoutput`, 'utf8');

async function openPlayground(page) {
  await page.goto('/playground/');
  await expect(page.locator('#status-text')).toHaveText('Ready', { timeout: 30000 });
  await page.locator('#flavor-py').click();
  await expect(page.locator('#flavor-py')).toHaveAttribute('aria-pressed', 'true');
}

test('Python playground opens on the install sample, compiles and runs it', async ({ page }, testInfo) => {
  await page.route('**/api/*', async route => {
    const { code, flavor } = route.request().postDataJSON();
    expect(flavor).toBe('python');
    const file = testInfo.outputPath('playground.py');
    await writeFile(file, code);
    const run = route.request().url().endsWith('/run');
    const result = execFileSync(path.resolve('../../../target/release/osprey'), [file, '--sandbox', run ? '--run' : '--ast', '--quiet'], { encoding: 'utf8', timeout: 20000 });
    if (run) expect(result).toBe(pythonOutput);
    await route.fulfill({ json: { success: true, programOutput: run ? result : '', compilerOutput: '' } });
  });
  await openPlayground(page);
  expect(await page.evaluate(() => editor.getModel().getLanguageId())).toBe('python');
  expect(await page.evaluate(() => editor.getValue())).toBe(readFileSync(`${sample}.py`, 'utf8'));
  await page.getByRole('button', { name: 'Compile', exact: true }).click();
  await expect(page.locator('#output')).toContainText('Compilation successful');
  await page.getByRole('button', { name: 'Run', exact: true }).click();
  await expect(page.locator('#output')).toContainText('ok 3 - a read-only disk changes nothing');
});

test('playground flavor changes preserve edits and switch highlighting', async ({ page }) => {
  await openPlayground(page);
  await page.evaluate(() => editor.setValue('# osprey: flavor=python\nprint("my code")'));
  for (const [flavor, language] of [['ospml', 'osprey'], ['osp', 'osprey'], ['py', 'python']]) {
    await page.locator(`#flavor-${flavor}`).click();
    expect(await page.evaluate(() => editor.getValue())).toContain('my code');
    expect(await page.evaluate(() => editor.getModel().getLanguageId())).toBe(language);
    await expect(page.locator('.flavor-btn[aria-pressed="true"]')).toHaveCount(1);
  }
});


test('all syntax flavor controls fit the mobile playground', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await openPlayground(page);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
  for (const button of await page.locator('.flavor-btn').all()) {
    const box = await button.boundingBox();
    expect(box.x).toBeGreaterThanOrEqual(0);
    expect(box.x + box.width).toBeLessThanOrEqual(390);
  }
});

test('the playground is online, and a refused request reports only the connection failure', async ({ page }) => {
  await page.route('**/api/*', (route) => route.abort('connectionrefused'));
  await page.goto('/playground/');
  await expect(page.locator('#status-text')).toHaveText('Ready', { timeout: 30000 });
  await expect(page.locator('#playground-outage')).toHaveCount(0);
  await expect(page.locator('body')).not.toContainText(/offline|CVE-/i);
  for (const action of ['Compile', 'Run']) {
    await page.getByRole('button', { name: action, exact: true }).click();
    await expect(page.locator('#output')).toContainText('Failed to connect to compiler');
    await expect(page.locator('#output')).not.toContainText(/offline|CVE-/i);
    await expect(page.locator('#status-text')).toHaveText('Connection failed');
  }
});

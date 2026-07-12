// End-to-end tests for Talon Bank: a real browser against the real compiled
// Osprey binary — server-driven HTML, the JSON API, domain refusals, the
// double-entry activity journal, and the architecture rule that the UI
// reflects exactly what the API serves.
const { test, expect } = require('@playwright/test');

test.describe.configure({ mode: 'serial' });

test.describe('JSON API over the data', () => {
  test('lists the seeded accounts with machine and display money', async ({ request }) => {
    const reply = await request.get('/api/accounts');
    expect(reply.status()).toBe(200);
    const accounts = await reply.json();
    expect(accounts).toEqual([
      { id: 1, owner: 'Amelia Chen', cents: 246240, balance: '$2,462.40' },
      { id: 2, owner: 'Marcus Webb', cents: 191785, balance: '$1,917.85' },
      { id: 3, owner: 'Priya Sharma', cents: 360000, balance: '$3,600.00' },
    ]);
  });

  test('journals every movement, refusals included, newest first', async ({ request }) => {
    const reply = await request.get('/api/activity');
    expect(reply.status()).toBe(200);
    const feed = await reply.json();
    expect(feed).toHaveLength(7);
    expect(feed[0]).toEqual({
      kind: 'refused',
      cents: 999900,
      note: 'vintage synthesizer',
      owner: 'Marcus Webb',
    });
    // The transfer is double-entry: one debit, one credit, same note.
    const dinner = feed.filter((entry) => entry.note === 'dinner split');
    expect(dinner.map((entry) => entry.kind).sort()).toEqual(['credit', 'debit']);
  });

  test('refuses an overdraft with 422 and a domain reason', async ({ request }) => {
    const reply = await request.post('/api/withdraw', {
      data: { account: 3, cents: 99999999, note: 'yacht' },
    });
    expect(reply.status()).toBe(422);
    expect(await reply.json()).toEqual({ error: 'insufficient funds' });
  });

  test('refuses an unnamed account with 422', async ({ request }) => {
    const reply = await request.post('/api/accounts', { data: {} });
    expect(reply.status()).toBe(422);
    expect(await reply.json()).toEqual({ error: 'owner name required' });
  });

  test('404s unknown endpoints', async ({ request }) => {
    const reply = await request.get('/api/nope');
    expect(reply.status()).toBe(404);
  });
});

test.describe('server-driven web UI', () => {
  test('renders tiles, accounts, and the activity feed from the API', async ({ page }) => {
    await page.goto('/');
    await expect(page).toHaveTitle('Talon Bank');
    await expect(page.locator('h1')).toContainText('Talon Bank');

    const tiles = page.locator('.tile');
    await expect(tiles).toHaveCount(3);
    await expect(tiles.nth(0)).toContainText('$7,980.25');
    await expect(tiles.nth(1)).toContainText('3');

    const rows = page.locator('tbody tr');
    await expect(rows).toHaveCount(3);
    await expect(rows.nth(0)).toContainText('Amelia Chen');
    await expect(rows.nth(0)).toContainText('$2,462.40');
    await expect(rows.nth(2)).toContainText('Priya Sharma');

    const refusal = page.locator('.feed li.refused', { hasText: 'vintage synthesizer' });
    await expect(refusal).toHaveCount(1);
    await expect(refusal).toContainText('Marcus Webb');
    await expect(refusal).toContainText('$9,999.00');
    await expect(page.locator('footer')).toContainText('holds no database capability');
  });

  test('is styled, not a bare document', async ({ page }) => {
    await page.goto('/');
    const bg = await page
      .locator('body')
      .evaluate((el) => getComputedStyle(el).backgroundColor);
    expect(bg).toBe('rgb(11, 18, 32)');
    const money = await page
      .locator('.money')
      .first()
      .evaluate((el) => getComputedStyle(el).color);
    expect(money).toBe('rgb(125, 219, 163)');
    const refusedAmount = await page
      .locator('.refused .amt')
      .first()
      .evaluate((el) => getComputedStyle(el).textDecorationLine);
    expect(refusedAmount).toBe('line-through');
    await page.screenshot({ path: 'dashboard.png', fullPage: true });
  });

  test('mutations through the API appear on the next server-rendered page', async ({
    page,
    request,
  }) => {
    const deposit = await request.post('/api/deposit', {
      data: { account: 3, cents: 25, note: 'browser e2e top-up' },
    });
    expect(deposit.status()).toBe(200);
    await page.goto('/');
    await expect(page.locator('tbody tr').nth(2)).toContainText('$3,600.25');
    await expect(page.locator('.feed li').first()).toContainText('browser e2e top-up');
  });

  test('unknown paths get the 404 page', async ({ page }) => {
    const reply = await page.goto('/nowhere');
    expect(reply.status()).toBe(200);
    await expect(page.locator('h1')).toHaveText('404');
  });
});

// Runs last: it opens an account, mutating the shared server's state, so it
// must not precede the count-sensitive assertions above.
test.describe('typed JSON encoding', () => {
  test('escapes quotes and backslashes so the response stays valid JSON', async ({ request }) => {
    // Without the Json encoder's escaping (domain/json.ospml), an owner name
    // containing a quote produces a malformed body and request.json() throws.
    const tricky = 'Amelia "Mel" Chen \\ Co.';
    const created = await request.post('/api/accounts', { data: { name: tricky } });
    expect(created.status()).toBe(201);
    expect((await created.json()).owner).toBe(tricky);

    const accounts = await (await request.get('/api/accounts')).json();
    expect(accounts.some((a) => a.owner === tricky)).toBe(true);

    // The dashboard re-parses that same JSON, so a broken escape would drop
    // the row; the server-rendered page must still show the tricky name.
    const page = await request.get('/');
    expect(await page.text()).toContain('Amelia "Mel" Chen');
  });
});

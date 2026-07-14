// End-to-end coverage for the complete Talon Bank stack: native Osprey API,
// Osprey WebAssembly application, generic React renderer, and real Chromium.
const { test, expect } = require('@playwright/test');

test.describe.configure({ mode: 'serial' });

const NEWEST_ACTIVITY = {
  id: 7,
  account: 2,
  kind: 'refused',
  cents: 999900,
  note: 'vintage synthesizer',
  owner: 'Marcus Webb',
};

const MALFORMED_MUTATIONS = [
  ['/api/withdraw', { account: 1, cents: -5000 }, 'amount must be positive'],
  ['/api/transfer', { from: 2, to: 999, cents: 5000 }, 'account not found'],
  ['/api/transfer', { from: 2, to: 2, cents: 5000 }, 'accounts must be different'],
  ['/api/accounts', {}, 'owner name required'],
];

async function openApp(page, hash = '') {
  await page.goto(`/${hash}`);
  await expect(page.locator('html')).toHaveAttribute('data-osprey-ready', 'true');
  await expect(page.locator('.loading-page')).toHaveCount(0);
}

async function accountById(request, id) {
  const accounts = await (await request.get('/api/accounts')).json();
  return accounts.find((account) => account.id === id);
}

function collectPageErrors(page) {
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  page.on('console', (message) => {
    if (message.type() === 'error') errors.push(message.text());
  });
  return errors;
}

async function expectPortfolioOverview(page) {
  await expect(page).toHaveTitle('Talon Bank');
  await expect(page.locator('.hero-card')).toContainText('$7,980.25');
  await expect(page.locator('.account-card')).toHaveCount(3);
  await expect(page.locator('.overview-page')).toContainText('Amelia Chen');
  await expect(page.locator('.overview-page')).toContainText('vintage synthesizer');
  await expect(page.locator('.sidebar')).toContainText('Talon Operations');
}

async function expectHealthyBridge(page, errors) {
  const telemetry = await page.evaluate(() => window.__TALON_BRIDGE__);
  expect(telemetry.ready).toBe(true);
  expect(telemetry.renders).toBeGreaterThanOrEqual(3);
  expect(telemetry.events).toBeGreaterThanOrEqual(2);
  expect(telemetry.lastPayloadBytes).toBeGreaterThan(100);
  expect(telemetry.lastDecodeMs).toBeLessThan(250);
  expect(errors).toEqual([]);
}

async function expectActivityDeepLinkAndFilters(page) {
  await openApp(page, '#/activity');
  await expect(page.locator('.activity-page h1')).toHaveText('Activity');
  await page.locator('#activity-search').fill('vintage synthesizer');
  await expect(page.locator('.movement-row')).toHaveCount(1);
  await expect(page.locator('.movement-row')).toContainText('Marcus Webb');
  await page.locator('#activity-search').fill('');
  await page.locator('#filter-refused').click();
  await expect(page.locator('.movement-row')).toHaveCount(2);
  await expect(page.locator('.movement-row').first()).toContainText('API protected refusal');
}

async function expectBrowserHistory(page) {
  await page.locator('#nav-accounts').click();
  await expect(page).toHaveURL(/#\/accounts$/);
  await expect(page.locator('.accounts-page h1')).toHaveText('Client accounts');
  await page.locator('#nav-move').click();
  await expect(page).toHaveURL(/#\/move$/);
  await page.goBack();
  await expect(page.locator('.accounts-page h1')).toHaveText('Client accounts');
}

async function expectSecurityPage(page) {
  await page.locator('#nav-security').click();
  await expect(page.locator('.security-page h1')).toHaveText('Trust you can inspect');
  await expect(page.locator('#architecture')).toContainText('Osprey Web App');
}

async function depositThroughForm(page) {
  await page.locator('#nav-move').click();
  await page.locator('#move-deposit').click();
  await page.locator('#deposit-account').selectOption('1');
  await page.locator('#deposit-amount').fill('12.34');
  await page.locator('#deposit-note').fill('Wasm deposit flow');
  await page.locator('#submit-deposit button[type="submit"]').click();
  await expect(page.locator('.toast.success')).toContainText('Deposit complete');
}

async function transferThroughForm(page) {
  await page.locator('#move-transfer').click();
  await page.locator('#transfer-from').selectOption('1');
  await page.locator('#transfer-to').selectOption('2');
  await page.locator('#transfer-amount').fill('10.00');
  await page.locator('#transfer-note').fill('Atomic browser transfer');
  await page.locator('#submit-transfer button[type="submit"]').click();
  await expect(page.locator('.toast.success')).toContainText('Transfer complete');
}

async function expectAtomicTransfer(request, accountOneBefore, accountTwoBefore) {
  const accountOneAfter = await accountById(request, 1);
  const accountTwoAfter = await accountById(request, 2);
  expect(accountOneAfter.cents).toBe(accountOneBefore.cents + 234);
  expect(accountTwoAfter.cents).toBe(accountTwoBefore.cents + 1000);
  const feed = await (await request.get('/api/activity')).json();
  const entries = feed.filter((entry) => entry.note === 'Atomic browser transfer');
  expect(entries.map((entry) => entry.kind).sort()).toEqual(['credit', 'debit']);
}

async function listsSeededAccounts({ request }) {
  const reply = await request.get('/api/accounts');
  expect(reply.status()).toBe(200);
  expect(await reply.json()).toEqual([
    { id: 1, owner: 'Amelia Chen', cents: 246240, balance: '$2,462.40' },
    { id: 2, owner: 'Marcus Webb', cents: 191785, balance: '$1,917.85' },
    { id: 3, owner: 'Priya Sharma', cents: 360000, balance: '$3,600.00' },
  ]);
}

async function servesActivityJournal({ request }) {
  const reply = await request.get('/api/activity');
  expect(reply.status()).toBe(200);
  const feed = await reply.json();
  expect(feed).toHaveLength(7);
  expect(feed[0]).toEqual(NEWEST_ACTIVITY);
  const transfer = feed.filter((entry) => entry.note === 'dinner split');
  expect(transfer.map((entry) => entry.kind).sort()).toEqual(['credit', 'debit']);
}

async function refusesOverdrafts({ request }) {
  const reply = await request.post('/api/withdraw', {
    data: { account: 3, cents: 99999999, note: 'API protected refusal' },
  });
  expect(reply.status()).toBe(422);
  expect(await reply.json()).toEqual({ error: 'insufficient funds' });
  const feed = await (await request.get('/api/activity')).json();
  expect(feed[0]).toMatchObject({
    account: 3,
    kind: 'refused',
    note: 'API protected refusal',
  });
}

async function rejectsMalformedMutations({ request }) {
  const before = await (await request.get('/api/accounts')).json();
  for (const [path, data, error] of MALFORMED_MUTATIONS) {
    const reply = await request.post(path, { data });
    expect(reply.status()).toBe(422);
    expect(await reply.json()).toEqual({ error });
  }
  expect(await (await request.get('/api/accounts')).json()).toEqual(before);
}

async function servesAssetsAnd404s({ request }) {
  const script = await request.get('/app.js');
  expect(script.status()).toBe(200);
  expect(script.headers()['content-type']).toContain('application/javascript');
  expect((await script.body()).byteLength).toBeGreaterThan(10_000);
  const styles = await request.get('/app.css');
  expect(styles.status()).toBe(200);
  expect(styles.headers()['content-type']).toContain('text/css');
  expect((await styles.body()).byteLength).toBeGreaterThan(5_000);
  expect((await request.get('/api/nope')).status()).toBe(404);
  expect((await request.get('/nowhere')).status()).toBe(404);
}

async function bootsPortfolioOverview({ page }) {
  const errors = collectPageErrors(page);
  await openApp(page);
  await expectPortfolioOverview(page);
  await expectHealthyBridge(page, errors);
}

async function supportsNavigation({ page }) {
  await expectActivityDeepLinkAndFilters(page);
  await expectBrowserHistory(page);
  await expectSecurityPage(page);
}

async function opensAccountModal({ page, request }) {
  await openApp(page);
  await page.locator('.top-open').click();
  await expect(page.getByRole('dialog')).toBeVisible();
  await expect(page.locator('#new-owner')).toBeFocused();
  await page.locator('#new-owner').fill('Nora Okafor');
  await page.locator('#submit-open-account button[type="submit"]').click();
  await expect(page.locator('.toast.success')).toContainText('Account opened');
  const accounts = await (await request.get('/api/accounts')).json();
  const created = accounts.find((account) => account.owner === 'Nora Okafor');
  expect(created).toMatchObject({ cents: 0, balance: '$0.00' });
  await page.locator('#nav-accounts').click();
  await expect(page.locator(`#account-${created.id}`)).toContainText('Nora Okafor');
}

async function movesFundsAtomically({ page, request }) {
  await openApp(page);
  const accountOneBefore = await accountById(request, 1);
  const accountTwoBefore = await accountById(request, 2);
  await depositThroughForm(page);
  await transferThroughForm(page);
  await expectAtomicTransfer(request, accountOneBefore, accountTwoBefore);
}

async function showsDomainRefusals({ page }) {
  await openApp(page);
  await page.locator('#nav-move').click();
  await page.locator('#move-withdraw').click();
  await page.locator('#withdraw-account').selectOption('3');
  await page.locator('#withdraw-amount').fill('999999.00');
  await page.locator('#withdraw-note').fill('Wasm protected refusal');
  await page.locator('#submit-withdraw button[type="submit"]').click();
  await expect(page.locator('.toast.error')).toContainText('Operation refused');
  await expect(page.locator('.toast.error')).toContainText('insufficient funds');
  await page.locator('#nav-activity').click();
  await page.locator('#filter-refused').click();
  await expect(page.locator('.activity-page')).toContainText('Wasm protected refusal');
}

async function rendersHostileTextSafely({ page, request }) {
  const hostile = '<img src=x onerror="window.__talonXss=1"> \\ Co.';
  const created = await request.post('/api/accounts', { data: { name: hostile } });
  expect(created.status()).toBe(201);
  const account = await created.json();
  expect(account.owner).toBe(hostile);
  await openApp(page, '#/accounts');
  await expect(page.locator(`#account-${account.id}`)).toContainText(hostile);
  await expect(page.locator('img[src="x"]')).toHaveCount(0);
  expect(await page.evaluate(() => window.__talonXss)).toBeUndefined();
}

async function supportsMobileLayout({ page }) {
  await page.setViewportSize({ width: 390, height: 844 });
  await openApp(page);
  const dimensions = await page.evaluate(() => ({
    viewport: document.documentElement.clientWidth,
    content: document.documentElement.scrollWidth,
  }));
  expect(dimensions.content).toBeLessThanOrEqual(dimensions.viewport);
  await page.locator('#toggle-menu').click();
  await expect(page.locator('.sidebar')).toHaveClass(/open/);
  await page.locator('#nav-activity').click();
  await expect(page.locator('.activity-page h1')).toHaveText('Activity');
  await expect(page.locator('.sidebar')).not.toHaveClass(/open/);
}

async function rendersStyled404({ page }) {
  const reply = await page.goto('/nowhere');
  expect(reply.status()).toBe(404);
  await expect(page.locator('h1')).toHaveText('That route has flown the coop.');
  await expect(page.locator('a')).toHaveAttribute('href', '/');
}

function protectedJsonApiSuite() {
  test('lists seeded accounts with exact machine and display money', listsSeededAccounts);
  test('serves a newest-first, double-entry activity journal', servesActivityJournal);
  test('refuses overdrafts and journals the attempt', refusesOverdrafts);
  test('rejects malformed mutations without changing balances', rejectsMalformedMutations);
  test('serves real app assets and proper 404 statuses', servesAssetsAnd404s);
}

function ospreyWebAssemblySuite() {
  test('boots cleanly and renders the complete portfolio overview', bootsPortfolioOverview);
  test('supports deep links, filtering, and browser history', supportsNavigation);
  test('opens an account through an accessible focused modal', opensAccountModal);
  test('deposits and atomically transfers funds through Osprey forms', movesFundsAtomically);
  test('shows domain refusals and refreshes the audit view', showsDomainRefusals);
  test('renders hostile account text without creating executable DOM', rendersHostileTextSafely);
  test('collapses to a usable mobile app without horizontal overflow', supportsMobileLayout);
  test('renders a styled 404 document for unknown native paths', rendersStyled404);
}

test.describe('protected JSON API', protectedJsonApiSuite);
test.describe('Osprey WebAssembly application', ospreyWebAssemblySuite);

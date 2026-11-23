// Talon Bank end to end: the JSON API, then the one Osprey Native app in real Chromium, drawn by
// Osprey's web host from the WebAssembly build of the same app Android and iOS run. Every control
// is found by its accessible role and name, as a person with a screen reader would find it.
const { test, expect } = require('@playwright/test');
const { API } = require('../web-client');

test.describe.configure({ mode: 'serial' });

const NEWEST_ACTIVITY = {
  id: 7, account: 2, kind: 'refused', cents: 999900,
  note: 'vintage synthesizer', owner: 'Marcus Webb',
};

const MALFORMED_MUTATIONS = [
  ['/api/withdraw', { account: 1, cents: -5000 }, 'amount must be positive'],
  ['/api/transfer', { from: 2, to: 999, cents: 5000 }, 'account not found'],
  ['/api/transfer', { from: 2, to: 2, cents: 5000 }, 'accounts must be different'],
  ['/api/accounts', {}, 'owner name required'],
];

const INVALID_AMOUNTS = ['-0.50', '1..2', '1.001', '1.00evil', '12,34', '$'];

async function accountById(request, id) {
  const accounts = await (await request.get(API + '/api/accounts')).json();
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

async function openApp(page) {
  await page.goto('/');
  await expect(page).toHaveTitle('Talon Bank');
  await expectCurrentPage(page, 'Overview');
  await expect(page.getByText('Contacting the ledger')).toHaveCount(0);
}

function navigation(page, name) {
  return page.getByRole('button', { name, exact: true });
}

async function expectCurrentPage(page, name) {
  await expect(page.getByRole('heading', { level: 1 })).toHaveText(name === 'Move' ? 'Move money' : name);
  await expect(navigation(page, name).locator('..')).toHaveCSS('background-color', 'rgb(255, 107, 74)');
}

async function goToPage(page, name) {
  await navigation(page, name).click();
  await expectCurrentPage(page, name);
}

async function noticed(page, words) {
  const message = page.getByText(words, { exact: true });
  const dismiss = page.getByRole('button', { name: 'Dismiss', exact: true });
  await expect(message).toBeVisible();
  await dismiss.click();
  await expect(dismiss).toHaveCount(0);
  await expect(message).toHaveCount(0);
}

async function moveMoney(page, mode, fields) {
  await goToPage(page, 'Move');
  await page.getByRole('button', { name: mode, exact: true }).click();
  for (const [label, value] of fields.choose ?? []) await page.getByRole('combobox', { name: label }).selectOption(value);
  await page.getByRole('textbox', { name: 'Amount' }).fill(fields.amount);
  await page.getByRole('textbox', { name: 'Description' }).fill(fields.note ?? '');
  await page.getByRole('button', { name: fields.confirm }).click();
}

async function listsSeededAccounts({ request }) {
  const reply = await request.get(API + '/api/accounts');
  expect(reply.status()).toBe(200);
  expect(await reply.json()).toEqual([
    { id: 1, owner: 'Amelia Chen', cents: 246240, balance: '$2,462.40' },
    { id: 2, owner: 'Marcus Webb', cents: 191785, balance: '$1,917.85' },
    { id: 3, owner: 'Priya Sharma', cents: 360000, balance: '$3,600.00' },
  ]);
}

async function servesActivityJournal({ request }) {
  const reply = await request.get(API + '/api/activity');
  expect(reply.status()).toBe(200);
  const feed = await reply.json();
  expect(feed).toHaveLength(7);
  expect(feed[0]).toEqual(NEWEST_ACTIVITY);
  const transfer = feed.filter((entry) => entry.note === 'dinner split');
  expect(transfer.map((entry) => entry.kind).sort()).toEqual(['credit', 'debit']);
}

async function rejectsMalformedMutations({ request }) {
  const before = await (await request.get(API + '/api/accounts')).json();
  for (const [path, data, error] of MALFORMED_MUTATIONS) {
    const reply = await request.post(API + path, { data });
    expect(reply.status()).toBe(422);
    expect(await reply.json()).toEqual({ error });
  }
  expect(await (await request.get(API + '/api/accounts')).json()).toEqual(before);
}

async function servesHostAnd404s({ request }) {
  const page = await request.get('/');
  expect(page.headers()['content-type']).toContain('text/html');
  expect(await page.text()).toContain('src="host.js"');
  const script = await request.get('/host.js');
  expect(script.headers()['content-type']).toContain('application/javascript');
  expect((await script.body()).byteLength).toBeGreaterThan(100_000);
  const styles = await request.get('/host.css');
  expect(styles.headers()['content-type']).toContain('text/css');
  for (const path of ['/app.js', '/nowhere', '/api/accounts', '/../build.mjs']) expect((await request.get(path)).status()).toBe(404);
  // The server is the API alone: it never serves the web client, and lets the client's origin read it.
  for (const path of ['/', '/host.js', '/nowhere', '/api/nowhere']) expect((await request.get(API + path)).status()).toBe(404);
  const listing = await request.get(API + '/api/accounts');
  expect(listing.headers()['access-control-allow-origin']).toBe('*');
  const preflight = await request.fetch(API + '/api/deposit', { method: 'OPTIONS', headers: { Origin: 'http://127.0.0.1:18791', 'Access-Control-Request-Method': 'POST' } });
  expect(preflight.status()).toBe(204);
  expect(preflight.headers()['access-control-allow-methods']).toContain('POST');
  expect(preflight.headers()['access-control-allow-headers']).toContain('Content-Type');
}

async function drawsTheOverview({ page }) {
  const errors = collectPageErrors(page);
  await openApp(page);
  await expect(page.getByText('$7,980.25')).toBeVisible();
  await expect(page.getByText('Across 3 protected client accounts')).toBeVisible();
  for (const owner of ['Amelia Chen', 'Marcus Webb', 'Priya Sharma']) await expect(page.getByText(owner).first()).toBeVisible();
  await expect(page.getByText('vintage synthesizer')).toBeVisible();
  await expect(page.getByText('−$1,850.00')).toBeVisible();
  await expect(page.getByText('Osprey', { exact: true })).toHaveCount(0);
  expect(errors).toEqual([]);
}

async function visitsEveryPage({ page }) {
  await openApp(page);
  await expect(page.getByRole('button', { name: /^(Overview|Accounts|Move|Activity|Security)$/ })).toHaveCount(5);
  await goToPage(page, 'Accounts');
  await expect(page.getByText('Ready to move')).toBeVisible();
  await goToPage(page, 'Activity');
  await expect(page.getByRole('textbox', { name: 'Search' })).toBeVisible();
  await goToPage(page, 'Security');
  await expect(page.getByText('Storage isolation')).toBeVisible();
  await goToPage(page, 'Move');
  await expect(page.getByRole('button', { name: 'Confirm transfer' })).toBeVisible();
  await goToPage(page, 'Overview');
  await page.getByRole('button', { name: 'Full history' }).click();
  await expectCurrentPage(page, 'Activity');
}

async function filtersAndSearchesActivity({ page }) {
  await openApp(page);
  await goToPage(page, 'Activity');
  await page.getByRole('button', { name: /^Declined \d+$/ }).click();
  await expect(page.getByText('vintage synthesizer')).toBeVisible();
  await expect(page.getByText('rent — 12 Fern St')).toHaveCount(0);
  await page.getByRole('button', { name: /^All \d+$/ }).click();
  await page.getByRole('textbox', { name: 'Search' }).fill('PRIYA');
  await expect(page.getByText('research grant Q1')).toBeVisible();
  await expect(page.getByText('dinner split')).toHaveCount(0);
  await page.getByRole('textbox', { name: 'Search' }).fill('nobody at all');
  await expect(page.getByText('No matching activity')).toBeVisible();
}

async function depositsAndTransfersAtomically({ page, request }) {
  const one = await accountById(request, 1);
  const two = await accountById(request, 2);
  await openApp(page);
  await moveMoney(page, 'Deposit', { choose: [['Deposit to', '1']], amount: '12.34', note: 'Native deposit flow', confirm: 'Confirm deposit' });
  await noticed(page, 'Deposit complete');
  await moveMoney(page, 'Transfer', { choose: [['From account', '1'], ['To account', '2']], amount: '$10.00', note: 'Atomic native transfer', confirm: 'Confirm transfer' });
  await noticed(page, 'Transfer complete');
  expect((await accountById(request, 1)).cents).toBe(one.cents + 234);
  expect((await accountById(request, 2)).cents).toBe(two.cents + 1000);
  const feed = await (await request.get(API + '/api/activity')).json();
  expect(feed.filter((entry) => entry.note === 'Atomic native transfer').map((entry) => entry.kind).sort()).toEqual(['credit', 'debit']);
}

async function showsLedgerRefusals({ page, request }) {
  await openApp(page);
  await moveMoney(page, 'Withdraw', { choose: [['Withdraw from', '3']], amount: '99,999.99', note: 'Native protected refusal', confirm: 'Confirm withdrawal' });
  await expect(page.getByText('Operation refused', { exact: true })).toBeVisible();
  await noticed(page, 'insufficient funds');
  const feed = await (await request.get(API + '/api/activity')).json();
  expect(feed[0]).toMatchObject({ account: 3, kind: 'refused', cents: 9999999, note: 'Native protected refusal' });
}

async function refusesMalformedMoney({ page, request }) {
  const before = await (await request.get(API + '/api/accounts')).json();
  await openApp(page);
  for (const amount of INVALID_AMOUNTS) {
    await moveMoney(page, 'Deposit', { amount, confirm: 'Confirm deposit' });
    await noticed(page, 'Check the amount');
  }
  await moveMoney(page, 'Transfer', { choose: [['From account', '2'], ['To account', '2']], amount: '5', confirm: 'Confirm transfer' });
  await noticed(page, 'Choose two accounts');
  expect(await (await request.get(API + '/api/accounts')).json()).toEqual(before);
}

async function opensAnAccount({ page, request }) {
  await openApp(page);
  await page.getByRole('button', { name: 'Open account' }).click();
  await page.getByRole('button', { name: 'Create account' }).click();
  await noticed(page, 'Owner name required');
  await page.getByRole('textbox', { name: 'Account owner' }).fill('<img src=x onerror=alert(1)> Ada');
  await page.getByRole('button', { name: 'Create account' }).click();
  await noticed(page, 'Account opened');
  const accounts = await (await request.get(API + '/api/accounts')).json();
  expect(accounts.at(-1).owner).toBe('<img src=x onerror=alert(1)> Ada');
  await goToPage(page, 'Accounts');
  await expect(page.getByText('<img src=x onerror=alert(1)> Ada').first()).toBeVisible();
  await expect(page.locator('#osprey-root img[src="x"]')).toHaveCount(0);
}

async function fitsAPhone({ page }) {
  await page.setViewportSize({ width: 390, height: 844 });
  await openApp(page);
  await goToPage(page, 'Activity');
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
  expect(overflow).toBeLessThanOrEqual(0);
}

test.describe('Talon Bank JSON API', () => {
  test('lists seeded accounts with exact machine and display money', listsSeededAccounts);
  test('serves a newest-first, double-entry activity journal', servesActivityJournal);
  test('rejects malformed mutations without changing balances', rejectsMalformedMutations);
  test('serves the web client apart from the API, with CORS and proper 404 statuses', servesHostAnd404s);
});

test.describe('Talon Bank as an Osprey Native app in the browser', () => {
  test('draws the portfolio overview with no errors and no host branding', drawsTheOverview);
  test('moves between all five pages', visitsEveryPage);
  test('filters and searches the activity journal', filtersAndSearchesActivity);
  test('deposits and atomically transfers funds', depositsAndTransfersAtomically);
  test('reports a refused overdraft with the ledger\'s reason', showsLedgerRefusals);
  test('refuses malformed money before it reaches the ledger', refusesMalformedMoney);
  test('opens an account and shows hostile text as text', opensAnAccount);
  test('fits a phone screen without horizontal overflow', fitsAPhone);
});

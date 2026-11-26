// The web client is static files (make _bank-web builds them) served apart from the Talon server
// they call; both Playwright configs serve them the same way. Each config gets fresh loopback ports.
const { execFileSync } = require('node:child_process');
const { mkdtempSync, rmSync } = require('node:fs');
const { tmpdir } = require('node:os');
const path = require('node:path');

function freePort() {
  const source = 'const s = require("node:net").createServer(); s.listen(0, "127.0.0.1", () => { console.log(s.address().port); s.close(); });';
  return Number(execFileSync(process.execPath, ['-e', source], { encoding: 'utf8' }).trim());
}

const API_PORT = Number(process.env.TALON_API_PORT) || freePort();
let WEB_PORT = Number(process.env.TALON_WEB_PORT) || freePort();
while (WEB_PORT === API_PORT) WEB_PORT = freePort();
const OWNS_RUN_DIR = !process.env.TALON_RUN_DIR;
const RUN_DIR = process.env.TALON_RUN_DIR || mkdtempSync(path.join(tmpdir(), 'talon-bank-e2e-'));
const HOLD = process.env.TALON_HOLD || path.join(RUN_DIR, 'hold');
const DB = process.env.TALON_DB || path.join(RUN_DIR, 'ledger.db');
Object.assign(process.env, { TALON_API_PORT: String(API_PORT), TALON_WEB_PORT: String(WEB_PORT), TALON_RUN_DIR: RUN_DIR, TALON_HOLD: HOLD, TALON_DB: DB });
if (OWNS_RUN_DIR) process.once('exit', () => rmSync(RUN_DIR, { recursive: true, force: true }));
const WEB = `http://127.0.0.1:${WEB_PORT}`;
const API = `http://127.0.0.1:${API_PORT}`;

// The static build's standalone default remains port 18790. Override its launch
// argument before the host reads it, without rebuilding or modifying shared output.
const useApi = (page) => page.addInitScript((api) => {
  const apply = () => { document.documentElement.dataset.ospreyArgument = api; };
  if (document.documentElement) apply();
  else document.addEventListener('readystatechange', apply, { once: true });
}, API);

const webClient = {
  command: `node ../../../../toolchain/hosts/web/scripts/serve.mjs --root ../../../../toolchain/hosts/web/build/talon --port ${WEB_PORT}`,
  cwd: __dirname,
  url: `${WEB}/`,
  reuseExistingServer: false,
  timeout: 30_000,
};

module.exports = { WEB, API, HOLD, DB, RUN_DIR, useApi, webClient };

// The web client is static files (make _bank-web builds them) served apart from the Talon server
// they call; both Playwright configs serve them the same way.
const WEB = 'http://127.0.0.1:18791';
const API = 'http://127.0.0.1:18790';

const webClient = {
  command: 'node ../../../../toolchain/hosts/web/scripts/serve.mjs --root ../../../../toolchain/hosts/web/build/talon --port 18791',
  cwd: __dirname,
  url: `${WEB}/`,
  reuseExistingServer: false,
  timeout: 30_000,
};

module.exports = { WEB, API, webClient };

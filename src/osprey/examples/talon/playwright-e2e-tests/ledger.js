// A fresh Talon server for one journey: the browser, SwiftUI, iOS and Android journeys each start
// from the same seeded ledger, so every host's goldens show the same balances.
//
//   node playwright-e2e-tests/ledger.js COMMAND [ARGS...]   runs COMMAND while a fresh server answers, then stops it
//
// The server serves while its per-run marker exists and stops cleanly once it is removed.
// Native host journeys retain their default API port; Playwright selects one in web-client.js.
if (require.main === module && process.argv[2] !== '--serve') process.env.TALON_API_PORT ||= '18790';
const { spawn, spawnSync } = require('node:child_process');
const { mkdirSync, rmSync, writeFileSync } = require('node:fs');
const path = require('node:path');
const { API, HOLD, RUN_DIR } = require('./web-client');

const BACKEND = path.join(__dirname, '..', 'backend');
const OSPREY = path.join(BACKEND, '..', '..', '..', '..', '..', 'target', 'release', 'osprey');
const READY = `${API}/api/accounts`;
const PATIENCE_MS = 120_000;

async function answering() {
  try {
    return (await fetch(READY)).ok;
  } catch {
    return false;
  }
}

function launch() {
  let output = '';
  const child = spawn(OSPREY, [BACKEND, '--run'], { stdio: ['ignore', 'pipe', 'pipe'] });
  for (const stream of [child.stdout, child.stderr]) stream.on('data', (chunk) => { output += chunk; });
  const exited = new Promise((resolve) => child.once('exit', resolve));
  return { child, exited, output: () => output };
}

/** Starts a fresh server and resolves to a function that stops it. */
async function start() {
  if (await answering()) throw new Error(`A Talon server already answers on ${API}; stop it so the journey starts fresh`);
  mkdirSync(RUN_DIR, { recursive: true });
  writeFileSync(HOLD, '');
  const running = launch();
  let stopping = false;
  // A server that dies mid-journey is otherwise silent: the browser only sees refused requests.
  running.exited.then((code) => {
    if (!stopping) console.error(`FAIL: the Talon server exited (${code ?? running.child.signalCode}) while serving:\n${running.output()}`);
  });
  const stop = async () => {
    stopping = true;
    rmSync(HOLD, { force: true });
    await Promise.race([running.exited, new Promise((resolve) => setTimeout(resolve, 10_000))]);
    if (running.child.exitCode === null && running.child.signalCode === null) running.child.kill('SIGTERM');
  };
  const deadline = Date.now() + PATIENCE_MS;
  while (!(await answering())) {
    if (running.child.exitCode !== null || running.child.signalCode !== null || Date.now() > deadline) {
      await stop();
      throw new Error(`The Talon server never answered: ${running.output()}`);
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  return stop;
}

async function main([command, ...args]) {
  if (!command) throw new Error('usage: node playwright-e2e-tests/ledger.js COMMAND [ARGS...]');
  const stop = await start();
  try {
    return spawnSync(command, args, { stdio: 'inherit' }).status ?? 1;
  } finally {
    await stop();
  }
}

async function serve() {
  const stop = await start();
  const shutdown = () => stop().catch((problem) => {
    console.error(`FAIL: ${problem.message}`);
    process.exitCode = 1;
  });
  process.once('SIGTERM', shutdown);
  process.once('SIGINT', shutdown);
}

if (require.main === module) {
  const failed = (problem) => {
    console.error(`FAIL: ${problem.message}`);
    process.exit(1);
  };
  if (process.argv[2] === '--serve') serve().catch(failed);
  else main(process.argv.slice(2)).then((status) => process.exit(status), failed);
}

module.exports = { start };

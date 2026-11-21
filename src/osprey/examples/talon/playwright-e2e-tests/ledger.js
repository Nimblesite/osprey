// A fresh Talon server for one journey: the browser, SwiftUI, iOS and Android journeys each start
// from the same seeded ledger, so every host's goldens show the same balances.
//
//   node playwright-e2e-tests/ledger.js COMMAND [ARGS...]   runs COMMAND while a fresh server answers, then stops it
//
// The server serves while /tmp/talon_bank.hold exists and stops cleanly once it is removed.
const { spawn, spawnSync } = require('node:child_process');
const { rmSync, writeFileSync } = require('node:fs');
const path = require('node:path');

const BACKEND = path.join(__dirname, '..', 'backend');
const OSPREY = path.join(BACKEND, '..', '..', '..', '..', '..', 'target', 'release', 'osprey');
const HOLD = '/tmp/talon_bank.hold';
const READY = 'http://127.0.0.1:18790/api/accounts';
const PATIENCE_MS = 120_000;

async function answering() {
  try {
    return (await fetch(READY)).ok;
  } catch {
    return false;
  }
}

/** Starts a fresh server and resolves to a function that stops it. */
async function start() {
  if (await answering()) throw new Error('A Talon server already answers on 18790; stop it so the journey starts fresh');
  writeFileSync(HOLD, '');
  const server = spawn(OSPREY, [BACKEND, '--run'], { stdio: 'ignore' });
  const exited = new Promise((resolve) => server.once('exit', resolve));
  const stop = async () => {
    rmSync(HOLD, { force: true });
    await Promise.race([exited, new Promise((resolve) => setTimeout(resolve, 10_000))]);
    if (server.exitCode === null) server.kill('SIGTERM');
  };
  const deadline = Date.now() + PATIENCE_MS;
  while (!(await answering())) {
    if (server.exitCode !== null || Date.now() > deadline) {
      await stop();
      throw new Error('The Talon server never answered');
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

if (require.main === module) {
  main(process.argv.slice(2)).then((status) => process.exit(status), (problem) => {
    console.error(`FAIL: ${problem.message}`);
    process.exit(1);
  });
}

module.exports = { start };

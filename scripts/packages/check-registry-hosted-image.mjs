import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { request } from 'node:http';
import { setTimeout as delay } from 'node:timers/promises';

const image = process.argv[2];
assert.ok(image, 'Pass the locally built registry image');
const fixture = `registry-image-check-${randomUUID()}`;
const edge = 'fixture-edge-secret-for-container-smoke';
const host = 'api.example.test';

function docker(args, input) {
  const result = spawnSync('docker', args, { input, encoding: 'utf8', timeout: 30000 });
  assert.equal(result.status, 0, result.stderr || result.error?.message || `docker ${args[0]} failed`);
  return result.stdout.trim();
}

function mounted(target, readOnly = true) {
  return ['--mount', `type=volume,source=${fixture},target=${target}${readOnly ? ',readonly' : ''}`];
}

function port(container, number) {
  const mapping = docker(['port', container, `${number}/tcp`]);
  const match = /^127\.0\.0\.1:(\d+)$/.exec(mapping);
  assert.ok(match, `Expected a loopback mapping for ${number}`);
  return Number(match[1]);
}

function get(portNumber, path, headers = {}) {
  return new Promise((resolve, reject) => {
    const http = request({ hostname: '127.0.0.1', port: portNumber, path, headers, timeout: 2000 }, response => {
      response.resume();
      response.once('error', reject);
      response.once('end', () => resolve(response.statusCode));
    });
    http.on('error', reject);
    http.on('timeout', () => http.destroy(Error('Registry image request timed out')));
    http.end();
  });
}

async function ready(container, management) {
  for (let attempt = 0; attempt < 40; attempt++) {
    if (await get(management, '/v1/ready').catch(() => 0) === 200) return;
    if (docker(['inspect', '--format', '{{.State.Running}}', container]) !== 'true') break;
    await delay(250);
  }
  assert.fail('The registry management port never became ready');
}

function prepareMount() {
  const command = [
    'cat > /fixture/runtime.json',
    'cp /etc/ssl/certs/ca-certificates.crt /fixture/ca.pem',
    `printf '%s\n' '[catalog]' 'host=127.0.0.1' 'port=1' 'dbname=fixture' 'user=registry_catalog_reader' > /fixture/pg_service.conf`,
    'chmod 0444 /fixture/runtime.json /fixture/ca.pem /fixture/pg_service.conf',
  ].join(' && ');
  const config = JSON.stringify({ apiHost: host, websiteOrigin: 'https://packages.example.test', edgeSecret: edge });
  docker(['run', '--rm', '-i', '--user', '0:0', ...mounted('/fixture', false), '--entrypoint', 'sh', image, '-ec', command], config);
}

assert.equal(docker(['image', 'inspect', '--format', '{{.Os}}/{{.Architecture}}', image]), 'linux/amd64');
assert.match(docker(['image', 'inspect', '--format', '{{ index .Config.Labels "org.opencontainers.image.revision" }}', image]), /^[a-f0-9]{40}$/);
const dependencies = docker(['run', '--rm', '--entrypoint', 'sh', image, '-ec', 'id -u; psql --version; openssl version']);
assert.match(dependencies, /^65532\npsql \(PostgreSQL\) 17\..*\nOpenSSL 3\./);
docker(['volume', 'create', fixture]);
let container;
try {
  prepareMount();
  container = docker(['run', '-d', '--read-only', '--tmpfs', '/tmp:nosuid,nodev',
    '-p', '127.0.0.1::8080', '-p', '127.0.0.1::8081', ...mounted('/run/registry'),
    ...mounted('/run/catalog-ca'), ...mounted('/run/catalog-service'), ...mounted('/run/registry-source'), image]);
  const ingress = port(container, 8080);
  const management = port(container, 8081);
  const trusted = { host, 'X-Edge-Secret': edge };
  await ready(container, management);
  assert.equal(await get(management, '/v1/packages', trusted), 404);
  assert.equal(await get(ingress, '/v1/ready'), 403);
  assert.equal(await get(ingress, '/v1/ready', trusted), 404);
  assert.equal(await get(ingress, '/v1/auth/session', trusted), 401);
  assert.equal(await get(ingress, '/v1/packages', trusted), 503);
  assert.equal(docker(['exec', container, 'sh', '-ec', 'find /tmp/.osprey/scratch -type f -print']), '');
  assert.ok(!docker(['logs', container]).includes(edge));
  console.log('Cloud Run image smoke passed for the non-root process, mounted configuration and isolated listener ports.');
} finally {
  try {
    if (container) {
      docker(['stop', '--time', '2', container]);
      docker(['container', 'rm', container]);
    }
  } finally {
    docker(['volume', 'rm', fixture]);
  }
}

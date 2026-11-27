// Feed an anonymous API response to the registry's WASI renderer.
const binding = document.querySelector('script[data-catalog]');
const detail = document.getElementById('package-details');
const results = detail ?? document.getElementById('results');
const status = document.getElementById('catalog-status');
const field = document.querySelector('input[type=search]');
if (field) field.value = new URLSearchParams(location.search).get('q') ?? '';
try {
  const options = {credentials: 'omit', redirect: 'error', cache: 'no-store', signal: AbortSignal.timeout(10000)};
  const endpoint = new URL(binding.dataset.catalog);
  if (detail && (location.pathname.startsWith('/packages/') || /^\/publishers\/[^/]+/u.test(location.pathname))) {
    endpoint.pathname = '/v1' + location.pathname.replace(/\/$/u, '');
    if (location.pathname.startsWith('/publishers/')) endpoint.pathname += '/releases';
  }
  const [api, guest, {runModule}] = await Promise.all([
    fetch(endpoint, options), fetch(binding.dataset.guest), import(binding.dataset.host),
  ]);
  if (detail && api.status === 404) throw new Error(status.dataset.missing);
  if (!api.ok || !guest.ok) throw Error('Catalog unavailable');
  const source = JSON.stringify(await api.json());
  const lines = ['hosted-results', location.pathname, (field?.value ?? '').replace(/[\r\n]/gu, ' '), binding.dataset.content, source, ''];
  let html = '';
  await runModule(await guest.arrayBuffer(), part => { html += part; }, lines.join('\n'));
  if (!html.trim()) throw Error('Invalid projection');
  results.innerHTML = html;
  if (location.hash) document.getElementById(location.hash.slice(1))?.scrollIntoView();
  status.textContent = '';
} catch (error) {
  results.replaceChildren();
  status.textContent = detail && error.message === status.dataset.missing ? status.dataset.missing : status.dataset.error;
}

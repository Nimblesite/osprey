// Feed an anonymous API response to the registry's WASI renderer.
const binding = document.querySelector('script[data-catalog]');
const results = document.getElementById('results');
const status = document.getElementById('catalog-status');
const field = document.querySelector('input[type=search]');
field.value = new URLSearchParams(location.search).get('q') ?? '';
try {
  const options = {credentials: 'omit', redirect: 'error', cache: 'no-store', signal: AbortSignal.timeout(10000)};
  const [api, guest, {runModule}] = await Promise.all([
    fetch(binding.dataset.catalog, options), fetch(binding.dataset.guest), import(binding.dataset.host),
  ]);
  if (!api.ok || !guest.ok) throw Error('Catalog unavailable');
  const source = JSON.stringify(await api.json());
  const lines = ['hosted-results', location.pathname, field.value.replace(/[\r\n]/gu, ' '), source, ''];
  let html = '';
  await runModule(await guest.arrayBuffer(), part => { html += part; }, lines.join('\n'));
  if (!html.trim()) throw Error('Invalid projection');
  results.innerHTML = html;
  status.textContent = '';
} catch {
  results.replaceChildren();
  status.textContent = status.dataset.error;
}

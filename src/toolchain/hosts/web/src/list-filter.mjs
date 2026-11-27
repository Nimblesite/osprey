// A WASI filter receives a query and one line per item, then returns one visibility bit per item.
const binding = document.querySelector('script[data-list-filter]');
const list = document.getElementById(binding.dataset.listFilter);
const field = document.querySelector('input[type=search]');
const items = [...list.children];
const message = document.getElementById(binding.dataset.status);
field.value = new URLSearchParams(location.search).get(field.name) ?? '';
const line = text => text.replace(/\s+/gu, ' ').trim();
try {
  if (field.value.trim()) {
    const { runModule } = await import(binding.dataset.host);
    const response = await fetch(binding.dataset.guest);
    if (!response.ok) throw Error('Guest unavailable');
    const lines = ['filter', line(field.value), String(items.length), ...items.map(item => line(item.innerText))];
    let output = '';
    await runModule(await response.arrayBuffer(), part => { output += part; }, lines.join('\n') + '\n');
    const mask = output.trim();
    if (!/^[01]*$/.test(mask) || mask.length !== items.length) throw Error('Invalid guest output');
    items.forEach((item, i) => { item.hidden = mask[i] === '0'; });
  }
  if (items.every(item => item.hidden)) message.textContent = message.dataset.empty;
} catch {
  list.replaceChildren();
  message.textContent = message.dataset.error;
}

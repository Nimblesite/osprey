// Filter the already rendered local catalog. Native GET preserves shareable searches and Back.
const field = document.querySelector('input[type=search]');
field.value = new URLSearchParams(location.search).get('q') || '';
const entries = Array.from(document.querySelector('#results').children);
const terms = field.value.toLowerCase().trim().split(/\s+/);
for (const entry of entries) {
  const text = entry.textContent.toLowerCase();
  entry.hidden = !terms.every(term => text.includes(term));
}
if (entries.every(entry => entry.hidden)) {
  const message = document.createElement('p');
  message.textContent = entries.length ? 'No matching packages.' : 'This registry has no packages.';
  document.querySelector('#results').append(message);
}

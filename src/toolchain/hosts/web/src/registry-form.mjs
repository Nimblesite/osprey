// Populate a form from a JSON collection and review its values before authorization.
const form = document.querySelector('[data-collection-form]');
const select = form.querySelector('select');
const review = document.getElementById(form.dataset.review);
const message = form.querySelector('[role=status]');
try {
  const response = await fetch(form.dataset.collection, {credentials: 'omit', signal: AbortSignal.timeout(10000)});
  if (!response.ok) throw Error('Collection unavailable');
  const items = (await response.json())[select.dataset.collectionKey];
  if (!Array.isArray(items)) throw Error('Invalid collection');
  for (const item of items) {
    const name = item[select.dataset.valueKey];
    if (typeof name !== 'string') throw Error('Invalid entry');
    select.add(new Option(name, name));
  }
  if (items.length) {
    form.querySelector('fieldset').disabled = false;
    message.textContent = '';
  } else message.textContent = form.dataset.empty;
} catch {
  message.textContent = form.dataset.error;
}
form.addEventListener('submit', event => {
  event.preventDefault();
  for (const output of review.querySelectorAll('[data-field]')) {
    output.textContent = form.elements.namedItem(output.dataset.field).value;
  }
  review.hidden = false;
  review.focus();
});
form.addEventListener('input', () => { review.hidden = true; });

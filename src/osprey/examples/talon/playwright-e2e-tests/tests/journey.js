// Implements [UI-MACHINE-SELF-TEST] in the browser: the steps of an app's self-test.json, the same
// file the SwiftUI, iOS and Android hosts play, acted out in real Chromium. Every step finds what
// it acts on by accessible role and name, as a screen reader would; a `shot` compares the page
// with its committed golden.
const { readFileSync } = require('node:fs');
const { expect, test } = require('@playwright/test');

/** The steps of `file` as { verb, words }, refusing anything but one verb a step. */
function steps(file) {
  const script = JSON.parse(readFileSync(file, 'utf8'));
  return script.steps.map((step) => {
    const entries = Object.entries(step);
    if (entries.length !== 1) throw new Error(`A step is one verb: ${JSON.stringify(step)}`);
    const [verb, value] = entries[0];
    return { verb, words: Array.isArray(value) ? value : [value] };
  });
}

function shown(page, words) {
  return page.getByText(words).filter({ visible: true });
}

/** What a person presses to invoke `words`: a button or tab of that name, or a notice that says it. */
function invocable(page, words) {
  return page.getByRole('button', { name: words, exact: true })
    .or(page.getByRole('tab', { name: words, exact: true }))
    .or(page.getByRole('alert').filter({ hasText: words }))
    .filter({ visible: true }).first();
}

/** The picker named `picker`'s option for `words`: its label, an owner after ` · `, or its key. */
async function chooseIn(page, picker, words) {
  const select = page.getByRole('combobox', { name: picker, exact: true });
  const value = await select.evaluate((element, wanted) => {
    const options = [...element.options];
    const hit = options.find((o) => o.label === wanted || o.label.endsWith(` · ${wanted}`) || o.value === wanted)
      ?? options.find((o) => o.label.includes(wanted));
    return hit?.value ?? null;
  }, words);
  if (value === null) throw new Error(`nothing named "${picker}" offers "${words}"`);
  await select.selectOption(value);
}

// Every control pauses while the application answers an event ([UI-HOST-MORPH]); a step that
// acts before the answer lands can press a button the pause then swallows, so acting waits for idle.
const idle = (page) => expect(page.locator('#osprey-root')).toHaveAttribute('aria-busy', 'false');
const ACTS = new Set(['invoke', 'type', 'choose']);

const VERBS = {
  wait: async (page, [words]) => expect(shown(page, words).first()).toBeVisible(),
  gone: async (page, [words]) => expect(shown(page, words)).toHaveCount(0),
  invoke: async (page, [words]) => invocable(page, words).click(),
  type: async (page, [label, words]) => page.getByRole('textbox', { name: label, exact: true }).fill(words),
  choose: async (page, words) => (words.length === 2
    ? chooseIn(page, words[0], words[1])
    : page.getByRole('tab', { name: words[0], exact: true }).click()),
  shot: async (page, [name]) => expect(page).toHaveScreenshot(`${name}.png`, { fullPage: true, animations: 'disabled' }),
};

/** Plays every step of `file` on `page`, naming the failing step. */
async function play(page, file) {
  for (const [index, { verb, words }] of steps(file).entries()) {
    const act = VERBS[verb];
    if (!act) throw new Error(`Unknown step ${verb}`);
    await test.step(`${index + 1} ${verb} ${words.join(' · ')}`, async () => {
      if (ACTS.has(verb)) await idle(page);
      await act(page, words);
    });
  }
}

module.exports = { play, steps };

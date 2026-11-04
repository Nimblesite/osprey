// Mermaid diagram rendering. Injected by the `mermaid-render` transform in
// eleventy.config.mjs, and ONLY into pages that actually contain a diagram, so
// no other page pays for the runtime.
//
// typeDiagram blocks are NOT handled here: they are rendered to inline SVG at
// build time (see the `typediagram-render` transform), so they need no client
// JavaScript at all.
const MERMAID_ENTRY = '/assets/vendor/mermaid/mermaid.esm.min.mjs';

// Theme comes from the page's ACTUAL background luminance, never from the
// `data-theme` attribute. The theme script sets that attribute from
// prefers-color-scheme, but this site renders dark-only (`.theme-toggle` is
// hidden), so `data-theme="light"` on a dark page is normal here — trusting it
// would put light diagrams on a dark background.
function isDark() {
  const of = (element) => getComputedStyle(element).backgroundColor;
  const parse = (color) => (color.match(/[\d.]+/g) || []).map(Number);
  const body = parse(of(document.body));
  // A transparent body paints the root element's background instead.
  const [r, g, b] = body.length >= 3 && body[3] !== 0 ? body : parse(of(document.documentElement));
  if (![r, g, b].every(Number.isFinite)) return false;
  return (0.299 * r + 0.587 * g + 0.114 * b) / 255 < 0.5;
}

// `mermaid.render(id, source)` is used instead of `mermaid.run({nodes})` so the
// source is handed over as a STRING. `run()` re-reads each node's markup, which
// makes a re-render (theme change) parse the SVG it produced last time.
async function renderDiagrams(mermaid, diagrams) {
  // No `fontFamily: 'inherit'` — mermaid measures label text with this value to
  // size each node, and `inherit` measures as a fallback font, which clips
  // longer labels. A real stack keeps the box and the text in agreement.
  // `htmlLabels: false` draws labels as SVG <text>, which mermaid measures
  // directly. The default foreignObject labels are measured against assumed
  // metrics and clip multi-line labels once the page font differs.
  mermaid.initialize({
    startOnLoad: false,
    theme: isDark() ? 'dark' : 'neutral',
    securityLevel: 'strict',
    fontFamily: 'ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, sans-serif',
    flowchart: { htmlLabels: false, useMaxWidth: true },
  });
  await Promise.all(
    diagrams.map(async ({ block, source }, index) => {
      const { svg } = await mermaid.render(`diagram-${index}`, source);
      block.innerHTML = svg;
    })
  );
}

async function main() {
  const blocks = Array.from(document.querySelectorAll('pre.mermaid'));
  if (blocks.length === 0) return;

  const diagrams = blocks.map((block) => ({ block, source: block.textContent }));
  const { default: mermaid } = await import(MERMAID_ENTRY);
  await renderDiagrams(mermaid, diagrams);

  // A theme toggle flips `data-theme` on <html>; diagrams must follow it or a
  // dark page shows light-mode diagrams (and vice versa).
  let dark = isDark();
  new MutationObserver(async () => {
    if (isDark() === dark) return;
    dark = isDark();
    await renderDiagrams(mermaid, diagrams);
  }).observe(document.documentElement, { attributeFilter: ['data-theme'] });
}

main().catch((error) => console.error('Diagram rendering failed:', error));

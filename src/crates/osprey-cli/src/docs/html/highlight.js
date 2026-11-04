// Safe presentation of code using the shared Osprey grammar. Text is never HTML.
(function () {
  const rules = Object.entries(ospreyGrammar).flatMap(([kind, entries]) =>
    [entries].flat().map(entry => ({ kind, pattern: entry.pattern || entry, lookbehind: entry.lookbehind })));
  function next(source) {
    return rules.reduce((best, rule) => {
      const match = rule.pattern.exec(source);
      if (!match) return best;
      const prefix = rule.lookbehind ? (match[1] || '').length : 0;
      const token = { kind: rule.kind, start: match.index + prefix, text: match[0].slice(prefix) };
      return token.text && (!best || token.start < best.start) ? token : best;
    }, null);
  }
  function highlight(code) {
    const fragment = document.createDocumentFragment();
    let source = code.textContent;
    while (source) {
      const token = next(source);
      if (!token) { fragment.append(document.createTextNode(source)); break; }
      fragment.append(document.createTextNode(source.slice(0, token.start)));
      const span = document.createElement('span');
      span.className = 'token ' + token.kind;
      span.textContent = token.text;
      fragment.append(span);
      source = source.slice(token.start + token.text.length);
    }
    code.replaceChildren(fragment);
  }
  document.querySelectorAll('pre code.language-osprey, pre code.language-osprey-ml, pre code.language-osp, pre code.language-ospml').forEach(highlight);
})();

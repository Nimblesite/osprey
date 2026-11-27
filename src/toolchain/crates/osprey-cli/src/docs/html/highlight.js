// Safe presentation of code using the shared Osprey grammar. Text is never HTML.
(function () {
  const rulesFor = grammar => Object.entries(grammar).flatMap(([kind, entries]) =>
    [entries].flat().map(entry => ({ kind, pattern: entry.pattern || entry, lookbehind: entry.lookbehind })));
  const common = rulesFor(ospreyGrammar);
  const python = rulesFor({
    comment: /#[^\r\n]*/,
    string: /'(?:[^'\\\r\n]|\\.)*'/,
    keyword: /\b(?:as|assert|async|class|def|del|elif|except|finally|for|from|global|is|lambda|nonlocal|not|or|and|pass|raise|try|while|with|True|False)\b/,
    type: /\b(?:str|list|dict|tuple|set|bytes)\b/,
  }).concat(common);
  const typescript = rulesFor({
    string: /'(?:[^'\\\r\n]|\\.)*'/,
    keyword: /\b(?:as|async|break|case|class|const|continue|export|for|from|function|interface|new|null|switch|throw|try|undefined|while)\b/,
    type: /\b(?:boolean|number|unknown|void)\b/,
  }).concat(common);
  function next(source, rules) {
    return rules.reduce((best, rule) => {
      const match = rule.pattern.exec(source);
      if (!match) return best;
      const prefix = rule.lookbehind ? (match[1] || '').length : 0;
      const token = { kind: rule.kind, start: match.index + prefix, text: match[0].slice(prefix) };
      return token.text && (!best || token.start < best.start) ? token : best;
    }, null);
  }
  function appendTokens(fragment, source, rules) {
    while (source) {
      const token = next(source, rules);
      if (!token) { fragment.append(document.createTextNode(source)); break; }
      fragment.append(document.createTextNode(source.slice(0, token.start)));
      const span = document.createElement('span');
      span.className = 'token ' + token.kind;
      if (token.kind === 'interpolation') {
        span.append(token.text.slice(0, 2));
        appendTokens(span, token.text.slice(2, -1), rules.filter(rule => rule.kind !== 'interpolation'));
        span.append('}');
      } else {
        span.textContent = token.text;
      }
      fragment.append(span);
      source = source.slice(token.start + token.text.length);
    }
  }
  const byLanguage = {
    osprey: common, 'osprey-ml': common, osp: common, ospml: common, python, typescript,
  };
  document.querySelectorAll('pre code[class^="language-"]').forEach(code => {
    const language = code.classList[0].slice('language-'.length);
    if (!Object.hasOwn(byLanguage, language)) return;
    const fragment = document.createDocumentFragment();
    appendTokens(fragment, code.textContent, byLanguage[language]);
    code.replaceChildren(fragment);
  });
})();

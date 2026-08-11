// The Osprey Prism grammar, in ONE place. Both consumers import it: the
// Eleventy build (eleventy.config.mjs) highlights every `.osp` snippet on the
// site with it at build time, and the /wasm/ studio page highlights the source
// it runs with it in the browser. Two copies drifted into two colour schemes
// for the same language, so the token palette lives here instead.
export const ospreyGrammar = {
  comment: [
    { pattern: /(^|[^\\])\/\*[\s\S]*?(?:\*\/|$)/, lookbehind: true },
    { pattern: /(^|[^\\:])\/\/.*/, lookbehind: true },
  ],
  string: { pattern: /"(?:[^"\\]|\\.)*"/, greedy: true },
  interpolation: {
    pattern: /\$\{[^}]+\}/,
    inside: { punctuation: /^\$\{|\}$/ },
  },
  keyword:
    /\b(?:fn|let|mut|match|type|effect|perform|handle|in|extern|spawn|await|yield|if|else|import|module|true|false|where|Unit|Result|Option|Some|None|Ok|Err)\b/,
  type: /\b(?:int|float|string|bool|List|Map|Set|Ptr|Channel|Fiber|Json|HttpResponse)\b/,
  function: /\b[a-zA-Z_][a-zA-Z0-9_]*(?=\s*\()/,
  number: /\b(?:0x[\da-f]+|\d*\.?\d+(?:e[+-]?\d+)?)\b/i,
  operator: /\|>|->|=>|<-|\+|-|\*|\/|%|==|!=|<=|>=|<|>|=|!|&&|\|\|/,
  punctuation: /[{}[\];(),.:]/,
};

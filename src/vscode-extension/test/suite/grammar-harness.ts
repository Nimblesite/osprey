// Shared harness for the TextMate grammar suites. The grammars ship in the
// VSIX and decide every colour a user sees; these helpers tokenize with the
// SAME engine VS Code uses (vscode-textmate over vscode-oniguruma) so a
// passing assertion is a statement about the editor, not about a regex read
// by eye.

import * as assert from "assert";
import * as fs from "fs";
import * as path from "path";
import * as vsctm from "vscode-textmate";
import * as oniguruma from "vscode-oniguruma";
import { extensionRoot } from "./osprey-test-env";

export const repoRoot = path.resolve(extensionRoot, "..");
const syntaxesDir = path.join(extensionRoot, "syntaxes");

const GRAMMARS: Record<string, string> = {
  "source.osprey": "osprey.tmGrammar.json",
  "source.osprey-ml": "osprey-ml.tmLanguage.json",
};

/**
 * One token: its raw text, its scopes with the grammar's root scope dropped,
 * and the rest of its line from where it starts.
 */
export interface Token {
  text: string;
  scopes: string[];
  line: number;
  rest: string;
}

let registry: vsctm.Registry | undefined;

async function loadRegistry(): Promise<vsctm.Registry> {
  const wasmPath = path.join(
    path.dirname(require.resolve("vscode-oniguruma")),
    "onig.wasm",
  );
  const wasm = fs.readFileSync(wasmPath);
  await oniguruma.loadWASM(
    wasm.buffer.slice(
      wasm.byteOffset,
      wasm.byteOffset + wasm.byteLength,
    ) as ArrayBuffer,
  );
  return new vsctm.Registry({
    onigLib: Promise.resolve({
      createOnigScanner: (sources) => new oniguruma.OnigScanner(sources),
      createOnigString: (value) => new oniguruma.OnigString(value),
    }),
    loadGrammar: async (scope) => {
      const file = GRAMMARS[scope];
      if (!file) {
        return null;
      }
      const full = path.join(syntaxesDir, file);
      return vsctm.parseRawGrammar(fs.readFileSync(full, "utf8"), full);
    },
  });
}

export async function grammarFor(scope: string): Promise<vsctm.IGrammar> {
  registry = registry ?? (await loadRegistry());
  const grammar = await registry.loadGrammar(scope);
  assert.ok(grammar, `grammar ${scope} loads`);
  return grammar;
}

/** The grammar a corpus file is written in, chosen by its extension. */
export function grammarScopeOf(file: string): string {
  return file.endsWith(".ospml") ? "source.osprey-ml" : "source.osprey";
}

/**
 * Tokenize `source`, keeping the ruleStack across lines as an editor does.
 * Whitespace-only tokens are dropped: they carry no colour.
 */
export function tokenize(grammar: vsctm.IGrammar, source: string): Token[] {
  const tokens: Token[] = [];
  let stack = vsctm.INITIAL;
  source.split("\n").forEach((line, index) => {
    const result = grammar.tokenizeLine(line, stack);
    stack = result.ruleStack;
    for (const token of result.tokens) {
      const text = line.slice(token.startIndex, token.endIndex);
      if (text.trim()) {
        tokens.push({
          text,
          scopes: token.scopes.slice(1),
          line: index + 1,
          rest: line.slice(token.startIndex),
        });
      }
    }
  });
  return tokens;
}

/** Every token whose trimmed text is exactly `text`; at least one must exist. */
function tokensNamed(tokens: Token[], text: string): Token[] {
  const found = tokens.filter((token) => token.text.trim() === text);
  assert.ok(found.length > 0, `token ${JSON.stringify(text)} exists`);
  return found;
}

/** The scopes of the first token whose text is exactly `text`. */
export function scopesOf(tokens: Token[], text: string): string {
  return tokensNamed(tokens, text)[0].scopes.join("|");
}

function assertTokenScoped(token: Token, expected: string): void {
  const scopes = token.scopes.join("|");
  assert.ok(
    scopes.includes(expected),
    `${JSON.stringify(token.text.trim())} on line ${token.line} should be ${expected}, got ${scopes || "NONE"}`,
  );
}

/** The first token spelled `text` carries `expected`. */
export function assertScoped(
  tokens: Token[],
  text: string,
  expected: string,
): void {
  assertTokenScoped(tokensNamed(tokens, text)[0], expected);
}

/** Every token spelled `text` carries `expected`. */
export function assertEveryScoped(
  tokens: Token[],
  text: string,
  expected: string,
): void {
  for (const token of tokensNamed(tokens, text)) {
    assertTokenScoped(token, expected);
  }
}

/** Every Osprey source file in the repository, excluding build output. */
export function corpusFiles(): string[] {
  const out: string[] = [];
  const walk = (dir: string): void => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        if (entry.name !== "build" && entry.name !== "node_modules") {
          walk(full);
        }
      } else if (/\.(osp|ospml)$/.test(entry.name)) {
        out.push(full);
      }
    }
  };
  for (const top of ["tests", "examples"]) {
    walk(path.join(repoRoot, top));
  }
  return out.sort();
}

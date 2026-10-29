// Implements [UI-COMPONENTS]: approved assets supply semantic symbols on every host.
import { record } from "./protocol";

const MAX_BYTES = 65536;
const MAX_SYMBOLS = 128;

function decoded(source: string): unknown {
  try { return JSON.parse(source); }
  catch { throw new Error("Invalid symbol registry JSON"); }
}

function resolved(entry: [string, unknown], asset: (path: string) => string): [string, string] {
  const [name, path] = entry;
  if (!/^[A-Za-z0-9._-]{1,128}$/.test(name) || typeof path !== "string"
    || !/^(icons|symbols)\/[A-Za-z0-9_-]+\.svg$/.test(path)) throw new Error("Invalid symbol registry entry");
  const uri = asset(path);
  if (uri === "") throw new Error(`Missing symbol asset: ${path}`);
  return [name, uri];
}

export function resolveSymbols(source: string, asset: (path: string) => string): ReadonlyMap<string, string> {
  if (Buffer.byteLength(source, "utf8") > MAX_BYTES) throw new Error("Symbol registry exceeds 64 KiB");
  const value = decoded(source);
  if (!record(value) || Object.keys(value).length === 0 || Object.keys(value).length > MAX_SYMBOLS) {
    throw new Error("Invalid symbol registry inventory");
  }
  return new Map(Object.entries(value).map(entry => resolved(entry, asset)));
}

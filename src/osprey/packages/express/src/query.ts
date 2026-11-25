// osprey: namespace=express
// osprey: module=Query

/**
 * The query string of a request URL, read as Node.js `querystring.parse` reads
 * it for Express's default query parser. [TS-PORT-EXPRESS]
 */
import { byteOr, printable } from "./encode.ts";

/**
 * Each name in the query of `url` with its decoded value. A repeated name
 * keeps its first value, where Node.js collects them into an array.
 * # Examples
 * ```osprey
 * const query = queryOf("/search?q=ru%73t+lang&page=2&q=again&flag");
 * print(`[${mapGet(query, "q")}] [${mapGet(query, "page")}] [${mapGet(query, "flag")}] [${mapGet(queryOf("/plain"), "q")}]`);
 * ```
 * ```output
 * [Success(rust lang)] [Success(2)] [Success()] [Error(mapGet: key not found)]
 * ```
 */
export function queryOf(url: string): Map<string, string> {
  const mark = indexOf(url, "?");
  switch (mark.kind) {
    case "success":
      return collected(new Map<string, string>(), drop(url, satAdd(mark.value, 1)));
    case "error":
      return new Map<string, string>();
  }
}

/** The parts of `text` between each `&`, added in order. */
function collected(found, text) {
  const mark = indexOf(text, "&");
  switch (mark.kind) {
    case "success":
      return collected(added(found, take(text, mark.value)), drop(text, satAdd(mark.value, 1)));
    case "error":
      return added(found, text);
  }
}

/** `name=value` splits at its first `=`; a part with none is a name with an empty value. */
function added(found, part) {
  if (part === "") {
    return found;
  }
  const mark = indexOf(part, "=");
  switch (mark.kind) {
    case "success":
      return kept(found, decoded(take(part, mark.value)), decoded(drop(part, satAdd(mark.value, 1))));
    case "error":
      return kept(found, decoded(part), "");
  }
}

function kept(found, name, text) {
  const held = mapGet(found, name);
  switch (held.kind) {
    case "success":
      return found;
    case "error":
      return mapSet(found, name, text);
  }
}

/**
 * `+` is a space, and `%XX` is the printable ASCII character with that code.
 * Any other escape stays as written: Osprey has no byte-to-text conversion yet.
 */
function decoded(text) {
  return unescaped(spaced(text));
}

function spaced(text) {
  const mark = indexOf(text, "+");
  switch (mark.kind) {
    case "success":
      return `${take(text, mark.value)} ${spaced(drop(text, satAdd(mark.value, 1)))}`;
    case "error":
      return text;
  }
}

function unescaped(text) {
  const mark = indexOf(text, "%");
  switch (mark.kind) {
    case "success":
      return `${take(text, mark.value)}${escape(drop(text, satAdd(mark.value, 1)))}`;
    case "error":
      return text;
  }
}

/** `rest` follows a `%`: its two hex digits become their character, or the `%` stays. */
function escape(rest) {
  const code = hexPair(rest);
  if (code === 32) {
    return ` ${unescaped(drop(rest, 2))}`;
  }
  return code > 32 && code <= 126 ? `${printable(code)}${unescaped(drop(rest, 2))}` : `%${unescaped(rest)}`;
}

/** The value of the two hex digits that start `text`, or -1 when it does not start with two. */
function hexPair(text) {
  const high = hexValue(byteOr(text, 0));
  const low = hexValue(byteOr(text, 1));
  return high < 0 || low < 0 ? -1 : satAdd(satMul(high, 16), low);
}

function hexValue(byte) {
  if (byte >= 48 && byte <= 57) {
    return satSub(byte, 48);
  }
  if (byte >= 65 && byte <= 70) {
    return satSub(byte, 55);
  }
  return byte >= 97 && byte <= 102 ? satSub(byte, 87) : -1;
}

// osprey: namespace=express
// osprey: module=Headers

/**
 * Request headers as Express reads them: a name matches without regard to
 * case, and a value is trimmed. The raw block is the listener's
 * `Name: value` lines. [TS-PORT-EXPRESS-SURFACE]
 */

/** The value of the first header called `name`, or an error when the request has none.
 * # Examples
 * ```osprey
 * const headers = "Host: localhost:3000\r\nX-Request-Id:  42 \r\n";
 * print(`[${headerOf(headers, "host")}] [${headerOf(headers, "X-REQUEST-ID")}] [${headerOf(headers, "Cookie")}]`);
 * ```
 * ```output
 * [Success(localhost:3000)] [Success(42)] [Error(no such header)]
 * ```
 */
export function headerOf(headers: string, name: string): Result<string, string> {
  return lineValue(headers, `${toLowerCase(name)}:`);
}

/** The value after `prefix` on the first line of `rest` that starts with it, ignoring case. */
function lineValue(rest, prefix) {
  if (rest === "") {
    return Error("no such header");
  }
  const end = indexOf(rest, "\n");
  switch (end.kind) {
    case "success":
      return matched(take(rest, end.value), drop(rest, satAdd(end.value, 1)), prefix);
    case "error":
      return matched(rest, "", prefix);
  }
}

function matched(line, rest, prefix) {
  return startsWith(toLowerCase(line), prefix) ? Success(trim(drop(line, length(prefix)))) : lineValue(rest, prefix);
}

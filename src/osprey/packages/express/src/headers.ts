// osprey: namespace=express
// osprey: module=Headers

/**
 * Request headers as Express reads them: a name matches without regard to
 * case, and a value is trimmed. The raw block is the listener's
 * `Name: value` lines. [TS-PORT-EXPRESS-SURFACE]
 */

/** The value of the first header called `name`, or an error when the request has none. */
export function headerOf(headers: string, name: string): Result<string, Error> {
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

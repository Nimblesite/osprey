// osprey: namespace=express
// osprey: module=Encode

/**
 * The text encoders Express takes from `escape-html` and `encodeurl`, and the
 * byte helpers the query parser shares with them. [TS-PORT-EXPRESS]
 */

/** The digits of a percent escape. */
const HEX = "0123456789ABCDEF";
/** The printable ASCII characters from `!` (33) to `~` (126), by code. */
const PRINTABLE = "!\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~";
const FIRST_PRINTABLE = 33;

/** The five replacements of `escape-html`, applied to every reflected path.
 * # Examples
 * ```osprey
 * print(`[${escapeHtml("<b>\"&'</b>")}] [${escapeHtml("plain")}]`);
 * ```
 * ```output
 * [Success(&lt;b&gt;&quot;&amp;&#39;&lt;/b&gt;)] [Success(plain)]
 * ```
 */
export function escapeHtml(text) {
  return andThen(replace(text, "&", "&amp;"), (amp) =>
    andThen(replace(amp, "<", "&lt;"), (lt) =>
      andThen(replace(lt, ">", "&gt;"), (gt) =>
        andThen(replace(gt, "\"", "&quot;"), (quot) => replace(quot, "'", "&#39;")),
      ),
    ),
  );
}

/** `encodeurl`: percent-encode every byte a URL cannot carry as written.
 * # Examples
 * ```osprey
 * print(`[${encodeUrl("/find/a b?x=1&y=2")}] [${encodeUrl("/done%20already")}]`);
 * ```
 * ```output
 * [/find/a%20b?x=1&y=2] [/done%20already]
 * ```
 */
export function encodeUrl(text) {
  return encodeFrom(text, 0, "");
}

function encodeFrom(text, index, encoded) {
  const byte = byteOr(text, index);
  if (byte < 0) {
    return encoded;
  }
  return encodeFrom(text, satAdd(index, 1), `${encoded}${encodedByte(text, index, byte)}`);
}

/** The byte at `index`, or -1 past the end of `text`.
 * # Examples
 * ```osprey
 * print(`[${byteOr("A", 0)}] [${byteOr("A", 5)}]`);
 * ```
 * ```output
 * [65] [-1]
 * ```
 */
export function byteOr(text, index) {
  const byte = byteAt(text, index);
  switch (byte.kind) {
    case "success":
      return byte.value;
    case "error":
      return -1;
  }
}

function encodedByte(text, index, byte) {
  if (byte === 37) {
    return isHex(byteOr(text, satAdd(index, 1))) && isHex(byteOr(text, satAdd(index, 2))) ? "%" : "%25";
  }
  return urlSafe(byte) ? printable(byte) : percent(byte);
}

/** Whether `encodeurl` leaves this byte as written. */
function urlSafe(byte) {
  return byte === 33 || (byte >= 35 && byte <= 59) || byte === 61 || (byte >= 63 && byte <= 95) || (byte >= 97 && byte <= 122) || byte === 124 || byte === 126;
}

function isHex(byte) {
  return (byte >= 48 && byte <= 57) || (byte >= 65 && byte <= 70) || (byte >= 97 && byte <= 102);
}

function percent(byte) {
  return `%${hexDigit(shiftRight(byte, 4))}${hexDigit(bitAnd(byte, 15))}`;
}

function hexDigit(value) {
  return take(drop(HEX, value), 1);
}

/** The printable ASCII character with this code, from `!` (33) to `~` (126).
 * # Examples
 * ```osprey
 * print(`[${printable(33)}] [${printable(65)}] [${printable(126)}]`);
 * ```
 * ```output
 * [!] [A] [~]
 * ```
 */
export function printable(code) {
  return take(drop(PRINTABLE, satSub(code, FIRST_PRINTABLE)), 1);
}

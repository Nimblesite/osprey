---
layout: page
title: "Built-in Functions"
description: "Complete reference for all built-in functions in Osprey"
---

All built-in functions available in Osprey.

## [Channel](channel/)

**Signature:** `Channel(capacity: int) -> Channel<t0>`

Creates a new channel with the specified capacity.

## [List](list/)

**Signature:** `List() -> List<t0>`

Creates a new empty list.

## [Map](map-type/)

**Signature:** `Map() -> Map<t0, t1>`

Creates a new, empty persistent map.

## [abs](abs/)

**Signature:** `abs(value: int) -> int`

Returns the absolute value of an integer.

## [await](await/)

**Signature:** `await(fiber: Fiber<t0>) -> t0`

Waits for a fiber to finish and returns its result, suspending the current fiber until then.

## [awaitProcess](awaitprocess/)

**Signature:** `awaitProcess(handle: int) -> int`

Waits for a spawned process to complete and returns its exit code. Blocks until the process finishes.

## [byteAt](byteat/)

**Signature:** `byteAt(text: string, index: int) -> Result<int, Error>`

Returns the byte at the given index (0-255), or an error if the index is out of range.

## [byteLength](bytelength/)

**Signature:** `byteLength(text: string) -> int`

Returns the number of bytes in the string's UTF-8 encoding.

## [check](check/)

**Signature:** `check(label: string, expected: any, actual: any) -> Unit`

Labeled equality assertion in Alcotest argument order (expected before actual). Behaves exactly like expect, with the label in the failure diagnostic.

## [checkedAdd](checkedadd/)

**Signature:** `checkedAdd(a: int, b: int) -> Result<int, Error>`

Integer addition that reports overflow instead of wrapping. The `+` operator returns plain int because a wrapped result is still representable; this returns Result<int, MathError>.

## [checkedMul](checkedmul/)

**Signature:** `checkedMul(a: int, b: int) -> Result<int, Error>`

Integer multiplication that reports overflow instead of wrapping, returning Result<int, MathError>. The guarded sibling of `*`.

## [checkedSub](checkedsub/)

**Signature:** `checkedSub(a: int, b: int) -> Result<int, Error>`

Integer subtraction that reports overflow instead of wrapping, returning Result<int, MathError>. The guarded sibling of `-`.

## [cleanupProcess](cleanupprocess/)

**Signature:** `cleanupProcess(handle: int) -> Unit`

Cleans up resources associated with a completed process. Should be called after awaitProcess.

## [codePointAt](codepointat/)

**Signature:** `codePointAt(text: string, index: int) -> Result<int, Error>`

Returns the Unicode code point that begins at the given byte index. Fails on an invalid index or malformed UTF-8.

## [codePointWidth](codepointwidth/)

**Signature:** `codePointWidth(codePoint: int) -> Result<int, Error>`

Returns how many bytes the given Unicode code point occupies in UTF-8 (1-4).

## [contains](contains/)

**Signature:** `contains(s: string, needle: string) -> bool`

True if needle appears anywhere in s. Empty needle returns true.

## [deleteFile](deletefile/)

**Signature:** `deleteFile(path: string) -> Result<Unit, Error>`

Deletes the file at the given path, returning Unit on success or an error.

## [drop](drop/)

**Signature:** `drop(s: string, n: int) -> string`

Returns s without its first n bytes. Clamps; never fails.

## [endsWith](endswith/)

**Signature:** `endsWith(s: string, suffix: string) -> bool`

True if s ends with suffix.

## [expect](expect/)

**Signature:** `expect(actual: any, expected: any) -> Unit`

Asserts two values are equal (canonical-string equality, Results auto-unwrapped). On mismatch, marks the enclosing test failed and prints a diagnostic; execution continues.

## [fiberDone](fiberdone/)

**Signature:** `fiberDone(fiber: any) -> int`

Returns 1 if the given fiber has finished, 0 otherwise.

## [fiber_yield](fiber_yield/)

**Signature:** `fiber_yield(value: int) -> int`

Yields control to the fiber scheduler with an optional value.

## [filter](filter/)

**Signature:** `filter(iterator: List<t0>, predicate: (t0) -> bool) -> List<t0>`

Filters elements in an iterator based on a predicate function.

## [fold](fold/)

**Signature:** `fold(iterator: List<t0>, initial: t1, fn: (t1, t0) -> t1) -> t1`

Reduces an iterator to a single value by repeatedly applying a function.

## [forEach](foreach/)

**Signature:** `forEach(iterator: List<t0>, function: (t0) -> Unit) -> Unit`

Applies a function to each element in an iterator.

## [forEachList](foreachlist/)

**Signature:** `forEachList(list: List<t0>, function: (t0) -> Unit) -> Unit`

Apply function to every element of list. Phase 7 of collections plan.

## [fromCodePoint](fromcodepoint/)

**Signature:** `fromCodePoint(codePoint: int) -> Result<string, Error>`

Returns the single-character string for a Unicode code point, or an error if it is not a valid scalar value.

## [httpCloseClient](httpcloseclient/)

**Signature:** `httpCloseClient(clientID: int) -> Unit`

Closes the HTTP client and cleans up resources.

## [httpCreateClient](httpcreateclient/)

**Signature:** `httpCreateClient(base_url: string, timeout: int) -> int`

Creates an HTTP client for making requests to a base URL.

## [httpCreateServer](httpcreateserver/)

**Signature:** `httpCreateServer(port: int, address: string) -> int`

Creates an HTTP server bound to the specified port and address.

## [httpDelete](httpdelete/)

**Signature:** `httpDelete(clientID: int, path: string, headers: string) -> Result<string, Error>`

Makes an HTTP DELETE request to the specified path.

## [httpGet](httpget/)

**Signature:** `httpGet(clientID: int, path: string, headers: string) -> Result<string, Error>`

Makes an HTTP GET request to the specified path.

## [httpGetResponse](httpgetresponse/)

**Signature:** `httpGetResponse(clientID: int, path: string, headers: string) -> Result<int, Error>`

Sends an HTTP GET request and returns a response handle for inspecting the status, headers, and body.

## [httpListen](httplisten/)

**Signature:** `httpListen(serverID: int, handler: any) -> int`

Starts the HTTP server listening for requests with a handler function.

## [httpPost](httppost/)

**Signature:** `httpPost(clientID: int, path: string, body: string, headers: string) -> Result<string, Error>`

Makes an HTTP POST request with a request body.

## [httpPut](httpput/)

**Signature:** `httpPut(clientID: int, path: string, body: string, headers: string) -> Result<string, Error>`

Makes an HTTP PUT request with a request body.

## [httpResponseBody](httpresponsebody/)

**Signature:** `httpResponseBody(responseID: int) -> Result<string, Error>`

Returns the body of a response handle as a string.

## [httpResponseFree](httpresponsefree/)

**Signature:** `httpResponseFree(responseID: int) -> Unit`

Releases a response handle obtained from httpGetResponse.

## [httpResponseHeader](httpresponseheader/)

**Signature:** `httpResponseHeader(responseID: int, name: string) -> Result<string, Error>`

Returns the value of the named header from a response handle.

## [httpResponseStatus](httpresponsestatus/)

**Signature:** `httpResponseStatus(responseID: int) -> int`

Returns the HTTP status code of a response handle.

## [httpStopServer](httpstopserver/)

**Signature:** `httpStopServer(serverID: int) -> Unit`

Stops the HTTP server and closes all connections.

## [indexOf](indexof/)

**Signature:** `indexOf(s: string, needle: string) -> Result<int, Error>`

Returns byte-index of first occurrence of needle, or Error(NotFound).

## [input](input/)

**Signature:** `input() -> string`

Reads a string from the user's input.

## [intDiv](intdiv/)

**Signature:** `intDiv(a: int, b: int) -> Result<int, Error>`

Truncating integer division (rounds toward zero), divide-by-zero checked. The `/` operator is float-only; this is its integer sibling, returning Result<int, MathError>.

## [isEmpty](isempty/)

**Signature:** `isEmpty(s: any) -> bool`

True if string has zero length.

## [join](join/)

**Signature:** `join(parts: List<string>, separator: string) -> string`

Concatenates parts with separator between each pair.

## [jsonFree](jsonfree/)

**Signature:** `jsonFree(document: int) -> Unit`

Releases a parsed JSON document handle obtained from jsonParse.

## [jsonGet](jsonget/)

**Signature:** `jsonGet(document: int, path: string) -> Result<string, Error>`

Returns the string value at the given path within a parsed JSON document.

## [jsonLength](jsonlength/)

**Signature:** `jsonLength(document: int, path: string) -> int`

Returns the number of elements in the JSON array at the given path.

## [jsonParse](jsonparse/)

**Signature:** `jsonParse(text: string) -> Result<int, Error>`

Parses a JSON string and returns an opaque document handle for querying, or an error on malformed input.

## [length](length/)

**Signature:** `length(s: any) -> int`

Returns the byte length of a string. Total — never fails.

## [lines](lines/)

**Signature:** `lines(s: string) -> List<string>`

Splits on '\n'. A trailing newline does not produce an empty entry.

## [listAppend](listappend/)

**Signature:** `listAppend(list: List<t0>, value: t0) -> List<t0>`

Returns a new list with value at the end. O(log32 n) amortised.

## [listConcat](listconcat/)

**Signature:** `listConcat(left: List<t0>, right: List<t0>) -> List<t0>`

Returns left ++ right. Same as left + right.

## [listContains](listcontains/)

**Signature:** `listContains(list: List<t0>, value: t0) -> bool`

True iff some element equals value. O(n).

## [listGet](listget/)

**Signature:** `listGet(list: List<t0>, index: int) -> Result<t0, Error>`

Returns the element at the given index, or an error if the index is out of range.

## [listLength](listlength/)

**Signature:** `listLength(list: List<t0>) -> int`

Returns the number of elements in a list. O(1).

## [listPrepend](listprepend/)

**Signature:** `listPrepend(list: List<t0>, value: t0) -> List<t0>`

Returns a new list with value at the front. O(n).

## [listReverse](listreverse/)

**Signature:** `listReverse(list: List<t0>) -> List<t0>`

Returns a new list in reverse order.

## [map](map/)

**Signature:** `map(iterator: List<t0>, fn: (t0) -> t1) -> List<t1>`

Transforms each element in an iterator using a function, returning a new iterator.

## [mapContains](mapcontains/)

**Signature:** `mapContains(map: Map<t0, t1>, key: t0) -> bool`

True iff key is present in map.

## [mapGet](mapget/)

**Signature:** `mapGet(map: Map<t0, t1>, key: t0) -> Result<t1, Error>`

Returns the value associated with the key, or an error if the key is absent.

## [mapKeys](mapkeys/)

**Signature:** `mapKeys(map: Map<t0, t1>) -> List<t0>`

All keys of the map as a list. Order unspecified.

## [mapLength](maplength/)

**Signature:** `mapLength(map: Map<t0, t1>) -> int`

Returns the number of entries in a map. O(1).

## [mapMerge](mapmerge/)

**Signature:** `mapMerge(left: Map<t0, t1>, right: Map<t0, t1>) -> Map<t0, t1>`

Right-biased union. Same as left + right.

## [mapRemove](mapremove/)

**Signature:** `mapRemove(map: Map<t0, t1>, key: t0) -> Map<t0, t1>`

Returns a new map without key. No-op if key is absent.

## [mapSet](mapset/)

**Signature:** `mapSet(map: Map<t0, t1>, key: t0, value: t1) -> Map<t0, t1>`

Returns a new map with key bound to value (replaces prior binding).

## [mapValues](mapvalues/)

**Signature:** `mapValues(map: Map<t0, t1>) -> List<t1>`

All values of the map as a list. Order matches mapKeys.

## [not](not/)

**Signature:** `not(value: bool) -> bool`

Returns the logical negation of a boolean.

## [padEnd](padend/)

**Signature:** `padEnd(s: string, targetLength: int, fill: string) -> Result<string, Error>`

Pads s on the right with copies of fill to reach targetLength bytes.

## [padStart](padstart/)

**Signature:** `padStart(s: string, targetLength: int, fill: string) -> Result<string, Error>`

Pads s on the left with copies of fill to reach targetLength bytes.

## [parseFloat](parsefloat/)

**Signature:** `parseFloat(s: string) -> Result<float, Error>`

Strict base-10 floating-point parser. No whitespace tolerance.

## [parseInt](parseint/)

**Signature:** `parseInt(s: string) -> Result<int, Error>`

Strict base-10 signed-int parser. No whitespace tolerance.

## [print](print/)

**Signature:** `print(value: any) -> Unit`

Prints a value to the console. Automatically converts the value to a string representation.

## [random](random/)

**Signature:** `random() -> int`

A cryptographically-secure uniform random non-negative integer (0 .. 2^63-1), drawn fresh from the OS entropy source. Unseeded and unpredictable.

## [randomBelow](randombelow/)

**Signature:** `randomBelow(n: int) -> Result<int, Error>`

A cryptographically-secure uniform random integer in [0, n), unbiased by rejection sampling. Returns Result<int, MathError> — Error when n <= 0.

## [range](range/)

**Signature:** `range(start: int, end: int) -> List<int>`

Creates an iterator that generates numbers from start to end (exclusive).

## [readFile](readfile/)

**Signature:** `readFile(filename: string) -> Result<string, Error>`

Reads the entire contents of a file as a string.

## [recv](recv/)

**Signature:** `recv(channel: Channel<t0>) -> t0`

Receives a value from a channel.

## [repeat](repeat/)

**Signature:** `repeat(s: string, n: int) -> Result<string, Error>`

Concatenates s with itself n times. Error(InvalidArgument) on negative n.

## [replace](replace/)

**Signature:** `replace(s: string, needle: string, replacement: string) -> Result<string, Error>`

Replaces every occurrence of needle. Error(InvalidArgument) on empty needle.

## [reverse](reverse/)

**Signature:** `reverse(s: string) -> string`

Reverses byte order. Grapheme-cluster reversal is future work.

## [send](send/)

**Signature:** `send(channel: Channel<t0>, value: t0) -> Unit`

Sends a value to a channel. Returns 1 for success, 0 for failure.

## [sleep](sleep/)

**Signature:** `sleep(milliseconds: int) -> Unit`

Pauses execution for the specified number of milliseconds.

## [spawnProcess](spawnprocess/)

**Signature:** `spawnProcess(command: string, callback: any) -> int`

Spawns an external async process with MANDATORY callback for stdout/stderr capture. The callback function receives (processID: int, eventType: int, data: string) and is called for stdout (1), stderr (2), and exit (3) events. Returns a handle for the running process. CALLBACK IS REQUIRED - NO FUNCTION OVERLOADING!

## [split](split/)

**Signature:** `split(s: string, separator: string) -> Result<List<string>, Error>`

Splits s on separator. Error(InvalidArgument) on empty separator.

## [startsWith](startswith/)

**Signature:** `startsWith(s: string, prefix: string) -> bool`

True if s begins with prefix.

## [substring](substring/)

**Signature:** `substring(s: string, start: int, end: int) -> Result<string, Error>`

Extracts s[start, end). Returns Error(IndexOutOfRange) if start<0, end>len, or start>end.

## [take](take/)

**Signature:** `take(s: string, n: int) -> string`

Returns at most the first n bytes of s. Clamps; never fails.

## [termClear](termclear/)

**Signature:** `termClear() -> int`

Clears the terminal screen.

## [termCols](termcols/)

**Signature:** `termCols() -> int`

Returns the terminal width in columns.

## [termHideCursor](termhidecursor/)

**Signature:** `termHideCursor() -> int`

Hides the terminal cursor.

## [termMoveCursor](termmovecursor/)

**Signature:** `termMoveCursor(row: int, col: int) -> int`

Moves the terminal cursor to the given row and column.

## [termRawMode](termrawmode/)

**Signature:** `termRawMode(enabled: int) -> Unit`

Enables (1) or disables (0) raw terminal input mode, so keypresses arrive unbuffered.

## [termReadKey](termreadkey/)

**Signature:** `termReadKey() -> Result<string, Error>`

Reads a single keypress from the terminal and returns it as a string.

## [termRows](termrows/)

**Signature:** `termRows() -> int`

Returns the terminal height in rows.

## [termShowCursor](termshowcursor/)

**Signature:** `termShowCursor() -> int`

Shows the terminal cursor.

## [test](test/)

**Signature:** `test(name: string, body: () -> t0) -> Unit`

Runs `body` as one named test case and prints a TAP result line. A case fails when any assertion inside it fails; the program exits non-zero if any case failed.

## [toLowerCase](tolowercase/)

**Signature:** `toLowerCase(s: string) -> string`

ASCII-aware lowercase.

## [toString](tostring/)

**Signature:** `toString(value: any) -> string`

Converts a value to its string representation.

## [toUpperCase](touppercase/)

**Signature:** `toUpperCase(s: string) -> string`

ASCII-aware uppercase. Unicode simple case mapping is a future addition.

## [trim](trim/)

**Signature:** `trim(s: string) -> string`

Removes leading and trailing whitespace.

## [trimEnd](trimend/)

**Signature:** `trimEnd(s: string) -> string`

Removes trailing whitespace.

## [trimStart](trimstart/)

**Signature:** `trimStart(s: string) -> string`

Removes leading whitespace.

## [websocketClose](websocketclose/)

**Signature:** `websocketClose(wsID: int) -> Unit`

Closes the WebSocket connection and cleans up resources. *(Implementation note: currently returns an integer status code; the `Result`-typed API shown in the signature is planned.)*

## [websocketConnect](websocketconnect/)

**Signature:** `websocketConnect(url: string) -> int`

Connects to a WebSocket server at the given URL and returns a connection id.

## [websocketCreateServer](websocketcreateserver/)

**Signature:** `websocketCreateServer(port: int, address: string, path: string) -> int`

Creates a WebSocket server bound to the specified port, address, and path. *(Implementation note: currently returns an integer status code; the `Result`-typed API shown in the signature is planned.)*

## [websocketKeepAlive](websocketkeepalive/)

**Signature:** `websocketKeepAlive() -> Unit`

Keeps the WebSocket server running indefinitely until interrupted (blocking operation). *(Implementation note: currently returns an integer status code; the `Result`-typed API shown in the signature is planned.)*

## [websocketSend](websocketsend/)

**Signature:** `websocketSend(wsID: int, message: string) -> int`

Sends a message through the WebSocket connection. *(Implementation note: currently returns an integer status code; the `Result`-typed API shown in the signature is planned.)*

## [websocketServerBroadcast](websocketserverbroadcast/)

**Signature:** `websocketServerBroadcast(serverID: int, message: string) -> int`

Broadcasts a message to all connected WebSocket clients. *(Implementation note: currently returns an integer status code; the `Result`-typed API shown in the signature is planned.)*

## [websocketServerListen](websocketserverlisten/)

**Signature:** `websocketServerListen(serverID: int) -> int`

Starts the WebSocket server listening for connections. *(Implementation note: currently returns an integer status code; the `Result`-typed API shown in the signature is planned.)*

## [words](words/)

**Signature:** `words(s: string) -> List<string>`

Splits on runs of whitespace; empty results dropped.

## [writeFile](writefile/)

**Signature:** `writeFile(filename: string, content: string) -> Result<Unit, Error>`

Writes content to a file. Creates the file if it doesn't exist. Returns number of bytes written.

## [yield](yield/)

**Signature:** `yield() -> Unit`

Yields control from the current fiber, letting other ready fibers run.


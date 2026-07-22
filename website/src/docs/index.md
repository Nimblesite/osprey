---
layout: page
title: "API Reference"
description: "Complete reference documentation for the Osprey programming language"
---

## Flavors

Osprey is **one language** you can write **two different ways**. Both compile to the exact
same program and run identically — only the way you type the code differs. Pick whichever
you prefer, per file, by extension.

- **Default** (`.osp`) — C-style braces. `fn f(a, b) = …`, `let x = v`, `f(x)` calls,
  `{ }` blocks. Familiar from C, Rust, Swift, or TypeScript.
- **ML** (`.ospml`) — offside (indentation) layout, curry-by-default. No `fn`/`let`,
  whitespace application (`f x`), `\x => e` lambdas, `:=` for mutation. Reads like
  OCaml, F#, or Haskell.

The same program, both flavors — a union type, a function that matches on it, a
binding, and interpolated output. Both compile to identical IR:

```osprey
type Shape = Circle | Square

fn area(s, size) = match s {
    Circle => size * size * 3
    Square => size * size
}

let total = area(Circle, 4) + area(Square, 2)
print("total: ${total}")
```

```osprey-ml
type Shape =
    Circle
    Square

area (s, size) =
    match s
        Circle => size * size * 3
        Square => size * size

total = area (Circle, 4) + area (Square, 2)
print "total: ${total}"
```

## Quick Navigation

- [Web Apps](web-apps/) - Build React-rendered browser apps with an Osprey WebAssembly model/update core
- [Functions](functions/) - Built-in functions for I/O, iteration, and data transformation
- [Types](types/) - Built-in data types (Int, String, Bool, Any)
- [Operators](operators/) - Arithmetic, comparison, and logical operators
- [Keywords](keywords/) - Language keywords (fn, let, type, match, import)

## Function Reference

| Function | Description |
|----------|-------------|
| [Channel](functions/channel/) | Creates a new channel with the specified capacity. |
| [List](functions/list/) | Creates a new empty list. |
| [Map](functions/map/) | Creates a new empty map. |
| [awaitProcess](functions/awaitprocess/) | Waits for a spawned process to complete and returns its exit code. Blocks until the process finishes. |
| [cleanupProcess](functions/cleanupprocess/) | Cleans up resources associated with a completed process. Should be called after awaitProcess. |
| [contains](functions/contains/) | True if needle appears anywhere in s. Empty needle returns true. |
| [drop](functions/drop/) | Returns s without its first n bytes. Clamps; never fails. |
| [endsWith](functions/endswith/) | True if s ends with suffix. |
| [fiber_yield](functions/fiber_yield/) | Yields control to the fiber scheduler with an optional value. |
| [filter](functions/filter/) | Filters elements in an iterator based on a predicate function. |
| [fold](functions/fold/) | Reduces an iterator to a single value by repeatedly applying a function. |
| [forEach](functions/foreach/) | Applies a function to each element in an iterator. |
| [forEachList](functions/foreachlist/) | Apply function to every element of list. Phase 7 of collections plan. |
| [httpCloseClient](functions/httpcloseclient/) | Closes the HTTP client and cleans up resources. |
| [httpCreateClient](functions/httpcreateclient/) | Creates an HTTP client for making requests to a base URL. |
| [httpCreateServer](functions/httpcreateserver/) | Creates an HTTP server bound to the specified port and address. |
| [httpDelete](functions/httpdelete/) | Makes an HTTP DELETE request to the specified path. |
| [httpGet](functions/httpget/) | Makes an HTTP GET request to the specified path. |
| [httpListen](functions/httplisten/) | Starts the HTTP server listening for requests with a handler function. |
| [httpPost](functions/httppost/) | Makes an HTTP POST request with a request body. |
| [httpPut](functions/httpput/) | Makes an HTTP PUT request with a request body. |
| [httpStopServer](functions/httpstopserver/) | Stops the HTTP server and closes all connections. |
| [indexOf](functions/indexof/) | Returns byte-index of first occurrence of needle, or Error(NotFound). |
| [input](functions/input/) | Reads a string from the user's input. |
| [isEmpty](functions/isempty/) | True if string has zero length. |
| [join](functions/join/) | Concatenates parts with separator between each pair. |
| [length](functions/length/) | Returns the byte length of a string. Total — never fails. |
| [lines](functions/lines/) | Splits on '\n'. A trailing newline does not produce an empty entry. |
| [listAppend](functions/listappend/) | Returns a new list with value at the end. O(log32 n) amortised. |
| [listConcat](functions/listconcat/) | Returns left ++ right. Same as left + right. |
| [listContains](functions/listcontains/) | True iff some element equals value. O(n). |
| [listLength](functions/listlength/) | Returns the number of elements in a list. O(1). |
| [listPrepend](functions/listprepend/) | Returns a new list with value at the front. O(n). |
| [listReverse](functions/listreverse/) | Returns a new list in reverse order. |
| [map](functions/map/) | Transforms each element in an iterator using a function, returning a new iterator. |
| [mapContains](functions/mapcontains/) | True iff key is present in map. |
| [mapKeys](functions/mapkeys/) | All keys of the map as a list. Order unspecified. |
| [mapLength](functions/maplength/) | Returns the number of entries in a map. O(1). |
| [mapMerge](functions/mapmerge/) | Right-biased union. Same as left + right. |
| [mapRemove](functions/mapremove/) | Returns a new map without key. No-op if key is absent. |
| [mapSet](functions/mapset/) | Returns a new map with key bound to value (replaces prior binding). |
| [mapValues](functions/mapvalues/) | All values of the map as a list. Order matches mapKeys. |
| [padEnd](functions/padend/) | Pads s on the right with copies of fill to reach targetLength bytes. |
| [padStart](functions/padstart/) | Pads s on the left with copies of fill to reach targetLength bytes. |
| [parseFloat](functions/parsefloat/) | Strict base-10 floating-point parser. No whitespace tolerance. |
| [parseInt](functions/parseint/) | Strict base-10 signed-int parser. No whitespace tolerance. |
| [print](functions/print/) | Prints a value to the console. Automatically converts the value to a string representation. |
| [range](functions/range/) | Creates an iterator that generates numbers from start to end (exclusive). |
| [readFile](functions/readfile/) | Reads the entire contents of a file as a string. |
| [recv](functions/recv/) | Receives a value from a channel. |
| [repeat](functions/repeat/) | Concatenates s with itself n times. Error(InvalidArgument) on negative n. |
| [replace](functions/replace/) | Replaces every occurrence of needle. Error(InvalidArgument) on empty needle. |
| [reverse](functions/reverse/) | Reverses byte order. Grapheme-cluster reversal is future work. |
| [send](functions/send/) | Sends a value to a channel. Returns 1 for success, 0 for failure. |
| [sleep](functions/sleep/) | Pauses execution for the specified number of milliseconds. |
| [spawnProcess](functions/spawnprocess/) | Spawns an external async process with MANDATORY callback for stdout/stderr capture. The callback function receives (processID: int, eventType: int, data: string) and is called for stdout (1), stderr (2), and exit (3) events. Returns a handle for the running process. CALLBACK IS REQUIRED - NO FUNCTION OVERLOADING! |
| [split](functions/split/) | Splits s on separator. Error(InvalidArgument) on empty separator. |
| [startsWith](functions/startswith/) | True if s begins with prefix. |
| [substring](functions/substring/) | Extracts s[start, end). Returns Error(IndexOutOfRange) if start<0, end>len, or start>end. |
| [take](functions/take/) | Returns at most the first n bytes of s. Clamps; never fails. |
| [toLowerCase](functions/tolowercase/) | ASCII-aware lowercase. |
| [toString](functions/tostring/) | Converts a value to its string representation. |
| [toUpperCase](functions/touppercase/) | ASCII-aware uppercase. Unicode simple case mapping is a future addition. |
| [trim](functions/trim/) | Removes leading and trailing whitespace. |
| [trimEnd](functions/trimend/) | Removes trailing whitespace. |
| [trimStart](functions/trimstart/) | Removes leading whitespace. |
| [websocketClose](functions/websocketclose/) | Closes the WebSocket connection and cleans up resources. |
| [websocketConnect](functions/websocketconnect/) | Establishes a WebSocket connection with a message handler callback. |
| [websocketCreateServer](functions/websocketcreateserver/) | Creates a WebSocket server bound to the specified port, address, and path. |
| [websocketKeepAlive](functions/websocketkeepalive/) | Keeps the WebSocket server running indefinitely until interrupted (blocking operation). |
| [websocketSend](functions/websocketsend/) | Sends a message through the WebSocket connection. |
| [websocketServerBroadcast](functions/websocketserverbroadcast/) | Broadcasts a message to all connected WebSocket clients. |
| [websocketServerListen](functions/websocketserverlisten/) | Starts the WebSocket server listening for connections. |
| [words](functions/words/) | Splits on runs of whitespace; empty results dropped. |
| [writeFile](functions/writefile/) | Writes content to a file. Creates the file if it doesn't exist. Returns number of bytes written. |

## Type Reference

| Type | Description |
|------|-------------|
| [Any](types/any/) | A type that can represent any value. Useful for generic programming but should be used carefully as it bypasses type checking. |
| [Bool](types/bool/) | A boolean type that can be either true or false. Used for logical operations and conditionals. |
| [HttpResponse](types/httpresponse/) | A built-in type representing an HTTP response with status code, headers, content type, body, and streaming capabilities. Used by HTTP server handlers to return structured responses to clients. |
| [Int](types/int/) | A 64-bit signed integer type. Can represent whole numbers from -9,223,372,036,854,775,808 to 9,223,372,036,854,775,807. |
| [ProcessHandle](types/processhandle/) | A handle to a spawned async process. Contains the process ID and allows waiting for completion and cleanup. Process output is delivered via callbacks registered with the runtime. |
| [String](types/string/) | A sequence of characters representing text. Supports string interpolation and escape sequences. |

## Operator Reference

| Operator | Name | Description |
|----------|------|-------------|
| [!=](operators/not-equal/) | Inequality | Compares two values for inequality. |
| [%](operators/modulo/) | Modulo | Returns the remainder of dividing the first number by the second. |
| [*](operators/multiply/) | Multiplication | Multiplies two numbers. |
| [+](operators/plus/) | Addition | Adds two numbers together. |
| [-](operators/minus/) | Subtraction | Subtracts the second number from the first. |
| [/](operators/divide/) | Division | Divides the first number by the second. |
| [<](operators/less-than/) | Less Than | Checks if the first value is less than the second. |
| [<=](operators/less-equal/) | Less Than or Equal | Checks if the first value is less than or equal to the second. |
| [==](operators/equal/) | Equality | Compares two values for equality. |
| [>](operators/greater-than/) | Greater Than | Checks if the first value is greater than the second. |
| [>=](operators/greater-equal/) | Greater Than or Equal | Checks if the first value is greater than or equal to the second. |
| [|>](operators/pipe-operator/) | Pipe Operator | Takes the result of the left expression and passes it as the first argument to the right function. Enables functional programming and method chaining. |

## Keyword Reference

| Keyword | Description |
|---------|-------------|
| [false](keywords/false/) | Boolean literal representing the logical value false. |
| [fn](keywords/fn/) | Function declaration keyword. Used to define functions with parameters and return types. |
| [import](keywords/import/) | Import declaration keyword. Used to bring modules and their exports into the current scope. |
| [let](keywords/let/) | Variable declaration keyword. Used to bind values to identifiers. Variables are immutable by default in Osprey. |
| [match](keywords/match/) | Pattern matching expression. Used for destructuring values and control flow based on patterns. |
| [true](keywords/true/) | Boolean literal representing the logical value true. |
| [type](keywords/type/) | Type declaration keyword. Used to define custom types and type aliases. |

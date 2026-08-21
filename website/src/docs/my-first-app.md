---
layout: page
permalink: /docs/my-first-app/
title: My First App
description: Build a small native Osprey CLI that models JSON as a recursive algebraic data type, reads a JSON file, validates it, updates it, and writes it back.
mlTwins: false
tags:
- getting-started
- cli
- json
- algebraic-data-types
---

Build a native command-line app that remembers how many times it has run. It reads `first-app.json`, turns the file into typed Osprey data, advances the state, and writes valid JSON back.

The important part is the model. JSON is not `any`, and it is not a map whose values are all strings. JSON has six possible value shapes, so the program represents those shapes with one recursive algebraic data type.

You need a working native installation. Complete [Installing Osprey](/docs/installation/) first if `osprey --version` does not work.

## Create the app

Make a directory and an empty Default-flavor source file:

```sh
mkdir my-first-app
cd my-first-app
touch app.osp
```

Add each code block in this guide to `app.osp`, in order.

## Model JSON as data

Start with the possible shapes of a JSON value:

```osprey
type JsonValue =
    JsonNull
    | JsonBoolean { value: bool }
    | JsonNumber { value: float }
    | JsonString { value: string }
    | JsonArray { values: List<JsonValue> }
    | JsonObject { fields: Map<string, JsonValue> }
```

```typediagram
typeDiagram
union JsonValue {
  JsonNull
  JsonBoolean { value: Bool }
  JsonNumber { value: Float }
  JsonString { value: String }
  JsonArray { values: List<JsonValue> }
  JsonObject { fields: Map<String, JsonValue> }
}
```

This is a tagged union, also called a sum type. A `JsonValue` is exactly one case at a time. Arrays contain more `JsonValue` values, and objects map string keys to more `JsonValue` values, which makes the union recursive.

There is no invalid half-state such as a value claiming to be both a number and a boolean. A `match` over `JsonValue` must account for all six cases.

## Encode every case

JSON is text on disk, but it stays structured data inside the program. The encoder is the one place that crosses from `JsonValue` to `string`:

```osprey
fn escapeJson(source) = {
    let slashes = replace(source, "\\", "\\\\") ?: source
    let quotes = replace(slashes, "\"", "\\\"") ?: slashes
    let newlines = replace(quotes, "\n", "\\n") ?: quotes
    let returns = replace(newlines, "\r", "\\r") ?: newlines
    replace(returns, "\t", "\\t") ?: returns
}

fn quoteJson(source) = "\"${escapeJson(source)}\""

fn encodeArray(values: List<JsonValue>) -> string = match values {
    [] => ""
    [only] => encodeJson(only)
    [head, ...tail] => "${encodeJson(head)},${encodeArray(tail)}"
}

fn encodeFields(fields: Map<string, JsonValue>, keys: List<string>) -> string = match keys {
    [] => ""
    [only] => "${quoteJson(only)}:${encodeJson(mapGet(fields, only) ?: JsonNull)}"
    [head, ...tail] => "${quoteJson(head)}:${encodeJson(mapGet(fields, head) ?: JsonNull)},${encodeFields(fields, tail)}"
}

fn encodeObject(fields: Map<string, JsonValue>) -> string = encodeFields(fields, mapKeys(fields))

fn encodeJson(value: JsonValue) -> string = match value {
    JsonNull => "null"
    JsonBoolean { value } => match value { true => "true" false => "false" }
    JsonNumber { value } => toString(value)
    JsonString { value } => quoteJson(value)
    JsonArray { values } => "[${encodeArray(values)}]"
    JsonObject { fields } => "{${encodeObject(fields)}}"
}
```

The annotations on the three recursive functions are useful rather than decorative: they give the compiler a fixed boundary for the mutually recursive encoder. Ordinary non-recursive functions continue to rely on inference.

`mapKeys` does not promise object-field order. That is fine because JSON object order has no meaning; two runs may produce semantically identical objects with a different field order.

This small encoder escapes every control character used by this app: quotes, backslashes, newlines, carriage returns, and tabs. A general-purpose codec for arbitrary untrusted strings must also escape every remaining character below U+0020. Keep that qualification beside the example instead of treating a tutorial codec as a complete serialization library.

## Build the first document

The application document deliberately uses every JSON case. Its title is a string, `runs` is a number, `ready` is a boolean, `tags` is an array, `lastError` is null, and the root is an object.

```osprey
fn appDocument(title, runs, ready, tags, lastError) = JsonObject {
    fields: Map()
        |> mapSet("title", title)
        |> mapSet("runs", runs)
        |> mapSet("ready", ready)
        |> mapSet("tags", tags)
        |> mapSet("lastError", lastError)
}

fn defaultDocument() = appDocument(
    JsonString { value: "My First App" },
    JsonNumber { value: 0.0 },
    JsonBoolean { value: false },
    JsonArray { values: [
        JsonString { value: "cli" },
        JsonString { value: "adt" }
    ] },
    JsonNull
)
```

Notice what is absent: there is no `Map<string, any>`, no stringly boolean such as `"true"`, and no magic null sentinel. The constructors retain what each value means.

## Decode at the boundary

The current `jsonParse` built-in validates JSON and returns an opaque document handle. `jsonGet` projects a scalar at a known path, while `jsonLength` reports the size of an array or object. Those runtime functions do not fabricate a typed application model for us, so the decoder reconstructs `JsonValue` explicitly.

First decode individual scalar cases and the `tags` array:

```osprey
fn decodeString(documentId, path) = match jsonGet(documentId, path) {
    Success { value } => {
        let decoded = JsonString { value: value }
        Success { value: decoded }
    }
    Error { message } => Error { message: message }
}

fn decodeNumber(documentId, path) = match jsonGet(documentId, path) {
    Error { message } => Error { message: message }
    Success { value } => match parseFloat(value) {
        Success { value } => {
            let decoded = JsonNumber { value: value }
            Success { value: decoded }
        }
        Error { message } => Error { message: message }
    }
}

fn decodeBoolean(documentId, path) = match jsonGet(documentId, path) {
    Error { message } => Error { message: message }
    Success { value } => match value {
        "true" => {
            let decoded = JsonBoolean { value: true }
            Success { value: decoded }
        }
        "false" => {
            let decoded = JsonBoolean { value: false }
            Success { value: decoded }
        }
        _ => Error { message: "${path} must be a JSON boolean" }
    }
}

fn decodeNull(documentId, path) = match jsonGet(documentId, path) {
    Error { message } => Error { message: message }
    Success { value } => match value {
        "null" => Success { value: JsonNull }
        _ => Error { message: "${path} must be null" }
    }
}

fn decodeTagsFrom(documentId, index, count, decoded) -> Result<JsonValue, Error> = match index == count {
    true => {
        let array = JsonArray { values: decoded }
        Success { value: array }
    }
    false => match decodeString(documentId, "tags[${index}]") {
        Error { message } => Error { message: message }
        Success { value } => decodeTagsFrom(
            documentId,
            index + 1 ?: count,
            count,
            listAppend(decoded, value)
        )
    }
}

fn decodeTags(documentId) = {
    let count = jsonLength(documentId, "tags")
    match count >= 0 {
        true => decodeTagsFrom(documentId, 0, count, List())
        false => Error { message: "tags must be a JSON array" }
    }
}
```

Each decoder returns `Result`. A missing field, malformed number, wrong boolean, or non-array `tags` value remains a visible failure rather than becoming an invented default.

Now compose those small decoders into the application schema and release the runtime document handle after rebuilding the ADT:

```osprey
fn decodeDocument(documentId) = match decodeString(documentId, "title") {
    Error { message } => Error { message: message }
    Success { value } => {
        let title = value
        match decodeNumber(documentId, "runs") {
            Error { message } => Error { message: message }
            Success { value } => {
                let runs = value
                match decodeBoolean(documentId, "ready") {
                    Error { message } => Error { message: message }
                    Success { value } => {
                        let ready = value
                        match decodeTags(documentId) {
                            Error { message } => Error { message: message }
                            Success { value } => {
                                let tags = value
                                match decodeNull(documentId, "lastError") {
                                    Error { message } => Error { message: message }
                                    Success { value } => Success {
                                        value: appDocument(title, runs, ready, tags, value)
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn parseDocument(source) = match jsonParse(source) {
    Error { message } => Error { message: message }
    Success { value } => {
        let decoded = decodeDocument(value)
        let released = jsonFree(value) ?: 0
        decoded
    }
}
```

The nesting is honest. Every boundary operation can fail, and this first version shows each decision directly. A larger program would extract a reusable validation combinator, but it should preserve the same `Result` behavior.

## Update and save

`advance` accepts the ADT, narrows the object and number cases, and creates a new persistent map. The old document is unchanged.

```osprey
fn advance(document) = match document {
    JsonObject { fields } => match mapGet(fields, "runs") {
        Error { message } => Error { message: message }
        Success { value } => match value {
            JsonNumber { value } => Success { value: JsonObject {
                fields: fields
                    |> mapSet("runs", JsonNumber { value: value + 1.0 })
                    |> mapSet("ready", JsonBoolean { value: true })
                    |> mapSet("lastError", JsonNull)
            } }
            _ => Error { message: "runs must be a JSON number" }
        }
    }
    _ => Error { message: "the document root must be a JSON object" }
}

fn load(path) = match readFile(path) {
    Success { value } => parseDocument(value)
    Error { message } => Success { value: defaultDocument() }
}

fn main() = {
    let path = "first-app.json"
    match load(path) {
        Error { message } => print("Could not load ${path}: ${message}")
        Success { value } => match advance(value) {
            Error { message } => print("Could not update ${path}: ${message}")
            Success { value } => match writeFile(path, "${encodeJson(value)}\n") {
                Error { message } => print("Could not save ${path}: ${message}")
                Success { value } => print("Saved ${path} (${value} bytes)")
            }
        }
    }
}
```

For this first-run experience, any `readFile` failure starts from `defaultDocument`. That keeps the initial launch portable, but it is a deliberately small policy. A production CLI should distinguish a missing file from permission, device, and other I/O failures before deciding to create new state.

## Run it twice

Check the program before running it:

```sh
osprey app.osp --check
osprey app.osp --run
```

The first run creates the file and reports the encoded byte count:

```text
Saved first-app.json (87 bytes)
```

Run it again, then open `first-app.json`. The `runs` value advances from `1.0` to `2.0` while the other cases survive the round trip:

```json
{"tags":["cli","adt"],"title":"My First App","ready":true,"runs":2.0,"lastError":null}
```

Your object fields may appear in another order. The data is the same JSON object.

You now have a native CLI with a narrow file boundary, visible failures, persistent updates, and a recursive ADT that says exactly what JSON can be. From here, add a command-line argument, introduce another field, or replace the first-run fallback with a stricter file policy—then run `--check` before trusting the change.

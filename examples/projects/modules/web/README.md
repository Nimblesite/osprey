# Osprey React host

This package is the deliberately small browser runtime for the Talon Bank
example. Osprey owns the application model, update function, view document and
effects. React only reconciles the JSON view document with the DOM; no React
component tree is hand-authored for the bank.

`npm run build` performs the complete pipeline:

1. bundles and minifies React, the host, and the design system with esbuild;
2. compiles `../client` to `build/talon-client.wasm` with the repository's
   release Osprey compiler (override with `OSPREY_BIN`);
3. embeds the wasm bytes into the browser bundle as base64; and
4. generates `../src/web/bundle.ospml`, exporting zero-argument
   `bank/web::Bundle::style()` and `bank/web::Bundle::script()` functions. They
   return raw CSS and JavaScript for the server's `/app.css` and `/app.js`
   routes; the assets contain no HTML wrapper tags.

The esbuild target explicitly lowers JavaScript template literals because
`${...}` is Osprey's own interpolation syntax. The embed step rejects that
sequence as a guard against generating an ambiguous Osprey source file.

Use `npm run build:host` while changing only JavaScript or CSS. Use
`OSPREY_WEB_WASM=/absolute/client.wasm npm run build -- --no-compile` to embed an
already compiled module.

## Bridge protocol

The wasm module exports `memory`, `_start`, `osp_alloc(i64)`, and
`osprey_web_dispatch(ptr)`. The host supplies WASI Preview 1 and these imports:

- `osprey_web.render(ptr)` reads one UTF-8, NUL-terminated envelope;
- `osprey_web.command(ptr)` reads one command or an array of commands.

An envelope is `{model, view, commands}`. `model` is an opaque JSON string and
is returned unchanged as the scalar `model` field on every dispatch. A view
node is `{tag, text?, props?, children?}`. `props.event` may be `click`, `input`,
`change`, or `submit`; the node's `id`, `name`, and current value become a flat
event object. Submit data is a JSON string in the scalar `data` field.

Commands are `{kind:"http",id,method,url,body}`, `{kind:"focus",id}`, and
`{kind:"navigate",url}` (`type` is accepted as an alias for `kind`). Browser
back/forward and deep-link startup dispatch `{kind:"location",value:hash}`.
HTTP completion dispatches
`{kind:"http",id,status,data,model}`, where `data` is the unmodified response
text. All Osprey/JavaScript crossings are whole-document messages, never
per-node calls.

For diagnostics and end-to-end assertions the host exposes
`window.__TALON_BRIDGE__ = {ready,renders,events,lastPayloadBytes,lastDecodeMs}`.

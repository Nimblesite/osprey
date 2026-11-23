# The browser host

This host runs any [Osprey Native](../../../osprey/packages/native/README.md) application in a web browser ([NATIVE-HOSTS-FAMILY](../../../../docs/specs/applications/0047-OspreyNativeHosts.md)). The application is compiled to WebAssembly and runs on the page. Each frame it writes is drawn as HTML, and each control sends a typed event back. Nothing here names an application.

```sh
node src/toolchain/hosts/web/scripts/build.mjs --app <application folder> --title <title> --out <folder> \
    [--argument <launch argument>]
node src/toolchain/hosts/web/scripts/serve.mjs --root <folder> --port <port>
```

The build compiles the application with `--target=wasm32` and writes `index.html`, `host.js` (the host with the application inlined) and `host.css` to `--out`. Serve that folder from any web server; `serve.mjs` serves it on this machine. They are build output, never committed. The application gets the page's origin as its first launch argument; `--argument` names another, such as the API server [Talon Bank](../../../osprey/examples/talon/README.md) calls.

## How it works

| File | Role |
| --- | --- |
| [`src/main.ts`](src/main.ts) | Loads the bridge, then the webview client the VS Code panel uses |
| [`src/bridge.ts`](src/bridge.ts) | Answers the client's messages on the page: `ready` opens the application with the page's origin as its first launch argument |
| [`src/session.ts`](src/session.ts) | Runs the instance one call at a time through the shared session loop and draws each frame with the shared renderer |
| [`src/http.ts`](src/http.ts) | The `http` capability over `fetch`: HTTPS, or HTTP to the page's origin or this machine, with a deadline and a response limit |
| [`src/wasi.ts`](src/wasi.ts) | The WASI imports the application needs: the clock, random bytes and the console |

The boundary calls, the session loop, the renderer and the webview client live in [`vscode-extension/client/src/osprey-ui`](../../vscode-extension/client/src/osprey-ui) and are shared with the VS Code panel ([`instance.ts`](../../vscode-extension/client/src/osprey-ui/instance.ts) holds the boundary calls). This host adds only the browser's WASI, its `fetch` and the page. It draws screens without the Osprey lockup the VS Code panel shows.

The host answers `http` only. Any other capability completes with an error naming it, so an application that needs files or processes says so on the page instead of failing silently.

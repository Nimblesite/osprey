# WebSocket server demo

Build the compiler and runtime, then start the Osprey server:

```bash
make build
target/release/osprey examples/websocketserver/osprey_websocket_server.osp --run
```

Open `examples/websocketserver/websocket_test.html` in a browser. It connects to
`ws://127.0.0.1:54321/chat`, displays the runtime's welcome frame, and can send
text messages to the echo loop. Stop the server with Ctrl-C.

The language API and framing limits are specified in
[`docs/specs/0015-WebSockets.md`](../../docs/specs/0015-WebSockets.md).

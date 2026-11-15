# Express port: licensing and acknowledgement

This package is a port of [Express](https://github.com/expressjs/express), pinned to 5.3.0. Express is MIT licensed, Copyright (c) 2009-2014 TJ Holowaychuk, Copyright (c) 2013-2014 Roman Shtylman, Copyright (c) 2014-2015 Douglas Christopher Wilson. We aim not to copy its JavaScript code verbatim, but a port inevitably resembles the original, so Express's copyright and permission notice is retained in [LICENSES/Express-MIT.txt](LICENSES/Express-MIT.txt) and applies to everything here derived from Express.

`examples/hello-world/index.ts` is the example application from the README of Express 5.3.0, reproduced byte for byte under that same notice.

`oracle/upstream/web-service/index.js` is `examples/web-service/index.js` from Express 5.3.0, reproduced byte for byte under that same notice. It is the reference the port's `examples/web-service` is compared with, and `examples/web-service/service.ts` is a port of it that keeps its comments.

`oracle/upstream/json-api/index.js` is an original reference application written for this port, not taken from Express; it is covered by this package's own license.

`src/query.ts` follows the documented behaviour of `querystring.parse` in [Node.js](https://github.com/nodejs/node), which is MIT licensed, Copyright Node.js contributors. No Node.js source is copied.

The Osprey implementation is licensed under MIT, copyright NIMBLESITE PTY LTD, Australia; see [LICENSE](LICENSE). This does not replace or claim ownership of Express's rights.

Express is not a dependency of compiled Osprey applications. Development oracle runs use the separately installed upstream distribution under its own license.

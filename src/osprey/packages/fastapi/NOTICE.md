# FastAPI port: licensing and acknowledgement

This package is a port of [FastAPI](https://github.com/fastapi/fastapi), pinned to 0.135.3, the version in NimblesiteAgenticPlatform's uv.lock. FastAPI is MIT licensed, Copyright (c) 2018 Sebastián Ramírez. We aim not to copy its Python code verbatim, but a port inevitably resembles the original, so FastAPI's copyright and permission notice is retained in [LICENSES/FastAPI-MIT.txt](LICENSES/FastAPI-MIT.txt) and applies to everything here derived from FastAPI.

The Osprey implementation is licensed under MIT, copyright NIMBLESITE PTY LTD, Australia; see [LICENSE](LICENSE). This does not replace or claim ownership of FastAPI's rights.

FastAPI is not a dependency of compiled Osprey applications. Development oracle runs use the separately installed upstream distribution under its own license.

CORS policy handling in `src/applications.py` adapts the behavior and decision structure of [Starlette 1.3.1 CORSMiddleware](https://github.com/Kludex/starlette/blob/1.3.1/starlette/middleware/cors.py), copyright © 2018 Encode OSS Ltd. Its BSD-3-Clause copyright, conditions and disclaimer are retained in [LICENSES/Starlette-BSD-3-Clause.txt](LICENSES/Starlette-BSD-3-Clause.txt); reproduce this notice with source and binary distributions containing that adapted implementation. NIMBLESITE PTY LTD's MIT license covers its own contributions and does not replace those conditions.

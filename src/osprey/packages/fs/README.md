# @osprey/fs

Directory walking and path handling for Osprey, over the `listDir` runtime
builtin ([BUILTIN-FILE-LISTDIR]).

- `Path`: central path handling with `join`, `dirOf`, `nameOf`, `slug`, `under`, `resolve` and
  `isDir`. It holds the one definition of separator handling, so no other module
  spells `"/"`.
- `Walk`: `tree root` returns every file under a directory, sorted, as
  `Result<List<string>, Error>`, recursing through the `/`-marked directory
  entries `listDir` returns. A directory that cannot be listed fails the walk
  with its error; it never reads as empty.

Platforms: macOS and Linux via `dirent`/`stat`, Windows via MinGW's
`dirent`/`stat` shims. On wasm32-wasip1 `listDir` refuses, so `tree` returns
that `Error`.

Tests: `tests/`, run through the package coverage gate.

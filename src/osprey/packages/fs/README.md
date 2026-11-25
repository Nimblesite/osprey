# @osprey/fs

Directory walking for Osprey, over the `listDir` runtime
builtin ([BUILTIN-FILE-LISTDIR]).

- Paths are the core library's: `pathJoin`, `pathParent`, `pathName`, `pathSlug`,
  `pathUnder`, `pathResolve`, `pathIsDir` and `pathPortable` come with every
  program ([BUILTIN-PATH]), so this package spells no separator of its own.
- `Walk`: `tree root` returns every file under a directory, sorted, as
  `Result<List<string>, string>`, recursing through the `/`-marked directory
  entries `listDir` returns. A directory that cannot be listed fails the walk
  with its error; it never reads as empty.

Platforms: macOS and Linux via `dirent`/`stat`, Windows via MinGW's
`dirent`/`stat` shims. On wasm32-wasip1 `listDir` refuses, so `tree` returns
that `Error`.

Tests: `tests/`, run through the package coverage gate.

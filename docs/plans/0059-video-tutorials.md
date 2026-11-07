# Plan 0059: Video Tutorials Teach the Code

A video in the [tutorials section](../../src/toolchain/website/src/docs/tutorials/index.md)
teaches someone to build or use something. It shows the real source, names the
package or example it comes from, explains how the code works, and runs it so
the viewer sees the effect. Short promotional films and app tours are useful,
but they are introductions, not tutorials, and they live on the feature's own
page.

Spec home: the website video rules in
[`src/toolchain/website/lib/videos.mjs`](../../src/toolchain/website/lib/videos.mjs)
and [`tests/videos.spec.js`](../../src/toolchain/website/tests/videos.spec.js).
The rules below go into the docs specs when this plan is implemented.

## Where things stand

- **There are eight application and tool tutorials**, and none of them meets this
  bar. Their lengths, measured with ffprobe:

  | Video | Length | What it shows |
  | --- | --- | --- |
  | `agent-native` | 1:20 | One app screenshot, panned. The agent is unfinished, so it has no tutorial (`[DOC-TUTORIAL-VIDEO-FINISHED]`). |
  | `package-manager` | 0:49 | A tour of the VS Code panel |
  | `static-typing-python` | 1:30 | A summary of the blog post |
  | `chromecast-from-mac` | 1:37 | A summary of the blog post |
  | `rust-ui-framework-without-rust` | 1:44 | A summary of the blog post |
  | `profiler` | 2:00 | The UI, with little code |
  | `ui-testing-by-label` | 2:08 | A summary of the blog post |
  | `swift-interop` | 3:14 | The closest to a tutorial, but under four minutes |

- **Videos are made in the Videos repository's factory.** A film's
  `film.mjs` reads its code excerpts from an Osprey checkout when it is
  published, so a renamed file stops the next publish. Nothing re-publishes a
  film when the code it quotes changes.
- **Nothing tests what a tutorial contains.** `videos.spec.js` checks the
  player, captions, chapters and structured data. It never checks length,
  that code is shown, or that the code shown still exists.
- **The tutorials index lists every `/docs/` page with a `video`**, so an
  introduction film becomes a tutorial just by having a watch page there.

## Rules

### `[DOC-TUTORIAL-VIDEO-KIND]` Tutorials and introductions are different kinds

Every entry in `src/_data/videos.json` has `"kind": "tutorial"` or
`"kind": "intro"`. The tutorials index lists only `tutorial` entries. An
`intro` plays on its feature page (for example `agent-native` on
[Osprey Native](../../src/toolchain/website/src/docs/osprey-native.md)) and has
no page under `/docs/tutorials/`. The proof films keep their own learning path
and are `intro` entries until they meet the tutorial rules.

### `[DOC-TUTORIAL-VIDEO-OSPREY]` A tutorial teaches Osprey

Tutorials exist to teach people Osprey: the language, its packages and its
tools. Each one is titled by the Osprey concept it teaches (effects for I/O,
type inference in the Python flavor, exports and externs, GPUI apps in
Osprey, testing by label, profiling) and uses an app only as the worked
example. A film about an app with no Osprey lesson is an `intro`.

### `[DOC-TUTORIAL-VIDEO-FINISHED]` Tutorials only for finished, working features

A tutorial teaches something a viewer can use today. A feature that is still
in progress, such as Osprey Agent, gets no tutorial and no film on the docs
home or install pages; at most a short `intro` on its own feature page.

### `[DOC-TUTORIAL-VIDEO-LENGTH]` A tutorial runs at least four minutes

A `tutorial` lasts at least 240 seconds and no more than 600 seconds (measured
`durationSeconds`). The minimum is a floor, not a target: length comes from
covering the code, never from padding or slowing the narration.

### `[DOC-TUTORIAL-VIDEO-CODE]` A tutorial shows the code it teaches

At least half of a tutorial's running time shows source code on screen at a
readable size (at least 28 px monospace at 1920x1080). Every code scene:

- shows the repository path of the file, such as
  `src/osprey/packages/chromecast/src/cast.py`, and the line range;
- highlights the lines the narration is explaining at that moment;
- shows code taken from the file at the recorded commit, never retyped or
  simplified.

### `[DOC-TUTORIAL-VIDEO-EXPLAIN]` The narration explains how the code works

For each code scene, the narration says what the highlighted code does, why it
is written that way (the effect, type or package API it uses), and what changes
when it runs. Reading the code aloud does not count. Each tutorial covers the
four steps below, in order, and has one chapter for each:

1. **Goal:** what the viewer will build or do, and the finished result
   (at most 30 seconds).
2. **Code:** a walk through the package or example source, in the order the
   program uses it.
3. **Run:** the real command, from `make help` or the page, and its real
   output or app window, with each visible effect tied back to the code that
   caused it.
4. **Change it:** one small edit the viewer can make, run again, with the
   different result shown.

### `[DOC-TUTORIAL-VIDEO-PACKAGES]` A tutorial points to the packages it uses

On screen and in the watch page's text, a tutorial links each package or
example it shows (its `src/osprey/packages/<name>` or `src/osprey/examples/<name>`
folder, and its `/docs/packages/<name>/` reference page where one exists). The
page gives the command that runs it.

### `[DOC-TUTORIAL-VIDEO-SOURCE]` Every video is built from checked-in source

Each video is a screen film in the Videos repository
(`projects/osprey/videos/<id>/`): `film.mjs` lists the scenes (code excerpts by
file and text, commands, app clips), `narration.json` holds the spoken lines,
and `film.source.html` draws the page. `./bin/factory film osprey/<id>`
renders the MP4, poster, captions and chapters. Captions come from the
narration text, so the two always match.

### `[DOC-TUTORIAL-VIDEO-FRESH]` A tutorial fails when its code drifts

The Videos factory checks each `tutorial` film against the Osprey checkout
when it publishes, and the website tests check what was published:

- every file it shows exists, and the shown lines match the file byte for
  byte, or the film is re-published;
- every command it runs is still a documented `make` target or CLI command;
- `durationSeconds`, the chapter list and the `[DOC-TUTORIAL-VIDEO-EXPLAIN]`
  chapters agree with the film's scenes.

### `[MEDIA-VIDEOS-REPO]` Videos are made in the Videos repository and committed here

The Videos repository (`github.com/MelbourneDeveloper/Videos`, `projects/osprey/`)
holds each film's source and the factory that renders it. Its `published/`
folder mirrors the paths this repository reads. `make media` copies that folder
from the Videos checkout next to this one (`../Videos`, or `VIDEOS_DIR=<path>`)
into this checkout, and the copied videos, posters and caption tracks are
committed here through Git LFS. CI, the deploy and releases read them from the
checkout: nothing downloads media at build time and no token is involved.

## Tutorials to make

Each tutorial below replaces the current video with the same id. Where the
current video is a good introduction, it stays as an `intro` on its feature
page.

### `package-manager`: add, audit and remove a package

Walks through the [`src/osprey/packages/manager`](../../src/osprey/packages/manager)
packages that the VS Code panel calls, and a project's `osprey.toml`:

- the `osprey.toml` dependency entry the panel writes;
- how audit warnings come from `license-audit.json` and `source-audit.json`;
- **Run:** add `@osprey/json`, then use `Doc` in code that compiles and runs;
- **Change it:** remove the package and show the compile error that appears.

### `static-typing-python`: types for Python without type hints

Uses [`src/osprey/examples/python`](../../src/osprey/examples/python) and a
package written in the Python flavor, such as
[`src/osprey/packages/chromecast`](../../src/osprey/packages/chromecast):

- how Hindley-Milner inference types a function with no annotations;
- the error a wrong call produces, and where it points;
- `interop.osp` calling Python, and what crosses the boundary;
- **Run:** compile and run `interop.osp`, then `run.py`;
- **Change it:** introduce a type mistake and fix it from the diagnostic.

### `chromecast-from-mac`: effects for I/O, with casting as the example

Walks through [`src/osprey/packages/chromecast/src`](../../src/osprey/packages/chromecast/src)
(`cast.py`, `media.py`, `wire.py`):

- device discovery, the connection, and the Cast message format in `wire.py`;
- loading media with `media.py`, and how failures come back as `Result`;
- the device double in `tests/device.py` that tests use instead of a TV;
- **Run:** `live/main.py` against a TV, with the TV shown;
- **Change it:** cast a different file, or pause and resume.

### `rust-ui-framework-without-rust`: GPUI apps written in Osprey

Walks through [`src/osprey/examples/gpui`](../../src/osprey/examples/gpui)
(`painting/main.ospml`, then `flock/main.ospml`) and
[`src/osprey/packages/native`](../../src/osprey/packages/native):

- how a view is declared as a value, and how state updates redraw it;
- how the GPUI host renders that view tree;
- **Run:** `make -C macos/tests painting-headless`, then
  the app itself;
- **Change it:** add a control, and see it in the window and the self-test.

### `ui-testing-by-label`: test an app through its accessibility tree

Walks through [`src/osprey/packages/uitest/src`](../../src/osprey/packages/uitest/src)
(`semantics.ospml`, `read.ospml`, `drive.ospml`) and an app's `self-test.json`:

- finding controls by label, not by position;
- driving a step and asserting on the tree it produces;
- **Run:** one app's headless self-test, with each step shown passing;
- **Change it:** rename a label and show the test failing with a clear
  message, then fix it.

### `profiler`: find the hot code and the leak

Uses [`src/osprey/examples/profiling`](../../src/osprey/examples/profiling)
(`busy.osp`, `words.osp`):

- which functions in `busy.osp` should be hot, and why;
- **Run:** profile it in VS Code, then match the flame graph and source heat to
  those functions;
- memory: run `words.osp` under ARC and read the leak report;
- **Change it:** make the hot function cheaper and profile again to show the
  difference.

### `swift-interop`: one Osprey app for the Mac and the iPhone

Extends the current video to the full four steps using
[`src/osprey/examples/macos`](../../src/osprey/examples/macos):

- `storage/src` (`main.ospml`, `disk.ospml`, `browse.ospml`, `view.ospml`) and
  the exported functions that become the C header;
- `OspreyStorage/StorageServices.swift` and `PhotoLibrary.swift`, and the
  `@_cdecl` functions Osprey imports;
- **Run:** `make -C macos storage`;
- **Change it:** add one export, call it from Swift, and see it in the app.

## Checklist

- [x] Mark `agent-native` as `kind: intro` and list no intro on the tutorials
      index (`[DOC-TUTORIAL-VIDEO-KIND]`). Every other entry still needs
      `"kind": "tutorial"`.
- [x] Move `agent-native` to an `intro` page on Osprey Native
      (`/docs/osprey-native/agent/`), with no tutorials page, and take it off
      the docs home and install pages (`[DOC-TUTORIAL-VIDEO-FINISHED]`).
- [x] Make every video in the Videos repository and copy its published files
      here with `make media`, committed through Git LFS (`[MEDIA-VIDEOS-REPO]`).
- [x] Every `videos.json` entry is `tutorial` or `intro`; a test checks the
      kinds and that every tutorial runs 240 to 600 seconds and is listed
      (`[DOC-TUTORIAL-VIDEO-KIND]`, `[DOC-TUTORIAL-VIDEO-LENGTH]`).
- [ ] Add the website tests for `[DOC-TUTORIAL-VIDEO-CODE]` (code-scene share
      and path labels) and `[DOC-TUTORIAL-VIDEO-FRESH]`.
- [x] Render six tutorials in the Videos repository, each with the code, a
      real run and one edit the tests catch: `chromecast-from-mac` (effects
      for I/O, 7:22), `ui-testing-by-label` (6:17), `rust-ui-framework-without-rust`
      (5:12), `static-typing-python` (4:51), `swift-interop` (4:21),
      `profiler` (4:17).
- [ ] Re-make `package-manager` (an `intro` until then; it needs a new shoot
      in VS Code).
- [x] Update each watch page with the package links and the commands it runs.
- [x] Re-measure durations with `make -C src/toolchain/website video-metadata`.
- [ ] Add the rules to the docs specs, and cite their IDs in the tests and
      the video script.

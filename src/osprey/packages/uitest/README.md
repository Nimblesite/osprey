# @osprey/uitest

Drive an Osprey Native app with no host and no window ([UI-MACHINE-TEST]). Find every control by its accessible label, as a screen reader or an AI agent would. Send host-shaped events and assert what the rendered view or canvas shows.

- `Drive` holds one live surface as a `Driven<S, A>`: the surface, its `ui::Program`, what it last showed and the host clock. Each step is the typed host event a real host sends, delivered through `ui::Hosting::event` ([NATIVE-SHM-EVENT]), so a test names a control by label and drives the very boundary a host calls. `Drive::quiet` takes the rows a step draws, as a host that is not watching. Its `shows`, `letters` and `offers` print what the view does show when a lookup fails.
- `Read` reads what a host is sent, typed: `ui::Surfaces::seen` walks the current view into the rows, fits and scenes a host gets, and `Read::seen` turns them into a `Rendered` (`Control` records with their words, hint, control number, ink, enabled state and picker choices; canvas lettering and curves; the scene's `ui::Accessible` tree; the kinds of the requests the last step made). Nothing is decoded.
- `Views` finds controls by their words, prompt or picker name, and reads lettering and requests.
- `Semantics` reads the typed `ui::Canvases::semantics` tree (`ui::Accessible`), with roles, values and pointer positions from accessible bounds. `Semantics::outline` is the tree as an agent reads it, one `button "Play"` or `slider "Volume" value 0.5 of 0.0 to 1.0` a line, which a failed `Drive::offers` prints. `Semantics::unclear` names every empty, cut-short or repeated label; every app's suite asserts it is empty.

A test keeps its `Driven` in a cell written by one effect arm. This is from the casting test:

```ospml
mut driven = Drive::idle (App::program (), casting::Link::store (ui::MachineDocs::faulty ""))
kept next = perform Bench.drive next
post step = hosted (\() => kept (step driven))

press label = post (\d => Drive::press (d, label))
pointer (x, y, down) = post (\d => Drive::pointer (d, x, y, down))
tapAt (label, fraction) =
    p = Semantics::point (element label, fraction)
    pointer (p.pointX, p.pointY, true)
    pointer (p.pointX, p.pointY, false)
```

`make _ui-test` runs the suites built on it: `src/osprey/packages/uitest/tests`, `src/osprey/examples/casting/test` and `src/osprey/packages/signal_flow/tests`. To drive the same app through a real host, with or without a window, write its `self-test.json` ([UI-MACHINE-SELF-TEST]).

## Origin and license

Original Osprey code, not a port of another project. MIT, Copyright NIMBLESITE PTY LTD, Australia; see [LICENSE](LICENSE) and [NOTICE.md](NOTICE.md).

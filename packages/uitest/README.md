# @osprey/uitest

Drive an Osprey Native app with no host and no window ([UI-MACHINE-TEST]). Find every control by its accessible label, as a screen reader or an AI agent would. Send host-shaped events and assert what the rendered view or canvas shows.

- `Read` is the package's only JSON code. It decodes each envelope a session sends (`Read::envelope`), or a node tree (`Read::tree`), into a typed `Rendered`: `Control` records, canvas lettering, the scene's `ui::Accessible` tree as the host receives it (`renderedAccess`), requests, the root's revision and its event ordinal.
- `Views` finds controls by label, text or title and reads lettering and requests.
- `Semantics` reads the typed `ui::Canvases::semantics` tree (`ui::Accessible`), with roles, values and pointer positions from accessible bounds.
- `Events` encodes presses, choices, pointers, file-picker and process completions, frames and lifecycle phases as the event text `ui::Runtime::proceed` takes.
- `Drive` holds one live session as a `Driven<S>`: the session, its `Rendered` view and the host clock. It builds every event from them, so a test names a control by label and never tracks revisions or ordinals. Its `shows`, `letters` and `offers` print what the view does show when a lookup fails.

A test keeps its `Driven` in a cell written by one effect arm. This is from the casting test:

```ospml
mut driven = Drive::idle (Runtime::idle (casting::Link::store (ui::MachineDocs::faulty "")))
kept (resident : ui::Resident<casting::Store>) = perform Bench.drive (Drive::moved (driven, resident))
post event = hosted (\() => kept (Runtime::proceed (driven.drivenSession, event, App::update, App::regions)))

press label = post (Drive::press (driven, label))
elements () = Canvases::semantics (machine ())
tapAt (label, fraction) =
    p = Semantics::point (Semantics::element (elements (), label), fraction)
    post (Drive::pointer (driven, p.pointX, p.pointY, true))
    post (Drive::pointer (driven, p.pointX, p.pointY, false))
```

`make _ui-test` runs the suites built on it: `packages/uitest/tests`, `examples/casting/test` and `packages/signal_flow/tests`.

# @osprey/uitest

Drive an Osprey Native app with no host and no window ([UI-MACHINE-TEST]). Find every control by its accessible label, as a screen reader or an AI agent would. Send host-shaped events and assert what the rendered view or canvas shows.

- `Read` is the package's only JSON code. It decodes each envelope a session sends (`Read::envelope`), or a node tree (`Read::tree`), into a typed `Rendered`: `Control` records, canvas lettering, requests, the root's revision and its event ordinal.
- `Views` finds controls by label, text or title and reads lettering and requests.
- `Semantics` reads the typed `ui::Canvases::semantics` tree (`ui::Accessible`), with roles, values and pointer positions from accessible bounds.
- `Events` encodes presses, choices, pointers, file-picker and process completions, frames and lifecycle phases as the event text `ui::Runtime::proceed` takes.

A test keeps the last `Rendered` and its session in cells, then wraps these calls. This is from the casting test:

```ospml
kept (resident : ui::Resident<casting::Store>) =
    perform Bench.keep resident.residentSession
    perform Bench.see (sight(sightSeen = Read::envelope (sight.sightSeen, resident.residentEnvelope), sightNow = sight.sightNow + 1))

control label = Views::control (seen (), label)
press label = post (Events::press ((seen ()).renderedRevision, (control label).controlOrdinal, stamp ()))
elements () = Canvases::semantics (machine ())
tapAt (label, fraction) =
    p = Semantics::point (Semantics::element (elements (), label), fraction)
    pointer (p.pointX, p.pointY, true)
    pointer (p.pointX, p.pointY, false)
```

`make ui-test` runs the suites built on it: `packages/uitest/tests`, `examples/casting/test` and `packages/signal_flow/tests`.

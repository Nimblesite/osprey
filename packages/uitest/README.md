# @osprey/uitest

Drive an Osprey Native app with no host and no window. Find every control by its accessible label, as a screen reader or an AI agent would, send host-shaped events, and assert what the rendered view or canvas draws.

- `Views` walks rendered JSON, finds labelled controls, reads text and canvas lettering, and checks requests.
- `Semantics` reads `ui::Templates::semantics` trees and calculates pointer positions from accessible bounds.
- `Events` builds control, pointer, completion, frame and lifecycle event JSON.

This excerpt from the casting test supplies its `sight` and `live` cells through thin wrappers:

```ospml
import uitest::Events
import uitest::Semantics
import uitest::Views
nodes () = Views::nodes sight.sightRoot
control label = Views::control (sight.sightRoot, label)
labels () = Semantics::labels (Templates::semantics (machine ()))
element label = Semantics::element (Templates::semantics (machine ()), label)
press label = post (Events::press (sight.sightRevision, ordinal label, stamp ()))
tapAt (label, fraction) =
    p = Semantics::point (element label, fraction)
    pointer (p.pointX, p.pointY, true)
    pointer (p.pointX, p.pointY, false)
tap label = tapAt (label, 0.5)
expect (shows "Play", true)
press "Play"
```

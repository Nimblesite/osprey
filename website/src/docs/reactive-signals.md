---
mlTwins: manual
layout: page
title: Reactive signals
description: Derive signal dependencies, cache immutable results, and rebuild affected computations.
permalink: /docs/reactive-signals/
---

# Rebuild only the views that depend on a change

Declare signal reads as static effects. A handler supplies their current values; the compiler derives each computation's possible signal reads through helpers and callbacks. You do not maintain a dependency array.

```osprey
static effect Signal<T> { read: fn() -> T }
fn count() = perform Signal<int>.read()
fn label() = "count=${count()}"
let first = {
    handle static Signal<int> { read => 1 }
    signalSnapshot(label)
}
let updated = {
    handle static Signal<int> { read => 2 }
    signalRefresh(first, signalDependencies(count), label)
}
print("${first.value} / ${updated.value}")
```

```osprey-ml
static effect Signal T
    read : Unit => T
count () = perform Signal<int>.read ()
label () = "count=${count ()}"
first =
    handle static Signal<int>
        read => 1
    signalSnapshot label
updated =
    handle static Signal<int>
        read => 2
    signalRefresh first (signalDependencies count) label
print "${first.value} / ${updated.value}"
```

This prints `count=1 / count=2`. `first` retains its original value. Each snapshot contains `value`, `dependencies`, and `revision`; the revision starts at 1 and increases when the computation rebuilds.

`signalDependencies(work)` returns sorted, distinct identities such as `Signal<int>.read`. It evaluates the callback argument once without invoking the callback. `signalSnapshot(work)` invokes it once and caches its result. `signalRefresh(cached, changed, work)` invokes it only when a dependency is in `changed`, the computation's dependency set changes, or either dependency information or changes include `<unknown>`. Otherwise it returns the existing snapshot. Arguments evaluate once, left to right, including when the cached value is reused. Cached callable values keep their own requirements. A callable or lazy iterator that escapes a statically selected handler while still requiring its operations is rejected before erasure; the compiler must not bind its future work to the construction handler.

Supply every changed signal identity together in one refresh to batch an update. Signal identity includes the effect, operation and resolved generic arguments. Two independently changeable inputs need different identities; using the same payload type twice does not create two signals. The caller supplies the current values and the changed identities; refresh runs synchronously, and a later update builds on the returned snapshot.

Dependencies are conservative **may-read** sets. Both arms of a runtime choice contribute possible reads, including an arm not taken on this execution. A resolved callback keeps its actual requirements. An unresolved callback, generic instance or open remainder contributes `<unknown>`, which forces refresh rather than risk retaining a stale value. Builtin I/O is still checked as an effect obligation; metadata queries do not make their argument expressions pure.

## The built-in signals layer

Every reactive app used to declare the same reader effect and hand-roll the same frame cache. Both are standard now: the `Signal<T>` reader effect, `SignalFrame`, `KeyedSnapshot<T>`, and the keyed refresh functions are prelude source attached on demand, so a program that names them stops declaring its own. A program that declares its own `Signal` keeps it and gets none of the standard layer.

A render keeps one frame per keyed region, refreshes each region through `signalRefreshKeyed`, and derives the changed identities from the slice encodings with `signalChanges`. The revision each region kept survives a JSON round trip of the frames.

```osprey
type Route = { tab: string }
fn tab() = (perform Signal<Route>.read()).tab
fn encode(route) = JsonString(route.tab)
fn render(route, frames, changed) = {
    handle static Signal<Route> { read => Route { tab: route } }
    [signalFrame(signalRefreshKeyed(frames, "nav", changed, "", tab))]
}
let home = Route { tab: "home" }
let away = Route { tab: "away" }
let first = render("home", [], [])
let second = render("away", first, signalChanges(home, away, encode, signalDependencies(tab)))
let across = signalFramesFromJson(jsonDecode(jsonEncode(signalFramesJson(second)) ?: "") ?: JsonNull)
print("nav@${toString(signalRevisionOf(second, "nav"))} kept@${toString(signalRevisionOf(across, "nav"))}")
```

```osprey-ml
type Route =
    tab : string
tab () = (perform Signal<Route>.read ()).tab
encode route = JsonString route.tab
render (route, frames, changed) =
    handle static Signal<Route>
        read => Route(tab = route)
    [signalFrame (signalRefreshKeyed frames "nav" changed "" tab)]
home = Route(tab = "home")
away = Route(tab = "away")
first = render ("home", [], [])
second = render ("away", first, signalChanges home away encode (signalDependencies tab))
across = signalFramesFromJson (jsonDecode (jsonEncode (signalFramesJson second) ?: "") ?: JsonNull)
print "nav@${toString (signalRevisionOf second "nav")} kept@${toString (signalRevisionOf across "nav")}"
```

This prints `nav@2 kept@2`: the region rebuilt because `Route` changed, and the frames read back from JSON keep that revision for the next render. `signalRetained` supplies the snapshot to refresh for a key — an unseen or forced key carries `<unknown>` dependencies and therefore rebuilds, and its placeholder value is never reused unless nothing the region read changed. `signalForced(frames, key)` rebuilds one key at its next refresh whatever changed, and `signalRevisionOf(frames, key)` reads a key's kept revision.

Run the [complete dashboard examples](https://github.com/Nimblesite/osprey/tree/main/examples/handlers), in Default and ML, with `python3 examples/handlers/run.py all --demo staged-rows --check`. They update counter and cursor views independently, rebuild a combined status view for either change, and retain all three on an idle update. Native programs can also rebuild computations that use fibers and deep one-shot continuations.

The [effects specification](/spec/0017-algebraiceffects/#reactive-signals--stage-signals) defines the contract.

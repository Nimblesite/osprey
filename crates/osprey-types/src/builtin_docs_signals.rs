//! Reactive signal API prose; signatures live in the canonical builtin table.
use crate::builtin_docs::BuiltinDoc;
use crate::builtin_docs_lang::builtin_doc;

const SIGNAL_EXAMPLE: &str = "static effect Signal<T> { read: fn() -> T }\ntype Count = { value: int }\nfn readCount() = (perform Signal<Count>.read()).value\nfn counterLabel() = \"count=${readCount()}\"\nlet cached = {\n    handle static Signal<Count> { read => Count { value: 1 } }\n    signalSnapshot(counterLabel)\n}\nlet updated = {\n    handle static Signal<Count> { read => Count { value: 2 } }\n    signalRefresh(cached, signalDependencies(readCount), counterLabel)\n}\nprint(\"${updated.value}; revision=${updated.revision}; ${join(signalDependencies(counterLabel), \",\")}\")";

pub(crate) static SIGNALS: &[BuiltinDoc] = &[
    builtin_doc!(
        "signalDependencies",
        "Returns the checked static signal operations a callback may read, without invoking it. An unresolved remainder is <unknown>.",
        ["work" => "A zero-argument computation"],
        SIGNAL_EXAMPLE,
    ),
    builtin_doc!(
        "signalSnapshot",
        "Runs a computation once and returns its value, compiler-derived signal dependencies and revision 1.",
        ["work" => "A zero-argument computation under its required handlers"],
        SIGNAL_EXAMPLE,
    ),
    builtin_doc!(
        "signalRefresh",
        "Rebuilds an immutable snapshot when a dependency changes, its callback dependency set changes or dependencies remain unknown. Otherwise returns the cached snapshot.",
        ["cached" => "The previous snapshot", "changed" => "Changed signal operation identities", "work" => "The computation using current signal values"],
        SIGNAL_EXAMPLE,
    ),
];

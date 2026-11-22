//! The multi-shot programs `staged_many.rs` runs, inspects and builds for
//! every target. Each pairs its source with the exact output a correct run
//! prints, derived by hand from the handler arms ([MULTI-HANDLE-MANY]).

/// Every program below resumes one captured continuation more than once.
/// Each pairs its source with the exact output a correct run prints
/// ([MULTI-HANDLE-MANY], [MULTI-FALSIFY] gates 3 and 4, plan 0016 phase 3).
pub(crate) const MANY_RUNS: &[(&str, &str, &str)] = &[
    (
        "every alternative is resumed and each branch keeps its own answer",
        r#"
effect Choice { control many pick: fn(int) -> int }
fn answer(seed) = {
    let chosen = perform Choice.pick(seed)
    "a${chosen}"
}
fn main() = {
    let collected = {
        handle Choice {
            pick bound => {
                let low = resume(bound)
                let high = resume(9)
                "${low}&${high}"
            }
        }
        answer(4)
    }
    print("collected=${collected}")
}
"#,
        "collected=a4&a9\n",
    ),
    (
        // A shared helper takes capturing callbacks with no annotation, a
        // lambda bound by `let` makes the request, work before a capture runs
        // once, and the right side of `&&` asks only when the left allows it.
        "a shared helper, a bound lambda and a lazy operator inside one search",
        r#"
effect Choice { control many pick: fn(int) -> int }
effect Trace { replayable note: fn(string) -> Unit }
fn both(first, second) = satAdd(first(), second())
fn pair(seed) = {
    let ask = fn(n) => perform Choice.pick(n)
    let left = ask(seed)
    perform Trace.note("L${left} ")
    let right = both(fn() => ask(left), fn() => 100)
    match left > 1 && perform Choice.pick(0) == 0 {
        true => "big${left}:${right}"
        false => "small${left}:${right}"
    }
}
fn main() = {
    mut log = ""
    let all = {
        handle Trace {
            note text => { log = "${log}${text}" }
        }
        handle Choice {
            pick bound => {
                let a = resume(bound)
                let b = resume(satAdd(bound, 1))
                "${a}|${b}"
            }
        }
        pair(1)
    }
    print("all=${all} log=${log}")
}
"#,
        "all=small1:101|small1:102|big2:102|small2:102|big2:103|small2:103 log=L1 L2 \n",
    ),
    (
        // `Stop` is a `control` effect handled between the request and its
        // `many` handler, so it is captured with it: an arm that never resumes
        // abandons only its own branch, and `finally` runs once per branch.
        "a non-resuming control effect and a finalizer inside the captured region",
        r#"
effect Choice { control many pick: fn(int) -> int }
effect Stop { control replayable quit: fn(string) -> int }
effect Audit { replayable close: fn(string) -> Unit }
fn work(seed) = {
    let n = perform Choice.pick(seed)
    match n > 5 {
        true => perform Stop.quit("too big ${n}")
        false => satMul(n, 10)
    }
}
fn main() = {
    mut log = ""
    let total = {
        handle Audit {
            close text => { log = "${log}${text}" }
        }
        handle Choice {
            pick bound => satAdd(satAdd(resume(bound), resume(6)), resume(2))
        }
        {
            handle Stop {
                quit _why => satSub(0, 1)
                finally => perform Audit.close("done;")
            }
            work(1)
        }
    }
    print("total=${total} log=${log}")
}
"#,
        "total=29 log=done;done;done;\n",
    ),
    (
        // A value operation of a captured effect, a return clause applied to
        // each branch, and a handler that answers without resuming at all.
        "a value operation, a return clause and zero resumptions",
        r#"
effect Ask {
    control many choose: fn(string) -> bool
    replayable hint: fn() -> string
}
fn decide() = {
    let label = perform Ask.hint()
    match perform Ask.choose(label) {
        true => "yes-${label}"
        false => "no-${label}"
    }
}
fn main() = {
    let both = {
        handle Ask {
            choose _label => "${resume(true)}+${resume(false)}"
            hint => "h"
            return answer => "<${answer}>"
        }
        decide()
    }
    let none = {
        handle Ask {
            choose _label => "skipped"
            hint => "h"
        }
        decide()
    }
    print("both=${both} none=${none}")
}
"#,
        "both=<yes-h>+<no-h> none=skipped\n",
    ),
    (
        // The handler is a value applied to the search: backtracking keeps the
        // newest version that the rest of the plan accepts.
        "a handler value backtracks to the first answer the plan accepts",
        r#"
effect Versions { control many pick: fn(string) -> string }
fn plan() = {
    let serde = perform Versions.pick("serde")
    let toml = perform Versions.pick("toml")
    match serde == "2" && toml == "2" {
        true => "conflict"
        false => "serde ${serde}, toml ${toml}"
    }
}
let newestThatWorks = handler Versions {
    pick _ => match resume("2") {
        "conflict" => resume("1")
        found => found
    }
}
print(newestThatWorks(plan))
"#,
        "serde 2, toml 1\n",
    ),
    (
        // Two `many` operations of one effect, a Unit request resumed with no
        // value, named and `let`-bound direct functions passed where a capturing
        // callback is expected, and a lazy `||` whose right side asks only when
        // the left is false. The tick before every capture is counted once.
        "direct functions as callbacks, a unit resume and a lazy or",
        r#"
effect Flip {
    control many replayable toss: fn() -> bool
    control many replayable tick: fn() -> Unit
}
effect Tally { replayable add: fn(int) -> Unit }
fn hundred() = 100
fn twice(first, second) = satAdd(first(), second())
fn coin() = match perform Flip.toss() {
    true => 1
    false => 0
}
fn walk() = {
    perform Flip.tick()
    let small = fn() => 7
    let total = satAdd(twice(coin, hundred), twice(small, coin))
    match total > 107 || perform Flip.toss() {
        true => "hi${total}"
        false => "lo${total}"
    }
}
fn main() = {
    mut ticks = 0
    let all = {
        handle Tally {
            add n => { ticks = satAdd(ticks, n) }
        }
        handle Flip {
            toss => "${resume(true)},${resume(false)}"
            tick => {
                perform Tally.add(1)
                resume()
            }
        }
        walk()
    }
    print("all=${all} ticks=${ticks}")
}
"#,
        "all=hi109,hi108,hi108,hi107,lo107 ticks=1\n",
    ),
    (
        // Two captured effects, one handled outside the other, around a direct
        // handler with a return clause: the outer
        // handler resumes continuations that pass back through the inner one.
        "nested multi-shot handlers resume through each other",
        r#"
effect Choice { control many replayable pick: fn(int) -> int }
effect Extra { control many replayable guess: fn(int) -> int }
effect Scale { replayable by: fn() -> int }
fn pair(seed) = satAdd(satMul(perform Choice.pick(seed), perform Scale.by()), perform Extra.guess(seed))
fn inner(seed) = {
    handle Scale {
        by => 10
        return found => satAdd(found, 1000)
    }
    pair(seed)
}
fn main() = {
    let all = {
        handle Extra {
            guess n => satAdd(resume(n), satMul(resume(satAdd(n, 1)), 1000000))
        }
        handle Choice {
            pick n => satAdd(resume(n), resume(satAdd(n, 2)))
        }
        inner(1)
    }
    print("all=${all}")
}
"#,
        "all=2044004086002042\n",
    ),
    (
        // `?:` asks for its fallback only when the left side failed.
        "a fallback operator captures only on failure",
        r#"
effect Choice { control many pick: fn(int) -> int }
fn risky(n) = checkedAdd(n, 9223372036854775807) ?: perform Choice.pick(n)
fn main() = {
    let fine = {
        handle Choice {
            pick _bound => satAdd(resume(1), resume(2))
        }
        risky(0)
    }
    let fallback = {
        handle Choice {
            pick bound => satAdd(resume(bound), resume(satMul(bound, 10)))
        }
        risky(3)
    }
    print("fine=${fine} fallback=${fallback}")
}
"#,
        "fine=9223372036854775807 fallback=33\n",
    ),
    (
        // [MULTI-REPLAY-STATE] resolved by scope: `mut hits` belongs to
        // `other`, while the `hits` the arm reads is `main`'s immutable
        // `let`. A checker that collects `mut` names program-wide would
        // collide the two and reject this program; only a `mut` binding
        // actually visible at the arm counts.
        "an immutable name that shares its spelling with another function's mut",
        r#"
effect Choice { control many pick: fn(int) -> string }
effect Journal { replayable note: fn(string) -> Unit }
fn other() = {
    mut hits = ""
    handle Journal {
        note t => { hits = "${hits}${t}" }
    }
    perform Journal.note("x")
    hits
}
fn main() = {
    let hits = "reads="
    let out = {
        handle Choice {
            pick _b => "${hits}${resume("1")}|${resume("2")}"
        }
        perform Choice.pick(0)
    }
    print(out)
    print(other())
}
"#,
        "reads=1|2\nx\n",
    ),
    (
        // The four list builtins with a callback that makes a captured request:
        // each element's request is its own choice point, so a fold over two
        // elements resumed twice each runs four branches.
        "callbacks of the list builtins make captured requests",
        r#"
effect Choice { control many pick: fn(int) -> int }
effect Trace { replayable note: fn(string) -> Unit }
fn sums() = foldList([1, 2], 0, fn(sum, n) => satAdd(sum, perform Choice.pick(n)))
fn doubled() = mapList([1, 2], fn(n) => satMul(perform Choice.pick(n), 10))
fn evens() = filterList([1, 2, 3], fn(n) => perform Choice.pick(n) > 1)
fn visit() = {
    forEachList([7, 8], fn(n) => perform Trace.note("v${perform Choice.pick(n)} "))
    "done"
}
fn main() = {
    let total = {
        handle Choice {
            pick _bound => satAdd(resume(1), resume(2))
        }
        sums()
    }
    let lists = {
        handle Choice {
            pick bound => "${resume(bound)}|${resume(satAdd(bound, 5))}"
        }
        join(mapList(doubled(), fn(n) => toString(n)), ",")
    }
    let kept = {
        handle Choice {
            pick bound => satAdd(resume(bound), satMul(resume(0), 100))
        }
        length(evens())
    }
    mut log = ""
    let walked = {
        handle Trace {
            note text => { log = "${log}${text}" }
        }
        handle Choice {
            pick bound => "${resume(bound)}+${resume(0)}"
        }
        visit()
    }
    print("total=${total} lists=${lists} kept=${kept} walked=${walked} log=${log}")
}
"#,
        "total=12 lists=10,20|10,70|60,20|60,70 kept=20402 walked=done+done+done+done log=v7 v8 v0 v0 v8 v0 \n",
    ),
    (
        // Two resume results, each itself a closure, combined in a third:
        // the rewrites of the arm's `let`-bound results must stay tracked
        // values, not untracked callables, at program entry
        // ([MULTI-HANDLE-MANY]).
        "two resumed closures combined into one closure result",
        r#"
effect Choice { control many pick: fn(int) -> int }
fn body() = {
    let a = perform Choice.pick(0)
    fn(z: int) => satAdd(z, a)
}
fn main() = {
    let combined = {
        handle Choice {
            pick _n => {
                let f = resume(1)
                let g = resume(2)
                fn(z: int) => satAdd(f(z), g(z))
            }
        }
        body()
    }
    print("combined=${combined(100)}")
}
"#,
        "combined=203\n",
    ),
    (
        // A handler value travels through a function parameter and is
        // applied there: the many pass must follow that application
        // ([MULTI-HANDLE-MANY-VALUE]).
        "a handler value passed as a parameter and applied in the callee",
        r"
effect Choice { control many pick: fn(int) -> int }

fn body() = {
    let a = perform Choice.pick(0)
    satMul(a, 2)
}

// Runs any Choice-to-int handler over body.
fn run(h) = {
    let r = h(fn() => body())
    satAdd(r, 100)
}

fn main() = {
    let doubler = handler Choice {
        pick n => {
            let x = resume(n)
            let y = resume(satAdd(n, 1))
            satAdd(x, y)
        }
    }
    print(run(doubler))
}
",
        "102\n",
    ),
];

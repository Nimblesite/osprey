//! Permutations of multi-shot resumption `staged_many.rs` runs beside
//! `many_programs.rs`: resume counts from zero to five, nesting orders,
//! recursion, wide environments and abandoned branches. Every expected output
//! is derived by hand from the handler arms ([MULTI-HANDLE-MANY],
//! [MULTI-REPLAY]) and must be identical under GC, ARC and wasm32
//! ([MULTI-WASM-MANY]).

/// Each entry pairs a source with the exact output a correct run prints.
pub(crate) const MANY_PERMUTATIONS: &[(&str, &str, &str)] = &[
    (
        "resumed or-pattern bodies retain the selected alternative binding",
        r#"
effect Choice { control many pick: fn(int) -> int }
fn branch(input) = match input {
    Success { value: seed } | Error { message: seed } => {
        let answer = perform Choice.pick(seed)
        "${seed}:${answer}"
    }
}

fn answersFor(input, twice) = {
    handle Choice {
        pick base => match twice {
            true => "${resume(base)},${resume(satAdd(base, 1))}"
            false => "${resume(base)},${resume(satAdd(base, 1))},${resume(satAdd(base, 2))}"
        }
    }
    branch(input)
}

fn main() = {
    print(answersFor(Success { value: 4 }, true))
    print(answersFor(Error { message: 9 }, false))
}
"#,
        "4:4,4:5\n9:9,9:10,9:11\n",
    ),
    (
        // One answer per `resume` call, in call order, for 0, 1, 2, 3 and 5
        // resumptions of the same request.
        "the answers number the resumptions, from none to five",
        r#"
effect Choice { control many pick: fn(int) -> int }
fn tag(seed) = {
    let n = perform Choice.pick(seed)
    "<${n}>"
}
fn main() = {
    let zero = {
        handle Choice {
            pick _bound => "none"
        }
        tag(1)
    }
    let one = {
        handle Choice {
            pick bound => resume(bound)
        }
        tag(1)
    }
    let two = {
        handle Choice {
            pick bound => "${resume(bound)}${resume(satAdd(bound, 1))}"
        }
        tag(1)
    }
    let three = {
        handle Choice {
            pick bound => "${resume(bound)}${resume(satAdd(bound, 1))}${resume(satAdd(bound, 2))}"
        }
        tag(1)
    }
    let five = {
        handle Choice {
            pick bound => {
                let low = "${resume(bound)}${resume(satAdd(bound, 1))}${resume(satAdd(bound, 2))}"
                "${low}${resume(satAdd(bound, 3))}${resume(satAdd(bound, 4))}"
            }
        }
        tag(1)
    }
    print("zero=${zero} one=${one} two=${two} three=${three} five=${five}")
}
"#,
        "zero=none one=<1> two=<1><2> three=<1><2><3> five=<1><2><3><4><5>\n",
    ),
    (
        // Three requests in a row, each resumed twice: eight leaves in
        // depth-first order. The note before the first request runs once, and
        // each later note once per branch that reaches it: 1, 2, 4 and 8.
        "three requests in sequence give eight leaves and replay nothing above a capture",
        r#"
effect Choice { control many pick: fn(int) -> int }
effect Trace { replayable note: fn(string) -> Unit }
fn triple() = {
    perform Trace.note("s")
    let a = perform Choice.pick(1)
    perform Trace.note("a")
    let b = perform Choice.pick(10)
    perform Trace.note("b")
    let c = perform Choice.pick(100)
    perform Trace.note("c")
    "${satAdd(satAdd(a, b), c)}"
}
fn main() = {
    mut log = ""
    let all = {
        handle Trace {
            note text => { log = "${log}${text}" }
        }
        handle Choice {
            pick bound => "${resume(bound)},${resume(satMul(bound, 2))}"
        }
        triple()
    }
    print("all=${all} log=${log}")
}
"#,
        "all=111,211,121,221,112,212,122,222 log=sabccbccabccbcc\n",
    ),
    (
        // The handler resumes a second and third time only while the answer
        // is still empty, so a found answer abandons every later branch. The
        // log lists each pair that was tried: six for 5, all nine for 7.
        "a search stops resuming at the first answer and tries everything when there is none",
        r#"
effect Choice { control many pick: fn(int) -> int }
effect Trace { replayable note: fn(string) -> Unit }
fn search(target) = {
    let a = perform Choice.pick(0)
    let b = perform Choice.pick(0)
    perform Trace.note("${a}${b} ")
    match satAdd(a, b) == target {
        true => "${a}+${b}"
        false => ""
    }
}
fn solve(target) = {
    handle Choice {
        pick _bound => match resume(1) {
            "" => match resume(2) {
                "" => resume(3)
                found => found
            }
            found => found
        }
    }
    search(target)
}
fn main() = {
    mut log = ""
    let both = {
        handle Trace {
            note text => { log = "${log}${text}" }
        }
        let five = solve(5)
        perform Trace.note("| ")
        let seven = solve(7)
        "five=${five} seven=${seven}"
    }
    print("${both} log=${log}")
}
"#,
        "five=2+3 seven= log=11 12 13 21 22 23 | 11 12 13 21 22 23 31 32 33 \n",
    ),
    (
        // Each branch returns a one-element list and the arm concatenates
        // three resumptions, at two levels: nine leaves, each its own list.
        "list answers of nine branches concatenate in order",
        r#"
effect Choice { control many pick: fn(int) -> int }
fn pairs() = {
    let a = perform Choice.pick(1)
    let b = perform Choice.pick(a)
    [satAdd(satMul(a, 10), b)]
}
fn main() = {
    let all = {
        handle Choice {
            pick bound => listConcat(
                resume(bound),
                listConcat(resume(satAdd(bound, 1)), resume(satAdd(bound, 2))))
        }
        pairs()
    }
    let sum = foldList(all, 0, fn(total, n) => satAdd(total, n))
    let text = foldList(all, "", fn(seen, n) => "${seen}${n} ")
    print("count=${length(all)} sum=${sum} all=${text}")
}
"#,
        "count=9 sum=207 all=11 12 13 22 23 24 33 34 35 \n",
    ),
    (
        // Two multi-shot effects, nested both ways round. The outer handler's
        // continuation holds the rest of the inner handler's arm, so the
        // nesting order decides how the answers group.
        "two multi-shot effects group their answers by nesting order",
        r#"
effect Coin { control many flip: fn() -> bool }
effect Die { control many roll: fn(int) -> int }
fn game() = {
    let first = perform Die.roll(1)
    let heads = perform Coin.flip()
    let face = match heads {
        true => "H"
        false => "T"
    }
    let second = match heads {
        true => perform Die.roll(first)
        false => 0
    }
    "${first}${face}${second}"
}
fn main() = {
    let coinOuter = {
        handle Coin {
            flip => "${resume(true)}/${resume(false)}"
        }
        handle Die {
            roll bound => "${resume(bound)},${resume(satAdd(bound, 4))}"
        }
        game()
    }
    let dieOuter = {
        handle Die {
            roll bound => "${resume(bound)},${resume(satAdd(bound, 4))}"
        }
        handle Coin {
            flip => "${resume(true)}/${resume(false)}"
        }
        game()
    }
    print("coinOuter=${coinOuter} dieOuter=${dieOuter}")
}
"#,
        "coinOuter=1H1,1H5,5H5,5H9/1H1,1H5,5T0/1T0,5H5,5H9/1T0,5T0 dieOuter=1H1/1T0,1H5/1T0,5H5/5T0,5H9/5T0\n",
    ),
    (
        // Four values of four types are bound before the first capture and
        // read after the second: every branch sees the same ones.
        "every branch reads the bindings made before its capture",
        r#"
effect Choice { control many pick: fn(int) -> int }
fn roomy(seed) = {
    let a = satAdd(seed, 1)
    let b = "s${a}"
    let c = match a > 1 {
        true => "Y"
        false => "N"
    }
    let d = [a, satMul(a, 2)]
    let chosen = perform Choice.pick(a)
    let f = satAdd(chosen, a)
    let again = perform Choice.pick(f)
    "${a}${b}${c}${length(d)}:${chosen}:${f}:${again}"
}
fn main() = {
    let all = {
        handle Choice {
            pick bound => "${resume(bound)}|${resume(satAdd(bound, 10))}"
        }
        roomy(1)
    }
    print("all=${all}")
}
"#,
        "all=2s2Y2:2:4:4|2s2Y2:2:4:14|2s2Y2:12:14:14|2s2Y2:12:14:24\n",
    ),
    (
        // Two requests are the arguments of one call: the left is asked
        // first, so it varies slowest.
        "requests in argument position are asked left to right",
        r#"
effect Choice { control many pick: fn(int) -> int }
fn added() = {
    let total = satAdd(satMul(perform Choice.pick(1), 100), perform Choice.pick(2))
    "${total}"
}
fn main() = {
    let all = {
        handle Choice {
            pick bound => "${resume(bound)},${resume(satAdd(bound, 5))}"
        }
        added()
    }
    print("all=${all}")
}
"#,
        "all=102,107,602,607\n",
    ),
    (
        // A recursive function asks once per level: three levels resumed
        // twice each enumerate the eight bit strings in order.
        "a recursive function asks at every level",
        r#"
effect Choice { control many pick: fn(int) -> int }
fn digits(depth, acc) = match depth == 0 {
    true => acc
    false => {
        let bit = perform Choice.pick(0)
        digits(satSub(depth, 1), "${acc}${bit}")
    }
}
fn main() = {
    let all = {
        handle Choice {
            pick _bound => "${resume(0)} ${resume(1)}"
        }
        digits(3, "")
    }
    print("all=${all}")
}
"#,
        "all=000 001 010 011 100 101 110 111\n",
    ),
    (
        // The same value resumed three times is three separate runs of the
        // rest: the note after the capture is made three times, the one
        // before it once.
        "resuming one value three times runs the rest three times",
        r#"
effect Choice { control many pick: fn(int) -> int }
effect Trace { replayable note: fn(string) -> Unit }
fn tag(seed) = {
    perform Trace.note("before;")
    let n = perform Choice.pick(seed)
    perform Trace.note("after${n};")
    "<${n}>"
}
fn main() = {
    mut log = ""
    let all = {
        handle Trace {
            note text => { log = "${log}${text}" }
        }
        handle Choice {
            pick bound => "${resume(bound)}${resume(bound)}${resume(bound)}"
        }
        tag(7)
    }
    print("all=${all} log=${log}")
}
"#,
        "all=<7><7><7> log=before;after7;after7;after7;\n",
    ),
    (
        // Zero, a negative and the largest integer are each resumed, and the
        // branch answers are added with saturation: 0 + -2 + max = max - 2.
        "branch answers at the integer limits add up",
        r#"
effect Choice { control many pick: fn(int) -> int }
fn doubled() = satMul(perform Choice.pick(0), 2)
fn main() = {
    let total = {
        handle Choice {
            pick _bound => satAdd(
                resume(0),
                satAdd(resume(satSub(0, 1)), resume(9223372036854775807)))
        }
        doubled()
    }
    print("total=${total}")
}
"#,
        "total=9223372036854775805\n",
    ),
    (
        // An arm that answers without resuming ends only the branch it is in:
        // its answer is the value of the `resume` call that reached it.
        "an arm that does not resume ends only its own branch",
        r#"
effect Choice { control many pick: fn(int) -> int }
fn climb(n) = {
    let got = perform Choice.pick(n)
    match got > 3 {
        true => "top${got}"
        false => climb(satAdd(got, 1))
    }
}
fn main() = {
    let all = {
        handle Choice {
            pick bound => match bound > 2 {
                true => "stop${bound}"
                false => "${resume(bound)}[${resume(satAdd(bound, 1))}]"
            }
        }
        climb(1)
    }
    print("all=${all}")
}
"#,
        "all=stop3[stop4][stop3]\n",
    ),
    (
        "short-circuit and fallback branches resume independently",
        r#"
effect Choice { control many pick: fn() -> bool }
effect Number { control many pick: fn() -> int }
fn result(left) = {
    handle Choice { pick => "${resume(false)},${resume(true)}" }
    "${left && perform Choice.pick()}|${left || perform Choice.pick()}"
}
fn fallback(items) = {
    handle Number { pick => satAdd(resume(1), resume(2)) }
    listGet(items, 0) ?: perform Number.pick()
}
print("${result(false)};${result(true)};${fallback([])};${fallback([9])}")
"#,
        "false|false,false|true;false|true,true|true;3;9\n",
    ),
];

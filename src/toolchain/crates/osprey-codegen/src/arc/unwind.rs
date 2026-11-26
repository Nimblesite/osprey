//! Preserve active ARC slots when a control operation abandons its stack.
//! Implements [GC-ARC-PERCEUS] and [MULTI-COST-ABORT].
use crate::builder::Codegen;

const FRAME: &str = "{ i8*, i8***, i64 }";
const PUSH: &str = "declare void @__osprey_owner_frame_push(i8*, i8***, i64)";
const POP: &str = "declare void @__osprey_owner_frame_pop(i8*)";

pub(crate) fn instrument(cg: &mut Codegen, count: usize, lines: Vec<String>) -> Vec<String> {
    let control = cg
        .prog
        .effects
        .values()
        .flat_map(|ops| ops.values())
        .any(|op| op.mode == osprey_ast::OperationMode::Control);
    if count > 0 && control {
        cg.add_extern(PUSH);
        cg.add_extern(POP);
        attach(lines, &prologue(count))
    } else {
        lines
    }
}

/// The `, !dbg !N` a line ends with. LLVM refuses a call without one to a
/// function it can inline, inside a function that has debug info, and the
/// symbolic engine links the runtime with its definitions ([SYMEX-CORPUS]).
fn location(line: &str) -> &str {
    line.rfind(", !dbg !").map_or("", |at| &line[at..])
}

fn attach(lines: Vec<String>, prologue: &[String]) -> Vec<String> {
    let first = lines
        .iter()
        .map(|line| location(line))
        .find(|at| !at.is_empty())
        .unwrap_or("")
        .to_owned();
    let mut result = Vec::new();
    let mut registered = false;
    for line in lines {
        if needs_pop(&line, &result) {
            let at = location(&line);
            result.push(format!(
                "  call void @__osprey_owner_frame_pop(i8* %arc.unwind.raw){}",
                if at.is_empty() { &first } else { at }
            ));
        }
        let initialize = !registered && line == "  store i8* null, i8** %arc.s0";
        result.push(line);
        if initialize {
            result.extend(prologue.iter().map(|line| located(line, &first)));
            registered = true;
        }
    }
    result
}

/// `line`, with the location `at` when it is a call.
fn located(line: &str, at: &str) -> String {
    if line.trim_start().starts_with("call ") {
        format!("{line}{at}")
    } else {
        line.to_owned()
    }
}

fn needs_pop(line: &str, previous: &[String]) -> bool {
    line.contains("musttail call ")
        || (line.trim_start().starts_with("ret ")
            && !previous
                .last()
                .is_some_and(|line| line.contains("musttail call ")))
}

fn prologue(count: usize) -> Vec<String> {
    let array = format!("[{count} x i8**]");
    let mut result = vec![
        format!("  %arc.unwind.frame = alloca {FRAME}"),
        format!("  %arc.unwind.raw = bitcast {FRAME}* %arc.unwind.frame to i8*"),
        format!("  %arc.unwind.slots = alloca {array}"),
        format!(
            "  %arc.unwind.first = getelementptr {array}, {array}* %arc.unwind.slots, i64 0, i64 0"
        ),
    ];
    for index in 0..count {
        result.push(format!("  %arc.unwind.slot{index} = getelementptr {array}, {array}* %arc.unwind.slots, i64 0, i64 {index}"));
        result.push(format!(
            "  store i8** %arc.s{index}, i8*** %arc.unwind.slot{index}"
        ));
    }
    result.push(format!("  call void @__osprey_owner_frame_push(i8* %arc.unwind.raw, i8*** %arc.unwind.first, i64 {count})"));
    result
}

#[cfg(test)]
mod tests {
    #[test]
    fn owner_frame_pop_preserves_llvm_musttail_return_adjacency() {
        let call = "  %answer = musttail call i64 @drive(i8* %env, i8* %coro)";
        let lines = vec![
            "  store i8* null, i8** %arc.s0".into(),
            call.into(),
            "  ret i64 %answer".into(),
        ];
        let actual = super::attach(lines, &["  register".into()]);
        assert_eq!(
            actual,
            [
                "  store i8* null, i8** %arc.s0",
                "  register",
                "  call void @__osprey_owner_frame_pop(i8* %arc.unwind.raw)",
                call,
                "  ret i64 %answer"
            ]
        );
    }

    #[test]
    fn inserted_calls_carry_a_debug_location() {
        // The pop takes the location of the line it precedes, and the push the
        // function's first: without one LLVM refuses a module whose runtime it
        // can inline, which stopped every hunt of a resumable handler.
        let lines = vec![
            "  store i8* null, i8** %arc.s0".into(),
            "  %x = add i64 1, 2, !dbg !7".into(),
            "  ret i64 %x, !dbg !9".into(),
        ];
        let actual = super::attach(
            lines,
            &["  %slot = alloca i8*".into(), "  call void @push()".into()],
        );
        assert_eq!(
            actual,
            [
                "  store i8* null, i8** %arc.s0",
                "  %slot = alloca i8*",
                "  call void @push(), !dbg !7",
                "  %x = add i64 1, 2, !dbg !7",
                "  call void @__osprey_owner_frame_pop(i8* %arc.unwind.raw), !dbg !9",
                "  ret i64 %x, !dbg !9"
            ]
        );
    }
}

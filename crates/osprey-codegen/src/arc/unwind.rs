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

fn attach(lines: Vec<String>, prologue: &[String]) -> Vec<String> {
    let mut result = Vec::new();
    let mut registered = false;
    for line in lines {
        if needs_pop(&line, &result) {
            result.push("  call void @__osprey_owner_frame_pop(i8* %arc.unwind.raw)".into());
        }
        let initialize = !registered && line == "  store i8* null, i8** %arc.s0";
        result.push(line);
        if initialize {
            result.extend_from_slice(prologue);
            registered = true;
        }
    }
    result
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
            &actual[actual.len() - 3..],
            &[
                "  call void @__osprey_owner_frame_pop(i8* %arc.unwind.raw)",
                call,
                "  ret i64 %answer"
            ]
        );
    }
}

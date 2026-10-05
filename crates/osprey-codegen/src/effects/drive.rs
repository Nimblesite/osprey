//! Continuation mailbox dispatch and completion.
use super::{unbox_coro_value, Codegen, DriveArm, LType, Result};

pub(super) fn emit_drive_fn(
    cg: &mut Codegen,
    name: &str,
    arms: &[DriveArm],
    return_fn: Option<&str>,
) -> Result<()> {
    let saved = cg.enter_nested_fn();
    let params = vec![
        (LType::Ptr, String::from("__env")),
        (LType::Ptr, String::from("__coro")),
    ];
    let done = cg.call("i64", "__osprey_coro_done", "i8*", &["%__coro"]);
    let done_cond = cg.emit_reg(format!("icmp ne i64 {done}, 0"));
    let done_lbl = cg.fresh_label();
    let dispatch_lbl = cg.fresh_label();
    cg.emit(format!(
        "br i1 {done_cond}, label %{done_lbl}, label %{dispatch_lbl}"
    ));

    cg.start_block(&done_lbl);
    let result = cg.call("i64", "__osprey_coro_result", "i8*", &["%__coro"]);
    let result = match return_fn {
        Some(function) => cg.emit_reg(format!("call i64 @{function}(i8* %__env, i64 {result})")),
        None => result,
    };
    cg.emit(format!("ret i64 {result}"));

    cg.start_block(&dispatch_lbl);
    // Take the mailbox before reading it. An arm that resumes lets the body
    // perform again, and that nested suspension installs a mailbox of its own;
    // taking clears the coro's slot so the two activations never alias, and
    // makes this activation responsible for retiring the one it holds.
    let mail = cg.call("i8*", "__osprey_coro_take_args", "i8*", &["%__coro"]);
    let op = cg.call("i64", "__osprey_coro_mail_op", "i8*", &[&mail]);
    let miss_lbl = cg.fresh_label();
    let check_labels: Vec<String> = arms.iter().map(|_| cg.fresh_label()).collect();
    let arm_labels: Vec<String> = arms.iter().map(|_| cg.fresh_label()).collect();
    if let Some(first) = check_labels.first() {
        cg.emit(format!("br label %{first}"));
    } else {
        cg.emit(format!("br label %{miss_lbl}"));
    }

    for (i, ((arm, check_label), arm_label)) in
        arms.iter().zip(&check_labels).zip(&arm_labels).enumerate()
    {
        cg.start_block(check_label);
        let cmp = cg.emit_reg(format!("icmp eq i64 {op}, {}", arm.op_id));
        let next = check_labels.get(i + 1).map_or(&miss_lbl, |label| label);
        cg.emit(format!("br i1 {cmp}, label %{arm_label}, label %{next}"));
    }

    for (arm, arm_label) in arms.iter().zip(&arm_labels) {
        cg.start_block(arm_label);
        let mut args = vec![String::from("i8* %__env"), String::from("i8* %__coro")];
        for (idx, param) in arm.sig.params.iter().cloned().enumerate() {
            let raw = cg.call(
                "i64",
                "__osprey_coro_mail_arg",
                "i8*, i64",
                &[&mail, &idx.to_string()],
            );
            let value = unbox_coro_value(cg, &raw, param.ty, param.result_inner);
            let value = crate::cast::coerce_param(cg, value, &param)?;
            args.push(value.typed());
        }
        let arm_result = cg.emit_reg(format!("call i64 @{}({})", arm.arm_fn, args.join(", ")));
        // The arm borrowed its operands, so retiring the mailbox now drops the
        // +1 the performer handed over. Anything the arm kept — stored into
        // handler state, returned, or passed to `resume` — it retained itself.
        cg.call_void("__osprey_coro_mail_free", "i8*", &[&mail]);
        if arm.sig.mode.is_control() {
            emit_abandon_or_answer(cg, &arm_result);
        } else {
            emit_substitute_and_continue(cg, name, &arm_result);
        }
    }

    cg.start_block(&miss_lbl);
    // No arm claimed this operation: the mailbox is still this activation's to
    // retire, or its managed operands outlive the program.
    cg.call_void("__osprey_coro_mail_free", "i8*", &[&mail]);
    cg.call_void("__osprey_coro_abort", "i8*", &["%__coro"]);
    cg.emit("ret i64 0");
    cg.exit_nested_fn(saved, "i64", name, &params);
    Ok(())
}

/// Dispatch tail for an arm that captures the continuation. Its `resume` calls
/// already drove the computation, so reaching here with the computation still
/// suspended means this arm returned WITHOUT resuming on the path it took: that
/// ABANDONS the continuation, and the arm's value answers for the whole region.
/// Implements [EFFECTS-HANDLER-ARMS].
pub(super) fn emit_abandon_or_answer(cg: &mut Codegen, arm_result: &str) {
    let done = cg.call("i64", "__osprey_coro_done", "i8*", &["%__coro"]);
    let cond = cg.emit_reg(format!("icmp ne i64 {done}, 0"));
    let abort_lbl = cg.fresh_label();
    let return_lbl = cg.fresh_label();
    cg.emit(format!(
        "br i1 {cond}, label %{return_lbl}, label %{abort_lbl}"
    ));
    cg.start_block(&abort_lbl);
    cg.call_void("__osprey_coro_abort", "i8*", &["%__coro"]);
    cg.emit(format!("br label %{return_lbl}"));
    cg.start_block(&return_lbl);
    cg.emit(format!("ret i64 {arm_result}"));
}

/// Dispatch tail for a declared value operation. Its value substitutes for the
/// operation's result: hand it to the suspended computation and keep driving,
/// so the rest of the handled body runs and the region still answers with the
/// body's own value. Killing the computation here instead is issue #177 — a
/// sibling arm's `resume` silently converted this arm into an early exit.
/// Implements [EFFECTS-HANDLER-ARMS].
pub(super) fn emit_substitute_and_continue(cg: &mut Codegen, drive_fn: &str, arm_result: &str) {
    let _ = cg.call(
        "i64",
        "__osprey_coro_resume",
        "i8*, i64",
        &["%__coro", arm_result],
    );
    // One completion path applies the return clause; musttail keeps repeated
    // value requests in a mixed handler constant-space.
    let answer = cg.emit_reg(format!(
        "musttail call i64 @{drive_fn}(i8* %__env, i8* %__coro)"
    ));
    cg.emit(format!("ret i64 {answer}"));
}

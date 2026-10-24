//! Ownership transferred into constructor fields.
use super::{conditional_floor, Codegen};

/// Perceus "move into constructor" (TR §2.3, the *own* argument rule): a store
/// into a heap slot normally dups, because the owning block's drop mask will
/// release that slot at the object's death. But when the stored pointer is a
/// freshly-produced owner the current region still holds, the store can MOVE it
/// instead — the +1 the region was about to drop at its end becomes the field's
/// reference. No dup, no region-end release: `store_field` calls this before
/// falling back to [`super::dup_store`], turning `retain(x); store x, field; …;
/// release(x)` into a single `store x, field` on every constructor argument.
///
/// The safety gate is *unnamed entry of the innermost frame* — the exact rc == 1
/// shape [`super::consume_fresh`] proves. `let`-bound owners are *named* and a value
/// that might be read again is therefore never moved out from under its later
/// uses; cell-backed `mut` owners ([`super::own_beyond_stmt`]) and every parent-region
/// owner live in an outer frame and are likewise excluded. A borrow (parameter,
/// field load, phi) was never registered, so it is not found and the caller
/// dups it as before. Returns true when it consumed an owner.
pub(crate) fn consume_into_store(cg: &mut Codegen, operand: &str) -> bool {
    match take_owner(cg, operand) {
        Some(slot) => {
            cg.emit(format!("store i8* null, i8** {slot}"));
            true
        }
        None => false,
    }
}

fn take_owner(cg: &mut Codegen, operand: &str) -> Option<String> {
    let floor = conditional_floor(cg);
    cg.arc.frames.last_mut().and_then(|frame| {
        frame
            .iter()
            .rposition(|e| e.operand == operand && e.name.is_none())
            .filter(|at| *at >= floor)
            .map(|at| frame.remove(at).slot)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arc::{frame_mark, open_conditional, own};
    use crate::llty::{LType, Value};

    #[test]
    fn a_branch_cannot_consume_an_owner_needed_by_its_sibling() {
        let mut cg = Codegen::new();
        cg.begin_function("conditional_constructor", None);
        let owner = Value::new("%before_branch", LType::Ptr);
        own(&mut cg, &owner);
        let mark = frame_mark(&cg);
        open_conditional(&mut cg, mark);
        assert!(!consume_into_store(&mut cg, &owner.operand));
        assert_eq!(frame_mark(&cg), mark);
    }

    #[test]
    fn a_constructor_can_consume_an_owner_created_inside_its_branch() {
        let mut cg = Codegen::new();
        cg.begin_function("branch_local_constructor", None);
        open_conditional(&mut cg, 0);
        let owner = Value::new("%in_branch", LType::Ptr);
        own(&mut cg, &owner);
        assert!(consume_into_store(&mut cg, &owner.operand));
        assert_eq!(frame_mark(&cg), 0);
    }

    #[test]
    fn matching_cannot_move_a_temporary_only_on_the_success_path() {
        let source = "type Box<T> = { items: List<T> }\n\
            fn choose(items, ok) = if ok { Box { items: items } } else { Box { items: [] } }\n\
            let answer = choose(listAppend([], 1), false)\n";
        let parsed = osprey_syntax::parse_program(source);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let ir = crate::compile_program(&parsed.program).expect("valid constructor match");
        let allocation = ir
            .lines()
            .find(|line| line.contains(" = call i8* @osprey_list_append_of"));
        assert!(allocation.is_some(), "missing temporary list: {ir}");
        let Some(operand) = allocation.and_then(|line| line.split_whitespace().next()) else {
            return;
        };
        assert!(
            ir.contains(&format!("@osp_retain(i8* {operand})")),
            "a conditional store must retain the outer owner: {ir}"
        );
    }
}

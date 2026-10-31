use crate::check::Checker;
use crate::ty::{names, Type};

impl Checker {
    /// A known erased arm keeps the whole result erased, regardless of arm
    /// order. A concrete result annotation must still pass one-way assignment.
    /// Implements [TYPE-ANY].
    pub(super) fn match_result(&mut self, result: Type, bodies: &[Type]) -> Type {
        if bodies
            .iter()
            .any(|ty| self.ctx.apply(ty).is_named(names::ANY))
        {
            return self.erased_match_result(bodies);
        }
        for body in bodies {
            self.push_unify(&result, body);
        }
        result
    }

    fn erased_match_result(&mut self, bodies: &[Type]) -> Type {
        let result = Type::con(names::ANY, Vec::new());
        for body in bodies {
            self.push_assign(&result, body);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use crate::testutil::{bad_with, ok};
    use osprey_syntax::parse_program;

    #[test]
    fn a_known_erased_match_arm_keeps_the_join_erased_in_either_order() {
        for arms in [
            "true => Viewed { text: \"new\" } false => old",
            "true => old false => Viewed { text: \"new\" }",
        ] {
            let source = format!("type Viewed = {{ text: string }}\nfn choose(flag, old: any) = match flag {{ {arms} }}\n");
            ok(&source);
            let parsed = parse_program(&source);
            assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
            let types = crate::infer_program(&parsed.program);
            let (_, returned) = types.functions.get("choose").expect("inferred function");
            assert!(
                returned.is_named(crate::names::ANY),
                "unsafe inferred join: {returned:?}"
            );
        }
    }

    #[test]
    fn a_match_cannot_recover_an_erased_arm_by_its_return_annotation() {
        bad_with("type Viewed = { text: string }\nfn choose(flag, old: any) -> Viewed = match flag { true => Viewed { text: \"new\" } false => old }\n", "cannot recover");
    }

    #[test]
    fn an_erased_match_join_cannot_erase_an_anonymous_record_arm() {
        for arms in [
            "true => { text: \"new\" } false => old",
            "true => old false => { text: \"new\" }",
        ] {
            bad_with(
                &format!("fn choose(flag, old: any) = match flag {{ {arms} }}\n"),
                "cannot erase an anonymous record",
            );
        }
    }
}

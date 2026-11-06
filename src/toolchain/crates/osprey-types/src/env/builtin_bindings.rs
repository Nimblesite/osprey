use super::TypeEnv;

#[derive(Clone, Copy, Debug)]
pub(super) enum Builtin {
    Pure,
    Runtime,
}

impl TypeEnv {
    pub(super) fn shadow_declaration(&mut self, name: &str) {
        if self.type_params(name).is_some() {
            let _ = self.frame().type_params.insert(name.to_owned(), None);
        }
        if self.is_builtin(name) {
            let _ = self.frame().builtins.insert(name.to_owned(), None);
        }
    }

    pub(crate) fn mark_builtin(&mut self, name: &str) {
        let _ = self
            .frame()
            .builtins
            .entry(name.to_owned())
            .or_insert(Some(Builtin::Pure));
    }

    pub(crate) fn mark_runtime_builtin(&mut self, name: &str) {
        let _ = self
            .frame()
            .builtins
            .insert(name.to_owned(), Some(Builtin::Runtime));
    }

    pub(crate) fn is_builtin(&self, name: &str) -> bool {
        self.lookup(|frame| frame.builtins.get(name))
            .is_some_and(Option::is_some)
    }

    pub(crate) fn is_runtime_builtin(&self, name: &str) -> bool {
        self.lookup(|frame| frame.builtins.get(name))
            .is_some_and(|binding| matches!(binding, Some(Builtin::Runtime)))
    }
}

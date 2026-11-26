//! The built-in source prelude: independent units of Osprey declarations a
//! program may use without declaring them. A unit attaches on demand — only
//! to a program that names one of its declarations — so a program that does
//! not is checked and compiled exactly as written. `json` implements
//! [BUILTIN-JSON-TYPED]; `signals` implements [STAGE-SIGNALS-STD]; `testing`
//! implements [TESTING-ARITH-POLICY]; `errors` implements [BUILTIN-LIST-GET].

use osprey_ast::{walk_program, AstVisitor, Expr, Program, Stmt};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

const ERRORS: &str = include_str!("prelude/errors.osp");
/// The built-in a list read is; `xs[i]` is its other spelling.
const LIST_GET: &str = "listGet";
const JSON: &str = include_str!("prelude/json.osp");
const SIGNALS: &str = include_str!("prelude/signals.osp");
const TESTING: &str = include_str!("prelude/testing.osp");
const TESTING_GPU: &str = include_str!("prelude/testing_gpu.osp");
const TESTING_RESULT: &str = include_str!("prelude/testing_result.osp");
const TESTING_SETUP: &str = include_str!("prelude/testing_setup.osp");

/// Prelude positions start here, past any assembled project, so no prelude
/// inference site shares a key with a source one and coverage can leave the
/// prelude's lines out of a program's report.
pub const PRELUDE_LINE_BASE: u32 = 1 << 24;

/// Each unit's rebased lines start one stride apart, so no unit shares a
/// line with another and every prelude line stays past every source line.
pub(crate) const PRELUDE_UNIT_STRIDE: u32 = 1 << 20;

/// Whether a source line belongs to the prelude rather than the program.
#[must_use]
pub fn is_prelude_line(line: u32) -> bool {
    line > PRELUDE_LINE_BASE
}

/// One parsed, rebased prelude unit and the names it declares.
struct Loaded {
    program: Program,
    names: Names,
}

fn loaded(source: &'static str, base: u32) -> Loaded {
    let mut program = osprey_syntax::parse_program(source).program;
    osprey_ast::span::offset_program(&mut program, base);
    Loaded {
        names: Names::declared(&program),
        program,
    }
}

/// Every unit, parsed once. Each source is a constant of this crate and a
/// unit test proves it parses clean, so the parse never reports errors here.
/// Each is rebased one stride past the last, so the units share no line.
struct Units {
    json: Loaded,
    signals: Loaded,
    testing: Loaded,
    testing_gpu: Loaded,
    testing_result: Loaded,
    testing_setup: Loaded,
    errors: Loaded,
}

fn units() -> &'static Units {
    static UNITS: OnceLock<Units> = OnceLock::new();
    let at = |source, n: u32| loaded(source, PRELUDE_LINE_BASE + n * PRELUDE_UNIT_STRIDE);
    UNITS.get_or_init(|| Units {
        json: at(JSON, 0),
        signals: at(SIGNALS, 1),
        testing: at(TESTING, 2),
        testing_gpu: at(TESTING_GPU, 3),
        testing_result: at(TESTING_RESULT, 4),
        errors: at(ERRORS, 5),
        testing_setup: at(TESTING_SETUP, 6),
    })
}

/// The program with the prelude units it needs in front of it. A unit
/// attaches when the program names one of its declarations and declares none
/// of its data names; `signals` is written against `json`, so it attaches
/// only beside it. A program that declares a prelude type, constructor or
/// effect keeps its own and gets none of that unit; one that declares a
/// prelude function name keeps its own function in place of that one.
/// `errors` attaches beside every list read, the units' own included.
/// Attaching is idempotent.
pub(crate) fn with_prelude(program: &Program) -> Cow<'_, Program> {
    if attached(program) {
        return Cow::Borrowed(program);
    }
    let own = Names::declared(program);
    let uses = Uses::of(program);
    let all = units();
    let (json, signals) = (&all.json, &all.signals);
    // `signals` is written against `json`'s declarations, so it attaches only
    // beside them: a program that declares json's data names gets no signals
    // unit either. [STAGE-SIGNALS-STD]
    let beside_json = own.data.is_disjoint(&json.names.data);
    let signals_wanted = beside_json && wanted(&signals.names, &own, &uses);
    let json_wanted = wanted(&json.names, &own, &uses) || signals_wanted;
    let gpu_wanted = wanted(&all.testing_gpu.names, &own, &uses);
    let reads_list = json_wanted || gpu_wanted || uses.names.contains(LIST_GET);
    let want = |unit: &Loaded| wanted(&unit.names, &own, &uses);
    let units: Vec<&Loaded> = [
        (reads_list || want(&all.errors), &all.errors),
        (json_wanted, json),
        (signals_wanted, signals),
        (want(&all.testing), &all.testing),
        (gpu_wanted, &all.testing_gpu),
        (want(&all.testing_result), &all.testing_result),
        (want(&all.testing_setup), &all.testing_setup),
    ]
    .into_iter()
    .filter_map(|(want, unit)| want.then_some(unit))
    .collect();
    if units.is_empty() {
        return Cow::Borrowed(program);
    }
    Cow::Owned(attach(program, &units, &own))
}

/// Whether `unit` attaches: the program names one of its declarations — a
/// type, constructor, effect or public function it does not define itself —
/// and declares none of its data names.
fn wanted(unit: &Names, own: &Names, uses: &Uses) -> bool {
    own.data.is_disjoint(&unit.data)
        && uses.names.iter().any(|name| {
            unit.data.contains(name)
                || (unit.functions.contains(name)
                    && !own.functions.contains(name)
                    && !name.starts_with("__"))
        })
}

/// The program's constructions re-folded against every attached unit's
/// declared arities: the program was parsed without the prelude in view, so
/// its parser left every `JsonInt(1)` a call.
fn unit_arities(units: &[&Loaded]) -> BTreeMap<String, usize> {
    let mut arities = BTreeMap::new();
    for unit in units {
        arities.extend(osprey_ast::positional::declared_arities(
            &unit.program.statements,
        ));
    }
    arities
}

/// The units' statements in front of the program's, each prelude function the
/// program redeclares left out.
fn attach(program: &Program, units: &[&Loaded], own: &Names) -> Program {
    let statements = units
        .iter()
        .flat_map(|unit| &unit.program.statements)
        .filter(|statement| {
            function_name(statement).is_none_or(|name| !own.functions.contains(name))
        })
        .chain(&program.statements)
        .cloned()
        .collect();
    let mut attached = Program {
        statements,
        doc: program.doc.clone(),
    };
    osprey_ast::positional::fold_constructions(&mut attached, &unit_arities(units));
    attached
}

/// Whether a program already has the prelude in front of it: units go first,
/// so a first statement on a prelude line is the tell.
fn attached(program: &Program) -> bool {
    program
        .statements
        .first()
        .and_then(osprey_ast::stmt_meta::statement_position)
        .is_some_and(|position| is_prelude_line(position.line))
}

fn function_name(statement: &Stmt) -> Option<&str> {
    match statement {
        Stmt::Function { name, .. } => Some(name),
        _ => None,
    }
}

#[derive(Default)]
struct Names {
    /// Declared type, constructor and effect names.
    data: BTreeSet<String>,
    /// Declared function names.
    functions: BTreeSet<String>,
}

impl Names {
    fn declared(program: &Program) -> Self {
        let mut names = Self::default();
        walk_program(program, &mut names);
        names
    }
}

impl AstVisitor for Names {
    fn statement(&mut self, statement: &Stmt) {
        match statement {
            Stmt::Type { name, variants, .. } => {
                let _ = self.data.insert(name.clone());
                self.data
                    .extend(variants.iter().map(|variant| variant.name.clone()));
            }
            Stmt::Function { name, .. } => {
                let _ = self.functions.insert(name.clone());
            }
            Stmt::Effect { name, .. } => {
                let _ = self.data.insert(name.clone());
            }
            _ => {}
        }
    }
}

#[derive(Default)]
struct Uses {
    names: BTreeSet<String>,
}

impl Uses {
    fn of(program: &Program) -> Self {
        let mut uses = Self::default();
        walk_program(program, &mut uses);
        uses
    }
}

impl AstVisitor for Uses {
    fn expression(&mut self, expression: &Expr) {
        match expression {
            Expr::Identifier(name) | Expr::TypeConstructor { name, .. } => {
                let _ = self.names.insert(name.clone());
            }
            Expr::MethodCall { method, .. } if method == LIST_GET => {
                let _ = self.names.insert(method.clone());
            }
            Expr::Index { .. } => {
                let _ = self.names.insert(LIST_GET.to_owned());
            }
            // A perform or handler names the effect declaration it uses,
            // whatever instantiation the mention carries.
            Expr::Perform { effect, .. } | Expr::Handler { effect, .. } => {
                let _ = self
                    .names
                    .insert(osprey_ast::effect_name::base(effect).to_owned());
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests;

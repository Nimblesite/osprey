//! Exhaustiveness and reachability over nested patterns, decided by pattern
//! usefulness (Maranget, "Warnings for pattern matching", 2007). A pattern is
//! useful against earlier rows when some value matches it and none of them;
//! an arm that is not useful is unreachable, and `_` useful against every arm
//! means some value reaches no arm — the witness shows one. Implements
//! [PATTERN-NESTED] and [TYPE-MATCH-EXHAUSTIVE].

use super::is_result_variant;
use crate::check::Checker;
use crate::error::TypeError;
use crate::ty::names;
use osprey_ast::{Expr, MatchArm, Pattern};

/// The value space a pattern covers: everything, one constructor applied to
/// the spaces of its slots, or one value of an unbounded domain (a number,
/// string or list) that no finite set of arms covers, keyed and spelled.
#[derive(Clone)]
enum Space {
    Any,
    Ctor(String, Vec<Space>),
    Opaque(String, String),
    /// An or-pattern: the union of its alternatives ([PATTERN-OR-EXHAUSTIVE]).
    Or(Vec<Space>),
}

/// A matrix row: one space per column.
type Row = Vec<Space>;

/// The constructor name a closed row pattern is analysed under: its fields,
/// sorted, so `{ x, y }` and `{ y, x }` are one shape.
const ROW_OPEN: &str = "{";

/// The two list constructors the analysis reads `[…]` patterns as.
const LIST_NIL: &str = "[]";
const LIST_CONS: &str = "::";

/// Whether a pattern tests a value below its top: a constructor slot or a row
/// field that is not a binding or `_`.
pub(super) fn nested(pattern: &Pattern) -> bool {
    match pattern {
        Pattern::Or(_) => true,
        Pattern::Constructor { sub_patterns, .. } => {
            sub_patterns.iter().any(|slot| !slot.is_irrefutable())
        }
        Pattern::Structural { fields, .. } => fields.iter().any(|(_, slot)| !slot.is_irrefutable()),
        _ => false,
    }
}

impl Checker {
    /// Reject each arm earlier arms already cover, then a match some value
    /// escapes, naming one such value.
    pub(super) fn check_nested_match(&mut self, arms: &[MatchArm]) {
        let rows: Vec<Row> = arms
            .iter()
            .map(|arm| vec![self.space(&arm.pattern)])
            .collect();
        for (index, row) in rows.iter().enumerate() {
            let earlier = rows.get(..index).unwrap_or_default();
            if self.useful(earlier, row).is_none() {
                self.errors.push(TypeError::new(format!(
                    "unreachable match arm: earlier arms cover every value `{}` matches",
                    self.render_row(row)
                )));
            } else {
                self.check_alternatives(earlier, row);
            }
        }
        if let Some(witness) = self.useful(&rows, &[Space::Any]) {
            self.errors.push(TypeError::new(format!(
                "non-exhaustive match: no arm matches `{}`",
                self.render_row(&witness)
            )));
        }
    }

    /// Reject each alternative of a reachable arm that the earlier arms, or
    /// the earlier alternatives of the same or-pattern, already cover; the
    /// diagnostic names that alternative, not the whole arm.
    fn check_alternatives(&mut self, earlier: &[Row], row: &[Space]) {
        let Some(space) = row.first() else {
            return;
        };
        for (vector, before, shown) in alternative_checks(space) {
            let mut rows = earlier.to_vec();
            rows.extend(before.into_iter().map(|space| vec![space]));
            if self.useful(&rows, &[vector]).is_none() {
                self.errors.push(TypeError::new(format!(
                    "unreachable alternative: earlier arms or alternatives cover every value `{}` matches",
                    self.render(&shown)
                )));
            }
        }
    }

    /// Spell a pattern as the Default pattern its space renders to.
    pub(super) fn render_pattern(&self, pattern: &Pattern) -> String {
        self.render(&self.space(pattern))
    }

    /// The value space of one pattern.
    fn space(&self, pattern: &Pattern) -> Space {
        match pattern {
            Pattern::Binding(name) if is_result_variant(name) => {
                Space::Ctor(name.clone(), vec![Space::Any])
            }
            Pattern::Binding(name) if self.names_variant(name) => {
                Space::Ctor(name.clone(), vec![Space::Any; self.arity(name)])
            }
            Pattern::Wildcard | Pattern::TypeAnnotated { .. } | Pattern::Binding(_) => Space::Any,
            Pattern::Literal(literal) => literal_space(literal),
            Pattern::List { elements, rest } => self.list_space(elements, rest.is_some()),
            Pattern::Constructor {
                name,
                fields,
                sub_patterns,
            } => self.constructor_space(name, fields, sub_patterns),
            Pattern::Structural { fields, open } => self.row_space(pattern, fields, *open),
            Pattern::Or(alternatives) => Space::Or(
                alternatives
                    .iter()
                    .map(|alternative| self.space(alternative))
                    .collect(),
            ),
        }
    }

    /// A list as nested cells: `[a, b]` is `a :: b :: []` and `[a, ...rest]`
    /// is `a :: _`, so `[]` and `[_, ..._]` together cover every list.
    fn list_space(&self, elements: &[Pattern], open: bool) -> Space {
        let end = if open {
            Space::Any
        } else {
            Space::Ctor(LIST_NIL.to_owned(), Vec::new())
        };
        elements.iter().rev().fold(end, |tail, element| {
            Space::Ctor(LIST_CONS.to_owned(), vec![self.space(element), tail])
        })
    }

    /// A constructor's slots in declared order: positional slots by column,
    /// labelled ones by field, and an unwritten slot matches anything.
    fn constructor_space(&self, name: &str, fields: &[String], slots: &[Pattern]) -> Space {
        if is_result_variant(name) {
            let payload = slots.first().map_or(Space::Any, |slot| self.space(slot));
            return Space::Ctor(name.to_owned(), vec![payload]);
        }
        let Some(info) = self.ctors.get(name) else {
            return Space::Any;
        };
        let args = info
            .fields
            .iter()
            .enumerate()
            .map(|(column, (declared, _))| {
                let slot = if fields.is_empty() {
                    slots.get(column)
                } else {
                    fields
                        .iter()
                        .position(|field| field == declared)
                        .and_then(|index| slots.get(index))
                };
                slot.map_or(Space::Any, |slot| self.space(slot))
            })
            .collect();
        Space::Ctor(name.to_owned(), args)
    }

    /// A closed row is the one shape of its record; an open row selects by a
    /// subset of fields, so it is analysed as a value of its own.
    fn row_space(&self, pattern: &Pattern, fields: &[(String, Pattern)], open: bool) -> Space {
        if open {
            return Space::Opaque(format!("{pattern:p}"), "{ .. }".to_owned());
        }
        let mut sorted: Vec<&(String, Pattern)> = fields.iter().collect();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));
        let name = sorted
            .iter()
            .map(|(field, _)| field.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let args = sorted.iter().map(|(_, slot)| self.space(slot)).collect();
        Space::Ctor(format!("{ROW_OPEN}{name}"), args)
    }

    fn arity(&self, name: &str) -> usize {
        self.ctors.get(name).map_or(0, |info| info.fields.len())
    }

    /// Every constructor of the type `name` belongs to, with its arity; `None`
    /// for a domain no finite set of constructors covers.
    fn signature(&self, name: &str) -> Option<Vec<(String, usize)>> {
        if name == "true" || name == "false" {
            return Some(vec![("true".to_owned(), 0), ("false".to_owned(), 0)]);
        }
        if is_result_variant(name) {
            return Some(vec![
                (names::SUCCESS.to_owned(), 1),
                ("Error".to_owned(), 1),
            ]);
        }
        if name == LIST_NIL || name == LIST_CONS {
            return Some(vec![(LIST_NIL.to_owned(), 0), (LIST_CONS.to_owned(), 2)]);
        }
        if let Some(row) = name.strip_prefix(ROW_OPEN) {
            return Some(vec![(name.to_owned(), row.split(',').count())]);
        }
        let info = self.ctors.get(name)?;
        let variants = self
            .union_variants
            .get(&info.owner)
            .cloned()
            .unwrap_or_else(|| vec![name.to_owned()]);
        Some(
            variants
                .into_iter()
                .map(|v| (v.clone(), self.arity(&v)))
                .collect(),
        )
    }

    /// The witness extending `vector` that no row of `matrix` matches, or
    /// `None` when every value `vector` matches some row matches first.
    fn useful(&self, matrix: &[Row], vector: &[Space]) -> Option<Row> {
        let expanded = expand(matrix);
        let matrix = expanded.as_slice();
        let Some((head, rest)) = vector.split_first() else {
            return matrix.is_empty().then(Vec::new);
        };
        match head {
            Space::Ctor(name, args) => {
                let inner = [args.as_slice(), rest].concat();
                let witness = self.useful(&specialize(matrix, name, args.len()), &inner)?;
                Some(rebuild(name, args.len(), &witness))
            }
            Space::Opaque(key, _) => {
                let witness = self.useful(&specialize(matrix, key, 0), rest)?;
                Some(prepend(Space::Any, witness))
            }
            Space::Any => self.useful_any(matrix, rest),
            Space::Or(alternatives) => alternatives.iter().find_map(|alternative| {
                self.useful(matrix, &prepend(alternative.clone(), rest.to_vec()))
            }),
        }
    }

    /// `_` against a column: when the column names every constructor of its
    /// type, try each; otherwise a constructor it lacks (or any value of an
    /// unbounded domain) escapes wherever the rows starting with `_` let it.
    fn useful_any(&self, matrix: &[Row], rest: &[Space]) -> Option<Row> {
        let heads = column_heads(matrix);
        let signature = heads.first().and_then(|name| self.signature(name));
        let missing = signature.as_ref().map(|signature| {
            signature
                .iter()
                .find(|(name, _)| !heads.contains(name))
                .cloned()
        });
        match (signature, missing) {
            (Some(signature), Some(None)) => self.useful_each(matrix, rest, signature),
            (_, missing) => {
                let witness = self.useful(&default_rows(matrix), rest)?;
                let head = missing.flatten().map_or(Space::Any, |(name, arity)| {
                    Space::Ctor(name, vec![Space::Any; arity])
                });
                Some(prepend(head, witness))
            }
        }
    }

    /// A complete column: `_` is useful exactly when it is for some
    /// constructor applied to `_`s.
    fn useful_each(
        &self,
        matrix: &[Row],
        rest: &[Space],
        signature: Vec<(String, usize)>,
    ) -> Option<Row> {
        signature.into_iter().find_map(|(name, arity)| {
            let inner = [vec![Space::Any; arity].as_slice(), rest].concat();
            let witness = self.useful(&specialize(matrix, &name, arity), &inner)?;
            Some(rebuild(&name, arity, &witness))
        })
    }

    fn render_row(&self, row: &[Space]) -> String {
        row.first()
            .map_or_else(|| "_".to_owned(), |space| self.render(space))
    }

    /// A space spelled as the Default pattern that matches it.
    fn render(&self, space: &Space) -> String {
        let (name, args) = match space {
            Space::Ctor(name, args) => (name, args),
            Space::Opaque(_, shown) => return shown.clone(),
            Space::Any => return "_".to_owned(),
            Space::Or(alternatives) => {
                let shown: Vec<String> = alternatives
                    .iter()
                    .map(|space| self.render(space))
                    .collect();
                return shown.join(" | ");
            }
        };
        let shown: Vec<String> = args.iter().map(|arg| self.render(arg)).collect();
        match name.as_str() {
            LIST_CONS => render_list(space, &|space| self.render(space)),
            _ if args.is_empty() => name.clone(),
            names::SUCCESS => format!("Success {{ value: {} }}", shown.join(", ")),
            "Error" => format!("Error {{ message: {} }}", shown.join(", ")),
            _ => match name.strip_prefix(ROW_OPEN) {
                Some(row) => labelled("", row.split(',').map(str::to_owned), &shown),
                None => self.render_ctor(name, &shown),
            },
        }
    }

    fn render_ctor(&self, name: &str, shown: &[String]) -> String {
        let declared: Vec<String> = self.ctors.get(name).map_or_else(Vec::new, |info| {
            info.fields.iter().map(|(field, _)| field.clone()).collect()
        });
        if declared
            .first()
            .is_none_or(|field| osprey_ast::is_positional_field(field))
        {
            return format!("{name}({})", shown.join(", "));
        }
        labelled(&format!("{name} "), declared.into_iter(), shown)
    }
}

/// A chain of list cells spelled as the list pattern it is: `[a, b]` when it
/// ends in `[]`, `[a, ..._]` when its tail is open.
fn render_list(space: &Space, render: &dyn Fn(&Space) -> String) -> String {
    let mut shown = Vec::new();
    let mut cell = space;
    loop {
        match cell {
            Space::Ctor(name, args) if name == LIST_CONS => {
                if let Some(head) = args.first() {
                    shown.push(render(head));
                }
                match args.get(1) {
                    Some(tail) => cell = tail,
                    None => break,
                }
            }
            Space::Ctor(name, _) if name == LIST_NIL => break,
            _ => {
                shown.push("..._".to_owned());
                break;
            }
        }
    }
    format!("[{}]", shown.join(", "))
}

/// `Name { a: x, b: y }`, or `{ a: x }` for a bare row.
fn labelled(prefix: &str, fields: impl Iterator<Item = String>, shown: &[String]) -> String {
    let slots: Vec<String> = fields
        .zip(shown)
        .map(|(field, value)| format!("{field}: {value}"))
        .collect();
    format!("{prefix}{{ {} }}", slots.join(", "))
}

fn literal_space(literal: &Expr) -> Space {
    let shown = match literal {
        Expr::Bool(value) => return Space::Ctor(value.to_string(), Vec::new()),
        Expr::Integer(value) => value.to_string(),
        Expr::Float(value) => value.to_string(),
        Expr::Str(text) => format!("{text:?}"),
        _ => "_".to_owned(),
    };
    Space::Opaque(format!("{literal:?}"), shown)
}

/// The constructors heading a column's rows, first seen first.
fn column_heads(matrix: &[Row]) -> Vec<String> {
    let mut heads: Vec<String> = Vec::new();
    for row in matrix {
        if let Some(Space::Ctor(name, _)) = row.first() {
            if !heads.contains(name) {
                heads.push(name.clone());
            }
        }
    }
    heads
}

/// The rows that match constructor `name` in the first column, with its
/// slots in place of that column.
fn specialize(matrix: &[Row], name: &str, arity: usize) -> Vec<Row> {
    matrix
        .iter()
        .filter_map(|row| {
            let (head, rest) = row.split_first()?;
            let args = match head {
                Space::Ctor(head, args) if head == name => args.clone(),
                Space::Opaque(key, _) if key == name => Vec::new(),
                Space::Any => vec![Space::Any; arity],
                _ => return None,
            };
            Some([args.as_slice(), rest].concat())
        })
        .collect()
}

/// Each row headed by an or-pattern as one row per alternative, in order:
/// `A | B => body` is the rows `A` and `B` ([PATTERN-OR-EXHAUSTIVE]).
fn expand(matrix: &[Row]) -> Vec<Row> {
    matrix
        .iter()
        .flat_map(|row| match row.split_first() {
            Some((Space::Or(alternatives), rest)) => expand(
                &alternatives
                    .iter()
                    .map(|alternative| prepend(alternative.clone(), rest.to_vec()))
                    .collect::<Vec<_>>(),
            ),
            _ => vec![row.clone()],
        })
        .collect()
}

/// One check per alternative of every or-pattern in `space`, outermost first:
/// the space with that or-pattern narrowed to the alternative, the spaces it
/// narrows to at each earlier alternative (which the check counts as earlier
/// rows), and the alternative alone, for the diagnostic.
fn alternative_checks(space: &Space) -> Vec<(Space, Vec<Space>, Space)> {
    match space {
        Space::Or(alternatives) => alternatives
            .iter()
            .enumerate()
            .flat_map(|(index, alternative)| {
                let earlier: Vec<Space> = alternatives.iter().take(index).cloned().collect();
                let own = (alternative.clone(), earlier.clone(), alternative.clone());
                let nested = alternative_checks(alternative).into_iter().map(
                    move |(vector, mut before, shown)| {
                        before.extend(earlier.iter().cloned());
                        (vector, before, shown)
                    },
                );
                std::iter::once(own).chain(nested).collect::<Vec<_>>()
            })
            .collect(),
        Space::Ctor(name, args) => args
            .iter()
            .enumerate()
            .flat_map(|(column, arg)| {
                alternative_checks(arg)
                    .into_iter()
                    .map(move |(vector, before, shown)| {
                        let with = |slot: Space| {
                            let mut slots = args.clone();
                            if let Some(target) = slots.get_mut(column) {
                                *target = slot;
                            }
                            Space::Ctor(name.clone(), slots)
                        };
                        (with(vector), before.into_iter().map(with).collect(), shown)
                    })
            })
            .collect(),
        Space::Any | Space::Opaque(..) => Vec::new(),
    }
}

/// The rows whose first column matches anything, without that column.
fn default_rows(matrix: &[Row]) -> Vec<Row> {
    matrix
        .iter()
        .filter_map(|row| match row.split_first() {
            Some((Space::Any, rest)) => Some(rest.to_vec()),
            _ => None,
        })
        .collect()
}

/// `name` applied to the first `arity` spaces of a witness, then the rest.
fn rebuild(name: &str, arity: usize, witness: &[Space]) -> Row {
    let split = arity.min(witness.len());
    let (args, rest) = witness.split_at(split);
    prepend(Space::Ctor(name.to_owned(), args.to_vec()), rest.to_vec())
}

fn prepend(head: Space, rest: Row) -> Row {
    let mut row = Vec::with_capacity(rest.len() + 1);
    row.push(head);
    row.extend(rest);
    row
}

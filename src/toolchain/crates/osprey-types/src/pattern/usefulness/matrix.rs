//! Row operations for match exhaustiveness and reachability.

use super::{Row, Space};

/// The constructors heading a column's rows, first seen first.
pub(super) fn column_heads(matrix: &[Row]) -> Vec<String> {
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
pub(super) fn specialize(matrix: &[Row], name: &str, arity: usize) -> Vec<Row> {
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
pub(super) fn expand(matrix: &[Row]) -> Vec<Row> {
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
pub(super) fn alternative_checks(space: &Space) -> Vec<(Space, Vec<Space>, Space)> {
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
pub(super) fn default_rows(matrix: &[Row]) -> Vec<Row> {
    matrix
        .iter()
        .filter_map(|row| match row.split_first() {
            Some((Space::Any, rest)) => Some(rest.to_vec()),
            _ => None,
        })
        .collect()
}

/// `name` applied to the first `arity` spaces of a witness, then the rest.
pub(super) fn rebuild(name: &str, arity: usize, witness: &[Space]) -> Row {
    let split = arity.min(witness.len());
    let (args, rest) = witness.split_at(split);
    prepend(Space::Ctor(name.to_owned(), args.to_vec()), rest.to_vec())
}

pub(super) fn prepend(head: Space, rest: Row) -> Row {
    let mut row = Vec::with_capacity(rest.len() + 1);
    row.push(head);
    row.extend(rest);
    row
}

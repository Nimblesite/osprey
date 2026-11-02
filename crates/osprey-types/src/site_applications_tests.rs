//! Queued specializations read exactly as copying and rewriting every site
//! would have. Implements [TYPE-GENERICS-APPLY].

use crate::site_applications::{Bindings, SiteApplications};
use crate::ty::Type;

fn con(name: &str, args: Vec<Type>) -> Type {
    Type::Con {
        name: name.to_owned(),
        args,
    }
}

fn sites() -> SiteApplications {
    [
        (
            (1, 1),
            Bindings::from([(0, con("List", vec![Type::Var(1)]))]),
        ),
        ((2, 5), Bindings::from([(3, Type::Var(2))])),
    ]
    .into_iter()
    .collect()
}

#[test]
fn an_unspecialized_site_reads_its_own_bindings() {
    let applications = sites();
    assert_eq!(
        applications.get(&(1, 1)),
        Some(Bindings::from([(0, con("List", vec![Type::Var(1)]))]))
    );
    assert_eq!(applications.get(&(9, 9)), None);
}

#[test]
fn nested_specializations_apply_in_order_and_leave_the_source_unchanged() {
    let applications = sites();
    let outer = applications.specialized(&Bindings::from([(1, Type::Var(2))]));
    let inner = outer.specialized(&Bindings::from([(2, con("int", vec![]))]));
    assert_eq!(
        inner.get(&(1, 1)),
        Some(Bindings::from([(0, con("List", vec![con("int", vec![])]))]))
    );
    assert_eq!(
        inner.get(&(2, 5)),
        Some(Bindings::from([(3, con("int", vec![]))]))
    );
    assert_eq!(
        outer.get(&(2, 5)),
        Some(Bindings::from([(3, Type::Var(2))]))
    );
    assert_eq!(applications.get(&(1, 1)), sites().get(&(1, 1)));
}

#[test]
fn an_inserted_site_joins_the_shared_sites_of_later_copies() {
    let mut applications = sites();
    applications.insert((0, 7), Bindings::from([(4, Type::Var(1))]));
    let specialized = applications.specialized(&Bindings::from([(1, con("str", vec![]))]));
    assert_eq!(
        specialized.get(&(0, 7)),
        Some(Bindings::from([(4, con("str", vec![]))]))
    );
}

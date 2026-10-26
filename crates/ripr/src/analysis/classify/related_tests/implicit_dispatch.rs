//! The unnamed owner calls a test can make through language syntax (#9567):
//! an operator, a loop or adaptor driving an iterator, the end of a scope that
//! drops a value. One owner has at most one reading, since each reads a
//! different trait's method.

use super::{
    FunctionSummary, RustIndex, TestSummary, drop_dispatch, iterator_dispatch, operator_dispatch,
};

/// The syntax-run reading of an owner.
pub(super) enum ImplicitOwner<'a> {
    Operator(operator_dispatch::OperatorOwner<'a>),
    Iterator(iterator_dispatch::IteratorOwner<'a>),
    Drop(drop_dispatch::DropOwner<'a>),
}

/// Whether `owner` is shaped like a method any reading can attribute. A
/// superset test for the candidate prefilter: no syntax here has a token worth
/// indexing, so same-file tests are scanned in full.
pub(super) fn has_implicit_shape(owner: &FunctionSummary) -> bool {
    operator_dispatch::has_operator_shape(owner)
        || iterator_dispatch::has_iterator_shape(owner)
        || drop_dispatch::has_drop_shape(owner)
}

/// The reading of `owner`, or `None` when no reading attributes it.
pub(super) fn implicit_owner<'a>(
    owner: &'a FunctionSummary,
    index: &RustIndex,
) -> Option<ImplicitOwner<'a>> {
    operator_dispatch::operator_owner(owner, index)
        .map(ImplicitOwner::Operator)
        .or_else(|| iterator_dispatch::iterator_owner(owner, index).map(ImplicitOwner::Iterator))
        .or_else(|| drop_dispatch::drop_owner(owner, index).map(ImplicitOwner::Drop))
}

/// Whether `test` reaches the owner through the syntax its reading follows.
pub(super) fn test_reaches_owner(
    test: &TestSummary,
    owner: &FunctionSummary,
    implicit: &ImplicitOwner<'_>,
    test_source: Option<&str>,
    index: &RustIndex,
) -> bool {
    match implicit {
        ImplicitOwner::Operator(operator) => operator_dispatch::test_applies_operator_to_owner(
            test,
            owner,
            operator,
            test_source,
            index,
        ),
        ImplicitOwner::Iterator(iterator) => {
            iterator_dispatch::test_drives_owner(test, owner, iterator, test_source, index)
        }
        ImplicitOwner::Drop(drop) => {
            drop_dispatch::test_drops_owner_value(test, owner, drop, test_source, index)
        }
    }
}

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

/// Whether byte `at` of a masked test body sits at the body's own top level,
/// not inside a closure, a nested `fn`, an `if`, a loop or any other block that
/// may never run when the test does. The body may begin with its `fn` header,
/// whose brace is then the top level.
pub(super) fn at_test_top_level(masked: &str, at: usize) -> bool {
    let depth_at = |end: usize| {
        masked[..end]
            .bytes()
            .fold(0isize, |depth, byte| match byte {
                b'{' => depth + 1,
                b'}' => depth - 1,
                _ => depth,
            })
    };
    let base = match masked.find('{') {
        Some(open) if masked[..open].split_whitespace().any(|word| word == "fn") => 1,
        _ => 0,
    };
    depth_at(at) == base
        && statement_runs_unconditionally(masked, at)
        && !may_exit_before(masked, at)
}

/// Whether an earlier statement can end the test before byte `at`: a `return`,
/// a `?`, or a panicking macro, at any depth (`if c { return; }` is the usual
/// shape). Conservative: it also refuses an exit inside a closure or an
/// unrelated branch, since the relation may only raise reach, never lower it.
fn may_exit_before(masked: &str, at: usize) -> bool {
    let body_start = match masked.find('{') {
        Some(open) if masked[..open].split_whitespace().any(|word| word == "fn") => open + 1,
        _ => 0,
    };
    let Some(prefix) = masked.get(body_start..at) else {
        return false;
    };
    if prefix.contains('?') {
        return true;
    }
    let bytes = prefix.as_bytes();
    let is_word = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
    let has_word = |word: &str, followed_by_bang: bool| {
        prefix.match_indices(word).any(|(start, _)| {
            let end = start + word.len();
            let before_ok = start == 0 || !is_word(bytes[start - 1]);
            let after = prefix[end..].trim_start();
            let after_ok = if followed_by_bang {
                after.starts_with('!')
            } else {
                end >= bytes.len() || !is_word(bytes[end])
            };
            before_ok && after_ok
        })
    };
    has_word("return", false)
        || ["panic", "unreachable", "todo", "unimplemented"]
            .iter()
            .any(|name| has_word(name, true))
}

/// Whether the statement that holds byte `at` is neither disabled by an
/// attribute (`#[cfg(any())] let n = ..;`) nor the body of a closure written
/// without braces (`let f = || drop(g);`). The statement starts after the last
/// `;`, `{` or `}` before `at`.
fn statement_runs_unconditionally(masked: &str, at: usize) -> bool {
    let start = masked[..at]
        .rfind([';', '{', '}'])
        .map_or(0, |separator| separator + 1);
    let head = masked[start..at].trim_start();
    !head.starts_with('#') && !head.contains('|')
}

/// Whether the impl that defines `owner` also defines a method called `name`:
/// an overridden provided method (`fn count(self)` inside `impl Iterator`) is
/// what the call runs, not the trait's default that drives `next`.
pub(super) fn impl_overrides(index: &RustIndex, owner: &FunctionSummary, name: &str) -> bool {
    let Some((impl_prefix, _)) = owner.id.0.rsplit_once("::") else {
        return false;
    };
    index.functions().iter().any(|function| {
        function.name == name
            && function
                .id
                .0
                .rsplit_once("::")
                .is_some_and(|(prefix, _)| prefix == impl_prefix)
    })
}

#[cfg(test)]
mod tests {
    use super::at_test_top_level;

    #[test]
    fn only_the_body_top_level_is_top_level() {
        let body = "fn t() {\n    a();\n    if x { b(); }\n    let f = || { c(); };\n    d();\n}";
        // Offset of the first `needle`; a missing one lands at the end, which
        // is the body's closing depth and fails the asserts below.
        let offset = |text: &str, needle: &str| text.split(needle).next().map_or(0, str::len);
        let at = |needle: &str| at_test_top_level(body, offset(body, needle));
        assert!(at("a()") && at("d()"));
        assert!(!at("b()") && !at("c()"));
        let gated =
            "fn t() {\n    #[cfg(any())]\n    drop(g);\n    let f = || drop(h);\n    keep(k);\n}";
        assert!(!at_test_top_level(gated, offset(gated, "drop(g")));
        assert!(!at_test_top_level(gated, offset(gated, "drop(h")));
        assert!(at_test_top_level(gated, offset(gated, "keep(k")));
        let bare = "a(); { b(); }";
        assert!(at_test_top_level(bare, 0));
        assert!(!at_test_top_level(bare, offset(bare, "b()")));
    }

    #[test]
    fn an_earlier_exit_refuses_the_top_level() {
        let offset = |text: &str, needle: &str| text.split(needle).next().map_or(0, str::len);
        let top_level = |exit: &str| {
            let body = format!("fn t() {{\n    {exit}\n    drop(g);\n}}");
            at_test_top_level(&body, offset(&body, "drop(g"))
        };
        assert!(!top_level("if c.get() == 0 { return; }"));
        assert!(!top_level("let v = parse()?;"));
        assert!(!top_level("if c { panic!(\"no\"); }"));
        assert!(!top_level("if c { unreachable!(); }"));
        // Names that merely contain an exit word do not refuse.
        assert!(top_level("let returned = 1; let not_panic = 2;"));
        assert!(top_level("keep(k);"));
        // An exit after the point does not refuse it.
        let after = "fn t() {\n    drop(g);\n    if c { return; }\n}";
        assert!(at_test_top_level(after, offset(after, "drop(g")));
    }
}

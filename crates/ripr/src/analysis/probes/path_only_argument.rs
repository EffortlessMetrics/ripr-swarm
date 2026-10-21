//! Changed arguments a value assertion cannot observe by construction (#9080).
//!
//! `compare_exchange(0, v, Ordering::AcqRel, ..)` rewritten to
//! `Ordering::Release` changes memory-ordering semantics, not any value a test
//! can compare. A strong whole-object or exact-value oracle on the reached
//! function says nothing about it, so the probe builder demotes such an edit to
//! `static_unknown` instead of seeding a value or effect family (the sibling
//! multi-line and `let`-bound forms of the same edit already carry it). This
//! module is the one rule that decides whether an edit is confined to
//! `std::sync::atomic::Ordering` variants.
//!
//! The rule is deliberately narrow and fail-closed: it reports `true` only
//! when `before` and `after` tokenize to the same shape and every differing
//! token pair is a qualified atomic ordering variant (`Ordering::Release`,
//! `atomic::Ordering::SeqCst`). A bare `Release` is not claimed (it could be
//! another enum), and any other edit on the line leaves the probe untouched.

const ATOMIC_ORDERING_VARIANTS: &[&str] = &["Relaxed", "Release", "Acquire", "AcqRel", "SeqCst"];

/// True when the edit from `before` to `after` changes only atomic
/// `Ordering` variant arguments.
pub(super) fn only_atomic_ordering_differs(before: &str, after: &str) -> bool {
    // Fail closed: literals and comments are not code, so a changed string
    // or comment must never read as an ordering swap.
    let opaque =
        |text: &str| text.contains(['"', '\'']) || text.contains("//") || text.contains("/*");
    if opaque(before) || opaque(after) {
        return false;
    }
    let before = tokens(before);
    let after = tokens(after);
    if before.len() != after.len() {
        return false;
    }
    let mut differed = false;
    for (old, new) in before.iter().zip(&after) {
        if old == new {
            continue;
        }
        if !(is_atomic_ordering_path(old) && is_atomic_ordering_path(new)) {
            return false;
        }
        differed = true;
    }
    differed
}

fn is_atomic_ordering_path(token: &str) -> bool {
    let mut segments = token.rsplit("::");
    let Some(variant) = segments.next() else {
        return false;
    };
    // Bare `Ordering::X` or `atomic::Ordering::X` only; `cmp::Ordering` and
    // user-defined `my::Ordering` are other types.
    segments.next() == Some("Ordering")
        && matches!(segments.next(), None | Some("atomic"))
        && ATOMIC_ORDERING_VARIANTS.contains(&variant)
}

/// Splits into `::`-joined path tokens and single punctuation characters,
/// dropping whitespace.
fn tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() || ch == '_' || ch == ':' {
            current.push(ch);
            continue;
        }
        if !current.is_empty() {
            out.push(std::mem::take(&mut current));
        }
        if !ch.is_whitespace() {
            out.push(ch.to_string());
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::only_atomic_ordering_differs as differs;

    #[test]
    fn given_ordering_variant_swap_then_it_is_path_only() {
        assert!(differs(
            "self.inner.compare_exchange(0, v, Ordering::AcqRel, Ordering::Acquire)",
            "self.inner.compare_exchange(0, v, Ordering::Release, Ordering::Acquire)",
        ));
        assert!(differs(
            "Ordering::AcqRel,",
            "std::sync::atomic::Ordering::Release,"
        ));
    }

    #[test]
    fn given_value_argument_change_then_it_is_not_path_only() {
        assert!(!differs(
            "x.compare_exchange(0, v, Ordering::AcqRel, Ordering::Acquire)",
            "x.compare_exchange(1, v, Ordering::AcqRel, Ordering::Acquire)",
        ));
        // A value change next to an ordering change is still observable.
        assert!(!differs(
            "x.compare_exchange(0, v, Ordering::AcqRel, Ordering::Acquire)",
            "x.compare_exchange(1, v, Ordering::Release, Ordering::Acquire)",
        ));
    }

    #[test]
    fn given_other_enums_or_bare_names_then_it_is_not_claimed() {
        assert!(!differs("sort(Ordering::Less)", "sort(Ordering::Greater)"));
        assert!(!differs("f(Release)", "f(Acquire)"));
        assert!(!differs("f(Mode::Release)", "f(Mode::Acquire)"));
        assert!(!differs("f(a)", "f(a)"));
        assert!(!differs(
            "s(cmp::Ordering::Less)",
            "s(cmp::Ordering::Release)"
        ));
        assert!(!differs(
            "f(my::Ordering::Release)",
            "f(my::Ordering::Acquire)"
        ));
        assert!(!differs("x.store(1, Release)", "x.store(2, Release)"));
        assert!(!differs(
            "x.store(1, Ordering::Release)",
            "x.store(2, Ordering::Release)"
        ));
    }

    #[test]
    fn given_literals_or_comments_then_it_fails_closed() {
        assert!(!differs(
            "log(\"Ordering::AcqRel\")",
            "log(\"Ordering::Release\")"
        ));
        assert!(!differs(
            "x.swap(1, Ordering::AcqRel); // a",
            "x.swap(1, Ordering::Release); // b"
        ));
        assert!(!differs(
            "f(Ordering::AcqRel, 'a')",
            "f(Ordering::Release, 'a')"
        ));
        assert!(!differs("f(Ordering::Release)", "f(Ordering::Release, 1)"));
    }
}

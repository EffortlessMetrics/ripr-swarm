//! `FromStr::from_str` run by `str::parse` (#5334).
//!
//! `"1.2".parse::<Version>()` runs the `from_str` of `impl FromStr for Version`
//! without spelling `from_str`, so a test that parses a string literal into the
//! owner's type calls the changed function though no call fact names it. The
//! same test written as `Version::from_str("1.2")` is a direct owner call; this
//! reading makes the turbofish spelling read the same way.
//!
//! The reading is as narrow as the operator one it follows
//! ([`super::operator_dispatch`]) and fails closed:
//!
//! - the owner is the `from_str` of a non-generic `impl FromStr for T` with a
//!   plain self type `T` (no workspace file declares a trait named `FromStr`,
//!   none is imported by a non-standard path), and it is the only indexed
//!   `from_str` of a `FromStr` impl for `T`;
//! - the test sits in the owner's own file and no scope between the test and
//!   the file declares or renames another `T`, and the file expands no
//!   `macro_rules!` that could add or hide an impl;
//! - the call is `.parse::<T>()` with the bare type name as its only generic
//!   argument and no arguments, outside property-test macros and
//!   `stringify!`;
//! - the receiver is a string literal (plain or raw, never a byte or C
//!   string, which a user trait may give a `parse`). A literal is a `&str`,
//!   whose inherent `parse` wins over any trait method of that name, so the
//!   call is `str::parse` and runs `<T as FromStr>::from_str`. A receiver
//!   ripr cannot type (`text.parse::<T>()`) relates nothing, and neither does
//!   an annotated binding (`let v: T = "1".parse()?`).
//!
//! This only relates the test to the owner. Whether its assertion reveals a
//! wrong result stays with the ordinary oracle pipeline; nothing here raises
//! a class.

use super::equality_dispatch::{
    impl_header, plain_type_name, std_trait_is_unshadowed, type_scope_is_clear,
};
use super::operator_dispatch::{file_is_clear, stringified_ranges};
use super::{FunctionSummary, RustIndex, TestSummary, is_ident_byte, mask_comments_and_strings};

/// The `str::parse` reading of an owner.
pub(super) struct ParseOwner<'a> {
    /// Self type of the `FromStr` impl (`Version`).
    impl_type: &'a str,
    /// File-level refusals are the same for every test of the owner's file,
    /// so they are computed once per owner on first use.
    file_level_clear: std::cell::OnceCell<bool>,
}

/// Whether `trait_ty` spells the standard `FromStr` trait.
fn is_from_str_trait(trait_ty: &str) -> bool {
    matches!(
        trait_ty,
        "FromStr" | "str::FromStr" | "std::str::FromStr" | "core::str::FromStr"
    )
}

/// The self type of the plain `impl FromStr for T` whose `from_str` is `owner`.
fn from_str_impl_type(owner: &FunctionSummary) -> Option<&str> {
    (owner.name == "from_str").then_some(())?;
    let (trait_ty, self_ty) = impl_header(owner)?;
    is_from_str_trait(trait_ty).then_some(())?;
    plain_type_name(self_ty)
}

/// Whether `owner` is shaped like `from_str` of a plain `impl FromStr for T`.
/// Id text alone cannot tell the standard trait from a local one of the same
/// name, so this is only the candidate prefilter's superset test.
pub(super) fn has_parse_shape(owner: &FunctionSummary) -> bool {
    from_str_impl_type(owner).is_some()
}

/// The `str::parse` reading of `owner`, or `None` when it is not `from_str` of
/// the one plain impl of the standard `FromStr` for its type.
pub(super) fn parse_owner<'a>(
    owner: &'a FunctionSummary,
    index: &RustIndex,
) -> Option<ParseOwner<'a>> {
    let impl_type = from_str_impl_type(owner)?;
    if !std_trait_is_unshadowed(owner, index, "FromStr") {
        return None;
    }
    let impls = index
        .functions()
        .iter()
        .filter(|function| from_str_impl_type(function) == Some(impl_type))
        .count();
    (impls == 1).then(|| ParseOwner {
        impl_type,
        file_level_clear: std::cell::OnceCell::new(),
    })
}

/// Whether `test` parses a string literal into the owner's type.
pub(super) fn test_parses_into_owner(
    test: &TestSummary,
    owner: &FunctionSummary,
    parse: &ParseOwner<'_>,
    test_source: Option<&str>,
    index: &RustIndex,
) -> bool {
    let Some(source) = test_source else {
        return false;
    };
    if test.file != owner.file {
        return false;
    }
    let impl_type = parse.impl_type;
    let masked = mask_comments_and_strings(&test.body);
    // Text inside a property-test macro is opaque to every other relation.
    let opaque: Vec<std::ops::Range<usize>> =
        crate::analysis::extract::property_macros::opaque_property_macros(&test.body)
            .into_iter()
            .map(|item| item.range)
            .collect();
    let stringified = stringified_ranges(&masked);
    let reaching = literal_parse_sites(&test.body, &masked, impl_type)
        .into_iter()
        .any(|at| {
            !opaque.iter().any(|range| range.contains(&at))
                && !stringified.iter().any(|range| range.contains(&at))
        });
    if !reaching || !type_scope_is_clear(test, owner, source, impl_type, index) {
        return false;
    }
    *parse
        .file_level_clear
        .get_or_init(|| file_is_clear(source, owner, impl_type, index))
}

/// Byte offsets of the `.` of each `.parse::<T>()` in `masked` whose receiver
/// is a string literal of `raw` (the unmasked body with the same byte layout).
fn literal_parse_sites(raw: &str, masked: &str, impl_type: &str) -> Vec<usize> {
    let bytes = masked.as_bytes();
    let mut sites = Vec::new();
    for (at, _) in masked.match_indices("parse") {
        let name_end = at + "parse".len();
        let whole = (at == 0 || !is_ident_byte(bytes[at - 1]))
            && bytes.get(name_end).is_none_or(|&byte| !is_ident_byte(byte));
        if !whole {
            continue;
        }
        // A method call: a single `.` (not a `..` range) before the name.
        let before = masked[..at].trim_end();
        let Some(dot) = before.len().checked_sub(1) else {
            continue;
        };
        if bytes[dot] != b'.' || (dot > 0 && bytes[dot - 1] == b'.') {
            continue;
        }
        if !turbofish_names_only(&masked[name_end..], impl_type) {
            continue;
        }
        if receiver_is_string_literal(raw, dot) {
            sites.push(dot);
        }
    }
    sites
}

/// Whether `after` (the text that follows `.parse`) is `::<T>()`, allowing
/// whitespace between tokens.
fn turbofish_names_only(after: &str, impl_type: &str) -> bool {
    let Some(rest) = after.trim_start().strip_prefix("::") else {
        return false;
    };
    let Some(rest) = rest.trim_start().strip_prefix('<') else {
        return false;
    };
    let Some(rest) = rest.trim_start().strip_prefix(impl_type) else {
        return false;
    };
    let Some(rest) = rest.trim_start().strip_prefix('>') else {
        return false;
    };
    let Some(rest) = rest.trim_start().strip_prefix('(') else {
        return false;
    };
    rest.trim_start().starts_with(')')
}

/// Whether the receiver that ends just before byte `dot` is a whole plain or
/// raw string literal of `raw`, read from the shared masking scan's literal
/// spans. A byte or C string (`b".."`, `br".."`, `c".."`) is masked the same
/// way but is no `&str`: a user trait may give it a `parse` method, so a
/// literal with any prefix but a raw `r` is refused.
fn receiver_is_string_literal(raw: &str, dot: usize) -> bool {
    let Some(head) = raw.get(..dot) else {
        return false;
    };
    let receiver_end = head.trim_end().len();
    let bytes = raw.as_bytes();
    crate::analysis::extract::string_literal_spans(raw)
        .into_iter()
        .any(|(open, close, is_raw)| {
            let (start, end) = if is_raw {
                // `r#"..."#`: the hashes between `r` and the opening quote
                // also close the literal.
                let hashes = bytes[..open]
                    .iter()
                    .rev()
                    .take_while(|&&byte| byte == b'#')
                    .count();
                let Some(r_at) = open.checked_sub(hashes + 1) else {
                    return false;
                };
                if bytes[r_at] != b'r' {
                    return false;
                }
                (r_at, close + 1 + hashes)
            } else {
                (open, close + 1)
            };
            end == receiver_end && (start == 0 || !is_ident_byte(bytes[start - 1]))
        })
}

#[cfg(test)]
mod tests {
    use super::{literal_parse_sites, turbofish_names_only};
    use crate::analysis::extract::mask_comments_and_strings;

    fn sites(body: &str) -> usize {
        literal_parse_sites(body, &mask_comments_and_strings(body), "Version").len()
    }

    #[test]
    fn a_string_literal_parsed_into_the_type_is_a_site() {
        assert_eq!(sites("let v = \"1\".parse::<Version>();"), 1);
        assert_eq!(sites("let v = \"1\" . parse :: < Version > ( );"), 1);
        assert_eq!(sites("let v = r#\"1\"#.parse::<Version>();"), 1);
    }

    #[test]
    fn other_receivers_and_targets_are_not_sites() {
        assert_eq!(sites("let v = text.parse::<Version>();"), 0);
        assert_eq!(sites("let v = \"1\".parse::<Other>();"), 0);
        assert_eq!(sites("let v = \"1\".parse::<Version<u8>>();"), 0);
        assert_eq!(sites("let v: Version = \"1\".parse().unwrap();"), 0);
        assert_eq!(sites("let v = \"1\".parse_with::<Version>();"), 0);
        assert_eq!(sites("// \"1\".parse::<Version>()"), 0);
        assert_eq!(sites("let s = \"\\\"1\\\".parse::<Version>()\";"), 0);
        assert_eq!(sites("let v = '\"'.parse::<Version>();"), 0);
        assert_eq!(sites("let v = b\"1\".parse::<Version>();"), 0);
        assert_eq!(sites("let v = br\"1\".parse::<Version>();"), 0);
        assert_eq!(sites("let v = br#\"1\"#.parse::<Version>();"), 0);
        assert_eq!(sites("let v = c\"1\".parse::<Version>();"), 0);
    }

    #[test]
    fn the_turbofish_must_name_only_the_type() {
        assert!(turbofish_names_only("::<Version>()", "Version"));
        assert!(!turbofish_names_only("::<Version>(x)", "Version"));
        assert!(!turbofish_names_only("::<VersionReq>()", "Version"));
        assert!(!turbofish_names_only("()", "Version"));
    }
}

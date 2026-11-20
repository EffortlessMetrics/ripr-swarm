//! #15277: a test receiver bound by `let s: Box<dyn Trait> = Box::new(value);`.
//!
//! Method syntax on such a receiver calls a `Trait` method through the
//! vtable of the boxed value's type, so `s.area()` runs the same body as
//! `value.area()` would for a `Trait` method. The reading is narrower than
//! the plain one on purpose:
//!
//! - only the owner's own trait may be the `dyn` trait. An inherent method
//!   of the boxed type is not callable through `dyn Trait`, and a method of
//!   another trait may resolve elsewhere;
//! - the trait must be declared exactly once in the workspace with a bare
//!   header (no generics, supertraits or `where` clause). `Box<T>` forwards
//!   std traits such as `Iterator` or `Display` when `T` implements them, and
//!   a supertrait is how `dyn Trait` would implement one, so a header with
//!   none leaves `Box` only its own methods and the blanket impls;
//! - a method name that `Box` itself or a blanket impl can answer
//!   ([`BOX_RECEIVER_METHODS`]) is refused, since method lookup tries the
//!   `Box` before the `dyn` value inside it;
//! - no workspace `impl` header or `type` item mentions `dyn` or `Box`, and
//!   `dyn Trait` (by any path) is spelled only as a `: Box<dyn Trait>`
//!   annotation ([`workspace_may_extend_dyn_or_box`]): `Box` forwards a std
//!   trait the trait object implements, and an impl on the object or the box
//!   is found before the boxed value's vtable;
//! - a `mut` binding may be reassigned to another boxed type, so it is not
//!   read;
//! - `Box` must be the std type: no workspace file declares or rebinds it.
//!
//! Every other gate of the plain receiver reading still applies to the
//! boxed value: its type must be one of the owner's receivers, competing
//! same-name methods refuse, and the trait must be in the test's scope.

use super::{
    OwnerPinSyntax, RustIndex, TestSummary, builtin_type_unshadowed, is_ident_byte,
    is_plain_identifier, matching_close, receiver_let_bindings, whole_word_offsets,
};
use crate::analysis::extract::mask_comments_and_strings;

/// Method names a `Box<dyn Trait>` receiver may resolve on the `Box` (its
/// inherent methods and its `Deref`, `AsRef` and `Borrow` family) or on a
/// blanket impl that covers it (`Any`, `Into`, `ToOwned`, ..), before the
/// lookup reaches the boxed value.
const BOX_RECEIVER_METHODS: &[&str] = &[
    "as_mut",
    "as_mut_ptr",
    "as_ptr",
    "as_ref",
    "borrow",
    "borrow_mut",
    "clone",
    "clone_from",
    "clone_into",
    "cmp",
    "deref",
    "deref_mut",
    "downcast",
    "eq",
    "fmt",
    "from",
    "ge",
    "gt",
    "hash",
    "into",
    "into_inner",
    "into_pin",
    "le",
    "lt",
    "ne",
    "partial_cmp",
    "to_owned",
    "to_string",
    "try_from",
    "try_into",
    "type_id",
];

/// The trait whose `Box<dyn ..>` bindings may type a receiver for a call of
/// `method`: `trait_name` when its declaration and the method name pass
/// the gates in the module docs.
pub(super) fn dyn_box_trait<'t>(
    trait_name: &'t str,
    method: &str,
    index: &RustIndex,
    syntax: &OwnerPinSyntax,
) -> Option<&'t str> {
    (!BOX_RECEIVER_METHODS.contains(&method)
        && trait_declared_once_bare(trait_name, index)
        && !workspace_may_extend_dyn_or_box(trait_name, index)
        && builtin_type_unshadowed("Box", index, syntax))
    .then_some(trait_name)
}

/// The relation-side reading (#15277): whether some `recv.method(..)` call
/// in the test has a receiver that every `let` binds as
/// `Box<dyn trait_name>` around a constructor of `impl_type` (`Square(3)`,
/// `Square { .. }`, or the unit value `Square`), under the same trait and
/// method-name gates as [`dyn_box_trait`]. Field and method-chain receivers
/// are not read.
pub(in crate::analysis::classify) fn boxed_receiver_calls_impl(
    test: &TestSummary,
    method: &str,
    impl_type: &str,
    trait_name: &str,
    index: &RustIndex,
) -> bool {
    if !is_plain_identifier(method) || BOX_RECEIVER_METHODS.contains(&method) {
        return false;
    }
    let masked = mask_comments_and_strings(&test.body);
    let bytes = masked.as_bytes();
    let call = format!(".{method}");
    let found = masked.match_indices(&call).any(|(dot, _)| {
        let after = dot + call.len();
        if bytes.get(after).copied().is_some_and(is_ident_byte)
            || !masked[after..].trim_start().starts_with('(')
        {
            return false;
        }
        let start = masked[..dot]
            .char_indices()
            .rev()
            .find(|&(_, character)| !(character.is_alphanumeric() || character == '_'))
            .map_or(0, |(at, character)| at + character.len_utf8());
        let receiver = &masked[start..dot];
        if !is_plain_identifier(receiver) || masked[..start].trim_end().ends_with(['.', ':']) {
            return false;
        }
        !binding_is_mut(&masked, receiver)
            && receiver_let_bindings(test, receiver).is_some_and(|bindings| {
                !bindings.is_empty()
                    && bindings.iter().all(|binding| {
                        dyn_box_value(binding, trait_name)
                            .is_some_and(|value| constructs(value, impl_type))
                    })
            })
    });
    found
        && trait_declared_once_bare(trait_name, index)
        && !workspace_may_extend_dyn_or_box(trait_name, index)
        && builtin_type_unshadowed("Box", index, &OwnerPinSyntax::default())
}

/// Whether the masked test body spells `mut receiver` anywhere (`let mut s`,
/// `&mut s`), so the boxed value may be replaced after its `let`.
pub(super) fn binding_is_mut(masked_body: &str, receiver: &str) -> bool {
    whole_word_offsets(masked_body, receiver)
        .into_iter()
        .any(|offset| masked_body[..offset].trim_end().ends_with("mut"))
}

/// Whether the workspace may give `Box<dyn trait_name>` a method the boxed
/// value's vtable does not answer, or spell the trait object in a way this
/// reading does not follow:
///
/// - an `impl` header mentions `Box` or `dyn` at all (`impl Iterator for dyn
///   crate::Shape`, `impl dyn Shape { .. }`, `impl<T> X for Box<T>`);
/// - a `type` item mentions `Box` or `dyn` (`type DynShape = dyn Shape;`
///   then `impl Iterator for DynShape`);
/// - an `impl` self type or a `type` alias's value names `trait_name` itself,
///   which before edition 2021 is the trait object (`impl Iterator for
///   Shape`, `impl Shape { .. }`), or the trait is renamed anywhere
///   (`use crate::Shape as Sh;`), which hides that spelling;
/// - `dyn <path>::trait_name` or `dyn trait_name` is spelled anywhere except
///   as a `: Box<dyn trait_name>` annotation (a macro argument such as
///   `exact!(dyn Shape)`, an `as` cast, a generic argument).
fn workspace_may_extend_dyn_or_box(trait_name: &str, index: &RustIndex) -> bool {
    index.files().values().any(|facts| {
        // A bare (edition 2018) object impl or a rename spells only the
        // trait's name, so a file naming none of the three is skipped.
        if !facts.source.contains("Box")
            && !facts.source.contains("dyn")
            && !facts.source.contains(trait_name)
        {
            return false;
        }
        let masked = mask_comments_and_strings(&facts.source);
        let mentions = |text: &str| {
            !whole_word_offsets(text, "Box").is_empty()
                || !whole_word_offsets(text, "dyn").is_empty()
        };
        let item_mentions = |keyword: &str, ends: &[char]| {
            whole_word_offsets(&masked, keyword)
                .into_iter()
                .any(|offset| {
                    let rest = &masked[offset + keyword.len()..];
                    mentions(&rest[..rest.find(ends).unwrap_or(rest.len())])
                })
        };
        // Before edition 2021 a bare trait name in type position is the
        // trait object: `impl Iterator for Shape` and `type S = Shape;`
        // extend `dyn Shape` without spelling `dyn`.
        let bare_object = whole_word_offsets(&masked, "impl")
            .into_iter()
            .any(|offset| {
                let rest = &masked[offset + "impl".len()..];
                let header = &rest[..rest.find(['{', ';']).unwrap_or(rest.len())];
                // Only the implemented-trait position (`Shape for ..`) may
                // name the trait; a self type, a bound or a `where` clause
                // (which may hold its own `for<'a>`) refuses.
                whole_word_offsets(header, trait_name)
                    .into_iter()
                    .any(|at| {
                        let after = header[at + trait_name.len()..].trim_start();
                        !after.strip_prefix("for").is_some_and(|rest| {
                            rest.starts_with(char::is_whitespace)
                                && !rest.trim_start().starts_with('<')
                        })
                    })
            })
            || whole_word_offsets(&masked, "type")
                .into_iter()
                .any(|offset| {
                    let rest = &masked[offset + "type".len()..];
                    let item = &rest[..rest.find([';', '{']).unwrap_or(rest.len())];
                    item.split_once('=')
                        .is_some_and(|(_, value)| !whole_word_offsets(value, trait_name).is_empty())
                });
        // `use crate::Shape as Sh;` gives the trait object a name the checks
        // above never read (`impl Iterator for Sh` before edition 2021).
        let renamed = whole_word_offsets(&masked, trait_name)
            .into_iter()
            .any(|at| {
                let after = masked[at + trait_name.len()..].trim_start();
                after.strip_prefix("as").is_some_and(|rest| {
                    !rest.starts_with(|character: char| {
                        character.is_alphanumeric() || character == '_'
                    })
                })
            });
        bare_object
            || renamed
            || item_mentions("impl", &['{', ';'])
            || item_mentions("type", &[';', '{'])
            || whole_word_offsets(&masked, "dyn")
                .into_iter()
                .any(|offset| names_trait_outside_annotation(&masked, offset, trait_name))
    })
}

/// Whether the `dyn` at byte `offset` of `masked` is followed by a path whose
/// last segment is `trait_name`, in any position other than
/// `: Box<dyn trait_name>`.
fn names_trait_outside_annotation(masked: &str, offset: usize, trait_name: &str) -> bool {
    let after = masked[offset + "dyn".len()..].trim_start();
    let path_len = after
        .find(|character: char| !(character.is_alphanumeric() || matches!(character, '_' | ':')))
        .unwrap_or(after.len());
    let path = &after[..path_len];
    if path.rsplit("::").next() != Some(trait_name) {
        return false;
    }
    let annotation = path == trait_name
        && masked[..offset]
            .trim_end()
            .strip_suffix('<')
            .map(str::trim_end)
            .and_then(|before| before.strip_suffix("Box"))
            .map(str::trim_end)
            .is_some_and(|before| before.ends_with(':') && !before.ends_with("::"));
    !annotation
}

/// Whether `value` is exactly `impl_type`, `impl_type(..)` or
/// `impl_type { .. }`, with nothing after it.
fn constructs(value: &str, impl_type: &str) -> bool {
    let masked = mask_comments_and_strings(value);
    let Some(rest) = masked.strip_prefix(impl_type) else {
        return false;
    };
    let rest_at = impl_type.len();
    if rest.is_empty() {
        return true;
    }
    if rest.as_bytes().first().copied().is_some_and(is_ident_byte) {
        return false;
    }
    let open_at = rest_at + (rest.len() - rest.trim_start().len());
    let (open, close) = match masked.as_bytes().get(open_at) {
        Some(b'(') => (b'(', b')'),
        Some(b'{') => (b'{', b'}'),
        _ => return false,
    };
    matching_close(&masked, open_at, open, close)
        .is_some_and(|closing| masked[closing + 1..].trim().is_empty())
}

/// The boxed value of one `let` binding's text after the name
/// (`: Box<dyn Trait> = Box::new(value)`), when the annotation is exactly
/// `Box<dyn trait_name>` and the initializer is exactly one `Box::new(..)`
/// call. `None` for any other spelling.
pub(super) fn dyn_box_value<'b>(binding: &'b str, trait_name: &str) -> Option<&'b str> {
    let masked = mask_comments_and_strings(binding);
    let colon = masked.find(':')?;
    if !masked[..colon].trim().is_empty() {
        return None;
    }
    let equals = colon + masked[colon..].find('=')?;
    let annotation: Vec<&str> = masked[colon + 1..equals]
        .split(|character: char| character.is_whitespace() || matches!(character, '<' | '>'))
        .filter(|part| !part.is_empty())
        .collect();
    let compact: String = masked[colon + 1..equals].split_whitespace().collect();
    if annotation != ["Box", "dyn", trait_name] || compact != format!("Box<dyn{trait_name}>") {
        return None;
    }
    let initializer = &masked[equals + 1..];
    let call = initializer.trim_start();
    let call_at = equals + 1 + (initializer.len() - call.len());
    let after_new = call.strip_prefix("Box::new")?;
    let open = call_at + "Box::new".len() + (after_new.len() - after_new.trim_start().len());
    let close = matching_close(&masked, open, b'(', b')')?;
    if !masked[close + 1..].trim().is_empty() {
        return None;
    }
    let value = binding.get(open + 1..close)?.trim();
    (!value.is_empty()).then_some(value)
}

/// Whether exactly one `trait trait_name` is declared across the workspace,
/// and its header is bare: nothing between the name and the `{`.
fn trait_declared_once_bare(trait_name: &str, index: &RustIndex) -> bool {
    let mut headers = index.files().values().flat_map(|facts| {
        if !facts.source.contains(trait_name) {
            return Vec::new();
        }
        let masked = mask_comments_and_strings(&facts.source);
        whole_word_offsets(&masked, "trait")
            .into_iter()
            .filter_map(|offset| {
                let after = masked[offset + "trait".len()..].trim_start();
                let rest = after.strip_prefix(trait_name)?;
                if rest.starts_with(|character: char| {
                    character.is_ascii_alphanumeric() || character == '_'
                }) {
                    return None;
                }
                Some(
                    rest.split('{')
                        .next()
                        .is_some_and(|header| header.trim().is_empty())
                        && rest.contains('{'),
                )
            })
            .collect()
    });
    matches!((headers.next(), headers.next()), (Some(true), None))
}

#[cfg(test)]
mod tests {
    use super::{binding_is_mut, constructs, dyn_box_value, names_trait_outside_annotation};

    #[test]
    fn only_a_box_annotation_may_spell_the_trait_object() {
        let at = |text: &str| text.find("dyn").unwrap_or(0);
        for text in [
            "let s: Box<dyn Shape> = x;",
            "struct H { s: Box< dyn Shape > }",
            "dyn Other",
            "x: Box<dyn ShapeExt>",
        ] {
            assert!(
                !names_trait_outside_annotation(text, at(text), "Shape"),
                "{text}"
            );
        }
        for text in [
            "impl Iterator for dyn crate::Shape {",
            "exact!(dyn Shape);",
            "let s = b as Box<dyn Shape>;",
            "let s: Box<dyn a::Shape> = x;",
            "let s: Rc<dyn Shape> = x;",
            "let s: std::boxed::Box<dyn Shape> = x;",
        ] {
            assert!(
                names_trait_outside_annotation(text, at(text), "Shape"),
                "{text}"
            );
        }
    }

    #[test]
    fn a_mut_spelling_of_the_receiver_is_mutable() {
        assert!(binding_is_mut("let mut s: Box<dyn Shape> = x;", "s"));
        assert!(binding_is_mut("let s = x; swap(&mut s, &mut t);", "s"));
        assert!(!binding_is_mut("let s: Box<dyn Shape> = x; s.area();", "s"));
        assert!(!binding_is_mut("let mut ss = x; s.area();", "s"));
    }

    #[test]
    fn a_boxed_value_constructs_only_its_own_head() {
        for value in ["Square", "Square(3)", "Square { side: 3 }", "Square (3)"] {
            assert!(constructs(value, "Square"), "{value}");
        }
        for value in [
            "Squares(3)",
            "Square::new(3)",
            "Square(3).scaled()",
            "make(Square(3))",
            "other::Square(3)",
            "Square(3) + 1",
        ] {
            assert!(!constructs(value, "Square"), "{value}");
        }
    }

    #[test]
    fn reads_the_boxed_value_of_an_exact_dyn_box_binding() {
        assert_eq!(
            dyn_box_value(": Box<dyn Shape> = Box::new(Square(3))", "Shape"),
            Some("Square(3)")
        );
        assert_eq!(
            dyn_box_value(" :Box< dyn Shape >=Box::new ( Unit )", "Shape"),
            Some("Unit")
        );
    }

    #[test]
    fn refuses_any_other_binding_spelling() {
        for binding in [
            "= Box::new(Square(3))",
            ": Box<dyn Other> = Box::new(Square(3))",
            ": Box<dyn Shape + Send> = Box::new(Square(3))",
            ": Box<dyn ShapeExt> = Box::new(Square(3))",
            ": Box<Square> = Box::new(Square(3))",
            ": &dyn Shape = &Square(3)",
            ": Box<dyn Shape> = make()",
            ": Box<dyn Shape> = Box::new(Square(3)).into()",
            ": Box<dyn Shape> = Box::new()",
            ": Box<dyn Shape> = other::Box::new(Square(3))",
            ": std::boxed::Box<dyn Shape> = Box::new(Square(3))",
        ] {
            assert_eq!(dyn_box_value(binding, "Shape"), None, "{binding}");
        }
    }
}

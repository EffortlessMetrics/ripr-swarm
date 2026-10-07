pub(in crate::analysis) fn enum_variant_values(text: &str) -> Vec<String> {
    let mut values = Vec::new();
    for token in text.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == ':')) {
        if !token.contains("::") {
            continue;
        }
        let Some(last) = token.rsplit("::").next() else {
            continue;
        };
        if last
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_uppercase())
        {
            values.push(token.to_string());
        }
    }
    values.sort();
    values.dedup();
    values
}

/// Primitive types: a path segment naming one of them can only qualify an
/// associated constant or function (`u64::MAX`), never an enum variant.
const PRIMITIVE_TYPE_NAMES: &[&str] = &[
    "bool", "char", "str", "f32", "f64", "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16",
    "u32", "u64", "u128", "usize",
];

/// Whether a path token collected by [`enum_variant_values`] is, by syntax
/// alone, a constant rather than an enum variant (#5357).
///
/// The evidence is the path's own spelling. A token answers `true` only when
/// its last segment is SCREAMING_CASE (no lowercase letter) and one of these
/// also holds:
///
/// - the qualifying segment is a primitive type (`u64::MAX`): primitives have
///   no variants;
/// - the qualifying segment starts lowercase (`crate::KIB`, `limits::MAX`): a
///   module path names an item, while a variant is named through its
///   CamelCase enum.
///
/// `Kind::ON`, `Grade::A` or `Limits::MAX_LEN` stay variants: an all-caps
/// name under a CamelCase type is legal for both a variant and an associated
/// constant (a `MAX_LEN` variant only draws a naming lint warning), so the
/// label keeps its prior meaning there instead of guessing.
pub(in crate::analysis) fn path_value_is_constant(path: &str) -> bool {
    let mut segments = path.rsplit("::");
    let Some(last) = segments.next() else {
        return false;
    };
    let Some(qualifier) = segments.next() else {
        return false;
    };
    let screaming = last.chars().any(|ch| ch.is_ascii_uppercase())
        && !last.chars().any(|ch| ch.is_ascii_lowercase());
    if !screaming {
        return false;
    }
    PRIMITIVE_TYPE_NAMES.contains(&qualifier)
        || qualifier
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enum_variant_values_returns_sorted_unique_variants() {
        let values = enum_variant_values(
            "Err(AuthError::RevokedToken) Err(AuthError::ExpiredToken) AuthError::RevokedToken",
        );

        assert_eq!(
            values,
            vec![
                "AuthError::ExpiredToken".to_string(),
                "AuthError::RevokedToken".to_string()
            ]
        );
    }

    #[test]
    fn enum_variant_values_ignores_lowercase_and_unqualified_tokens() {
        assert_eq!(
            enum_variant_values("err(auth_error::revoked) Revoked"),
            Vec::<String>::new()
        );
    }

    /// #5357: constants were labelled "enum variant value".
    #[test]
    fn path_value_is_constant_reads_only_syntax_evidence() {
        // Constants by evidence.
        assert!(path_value_is_constant("u64::MAX"));
        assert!(path_value_is_constant("f64::EPSILON"));
        assert!(path_value_is_constant("crate::KIB"));
        assert!(path_value_is_constant("bytesize::units::KIB"));
        // Real variants keep their label.
        assert!(!path_value_is_constant("AuthError::RevokedToken"));
        assert!(!path_value_is_constant("std::cmp::Ordering::Less"));
        assert!(!path_value_is_constant("Level::V2"));
        // Ambiguous all-caps under a CamelCase type: no evidence either way.
        assert!(!path_value_is_constant("Kind::ON"));
        // An underscore is no evidence: `enum Limits { MAX_LEN }` compiles
        // with only a naming lint warning.
        assert!(!path_value_is_constant("Limits::MAX_LEN"));
        assert!(!path_value_is_constant("Grade::A"));
        // Not a path.
        assert!(!path_value_is_constant("KIB"));
    }
}

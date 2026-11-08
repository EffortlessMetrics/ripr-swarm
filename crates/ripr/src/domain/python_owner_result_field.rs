//! The Python unread-owner-result-field fact, as an evidence line.
//!
//! The Python analysis emits the line only when the one related test binds
//! the instance a self-returning owner method (or `__init__`) produced and
//! never reads the changed field (`analysis/language/python/owner_result_field.rs`).
//! The repair card and the gap ledger read it back through
//! [`python_unread_owner_result_field`], which binds it to the card's selected
//! test and to the `self.<field>` discriminator, so a line for another test
//! or field never opens a repair route.

/// Evidence-line prefix of the fact: `python_owner_result_field_unread:
/// <binding>.<field> (<test name>)`.
pub(crate) const PYTHON_OWNER_RESULT_FIELD_UNREAD_PREFIX: &str =
    "python_owner_result_field_unread: ";

/// The evidence line for `binding.field` unread in `test_name`.
pub(crate) fn python_owner_result_field_unread_line(
    binding: &str,
    field: &str,
    test_name: &str,
) -> String {
    format!("{PYTHON_OWNER_RESULT_FIELD_UNREAD_PREFIX}{binding}.{field} ({test_name})")
}

/// The `(binding, field)` of the one unread-owner-result-field line in
/// `evidence`, when it names `test_name` and the field `missing_discriminator`
/// asserts on `self`. Zero or several lines, a malformed line, another test
/// or another field all read `None`.
pub(crate) fn python_unread_owner_result_field<'a>(
    evidence: impl IntoIterator<Item = &'a str>,
    test_name: &str,
    missing_discriminator: &str,
) -> Option<(&'a str, &'a str)> {
    let mut lines = evidence
        .into_iter()
        .filter_map(|line| line.strip_prefix(PYTHON_OWNER_RESULT_FIELD_UNREAD_PREFIX));
    let line = lines.next()?;
    if lines.next().is_some() {
        return None;
    }
    let (path, rest) = line.split_once(" (")?;
    let named_test = rest.strip_suffix(')')?;
    let (binding, field) = path.split_once('.')?;
    if named_test != test_name
        || !plain_identifier(binding)
        || !plain_identifier(field)
        || !plain_identifier(named_test)
    {
        return None;
    }
    let (lhs, _) = missing_discriminator.split_once(" == ")?;
    (lhs.trim().strip_prefix("self.") == Some(field)).then_some((binding, field))
}

fn plain_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DISCRIMINATOR: &str = "self.timeout_value == <expected value>";

    #[test]
    fn the_fact_binds_to_its_test_and_field() {
        let line = python_owner_result_field_unread_line("r", "timeout_value", "test_builder");
        assert_eq!(
            python_unread_owner_result_field([line.as_str()], "test_builder", DISCRIMINATOR),
            Some(("r", "timeout_value"))
        );
    }

    #[test]
    fn another_test_field_or_a_second_line_reads_nothing() {
        let line = python_owner_result_field_unread_line("r", "timeout_value", "test_builder");
        let other_field = python_owner_result_field_unread_line("r", "url", "test_builder");
        assert_eq!(
            python_unread_owner_result_field([line.as_str()], "test_other", DISCRIMINATOR),
            None
        );
        assert_eq!(
            python_unread_owner_result_field([other_field.as_str()], "test_builder", DISCRIMINATOR),
            None
        );
        assert_eq!(
            python_unread_owner_result_field(
                [line.as_str(), line.as_str()],
                "test_builder",
                DISCRIMINATOR
            ),
            None
        );
        assert_eq!(
            python_unread_owner_result_field(
                [line.as_str()],
                "test_builder",
                "result.timeout_value == <expected value>"
            ),
            None
        );
        for malformed in [
            "python_owner_result_field_unread: r.timeout_value test_builder",
            "python_owner_result_field_unread: r.x.timeout_value (test_builder)",
            "python_owner_result_field_unread: f(r).timeout_value (test_builder)",
            "python_owner_result_field_unread: timeout_value (test_builder)",
        ] {
            assert_eq!(
                python_unread_owner_result_field([malformed], "test_builder", DISCRIMINATOR),
                None,
                "{malformed}"
            );
        }
        assert_eq!(
            python_unread_owner_result_field(std::iter::empty(), "test_builder", DISCRIMINATOR),
            None
        );
    }
}

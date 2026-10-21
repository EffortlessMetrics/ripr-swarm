use crate::output::markdown::code_span;
use serde_json::Value;

/// Walk a JSON value along the given path segments.
/// Returns `None` if any segment is missing or if `value` is `None`.
pub(super) fn value_path<'a>(value: Option<&'a Value>, path: &[&str]) -> Option<&'a Value> {
    let mut current = value?;
    for segment in path {
        current = current.get(*segment)?;
    }
    Some(current)
}

/// The string at `key` as one code span, or `not_available`.
pub(super) fn string_field(value: Option<&Value>, key: &str) -> String {
    code_span(
        value
            .and_then(|value| value.get(key))
            .and_then(Value::as_str)
            .unwrap_or("not_available"),
    )
}

pub(super) fn summary_u64(value: Option<&Value>, key: &str) -> String {
    summary_field(value, key)
        .and_then(Value::as_u64)
        .map(|value| value.to_string())
        .unwrap_or_else(|| "not_available".to_string())
}

pub(super) fn summary_bool(value: Option<&Value>, key: &str) -> String {
    summary_field(value, key)
        .and_then(Value::as_bool)
        .map(|value| value.to_string())
        .unwrap_or_else(|| "not_available".to_string())
}

/// The summary string at `key` as one code span; `none` when null.
pub(super) fn summary_string_or_null(value: Option<&Value>, key: &str) -> String {
    let Some(value) = summary_field(value, key) else {
        return code_span("not_available");
    };
    if value.is_null() {
        code_span("none")
    } else {
        code_span(value.as_str().unwrap_or("invalid"))
    }
}

fn summary_field<'a>(value: Option<&'a Value>, key: &str) -> Option<&'a Value> {
    value
        .and_then(|value| value.get("summary"))
        .and_then(|summary| summary.get(key))
}

pub(super) fn first_line(value: &str) -> String {
    value.lines().next().unwrap_or(value).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn value_path_walks_segments_and_fails_closed_on_any_miss() {
        let doc = json!({"a": {"b": {"c": 7}}, "n": null});
        assert_eq!(value_path(Some(&doc), &["a", "b", "c"]), Some(&json!(7)));
        assert_eq!(value_path(Some(&doc), &[]), Some(&doc));
        assert_eq!(value_path(Some(&doc), &["a", "x", "c"]), None);
        assert_eq!(value_path(Some(&doc), &["a", "b", "c", "d"]), None);
        assert_eq!(value_path(None, &["a"]), None);
        // A present-but-null leaf is a value, not a missing segment.
        assert_eq!(value_path(Some(&doc), &["n"]), Some(&Value::Null));
    }

    #[test]
    fn string_field_code_spans_strings_and_rejects_other_types() {
        let doc = json!({"s": "a`b", "n": 3});
        assert_eq!(string_field(Some(&doc), "s"), "``a`b``");
        assert_eq!(string_field(Some(&doc), "n"), "`not_available`");
        assert_eq!(string_field(Some(&doc), "missing"), "`not_available`");
        assert_eq!(string_field(None, "s"), "`not_available`");
    }

    #[test]
    fn summary_scalars_render_values_and_not_available_for_wrong_types() {
        let doc = json!({"summary": {"n": 5, "neg": -1, "s": "5", "b": false, "z": null}});
        assert_eq!(summary_u64(Some(&doc), "n"), "5");
        // Negative, string and null values are not u64: never coerced.
        assert_eq!(summary_u64(Some(&doc), "neg"), "not_available");
        assert_eq!(summary_u64(Some(&doc), "s"), "not_available");
        assert_eq!(summary_u64(Some(&doc), "z"), "not_available");
        assert_eq!(summary_bool(Some(&doc), "b"), "false");
        assert_eq!(summary_bool(Some(&doc), "n"), "not_available");
        assert_eq!(summary_bool(Some(&doc), "missing"), "not_available");
        // No `summary` object at all, and no document at all.
        assert_eq!(summary_u64(Some(&json!({"n": 5})), "n"), "not_available");
        assert_eq!(summary_bool(None, "b"), "not_available");
    }

    #[test]
    fn summary_string_or_null_separates_missing_null_string_and_invalid() {
        let doc = json!({"summary": {"s": "base", "z": null, "bad": 4}});
        assert_eq!(summary_string_or_null(Some(&doc), "s"), "`base`");
        assert_eq!(summary_string_or_null(Some(&doc), "z"), "`none`");
        assert_eq!(summary_string_or_null(Some(&doc), "bad"), "`invalid`");
        assert_eq!(
            summary_string_or_null(Some(&doc), "missing"),
            "`not_available`"
        );
        assert_eq!(summary_string_or_null(None, "s"), "`not_available`");
    }

    #[test]
    fn first_line_trims_and_keeps_only_the_first_line() {
        assert_eq!(first_line("  head  \nsecond"), "head");
        assert_eq!(first_line("only"), "only");
        assert_eq!(first_line(""), "");
        // A leading blank line is the first line; it does not skip ahead.
        assert_eq!(first_line("\nsecond"), "");
        assert_eq!(first_line("a\r\nb"), "a");
    }
}

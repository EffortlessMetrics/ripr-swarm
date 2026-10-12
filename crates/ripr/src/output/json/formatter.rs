pub(crate) fn field(out: &mut String, indent: usize, name: &str, value: &str, trailing: bool) {
    out.push_str(&format!(
        "{}\"{}\": \"{}\"{}\n",
        "  ".repeat(indent),
        name,
        escape(value),
        if trailing { "," } else { "" }
    ));
}

pub(crate) fn number_field(
    out: &mut String,
    indent: usize,
    name: &str,
    value: usize,
    trailing: bool,
) {
    out.push_str(&format!(
        "{}\"{}\": {}{}\n",
        "  ".repeat(indent),
        name,
        value,
        if trailing { "," } else { "" }
    ));
}

pub(crate) fn float_field(out: &mut String, indent: usize, name: &str, value: f32, trailing: bool) {
    out.push_str(&format!(
        "{}\"{}\": {:.2}{}\n",
        "  ".repeat(indent),
        name,
        value,
        if trailing { "," } else { "" }
    ));
}

pub(crate) fn array_field(
    out: &mut String,
    indent: usize,
    name: &str,
    values: &[String],
    trailing: bool,
) {
    out.push_str(&format!("{}\"{}\": [", "  ".repeat(indent), name));
    for (idx, value) in values.iter().enumerate() {
        out.push_str(&format!("\"{}\"", escape(value)));
        if idx + 1 != values.len() {
            out.push_str(", ");
        }
    }
    out.push_str(&format!("]{}\n", if trailing { "," } else { "" }));
}

pub(crate) fn escape(value: &str) -> String {
    let mut out = String::new();
    escape_into(&mut out, value);
    out
}

/// [`escape`] writing into an existing buffer, so hot render loops reuse
/// one allocation instead of one per field (#6898). [`escape`] delegates
/// here, so the two spellings cannot diverge.
pub(crate) fn escape_into(out: &mut String, value: &str) {
    use std::fmt::Write as _;
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                let code = c as u32;
                if code <= 0xFFFF {
                    let _ = write!(out, "\\u{code:04x}");
                } else {
                    let adjusted = code - 0x10000;
                    let high = 0xD800 + (adjusted >> 10);
                    let low = 0xDC00 + (adjusted & 0x3FF);
                    let _ = write!(out, "\\u{high:04x}\\u{low:04x}");
                }
            }
            c => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{array_field, escape, field, float_field, number_field};

    #[test]
    fn escapes_json() {
        assert_eq!(escape("a\"b\n"), "a\\\"b\\n");
    }

    #[test]
    fn escapes_backslash_and_control_chars() {
        assert_eq!(escape("\\\u{0008}\t"), "\\\\\\u0008\\t");
    }

    #[test]
    fn escape_into_appends_without_disturbing_the_buffer() {
        use super::escape_into;
        let mut out = String::from("{\"a\": \"");
        escape_into(&mut out, "x\"y\n\\");
        out.push('"');
        assert_eq!(out, "{\"a\": \"x\\\"y\\n\\\\\"");
        // Delegation keeps the allocating spelling identical.
        assert_eq!(escape("x\"y\n\\"), "x\\\"y\\n\\\\");
    }

    #[test]
    fn renders_scalar_fields() {
        let mut out = String::new();

        field(&mut out, 1, "name", "a\"b", true);
        number_field(&mut out, 1, "count", 7, true);
        float_field(&mut out, 1, "score", 0.125, false);

        assert_eq!(
            out,
            "  \"name\": \"a\\\"b\",\n  \"count\": 7,\n  \"score\": 0.12\n"
        );
    }

    #[test]
    fn renders_array_fields_with_and_without_values() {
        let mut out = String::new();

        array_field(
            &mut out,
            1,
            "stop_reasons",
            &["a\"b".to_string(), "c".to_string()],
            true,
        );
        array_field(&mut out, 1, "missing", &[], false);

        assert_eq!(
            out,
            "  \"stop_reasons\": [\"a\\\"b\", \"c\"],\n  \"missing\": []\n"
        );
    }
}

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

/// Renders `confidence_basis` (list of `{code, summary}`) and the one-line
/// `confidence_why_capped` for a preview finding (#10700). Writes nothing for
/// an empty basis so Rust output bytes are unchanged.
pub(crate) fn confidence_basis_fields(
    out: &mut String,
    indent: usize,
    entries: &[crate::domain::ConfidenceBasisEntry],
    why_capped: Option<&str>,
) {
    if entries.is_empty() {
        return;
    }
    let pad = "  ".repeat(indent);
    out.push_str(&format!("{pad}\"confidence_basis\": [\n"));
    for (idx, entry) in entries.iter().enumerate() {
        out.push_str(&format!(
            "{pad}  {{\"code\": \"{}\", \"summary\": \"{}\"}}{}\n",
            escape(entry.code),
            escape(&entry.summary),
            if idx + 1 != entries.len() { "," } else { "" }
        ));
    }
    out.push_str(&format!("{pad}],\n"));
    if let Some(line) = why_capped {
        field(out, indent, "confidence_why_capped", line, true);
    }
}

#[cfg(test)]
mod tests {
    use super::{array_field, escape, field, float_field, number_field};

    #[test]
    fn confidence_basis_fields_write_nothing_when_empty() {
        let mut out = String::new();
        super::confidence_basis_fields(&mut out, 1, &[], Some("ignored"));
        assert!(out.is_empty());
    }

    #[test]
    fn confidence_basis_fields_render_valid_escaped_json() -> Result<(), String> {
        let entries = vec![
            crate::domain::ConfidenceBasisEntry {
                code: "a",
                summary: "say \"hi\"\n".to_string(),
            },
            crate::domain::ConfidenceBasisEntry {
                code: "b",
                summary: "two".to_string(),
            },
        ];
        let mut out = String::from("{\n");
        super::confidence_basis_fields(&mut out, 1, &entries, Some("why \\ line"));
        out.push_str("  \"next\": 1\n}");
        let value: serde_json::Value =
            serde_json::from_str(&out).map_err(|e| format!("{e}: {out}"))?;
        assert_eq!(value["confidence_basis"][0]["summary"], "say \"hi\"\n");
        assert_eq!(value["confidence_basis"][1]["code"], "b");
        assert_eq!(value["confidence_why_capped"], "why \\ line");
        Ok(())
    }

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

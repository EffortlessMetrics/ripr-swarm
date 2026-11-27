//! Whitespace-only presentation of the existing check JSON document (#13103).
//!
//! This is not another report producer or a JSON validator. The callers supply
//! their already-rendered document after bounding findings, stamping source
//! subjects and (where available) attaching the canonical next action.

use super::format::OutputFormat;

pub(crate) fn finish(rendered: String, format: &OutputFormat) -> String {
    if matches!(format, OutputFormat::JsonCompact) {
        compact(rendered)
    } else {
        rendered
    }
}

/// Remove only JSON's four insignificant whitespace characters outside strings.
/// Work in the existing allocation and retain every other character verbatim:
/// parsing into a Value and serializing it again could reorder object members,
/// normalize numbers or rewrite string escapes. Keep one final line terminator.
fn compact(mut rendered: String) -> String {
    let mut in_string = false;
    let mut escaped = false;
    rendered.retain(|character| {
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            true
        } else if character == '"' {
            in_string = true;
            true
        } else {
            !matches!(character, ' ' | '\t' | '\n' | '\r')
        }
    });
    rendered.push('\n');
    rendered
}

#[cfg(test)]
mod tests {
    use super::{compact, finish};
    use crate::output::format::OutputFormat;

    fn check(input: &str, expected: &str) -> Result<(), String> {
        let original: serde_json::Value =
            serde_json::from_str(input).map_err(|error| format!("invalid test input: {error}"))?;
        let actual = compact(input.to_string());
        if actual != expected {
            return Err(format!("lexical mismatch: {actual:?} != {expected:?}"));
        }
        let parsed: serde_json::Value = serde_json::from_str(&actual)
            .map_err(|error| format!("compaction broke JSON: {error}"))?;
        if parsed != original {
            return Err("compaction changed the JSON value".to_string());
        }
        if compact(actual.clone()) != actual {
            return Err("compaction must be idempotent".to_string());
        }
        Ok(())
    }

    #[test]
    fn json_compact_preserves_order_number_lexemes_and_empty_values() -> Result<(), String> {
        check(
            "{\r\n\t\"z\" : -0, \"a\" : 1.2300e+02, \"nested\" : [ {}, [], null, true, false ]\r\n}\n",
            "{\"z\":-0,\"a\":1.2300e+02,\"nested\":[{},[],null,true,false]}\n",
        )?;
        check(" \r\n { } \t", "{}\n")?;
        check("[ ]", "[]\n")?;
        Ok(())
    }

    #[test]
    fn json_compact_preserves_string_whitespace_unicode_and_escape_spelling() -> Result<(), String>
    {
        check(
            r#"{
  "text": " keep  spaces\tand\nlines\r; café 雪 🦀 ",
  "escape": "\u0061\/\"\\",
  "after": " still here "
}"#,
            concat!(
                r#"{"text":" keep  spaces\tand\nlines\r; café 雪 🦀 ","escape":"\u0061\/\"\\","after":" still here "}"#,
                "\n"
            ),
        )
    }

    #[test]
    fn json_compact_handles_even_and_odd_backslash_runs() -> Result<(), String> {
        for count in 0..12 {
            for suffix in ["", "\" keep  spaces ", "雪\n\t\r"] {
                let text = format!("{}{}", "\\".repeat(count), suffix);
                let literal = serde_json::to_string(&text)
                    .map_err(|error| format!("encode string control: {error}"))?;
                let input =
                    format!("{{\n  \"text\": {literal},\n  \"next\": [ true, false ]\n}}\n");
                let expected = format!("{{\"text\":{literal},\"next\":[true,false]}}\n");
                check(&input, &expected)?;
            }
        }
        Ok(())
    }

    #[test]
    fn json_compact_does_not_modify_legacy_json_bytes() -> Result<(), String> {
        let original = "{\n  \"text\": \" keep spaces \"\n}\n";
        if finish(original.to_string(), &OutputFormat::Json) != original {
            return Err("legacy JSON bytes changed".to_string());
        }
        if finish(original.to_string(), &OutputFormat::JsonCompact)
            != "{\"text\":\" keep spaces \"}\n"
        {
            return Err("compact format did not select the whitespace-only pass".to_string());
        }
        Ok(())
    }
}

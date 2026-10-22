//! Whether an edit only respelled integer literals (`21` to `0x15`).
//!
//! A respelling keeps the number and its type, so a value that holds one
//! changed nothing a test could observe (RIPR-SPEC-0229, #9409).

/// True when `added` and `removed` differ, and differ only in how integer
/// literals are written: every literal reads the same number with the same
/// suffix, and all other text is identical.
pub(super) fn only_respells_integer_literals(added: &str, removed: &str) -> bool {
    added != removed && canonical(added) == canonical(removed)
}

/// `text` with each integer literal rewritten as `<decimal value><suffix>`.
/// A literal that is not plainly an integer (a float, an overflow, a name
/// that merely ends in digits) is left as written.
fn canonical(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while at < bytes.len() {
        let byte = bytes[at];
        let starts_literal = byte.is_ascii_digit()
            && (at == 0
                || !(bytes[at - 1].is_ascii_alphanumeric()
                    || matches!(bytes[at - 1], b'_' | b'.')));
        if !starts_literal {
            let width = text[at..].chars().next().map_or(1, char::len_utf8);
            out.push_str(&text[at..at + width]);
            at += width;
            continue;
        }
        let run = |from: usize| {
            from + bytes[from..]
                .iter()
                .take_while(|byte| byte.is_ascii_alphanumeric() || **byte == b'_')
                .count()
        };
        let mut end = run(at);
        // `1.5` is not an integer: keep the whole token as written.
        let float = bytes.get(end) == Some(&b'.')
            && bytes.get(end + 1).is_some_and(|next| next.is_ascii_digit());
        if float {
            end = run(end + 1);
        }
        match (float, integer(&text[at..end])) {
            (false, Some(value)) => out.push_str(&value),
            _ => out.push_str(&text[at..end]),
        }
        at = end;
    }
    out
}

/// `<decimal value><suffix>` for a plain integer literal.
fn integer(literal: &str) -> Option<String> {
    let (radix, rest) = match literal.get(..2) {
        Some("0x") => (16, &literal[2..]),
        Some("0o") => (8, &literal[2..]),
        Some("0b") => (2, &literal[2..]),
        _ => (10, literal),
    };
    let digits_end = rest
        .find(|c: char| !(c.is_digit(radix) || c == '_'))
        .unwrap_or(rest.len());
    let (digits, suffix) = rest.split_at(digits_end);
    let suffix = suffix.trim_start_matches('_');
    let suffix_ok = suffix.is_empty()
        || ["u", "i"].iter().any(|kind| {
            suffix
                .strip_prefix(kind)
                .is_some_and(|width| matches!(width, "8" | "16" | "32" | "64" | "128" | "size"))
        });
    let digits: String = digits.chars().filter(|c| *c != '_').collect();
    if !suffix_ok || digits.is_empty() {
        return None;
    }
    let value = u128::from_str_radix(&digits, radix).ok()?;
    Some(format!("{value}{suffix}"))
}

#[cfg(test)]
mod tests {
    use super::only_respells_integer_literals as respells;

    #[test]
    fn a_respelled_integer_keeps_its_number_and_type() {
        assert!(respells("\"ftp\" => Some(0x15),", "\"ftp\" => Some(21),"));
        assert!(respells("x = 0b1010_u8;", "x = 10_u8;"));
        assert!(respells("x = 1_000;", "x = 1000;"));
        assert!(respells("x = 0o17;", "x = 15;"));
        assert!(respells("Some(0xFF)", "Some(255)"));
    }

    #[test]
    fn a_changed_number_type_or_other_text_is_not_a_respelling() {
        assert!(!respells("Some(0x16)", "Some(21)"));
        assert!(!respells("Some(21)", "Some(21)"));
        // A different suffix may change the type.
        assert!(!respells("x = 21_u16;", "x = 21;"));
        assert!(!respells("x = 21u8;", "x = 21u16;"));
        // Other text changed beside the literal.
        assert!(!respells(
            "\"ftp\" => Some(0x15),",
            "\"gopher\" => Some(21),"
        ));
        assert!(!respells("f(0x15, y)", "f(21, z)"));
        // Floats and names that end in digits are not integer literals.
        assert!(!respells("x = 1.50;", "x = 1.5;"));
        assert!(!respells("x = 1e3;", "x = 1000;"));
        assert!(!respells("x21 = 1;", "x0x15 = 1;"));
    }
}

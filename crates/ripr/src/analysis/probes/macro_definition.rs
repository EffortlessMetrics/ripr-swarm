//! `macro_rules!` definition extents (#8999 item 6).
//!
//! A line inside a `macro_rules!` definition is matcher or transcriber token
//! text (`($args:ident) => {`, `$field: $ty`), not a Rust expression. Seeding
//! it as `field_construction`, `match_arm` or `call_deletion` would grade a
//! limitation as `weakly_exposed` with confidence, so the probe seeder asks
//! this module which lines are macro-definition text and emits one honest
//! `static_unknown` probe per definition instead.

use crate::analysis::extract::mask_comments_and_strings;

/// One `macro_rules!` definition: its name and 1-based inclusive line extent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct MacroDefinition {
    pub name: String,
    pub start_line: usize,
    pub end_line: usize,
}

fn is_ident_byte(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphanumeric() || byte >= 0x80
}

fn skip_ws(bytes: &[u8], mut at: usize) -> usize {
    while bytes.get(at).is_some_and(|b| b.is_ascii_whitespace()) {
        at += 1;
    }
    at
}

/// Every `macro_rules! name { .. }` definition in `source`, read from the
/// comment/string-masked text so a mention in prose does not count. A
/// definition that never closes runs to the end of the file (fail-closed:
/// more lines are treated as macro text, never fewer).
pub(super) fn macro_definitions(source: &str) -> Vec<MacroDefinition> {
    let masked = mask_comments_and_strings(source);
    let bytes = masked.as_bytes();
    let line_of = |at: usize| bytes[..at].iter().filter(|b| **b == b'\n').count() + 1;
    let mut found = Vec::new();
    let mut cursor = 0usize;
    while let Some(relative) = masked[cursor..].find("macro_rules") {
        let at = cursor + relative;
        let after = at + "macro_rules".len();
        cursor = after;
        if at > 0 && is_ident_byte(bytes[at - 1]) {
            continue;
        }
        let bang = skip_ws(bytes, after);
        if bytes.get(bang) != Some(&b'!') {
            continue;
        }
        let name_start = skip_ws(bytes, bang + 1);
        let mut name_end = name_start;
        while bytes.get(name_end).is_some_and(|b| is_ident_byte(*b)) {
            name_end += 1;
        }
        if name_end == name_start {
            continue;
        }
        let open = skip_ws(bytes, name_end);
        if !matches!(bytes.get(open), Some(b'(' | b'[' | b'{')) {
            continue;
        }
        let mut depth = 0usize;
        let mut end = bytes.len().saturating_sub(1);
        for (offset, byte) in bytes[open..].iter().enumerate() {
            match byte {
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        end = open + offset;
                        break;
                    }
                }
                _ => {}
            }
        }
        found.push(MacroDefinition {
            name: masked[name_start..name_end].to_string(),
            start_line: line_of(at),
            end_line: line_of(end),
        });
        cursor = end.max(after);
    }
    found
}

/// The definition whose extent holds `line`, if any.
pub(super) fn definition_at(
    definitions: &[MacroDefinition],
    line: usize,
) -> Option<&MacroDefinition> {
    definitions
        .iter()
        .find(|definition| definition.start_line <= line && line <= definition.end_line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extents_cover_matcher_and_transcriber_lines() {
        let source = "fn a() {}\nmacro_rules! m {\n    ($x:ident) => {\n        $x { f: 1 }\n    };\n}\nfn b() {}\n";
        let defs = macro_definitions(source);
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "m");
        assert_eq!((defs[0].start_line, defs[0].end_line), (2, 6));
        assert!(definition_at(&defs, 1).is_none());
        assert!(definition_at(&defs, 4).is_some());
        assert!(definition_at(&defs, 7).is_none());
    }

    #[test]
    fn prose_and_lookalikes_are_not_definitions() {
        let source = "// macro_rules! m { (x) => {} }\nlet s = \"macro_rules! n { }\";\nfn my_macro_rules() {}\nmacro_rules_x! { }\n";
        assert!(macro_definitions(source).is_empty());
    }

    #[test]
    fn an_unclosed_definition_runs_to_the_end_of_the_file() {
        let defs = macro_definitions("macro_rules! m {\n    () => {\n");
        assert_eq!(defs.len(), 1);
        assert!(defs[0].end_line >= 2);
    }
}

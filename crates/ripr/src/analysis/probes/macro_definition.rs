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
    /// Byte offset of `macro_rules` and one past the closing delimiter.
    pub start_byte: usize,
    pub end_byte: usize,
    /// A `verus! { .. }` token tree, not a `macro_rules!` definition.
    pub verus_region: bool,
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
/// definition that never closes runs to the end of the file, so every later
/// line counts as macro text (typically an edit in progress); it is not
/// evidence that those lines are macro text.
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
        let mut name_start = skip_ws(bytes, bang + 1);
        // A raw identifier (`r#type`) is a legal macro name.
        if bytes.get(name_start) == Some(&b'r')
            && bytes.get(name_start + 1) == Some(&b'#')
            && bytes.get(name_start + 2).is_some_and(|b| is_ident_byte(*b))
        {
            name_start += 2;
        }
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
        let end = matching_close(bytes, open);
        let name = masked[name_start..name_end].to_string();
        let Some(end) = end else {
            // Unclosed: runs to the end of the file; nothing left to scan.
            found.push(MacroDefinition {
                name,
                start_line: line_of(at),
                end_line: line_of(bytes.len()),
                start_byte: at,
                end_byte: bytes.len(),
                verus_region: false,
            });
            break;
        };
        found.push(MacroDefinition {
            name,
            start_line: line_of(at),
            end_line: line_of(end),
            start_byte: at,
            end_byte: end + 1,
            verus_region: false,
        });
        cursor = end + 1;
    }
    found
}

/// Every `verus! { .. }` token tree in `source` (RIPR-SPEC-0369). The tree is
/// Verus syntax (`requires`, `ensures`, `spec fn`, `proof { .. }`), not Rust,
/// so ripr resolves no function, call or oracle inside it. Read from the
/// comment/string-masked text; an unclosed tree runs to the end of the file.
/// The `verus` token must start an identifier and be followed by `!` and an
/// opening delimiter, so `my_verus!`, `verus_other!` and prose do not count.
pub(super) fn verus_regions(source: &str) -> Vec<MacroDefinition> {
    if !source.contains("verus") {
        return Vec::new();
    }
    let masked = mask_comments_and_strings(source);
    let bytes = masked.as_bytes();
    let line_of = |at: usize| bytes[..at].iter().filter(|b| **b == b'\n').count() + 1;
    let mut found = Vec::new();
    let mut cursor = 0usize;
    while let Some(relative) = masked[cursor..].find("verus") {
        let at = cursor + relative;
        let after = at + "verus".len();
        cursor = after;
        if at > 0 && is_ident_byte(bytes[at - 1]) {
            continue;
        }
        let bang = skip_ws(bytes, after);
        if bytes.get(bang) != Some(&b'!') {
            continue;
        }
        let open = skip_ws(bytes, bang + 1);
        if !matches!(bytes.get(open), Some(b'(' | b'[' | b'{')) {
            continue;
        }
        let end = matching_close(bytes, open);
        found.push(MacroDefinition {
            name: "verus".to_string(),
            start_line: line_of(at),
            end_line: line_of(end.unwrap_or(bytes.len())),
            start_byte: at,
            end_byte: end.map_or(bytes.len(), |end| end + 1),
            verus_region: true,
        });
        let Some(end) = end else {
            break;
        };
        cursor = end + 1;
    }
    found
}

/// The 1-based inclusive line extent of the `verus! { .. }` region that wholly
/// owns `line` of `source`, if any (RIPR-SPEC-0369).
pub(crate) fn verus_region_lines(source: &str, line: usize) -> Option<(usize, usize)> {
    MacroMap::new(source)
        .verus_region_of_line(line)
        .map(|region| (region.start_line, region.end_line))
}

/// Byte offset of the delimiter closing the one at `open`, by depth count.
fn matching_close(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, byte) in bytes[open..].iter().enumerate() {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

/// The macro definitions of one source file plus its line offsets, so a
/// changed line can be tested for being macro text *entirely*.
pub(super) struct MacroMap {
    definitions: Vec<MacroDefinition>,
    line_starts: Vec<usize>,
    source: String,
}

impl MacroMap {
    pub(super) fn new(source: &str) -> Self {
        let mut line_starts = vec![0usize];
        line_starts.extend(source.match_indices('\n').map(|(at, _)| at + 1));
        Self {
            definitions: macro_definitions(source)
                .into_iter()
                .chain(verus_regions(source))
                .collect(),
            line_starts,
            source: source.to_string(),
        }
    }

    /// The definition that owns the whole trimmed content of 1-based `line`.
    /// A line that also holds code outside the definition's byte extent
    /// (`fn f() { macro_rules! m { () => { 1 } } let n = 3; }`) is mixed and
    /// owned by nobody, so normal seeding still sees its other code. A
    /// trailing `;` after the closing delimiter belongs to the definition.
    /// Whether the definition owning `line` is a `verus! { .. }` region.
    pub(super) fn verus_region_of_line(&self, line: usize) -> Option<&MacroDefinition> {
        self.owner_of_line(line)
            .filter(|definition| definition.verus_region)
    }

    pub(super) fn owner_of_line(&self, line: usize) -> Option<&MacroDefinition> {
        let start = *self.line_starts.get(line.checked_sub(1)?)?;
        let end = self
            .line_starts
            .get(line)
            .copied()
            .unwrap_or(self.source.len());
        let text = self.source.get(start..end)?;
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }
        let content_start = start + (text.len() - text.trim_start().len());
        let content_end = content_start + trimmed.len();
        self.definitions.iter().find(|definition| {
            // Allow the item-terminating `;` right after the definition.
            let def_end = definition.end_byte
                + usize::from(self.source.as_bytes().get(definition.end_byte) == Some(&b';'));
            definition.start_byte <= content_start && content_end <= def_end
        })
    }
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
        let map = MacroMap::new(source);
        assert!(map.owner_of_line(1).is_none());
        assert!(map.owner_of_line(4).is_some());
        assert!(map.owner_of_line(7).is_none());
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
        assert_eq!(defs[0].end_line, 3);
    }

    #[test]
    fn an_unclosed_definition_with_trailing_non_ascii_does_not_panic() {
        let defs = macro_definitions("macro_rules! m {\n    () => { é");
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].end_line, 2);
        let defs = macro_definitions("macro_rules! m (\n    é");
        assert_eq!(defs.len(), 1);
    }

    #[test]
    fn paren_and_bracket_delimiters_are_definitions() {
        let defs = macro_definitions(
            "macro_rules! p (\n    () => {};\n);\nmacro_rules! b [\n    () => {};\n];\n",
        );
        assert_eq!(defs.len(), 2);
        assert_eq!((defs[0].start_line, defs[0].end_line), (1, 3));
        assert_eq!((defs[1].start_line, defs[1].end_line), (4, 6));
    }

    #[test]
    fn nested_braces_and_repetitions_close_at_the_outer_delimiter() {
        let source = "macro_rules! m {\n    ($($x:expr),*) => {\n        $( { $x } )*\n    };\n}\nfn after() {}\n";
        let defs = macro_definitions(source);
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].end_line, 5);
    }

    #[test]
    fn a_brace_char_literal_does_not_unbalance_the_extent() {
        let source = "macro_rules! m {\n    () => { '{' };\n}\nfn after() {}\n";
        let defs = macro_definitions(source);
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].end_line, 3);
    }

    #[test]
    fn spacing_around_the_bang_is_accepted() {
        let defs = macro_definitions("macro_rules !  spaced {\n}\nmacro_rules\n!\nsecond {\n}\n");
        assert_eq!(defs.len(), 2);
        assert_eq!(defs[0].name, "spaced");
        assert_eq!(defs[1].name, "second");
    }

    #[test]
    fn two_same_named_definitions_are_both_found() {
        let defs = macro_definitions("macro_rules! m {\n}\nmacro_rules! m {\n}\n");
        assert_eq!(defs.len(), 2);
        assert_eq!(defs[0].start_line, 1);
        assert_eq!(defs[1].start_line, 3);
    }

    #[test]
    fn raw_identifier_macro_names_are_definitions() {
        let defs = macro_definitions("macro_rules! r#type {\n    () => {};\n}\n");
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "type");
        assert_eq!(defs[0].end_line, 3);
    }

    #[test]
    fn a_mixed_line_is_owned_by_no_definition() {
        let source = "fn f() { macro_rules! m { () => { 1 } } let n = 3; if n > 4 { go(); } }\nmacro_rules! solo { () => { 2 } }\nmacro_rules! tail { () => { 3 } };\n";
        let map = MacroMap::new(source);
        assert!(map.owner_of_line(1).is_none(), "code outside the extent");
        assert_eq!(map.owner_of_line(2).map(|d| d.name.as_str()), Some("solo"));
        assert_eq!(map.owner_of_line(3).map(|d| d.name.as_str()), Some("tail"));
    }

    /// RIPR-SPEC-0369.
    #[test]
    fn verus_regions_cover_the_token_tree_and_ignore_lookalikes() {
        let source = "use vstd::prelude::*;\nverus! {\nfn f(x: u32) -> (r: u32)\n    ensures r == x,\n{ x }\n}\nfn after() {}\n";
        let regions = verus_regions(source);
        assert_eq!(regions.len(), 1);
        assert!(regions[0].verus_region);
        assert_eq!((regions[0].start_line, regions[0].end_line), (2, 6));
        let map = MacroMap::new(source);
        assert!(map.verus_region_of_line(1).is_none());
        assert!(map.verus_region_of_line(4).is_some());
        assert!(map.verus_region_of_line(7).is_none());
        assert_eq!(verus_region_lines(source, 4), Some((2, 6)));
        assert_eq!(verus_region_lines(source, 7), None);
        // A path-qualified call, other delimiters and spacing still match.
        for source in [
            "vstd::prelude::verus! {\n    spec fn s() -> bool { true }\n}\n",
            "verus !(\n    proof fn p() {}\n);\n",
            "verus![\n    proof fn p() {}\n]\n",
        ] {
            assert_eq!(verus_regions(source).len(), 1, "{source}");
        }
        // Look-alikes, prose and a bare identifier are not regions.
        for source in [
            "my_verus! {\n    fn f() {}\n}\n",
            "verus_other! {\n    fn f() {}\n}\n",
            "// verus! { fn f() {} }\nlet s = \"verus! { }\";\n",
            "fn verus() {}\nlet verus = 1;\n",
            "/// Example:\n/// verus! {\n///     fn f() {}\n/// }\nfn g() {}\n",
            "//! verus! { fn f() {} }\nfn g() {}\n",
            "verus {\n}\n",
        ] {
            assert!(verus_regions(source).is_empty(), "{source}");
        }
    }

    #[test]
    fn a_verus_region_braces_in_comments_strings_and_chars_do_not_unbalance_it() {
        let source =
            "verus! {\n    // }\n    fn f() { let c = '}'; let s = \"}\"; }\n}\nfn after() {}\n";
        let regions = verus_regions(source);
        assert_eq!(regions.len(), 1);
        assert_eq!((regions[0].start_line, regions[0].end_line), (1, 4));
    }

    #[test]
    fn raw_strings_neither_open_nor_unbalance_a_verus_region() {
        let source = "verus! {\n    fn f() { let a = r\"}\"; let b = r#\"} \"x\" }\"#; }\n}\nfn after() {}\n";
        let regions = verus_regions(source);
        assert_eq!(regions.len(), 1);
        assert_eq!((regions[0].start_line, regions[0].end_line), (1, 3));
        let prose = "let s = r#\"verus! { fn f() {} }\"#;\nfn g() {}\n";
        assert!(verus_regions(prose).is_empty());
    }

    #[test]
    fn an_unclosed_verus_region_runs_to_the_end_and_a_mixed_line_is_unowned() {
        let source = "verus! {\n    fn f() {\n";
        let regions = verus_regions(source);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].end_line, 3);
        let mixed = "fn a() { verus! { fn b() {} } let n = 3; }\nverus! { fn c() {} }\n";
        let map = MacroMap::new(mixed);
        assert!(
            map.verus_region_of_line(1).is_none(),
            "code outside the extent"
        );
        assert!(map.verus_region_of_line(2).is_some());
    }
}

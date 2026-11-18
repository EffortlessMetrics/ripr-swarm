//! #14760: a line that only applies an identifier rename the same diff
//! proves at the declaration seeds no probe; every unproven rename stays
//! probed.

use super::SeededProbe;
use super::identifier_rename::rename_only_lines;
use super::replacement_identity_tests::git_probes;
use crate::analysis::diff::parse_unified_diff;

/// The httparse 9f6702b shape: a table renamed with its values unchanged,
/// its one use site respelled, and the commit's real change elsewhere.
const BEFORE: &str = "\
static HEADER_NAME_MAP: [bool; 4] = [
    false, true,
    true, false,
];

pub(crate) fn is_header_name_token(b: u8) -> bool {
    HEADER_NAME_MAP[b as usize]
}

pub fn is_method_token(b: u8) -> bool {
    b > 1
}
";

const AFTER: &str = "\
static TOKEN_MAP: [bool; 4] = [
    false, true,
    true, false,
];

pub(crate) fn is_header_name_token(b: u8) -> bool {
    TOKEN_MAP[b as usize]
}

pub fn is_method_token(b: u8) -> bool {
    b >= 1
}
";

/// The rename's declaration line and use-site line in [`AFTER`].
const DECLARATION_LINE: usize = 1;
const USE_LINE: usize = 7;
const METHOD_LINE: usize = 11;

fn probe_lines(probes: &[SeededProbe]) -> Vec<(usize, String)> {
    let mut lines = probes
        .iter()
        .map(|seed| (seed.probe.location.line, seed.probe.expression.clone()))
        .collect::<Vec<_>>();
    lines.sort();
    lines
}

fn probed(probes: &[SeededProbe], line: usize) -> bool {
    probes.iter().any(|seed| seed.probe.location.line == line)
}

#[test]
fn a_proven_rename_seeds_no_probe_but_the_real_change_does() -> Result<(), String> {
    let probes = git_probes(BEFORE, AFTER)?;
    let lines = probe_lines(&probes);
    assert!(!probed(&probes, DECLARATION_LINE), "{lines:?}");
    assert!(!probed(&probes, USE_LINE), "{lines:?}");
    assert!(probed(&probes, METHOD_LINE), "{lines:?}");
    Ok(())
}

/// Control: without the rename the same use-site respelling is a real edit
/// (a different table), so the guard is what removes the probe above.
#[test]
fn the_same_use_site_edit_without_a_proven_rename_is_probed() -> Result<(), String> {
    let cases = [
        (
            "the renamed item's body changed",
            AFTER.replace("    false, true,\n    true", "    true, true,\n    true"),
            BEFORE.to_owned(),
        ),
        (
            "the new name already existed in the base",
            format!("{AFTER}\nmod table {{\n    pub static TOKEN_MAP: [bool; 1] = [true];\n}}\n"),
            format!("{BEFORE}\nmod table {{\n    pub static TOKEN_MAP: [bool; 1] = [true];\n}}\n"),
        ),
        (
            "the old name is still used",
            format!("{AFTER}\npub fn still_old() -> bool {{\n    HEADER_NAME_MAP[0]\n}}\n"),
            format!("{BEFORE}\npub fn still_old() -> bool {{\n    HEADER_NAME_MAP[0]\n}}\n"),
        ),
        (
            "an attribute can observe the name",
            format!("#[unsafe(no_mangle)]\n{AFTER}"),
            format!("#[unsafe(no_mangle)]\n{BEFORE}"),
        ),
        (
            "a blank line separates the attribute from the item",
            format!("#[unsafe(no_mangle)]\n\n{AFTER}"),
            format!("#[unsafe(no_mangle)]\n\n{BEFORE}"),
        ),
        (
            "a field shares the renamed spelling",
            format!("{AFTER}\npub struct Row {{\n    pub TOKEN_MAP: u8,\n}}\n"),
            format!("{BEFORE}\npub struct Row {{\n    pub HEADER_NAME_MAP: u8,\n}}\n"),
        ),
        (
            "an assert prints the renamed name",
            format!("{AFTER}\npub fn check() {{\n    assert!(TOKEN_MAP[1]);\n}}\n"),
            format!("{BEFORE}\npub fn check() {{\n    assert!(HEADER_NAME_MAP[1]);\n}}\n"),
        ),
        (
            "the file can print a value's type name",
            format!(
                "{AFTER}\npub fn name() -> &'static str {{\n    core::any::type_name_of_val(&1)\n}}\n"
            ),
            format!(
                "{BEFORE}\npub fn name() -> &'static str {{\n    core::any::type_name_of_val(&1)\n}}\n"
            ),
        ),
        (
            "the file can observe identifier text",
            format!("{AFTER}\npub fn name() -> &'static str {{\n    stringify!(x)\n}}\n"),
            format!("{BEFORE}\npub fn name() -> &'static str {{\n    stringify!(x)\n}}\n"),
        ),
    ];
    let mut unprobed = Vec::new();
    for (case, after, before) in cases {
        let probes = git_probes(&before, &after)?;
        let use_line = after
            .lines()
            .position(|line| line.contains("TOKEN_MAP[b as usize]"))
            .ok_or("fixture lost its use site")?
            + 1;
        if !probed(&probes, use_line) {
            unprobed.push((case, probe_lines(&probes)));
        }
    }
    assert_eq!(unprobed, Vec::new());
    Ok(())
}

/// A use site that renames and also changes another token is a real edit
/// even when the rename itself is established.
#[test]
fn a_use_site_that_changes_more_than_the_name_stays_probed() -> Result<(), String> {
    let after = AFTER.replace("TOKEN_MAP[b as usize]", "TOKEN_MAP[b as usize + 1]");
    let probes = git_probes(BEFORE, &after)?;
    let lines = probe_lines(&probes);
    assert!(!probed(&probes, DECLARATION_LINE), "{lines:?}");
    assert!(probed(&probes, USE_LINE), "{lines:?}");
    Ok(())
}

/// A renamed function, its recursive call inside its own body and a caller
/// all respell the same name: nothing in the rename is probed.
#[test]
fn a_renamed_function_and_its_callers_seed_no_probe() -> Result<(), String> {
    let before = "\
pub fn count_down(n: u8) -> u8 {
    if n == 0 { 0 } else { count_down(n - 1) }
}

pub fn start() -> u8 {
    count_down(3)
}
";
    let after = before.replace("count_down", "countdown");
    let probes = git_probes(before, &after)?;
    assert_eq!(probe_lines(&probes), Vec::new());
    Ok(())
}

/// `main` is bound by its spelling, so renaming it is never established; the
/// same rename of an ordinary function is.
#[test]
fn renaming_an_entry_point_is_never_proven() -> Result<(), String> {
    let rename = |from: &str| -> Result<Vec<usize>, String> {
        let patch = format!(
            "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-fn {from}() {{\n+fn run() {{\n"
        );
        let files = parse_unified_diff(&patch);
        let [changed] = files.as_slice() else {
            return Err(format!("one changed file expected: {patch}"));
        };
        let old = format!("fn {from}() {{\n    work();\n}}\n");
        let new = "fn run() {\n    work();\n}\n";
        Ok(rename_only_lines(changed, new, &old)
            .added
            .into_iter()
            .collect())
    };
    assert_eq!(rename("start")?, vec![1]);
    assert_eq!(rename("main")?, Vec::<usize>::new());
    Ok(())
}

/// Same-spelled names that are not the renamed item, or a renamed item whose
/// value still changes: the use site keeps its probe (Devin review on #14779).
#[test]
fn a_rename_that_may_not_mean_the_same_item_stays_probed() -> Result<(), String> {
    let cases: [(&str, &str, &str, &str); 6] = [
        (
            "a receiver method shares the renamed spelling",
            "fn len() -> usize {\n    0\n}\n\npub fn measure(v: &Vec<u8>) -> usize {\n    v.len() + len()\n}\n",
            "fn capacity() -> usize {\n    0\n}\n\npub fn measure(v: &Vec<u8>) -> usize {\n    v.capacity() + capacity()\n}\n",
            "v.capacity() + capacity()",
        ),
        (
            "a macro defined elsewhere sees the renamed token",
            "const OLD: bool = true;\n\npub fn pick() -> bool {\n    dispatch!(OLD)\n}\n",
            "const NEW: bool = true;\n\npub fn pick() -> bool {\n    dispatch!(NEW)\n}\n",
            "dispatch!(NEW)",
        ),
        (
            "a block-valued constant's suffix changed",
            "const OLD: i32 = { 1 }\n    + 2;\n\npub fn value() -> i32 {\n    OLD * 2\n}\n",
            "const NEW: i32 = { 1 }\n    + 3;\n\npub fn value() -> i32 {\n    NEW * 2\n}\n",
            "NEW * 2",
        ),
        (
            "a foreign item binds its linker symbol by spelling",
            "unsafe extern \"C\" {\n    fn strlen(s: *const u8) -> usize;\n}\n\npub fn length(s: *const u8) -> usize {\n    unsafe { strlen(s) }\n}\n",
            "unsafe extern \"C\" {\n    fn my_strlen(s: *const u8) -> usize;\n}\n\npub fn length(s: *const u8) -> usize {\n    unsafe { my_strlen(s) }\n}\n",
            "unsafe { my_strlen(s) }",
        ),
        (
            "the declaration sits inside a macro",
            "named_const! {\n    const OLD: u32 = 1;\n}\n\npub fn bits() -> u32 {\n    OLD + 1\n}\n",
            "named_const! {\n    const NEW: u32 = 1;\n}\n\npub fn bits() -> u32 {\n    NEW + 1\n}\n",
            "NEW + 1",
        ),
        (
            "an enum variant shares the renamed spelling",
            "#[derive(Debug)]\npub enum Mode {\n    Old,\n}\n\nconst Old: u8 = 1;\n\npub fn mode() -> u8 {\n    Old + 1\n}\n",
            "#[derive(Debug)]\npub enum Mode {\n    New,\n}\n\nconst New: u8 = 1;\n\npub fn mode() -> u8 {\n    New + 1\n}\n",
            "New + 1",
        ),
    ];
    let mut unprobed = Vec::new();
    for (case, before, after, use_site) in cases {
        let probes = git_probes(before, after)?;
        let use_line = after
            .lines()
            .position(|line| line.contains(use_site))
            .ok_or("fixture lost its use site")?
            + 1;
        if !probed(&probes, use_line) {
            unprobed.push((case, probe_lines(&probes)));
        }
    }
    assert_eq!(unprobed, Vec::new());
    Ok(())
}

/// Control for the guards above: a struct, a macro call, an `extern "C" fn`
/// definition and a negation elsewhere in the file do not stop a real rename
/// from being established.
#[test]
fn unrelated_structs_macros_and_extern_fns_keep_the_rename_established() -> Result<(), String> {
    let tail = "\npub struct Row {\n    pub width: u8,\n}\n\npub extern \"C\" fn exported() -> u8 {\n    if !is_header_name_token(1) { 0 } else { 1 }\n}\n\npub fn check() {\n    assert_eq!(1, 1);\n}\n";
    let probes = git_probes(&format!("{BEFORE}{tail}"), &format!("{AFTER}{tail}"))?;
    let lines = probe_lines(&probes);
    assert!(!probed(&probes, DECLARATION_LINE), "{lines:?}");
    assert!(!probed(&probes, USE_LINE), "{lines:?}");
    assert!(probed(&probes, METHOD_LINE), "{lines:?}");
    Ok(())
}

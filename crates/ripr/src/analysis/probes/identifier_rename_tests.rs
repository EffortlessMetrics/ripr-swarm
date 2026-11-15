//! #14760: a line that only applies an identifier rename the same diff
//! proves at the declaration seeds no probe; every unproven rename stays
//! probed.

use super::SeededProbe;
use super::replacement_identity_tests::git_probes;

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
/// even when the rename itself is proven.
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

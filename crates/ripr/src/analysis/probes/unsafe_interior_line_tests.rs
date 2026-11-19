//! #14333: a changed line inside an unsafe boundary opened on an earlier line
//! names the edited line as its changed text, while the boundary label stays
//! the probe expression (identity, gap id and the unsafe obligation).

use super::SeededProbe;
use super::replacement_identity_tests::git_probes;
use crate::domain::ProbeFamily;

fn unsafe_probe_at(probes: &[SeededProbe], line: usize) -> Result<&SeededProbe, String> {
    probes
        .iter()
        .find(|seeded| {
            seeded.probe.location.line == line
                && seeded.probe.family == ProbeFamily::StaticUnknown
                && seeded.probe.expression.starts_with("unsafe ")
        })
        .ok_or_else(|| {
            format!(
                "no unsafe boundary probe on line {line}: {:?}",
                probes
                    .iter()
                    .map(|seeded| (
                        seeded.probe.location.line,
                        seeded.probe.expression.clone(),
                        seeded.probe.after.clone()
                    ))
                    .collect::<Vec<_>>()
            )
        })
}

/// dtolnay/semver `38fd181`: the `unsafe { .. }` block opens mid-line after
/// `let repr = `, so the enclosing `unsafe fn` owns the line. The finding
/// keeps that obligation and shows the edited statement, not the fn header.
#[test]
fn a_mid_line_block_inside_an_unsafe_fn_shows_the_edited_line() -> Result<(), String> {
    let before = "pub unsafe fn inline_len(repr: *const u64) -> u64 {\n    let repr = unsafe { core::ptr::read(repr) };\n    repr\n}\n";
    let after = "pub unsafe fn inline_len(repr: *const u64) -> u64 {\n    let repr = unsafe { repr.read() };\n    repr\n}\n";
    let probes = git_probes(before, after)?;
    let probe = &unsafe_probe_at(&probes, 2)?.probe;
    assert_eq!(probe.expression, "unsafe fn inline_len");
    assert_eq!(
        probe.after.as_deref(),
        Some("let repr = unsafe { repr.read() };")
    );
    assert_eq!(
        probe.before.as_deref(),
        Some("let repr = unsafe { core::ptr::read(repr) };")
    );
    Ok(())
}

/// Alternate: an interior line of an `unsafe { }` block opened on its own
/// line names the edit too, under the block's label.
#[test]
fn an_interior_line_of_an_unsafe_block_shows_the_edited_line() -> Result<(), String> {
    let before = "pub fn first(p: *const u8) -> u8 {\n    unsafe {\n        *p.add(0)\n    }\n}\n";
    let after = "pub fn first(p: *const u8) -> u8 {\n    unsafe {\n        *p.add(1)\n    }\n}\n";
    let probes = git_probes(before, after)?;
    let probe = &unsafe_probe_at(&probes, 3)?.probe;
    assert_eq!(probe.expression, "unsafe block");
    assert_eq!(probe.after.as_deref(), Some("*p.add(1)"));
    Ok(())
}

/// Negative: a changed line that opens the boundary itself keeps the label as
/// its changed text, and a mid-line block in a safe fn still gets no unsafe
/// boundary probe (the shared-edge rule fails closed).
#[test]
fn the_boundary_opening_line_and_a_safe_fn_mid_line_block_are_unchanged() -> Result<(), String> {
    let before = "pub fn first(p: *const u8) -> u8 {\n    unsafe { p.read() }\n}\n";
    let after = "pub fn first(p: *const u8) -> u8 {\n    unsafe { p.add(1).read() }\n}\n";
    let probes = git_probes(before, after)?;
    let probe = &unsafe_probe_at(&probes, 2)?.probe;
    assert_eq!(probe.expression, "unsafe block");
    assert_eq!(probe.after.as_deref(), Some("unsafe block"));

    let before = "pub fn first(p: *const u8) -> u8 {\n    let v = unsafe { p.read() };\n    v\n}\n";
    let after =
        "pub fn first(p: *const u8) -> u8 {\n    let v = unsafe { p.add(1).read() };\n    v\n}\n";
    let probes = git_probes(before, after)?;
    assert!(
        unsafe_probe_at(&probes, 2).is_err(),
        "a shared edge line outside any unsafe fn must not get a boundary probe"
    );
    assert!(
        probes.iter().any(|seeded| seeded.probe.location.line == 2),
        "the changed line keeps its ordinary probe: {probes:?}"
    );
    Ok(())
}

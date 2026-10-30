//! Issue #10925: `local_identifier_declared_in_test_body` is asked once per
//! candidate name per test body (about 20k calls from 200 bodies on zod). A
//! body that never spells the name cannot shadow it, so the answer skips the
//! lexer walk and the literal mask; every body that does spell it still gets
//! the full scan and the same answer as before.

use super::related_tests::{
    SHADOW_SCAN_RUNS, local_identifier_declared_in_test_body,
    local_identifier_declared_in_test_body_scanned,
};

fn scan_runs() -> usize {
    SHADOW_SCAN_RUNS.with(std::cell::Cell::get)
}

const BODY: &str = "it('adds', () => {\n  const total = add(1, 2);\n  // note: cost is read below\n  const label = 'price';\n  use(label);\n  try { run(); } catch (err) { log(err); }\n  for (const item of items) { use(item); }\n  expect(total).toBe(3);\n});";

const NAMES: &[&str] = &[
    "total", "label", "err", "item", "it", "cost", "price", "add", "run", "missing", "ghost",
    "owner", "a", "tot", "",
];

#[test]
fn answers_equal_the_unconditional_scan_for_every_name() -> Result<(), String> {
    let mut shadowed = Vec::new();
    for name in NAMES {
        let expected = local_identifier_declared_in_test_body_scanned(BODY, name);
        let actual = local_identifier_declared_in_test_body(BODY, name);
        if actual != expected {
            return Err(format!("{name:?}: expected {expected}, got {actual}"));
        }
        if actual {
            shadowed.push(*name);
        }
    }
    for name in ["total", "label", "err", "item"] {
        if !shadowed.contains(&name) {
            return Err(format!("{name} should be shadowed; got {shadowed:?}"));
        }
    }
    for name in ["cost", "price", "add", "missing", "tot"] {
        if shadowed.contains(&name) {
            return Err(format!("{name} must not be shadowed; got {shadowed:?}"));
        }
    }
    Ok(())
}

#[test]
fn names_the_body_never_spells_skip_the_scan() -> Result<(), String> {
    let before = scan_runs();
    for name in ["missing", "ghost", "owner", "zzz"] {
        if local_identifier_declared_in_test_body(BODY, name) {
            return Err(format!("{name} is not spelled in the body"));
        }
    }
    let after = scan_runs();
    if after != before {
        return Err(format!(
            "absent names must not scan the body; scans went {before} -> {after}"
        ));
    }
    if !local_identifier_declared_in_test_body(BODY, "total") {
        return Err("total is declared in the body".to_string());
    }
    if scan_runs() != after + 1 {
        return Err(format!(
            "a spelled name scans once; scans went {after} -> {}",
            scan_runs()
        ));
    }
    Ok(())
}

//! #15331: an unresolved TypeScript expected value is worded for the shape it
//! has. `undefined` is not "a variable", and an object literal holding
//! variables is not "computed". Wording only: all three shapes stay
//! unresolved and no repair is offered.

#![cfg(feature = "lang-typescript")]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

const GENERIC: &str = "compares against a variable or computed value, not a literal";

fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(from).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let target = to.join(entry.file_name());
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Human `check` output for the `ts_dynamic_oracle` fixture with its test
/// body replaced by `test_source`.
fn check_with_test(test_source: &str) -> Result<String, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/ts_dynamic_oracle");
    let root = std::env::temp_dir().join(format!(
        "ripr-ts-shape-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    copy_tree(&fixture.join("input"), &root)?;
    std::fs::copy(fixture.join("diff.patch"), root.join("diff.patch"))
        .map_err(|e| e.to_string())?;
    std::fs::write(root.join("tests/pricing.test.ts"), test_source).map_err(|e| e.to_string())?;
    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(&root)
        .args([
            "check",
            "--root",
            ".",
            "--diff",
            "diff.patch",
            "--mode",
            "fast",
        ])
        .output()
        .map_err(|e| e.to_string())?;
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    // Best-effort cleanup of this test's own temp tree.
    let _ = std::fs::remove_dir_all(&root);
    Ok(text)
}

const HEAD: &str = "import { computePrice } from '../src/pricing';\n\ntest('computePrice returns expected', () => {\n";

#[test]
fn a_named_variable_keeps_the_generic_wording() -> Result<(), String> {
    let text = check_with_test(&format!(
        "{HEAD}    const expected = 0;\n    expect(computePrice(0, 2)).toBe(expected);\n}});\n"
    ))?;
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(flat.contains(GENERIC), "{text}");
    Ok(())
}

#[test]
fn undefined_is_not_called_a_variable() -> Result<(), String> {
    let text = check_with_test(&format!(
        "{HEAD}    expect(computePrice(0, 2)).toBe(undefined);\n}});\n"
    ))?;
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(flat.contains("expects `undefined`"), "{text}");
    assert!(!flat.contains(GENERIC), "{text}");
    Ok(())
}

#[test]
fn an_object_literal_holding_variables_says_it_contains_variables() -> Result<(), String> {
    let text = check_with_test(&format!(
        "{HEAD}    const a = 1;\n    expect(computePrice(0, 2)).toEqual({{ items: [a, 2] }});\n}});\n"
    ))?;
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(flat.contains("contains variables"), "{text}");
    assert!(!flat.contains(GENERIC), "{text}");
    Ok(())
}

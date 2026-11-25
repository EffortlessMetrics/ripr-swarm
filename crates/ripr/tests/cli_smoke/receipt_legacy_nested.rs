//! #15899: `ripr receipt check --gap` finds the receipt ripr 0.10 wrote for an
//! ordinary Rust gap id. The id embeds the file path, so 0.10 stored the
//! receipt in a nested path below `target/ripr/receipts/`.

use super::{ignore_remove_dir_all, spawn_command, unique_temp_workspace};
use std::path::{Path, PathBuf};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const GAP: &str =
    "gap:rust:src/lib.rs:discounted_total:predicate_boundary:predicate:amount==discount_threshold";

/// The bytes the published ripr 0.10.0 wrote for
/// `ripr receipt write --gap <GAP> --verify-command "cargo test" --status passed`.
const RECEIPT_0_10: &str = r#"{
  "canonical_gap_id": "gap:rust:src/lib.rs:discounted_total:predicate_boundary:predicate:amount==discount_threshold",
  "kind": "receipt",
  "limits_note": "Static evidence only. Receipt records what was run; does not certify semantic correctness.",
  "packet_id": null,
  "packet_id_available": false,
  "schema_version": "0.1",
  "tool": "ripr",
  "verify_command": "cargo test",
  "verify_status": "passed",
  "written_at": "2026-11-21T21:24:42Z"
}"#;

/// The file name 0.11 writes and looks for first.
const CURRENT_RELATIVE: &str = "target/ripr/receipts/gap%3Arust%3Asrc%2Flib.rs%3Adiscounted_total%3Apredicate_boundary%3Apredicate%3Aamount==discount_threshold.json";

struct Cleanup(PathBuf);

impl Drop for Cleanup {
    fn drop(&mut self) {
        ignore_remove_dir_all(&self.0);
    }
}

fn check_gap(root: &Path) -> TestResult<(Option<i32>, String)> {
    let output = spawn_command(
        env!("CARGO_BIN_EXE_ripr"),
        Some(root),
        &["receipt", "check", "--gap", GAP],
        &[],
        None,
        Some(b""),
    )?;
    Ok((output.status.code(), String::from_utf8(output.stderr)?))
}

/// The path 0.10.0 wrote: the raw id, one directory per `/`.
fn legacy_relative() -> String {
    format!("target/ripr/receipts/{GAP}.json")
}

fn assert_explains_the_0_10_receipt(code: Option<i32>, stderr: &str) {
    assert_eq!(code, Some(2), "{stderr}");
    assert!(
        stderr.contains(&format!("receipt at {} is malformed", legacy_relative())),
        "the 0.10 receipt must be the file that is checked: {stderr}"
    );
    assert!(
        stderr.contains("written by ripr 0.10 or earlier"),
        "{stderr}"
    );
    assert!(stderr.contains("ripr receipt write --gap"), "{stderr}");
    assert!(!stderr.contains("not found"), "{stderr}");
}

fn assert_not_found_at_the_current_path(code: Option<i32>, stderr: &str) {
    assert_eq!(code, Some(2), "{stderr}");
    assert!(
        stderr.contains(&format!("receipt at {CURRENT_RELATIVE} not found")),
        "{stderr}"
    );
    assert!(stderr.contains("`--path`"), "{stderr}");
}

#[test]
fn receipt_check_gap_explains_a_0_10_receipt_for_a_rust_gap_id() -> TestResult {
    let root = unique_temp_workspace("receipt-legacy-nested");
    let _cleanup = Cleanup(root.clone());
    let receipt = root.join(legacy_relative());
    std::fs::create_dir_all(receipt.parent().ok_or("receipt path has a parent")?)?;
    std::fs::write(&receipt, RECEIPT_0_10)?;

    let (code, stderr) = check_gap(&root)?;
    assert_explains_the_0_10_receipt(code, &stderr);
    Ok(())
}

/// A `target` directory that is itself a link is still the receipts tree.
#[test]
fn receipt_check_gap_finds_a_0_10_receipt_under_a_linked_target() -> TestResult {
    let root = unique_temp_workspace("receipt-legacy-linked-target");
    let _cleanup = Cleanup(root.clone());
    let real_target = root.join("real-target");
    let receipt = real_target.join(format!("ripr/receipts/{GAP}.json"));
    std::fs::create_dir_all(receipt.parent().ok_or("receipt path has a parent")?)?;
    std::fs::write(&receipt, RECEIPT_0_10)?;
    std::os::unix::fs::symlink(&real_target, root.join("target"))?;

    let (code, stderr) = check_gap(&root)?;
    assert_explains_the_0_10_receipt(code, &stderr);
    Ok(())
}

/// A link on the nested 0.10 path that leads out of the receipts directory is
/// not followed, whether it is the nested directory or the receipt itself.
#[test]
fn receipt_check_gap_does_not_follow_a_0_10_path_out_of_the_receipts_directory() -> TestResult {
    let (nested_dir, file_name) = GAP.split_once('/').ok_or("the gap id has one separator")?;
    let file_name = format!("{file_name}.json");

    // The nested directory is a link to a directory outside.
    let root = unique_temp_workspace("receipt-legacy-dir-link");
    let _cleanup = Cleanup(root.clone());
    let outside = root.join("outside");
    std::fs::create_dir_all(&outside)?;
    std::fs::write(outside.join(&file_name), RECEIPT_0_10)?;
    std::fs::create_dir_all(root.join("target/ripr/receipts"))?;
    std::os::unix::fs::symlink(&outside, root.join("target/ripr/receipts").join(nested_dir))?;
    assert_eq!(
        std::fs::read_to_string(root.join(legacy_relative()))?,
        RECEIPT_0_10,
        "the escaping receipt must be readable through the link"
    );
    let (code, stderr) = check_gap(&root)?;
    assert_not_found_at_the_current_path(code, &stderr);

    // The receipt file is a link to a file outside.
    let root = unique_temp_workspace("receipt-legacy-file-link");
    let _cleanup = Cleanup(root.clone());
    let nested = root.join("target/ripr/receipts").join(nested_dir);
    std::fs::create_dir_all(&nested)?;
    std::fs::write(root.join("outside.json"), RECEIPT_0_10)?;
    std::os::unix::fs::symlink(root.join("outside.json"), nested.join(&file_name))?;
    assert_eq!(
        std::fs::read_to_string(root.join(legacy_relative()))?,
        RECEIPT_0_10,
        "the escaping receipt must be readable through the link"
    );
    let (code, stderr) = check_gap(&root)?;
    assert_not_found_at_the_current_path(code, &stderr);
    Ok(())
}

/// The current percent-encoded receipt wins over a 0.10 file for the same id.
#[test]
fn receipt_check_gap_prefers_the_current_file_over_a_0_10_receipt() -> TestResult {
    let root = unique_temp_workspace("receipt-legacy-current-wins");
    let _cleanup = Cleanup(root.clone());
    let legacy = root.join(legacy_relative());
    std::fs::create_dir_all(legacy.parent().ok_or("receipt path has a parent")?)?;
    std::fs::write(&legacy, RECEIPT_0_10)?;
    std::fs::write(root.join(CURRENT_RELATIVE), "not json")?;

    let (code, stderr) = check_gap(&root)?;
    assert_eq!(code, Some(2), "{stderr}");
    assert!(
        stderr.contains(&format!(
            "receipt at {CURRENT_RELATIVE} is malformed: invalid JSON"
        )),
        "{stderr}"
    );
    Ok(())
}

#[test]
fn receipt_check_gap_names_the_file_it_looked_for_and_the_path_route() -> TestResult {
    let root = unique_temp_workspace("receipt-legacy-absent");
    let _cleanup = Cleanup(root.clone());
    std::fs::create_dir_all(&root)?;

    let (code, stderr) = check_gap(&root)?;
    assert_not_found_at_the_current_path(code, &stderr);
    assert!(!stderr.contains("%25"), "{stderr}");
    Ok(())
}

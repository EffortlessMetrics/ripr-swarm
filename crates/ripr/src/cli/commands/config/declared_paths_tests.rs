//! #15495: `config validate` fails when an exact-path key names no file.

use super::validate_config;
use crate::config::CONFIG_FILE_NAME;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const ALL_KEYS: &str = "[analysis]\nproduction_like_targets = [\"tests/support.rs\"]\n\n[[analysis.test_harnesses]]\nregistration_id = \"h1\"\ntarget = \"tests/harness.rs\"\nkind = \"custom_harness\"\nadapter = \"libtest_mimic_v1\"\nmarker = \"libtest_mimic\"\n\n[languages.rust]\nhandwritten_files = [\"src/gen/handwritten.rs\"]\n";

const DECLARED: [&str; 3] = [
    "tests/support.rs",
    "tests/harness.rs",
    "src/gen/handwritten.rs",
];

fn workspace(label: &str) -> Result<PathBuf, String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let root = std::env::temp_dir().join(format!("ripr-config-paths-{label}-{nonce}"));
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    Ok(root)
}

fn write(root: &Path, relative: &str, text: &str) -> Result<(), String> {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(path, text).map_err(|error| error.to_string())
}

#[test]
fn present_declared_files_pass() -> Result<(), String> {
    let root = workspace("present")?;
    write(&root, CONFIG_FILE_NAME, ALL_KEYS)?;
    for file in DECLARED {
        write(&root, file, "")?;
    }
    let result = validate_config(&root);
    let _ = fs::remove_dir_all(&root);
    let output = result?;
    if !output.starts_with("✓ ripr.toml valid") {
        return Err(format!("unexpected output: {output}"));
    }
    Ok(())
}

#[test]
fn every_missing_declared_file_is_named_with_its_key() -> Result<(), String> {
    let root = workspace("missing")?;
    write(&root, CONFIG_FILE_NAME, ALL_KEYS)?;
    let result = validate_config(&root);
    let _ = fs::remove_dir_all(&root);
    let error = result.err().ok_or("missing declared files must fail")?;
    for expected in [
        "ripr.toml names 3 paths that are not a file under",
        "- analysis.production_like_targets: `tests/support.rs`",
        "- analysis.test_harnesses[0].target: `tests/harness.rs`",
        "- languages.rust.handwritten_files: `src/gen/handwritten.rs`",
        "Create the file, fix the path, or remove the entry.",
    ] {
        if !error.contains(expected) {
            return Err(format!("missing `{expected}` in: {error}"));
        }
    }
    Ok(())
}

#[test]
fn one_missing_file_among_present_ones_is_the_only_one_named() -> Result<(), String> {
    let root = workspace("one-missing")?;
    write(&root, CONFIG_FILE_NAME, ALL_KEYS)?;
    write(&root, "tests/support.rs", "")?;
    write(&root, "src/gen/handwritten.rs", "")?;
    let result = validate_config(&root);
    let _ = fs::remove_dir_all(&root);
    let error = result.err().ok_or("one missing declared file must fail")?;
    if !(error.contains("ripr.toml names 1 path that is not a file under")
        && error.contains("`tests/harness.rs`")
        && !error.contains("`tests/support.rs`")
        && !error.contains("`src/gen/handwritten.rs`"))
    {
        return Err(format!("unexpected error: {error}"));
    }
    Ok(())
}

#[test]
fn a_directory_at_a_declared_path_counts_as_missing() -> Result<(), String> {
    let root = workspace("directory")?;
    write(&root, CONFIG_FILE_NAME, ALL_KEYS)?;
    write(&root, "tests/support.rs", "")?;
    write(&root, "src/gen/handwritten.rs", "")?;
    fs::create_dir_all(root.join("tests/harness.rs")).map_err(|error| error.to_string())?;
    let result = validate_config(&root);
    let _ = fs::remove_dir_all(&root);
    let error = result.err().ok_or("a directory is not a declared file")?;
    if !error.contains("- analysis.test_harnesses[0].target: `tests/harness.rs`") {
        return Err(format!("unexpected error: {error}"));
    }
    Ok(())
}

#[test]
fn configs_without_declared_paths_stay_valid() -> Result<(), String> {
    let root = workspace("none")?;
    write(&root, CONFIG_FILE_NAME, "[analysis]\nmode = \"fast\"\n")?;
    let result = validate_config(&root);
    let _ = fs::remove_dir_all(&root);
    result.map(|_| ())
}

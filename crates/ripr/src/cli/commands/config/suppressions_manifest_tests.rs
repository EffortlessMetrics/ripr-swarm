//! #15308: `config validate` checks the suppressions manifest the config
//! selects, not only `ripr.toml`.

use super::validate_config;
use crate::config::CONFIG_FILE_NAME;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const VALID_MANIFEST: &str = "schema_version = 1\n[[suppressions]]\nkind = \"exposure_gap\"\npath = \"src/lib.rs\"\nreason = \"accepted debt\"\nowner = \"team\"\n";

fn workspace(label: &str) -> Result<PathBuf, String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let root = std::env::temp_dir().join(format!("ripr-config-suppr-{label}-{nonce}"));
    fs::create_dir_all(root.join(".ripr")).map_err(|error| error.to_string())?;
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
fn a_missing_configured_manifest_fails_and_names_the_key() -> Result<(), String> {
    let root = workspace("missing-configured")?;
    write(
        &root,
        CONFIG_FILE_NAME,
        "[suppressions]\npath = \"policy/missing.toml\"\n",
    )?;
    let result = validate_config(&root);
    let _ = fs::remove_dir_all(&root);
    let error = result
        .err()
        .ok_or("a missing configured manifest must fail")?;
    if !(error.contains("[suppressions] path `policy/missing.toml` does not exist")
        && error.contains("`.ripr/suppressions.toml`"))
    {
        return Err(format!("unexpected error: {error}"));
    }
    Ok(())
}

#[test]
fn a_missing_default_manifest_stays_valid_and_silent() -> Result<(), String> {
    let root = workspace("missing-default")?;
    write(&root, CONFIG_FILE_NAME, "[analysis]\nmode = \"fast\"\n")?;
    let with_config = validate_config(&root);
    fs::remove_file(root.join(CONFIG_FILE_NAME)).map_err(|error| error.to_string())?;
    let without_config = validate_config(&root);
    let _ = fs::remove_dir_all(&root);
    let with_config = with_config?;
    if with_config.contains("suppressions") || with_config.lines().count() != 1 {
        return Err(format!("unexpected manifest line: {with_config}"));
    }
    if without_config? != "✓ no ripr.toml found; built-in defaults apply" {
        return Err("the no-config line changed".to_string());
    }
    Ok(())
}

#[test]
fn a_malformed_default_manifest_fails_without_any_ripr_toml() -> Result<(), String> {
    let root = workspace("malformed-default")?;
    write(
        &root,
        ".ripr/suppressions.toml",
        &VALID_MANIFEST.replace("[[suppressions]]", "[[suppression]]"),
    )?;
    let result = validate_config(&root);
    let _ = fs::remove_dir_all(&root);
    let error = result
        .err()
        .ok_or("a malformed default manifest must fail")?;
    if !(error.starts_with(".ripr/suppressions.toml validation failed:\n")
        && error.contains(".ripr/suppressions.toml:2 "))
    {
        return Err(format!("unexpected error: {error}"));
    }
    Ok(())
}

#[test]
fn a_malformed_configured_manifest_fails_with_its_own_path() -> Result<(), String> {
    let root = workspace("malformed-configured")?;
    write(
        &root,
        CONFIG_FILE_NAME,
        "[suppressions]\npath = \"policy/s.toml\"\n",
    )?;
    write(
        &root,
        "policy/s.toml",
        &format!("{VALID_MANIFEST}owner = \"again\"\n"),
    )?;
    // A valid default manifest must not stand in for the configured one.
    write(&root, ".ripr/suppressions.toml", VALID_MANIFEST)?;
    let result = validate_config(&root);
    let _ = fs::remove_dir_all(&root);
    let error = result
        .err()
        .ok_or("a malformed configured manifest must fail")?;
    if !(error.starts_with("policy/s.toml validation failed:\n")
        && error.contains("policy/s.toml:7 duplicate `[[suppressions]]` field `owner`")
        && error.ends_with("\nfix the lines above, then re-run `ripr config validate`"))
    {
        return Err(format!("unexpected error: {error}"));
    }
    Ok(())
}

#[test]
fn a_valid_manifest_is_named_with_its_entry_count() -> Result<(), String> {
    let root = workspace("valid")?;
    write(
        &root,
        CONFIG_FILE_NAME,
        "[suppressions]\npath = \"policy/s.toml\"\n",
    )?;
    write(
        &root,
        "policy/s.toml",
        &format!(
            "{VALID_MANIFEST}[[suppressions]]\nkind = \"exposure_gap\"\nfinding_id = \"probe:x\"\nreason = \"r\"\nowner = \"o\"\n"
        ),
    )?;
    write(&root, "src/lib.rs", "")?;
    let configured = validate_config(&root);
    fs::remove_file(root.join(CONFIG_FILE_NAME)).map_err(|error| error.to_string())?;
    write(&root, ".ripr/suppressions.toml", VALID_MANIFEST)?;
    let default = validate_config(&root);
    let _ = fs::remove_dir_all(&root);
    let configured = configured?;
    if configured.lines().nth(1) != Some("✓ suppressions valid (policy/s.toml, 2 entries)") {
        return Err(format!("unexpected output: {configured}"));
    }
    let default = default?;
    if default
        != "✓ no ripr.toml found; built-in defaults apply\n✓ suppressions valid (.ripr/suppressions.toml, 1 entry)"
    {
        return Err(format!("unexpected output: {default}"));
    }
    Ok(())
}

#[test]
fn a_directory_at_the_manifest_path_fails_instead_of_passing() -> Result<(), String> {
    let root = workspace("directory")?;
    fs::create_dir_all(root.join(".ripr/suppressions.toml")).map_err(|error| error.to_string())?;
    let result = validate_config(&root);
    let _ = fs::remove_dir_all(&root);
    let error = result.err().ok_or("a directory is not a manifest")?;
    if !error.starts_with(".ripr/suppressions.toml validation failed:\nfailed to read ") {
        return Err(format!("unexpected error: {error}"));
    }
    Ok(())
}

// Windows refuses a control byte in a file name (os error 123), so the
// fixture file can exist only on Unix.
#[cfg(unix)]
#[test]
fn a_control_byte_in_the_manifest_path_is_escaped_on_success() -> Result<(), String> {
    let root = workspace("escape")?;
    write(
        &root,
        CONFIG_FILE_NAME,
        // TOML spells the control byte as an escape; the loaded path holds ESC.
        "[suppressions]\npath = \"s\\u001b[31m.toml\"\n",
    )?;
    write(&root, "s\u{1b}[31m.toml", VALID_MANIFEST)?;
    let result = validate_config(&root);
    let _ = fs::remove_dir_all(&root);
    let output = result?;
    // The path keeps its place on the line, with ESC spelled as `\u{1b}`.
    if output.contains('\u{1b}') || !output.contains("suppressions valid (s\\u{1b}[31m.toml, ") {
        return Err(format!("raw control byte or unescaped path: {output:?}"));
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_dangling_default_manifest_link_fails() -> Result<(), String> {
    let root = workspace("dangling-default")?;
    std::os::unix::fs::symlink(
        root.join(".ripr/missing.toml"),
        root.join(".ripr/suppressions.toml"),
    )
    .map_err(|error| error.to_string())?;
    let result = validate_config(&root);
    let _ = fs::remove_dir_all(&root);
    match result {
        Err(error) if error.contains("`.ripr/suppressions.toml` is a link that names no file") => {
            Ok(())
        }
        other => Err(format!("dangling default link was not refused: {other:?}")),
    }
}

#[cfg(unix)]
#[test]
fn a_dangling_configured_manifest_link_fails() -> Result<(), String> {
    let root = workspace("dangling-configured")?;
    write(
        &root,
        CONFIG_FILE_NAME,
        "[suppressions]\npath = \"policy/s.toml\"\n",
    )?;
    fs::create_dir_all(root.join("policy")).map_err(|error| error.to_string())?;
    std::os::unix::fs::symlink(root.join("policy/missing.toml"), root.join("policy/s.toml"))
        .map_err(|error| error.to_string())?;
    let result = validate_config(&root);
    let _ = fs::remove_dir_all(&root);
    match result {
        Err(error) if error.contains("`policy/s.toml` is a link that names no file") => Ok(()),
        other => Err(format!(
            "dangling configured link was not refused: {other:?}"
        )),
    }
}

#[cfg(unix)]
#[test]
fn a_manifest_link_to_a_real_file_is_read() -> Result<(), String> {
    let root = workspace("linked-default")?;
    write(&root, ".ripr/real.toml", VALID_MANIFEST)?;
    std::os::unix::fs::symlink(
        root.join(".ripr/real.toml"),
        root.join(".ripr/suppressions.toml"),
    )
    .map_err(|error| error.to_string())?;
    let result = validate_config(&root);
    let _ = fs::remove_dir_all(&root);
    let output = result?;
    if !output.contains("✓ suppressions valid (.ripr/suppressions.toml, 1 entry)") {
        return Err(format!("linked manifest was not read: {output:?}"));
    }
    Ok(())
}

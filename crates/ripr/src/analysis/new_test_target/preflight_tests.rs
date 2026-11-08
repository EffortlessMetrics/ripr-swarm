use super::{NewTestKind, ProposedTargetPreflight, preflight_new_test_target};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct Root(PathBuf);

impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn root() -> Result<Root, String> {
    let path = std::env::temp_dir().join(format!(
        "ripr-preflight-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(path.join("src")).map_err(|error| error.to_string())?;
    fs::write(path.join("Cargo.toml"), "[package]\nname = \"p\"\n")
        .map_err(|error| error.to_string())?;
    fs::write(path.join("src/lib.rs"), "pub fn f() {}\n").map_err(|error| error.to_string())?;
    Ok(Root(path))
}

fn refusal(root: &Path, kind: NewTestKind, file: &str) -> String {
    match preflight_new_test_target(root, kind, file) {
        ProposedTargetPreflight::Refused(reason) => reason,
        other => format!("unexpectedly accepted: {other:?}"),
    }
}

#[test]
fn a_plain_integration_path_with_no_file_has_no_test_yet() -> Result<(), String> {
    let root = root()?;
    assert_eq!(
        preflight_new_test_target(&root.0, NewTestKind::Integration, "tests/new.rs"),
        ProposedTargetPreflight::NoTestYet {
            package: ".".to_string()
        }
    );
    assert!(
        !root.0.join("tests").exists(),
        "the preflight writes nothing"
    );
    Ok(())
}

#[test]
fn a_member_package_is_named_for_its_own_tests_directory() -> Result<(), String> {
    let root = root()?;
    fs::create_dir_all(root.0.join("crates/m/src")).map_err(|error| error.to_string())?;
    fs::write(
        root.0.join("crates/m/Cargo.toml"),
        "[package]\nname = \"m\"\n",
    )
    .map_err(|error| error.to_string())?;
    fs::write(root.0.join("crates/m/src/lib.rs"), "pub fn g() {}\n")
        .map_err(|error| error.to_string())?;
    assert_eq!(
        preflight_new_test_target(&root.0, NewTestKind::Integration, "crates/m/tests/new.rs"),
        ProposedTargetPreflight::NoTestYet {
            package: "crates/m".to_string()
        }
    );
    assert_eq!(
        preflight_new_test_target(&root.0, NewTestKind::InlineUnit, "crates/m/src/lib.rs"),
        ProposedTargetPreflight::NoTestYet {
            package: "crates/m".to_string()
        }
    );
    Ok(())
}

#[test]
fn paths_that_escape_or_are_not_plain_are_refused() -> Result<(), String> {
    let root = root()?;
    for file in [
        "../tests/new.rs",
        "/tmp/tests/new.rs",
        "tests/../../x.rs",
        "",
    ] {
        let reason = refusal(&root.0, NewTestKind::Integration, file);
        assert!(reason.contains("not a plain path"), "{file}: {reason}");
    }
    let reason = refusal(&root.0, NewTestKind::Integration, "src/new.rs");
    assert!(reason.contains("`tests/<name>.rs`"), "{reason}");
    Ok(())
}

#[test]
fn a_stale_premise_is_refused_by_the_shared_rule() -> Result<(), String> {
    let root = root()?;
    fs::create_dir_all(root.0.join("tests")).map_err(|error| error.to_string())?;
    fs::write(root.0.join("tests/new.rs"), "").map_err(|error| error.to_string())?;
    let reason = refusal(&root.0, NewTestKind::Integration, "tests/new.rs");
    assert!(
        reason.contains("proposal is stale") && reason.contains("already exists"),
        "{reason}"
    );
    let reason = refusal(&root.0, NewTestKind::InlineUnit, "src/gone.rs");
    assert!(
        reason.contains("proposal is stale") && reason.contains("does not exist"),
        "{reason}"
    );
    Ok(())
}

#[test]
fn a_target_with_no_owning_package_is_refused() -> Result<(), String> {
    let root = root()?;
    fs::remove_file(root.0.join("Cargo.toml")).map_err(|error| error.to_string())?;
    let reason = refusal(&root.0, NewTestKind::Integration, "tests/new.rs");
    assert!(reason.contains("no Cargo package owns"), "{reason}");
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_symlinked_parent_directory_is_refused() -> Result<(), String> {
    let root = root()?;
    let outside = std::env::temp_dir().join(format!("ripr-preflight-out-{}", std::process::id()));
    fs::create_dir_all(&outside).map_err(|error| error.to_string())?;
    std::os::unix::fs::symlink(&outside, root.0.join("tests"))
        .map_err(|error| error.to_string())?;
    let reason = refusal(&root.0, NewTestKind::Integration, "tests/new.rs");
    let _ = fs::remove_dir_all(&outside);
    assert!(reason.contains("symlinked directory"), "{reason}");
    Ok(())
}

#[test]
fn a_virtual_workspace_root_does_not_own_tests() -> Result<(), String> {
    let root = root()?;
    fs::write(
        root.0.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/x\"]\n",
    )
    .map_err(|error| error.to_string())?;
    let reason = refusal(&root.0, NewTestKind::Integration, "crates/x/tests/new.rs");
    assert!(reason.contains("no Cargo package owns"), "{reason}");
    Ok(())
}

#[test]
fn an_empty_test_file_name_is_refused() -> Result<(), String> {
    let root = root()?;
    let reason = refusal(&root.0, NewTestKind::Integration, "tests/.rs");
    assert!(reason.contains("`tests/<name>.rs`"), "{reason}");
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_symlinked_production_file_is_refused_for_an_inline_module() -> Result<(), String> {
    let root = root()?;
    fs::write(root.0.join("src/real.rs"), "pub fn r() {}\n").map_err(|error| error.to_string())?;
    std::os::unix::fs::symlink(root.0.join("src/real.rs"), root.0.join("src/linked.rs"))
        .map_err(|error| error.to_string())?;
    let reason = refusal(&root.0, NewTestKind::InlineUnit, "src/linked.rs");
    assert!(reason.contains("not a regular file"), "{reason}");
    Ok(())
}

#[test]
fn a_broken_member_manifest_refuses_instead_of_falling_to_the_parent() -> Result<(), String> {
    let root = root()?;
    fs::create_dir_all(root.0.join("crates/x")).map_err(|error| error.to_string())?;
    fs::write(root.0.join("crates/x/Cargo.toml"), "[package\nname =")
        .map_err(|error| error.to_string())?;
    let reason = refusal(&root.0, NewTestKind::Integration, "crates/x/tests/new.rs");
    assert!(
        reason.contains("`crates/x/Cargo.toml`") && reason.contains("fix crates/x/Cargo.toml"),
        "{reason}"
    );
    Ok(())
}

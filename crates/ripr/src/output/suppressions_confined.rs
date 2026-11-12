//! Repository-confined loader for the implicit suppression policy.
//!
//! The suppression parser and explicit `--suppression-policy` loader remain in
//! `suppressions.rs`. This wrapper owns the narrower trust boundary for the
//! repository-controlled `suppressions.path`: resolve every path component,
//! prove the final regular file remains inside the analyzed repository, and
//! only then read it.

use std::ffi::OsString;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

pub use super::suppressions_core::*;

const CONFINEMENT_REFUSAL: &str =
    "`suppressions.path` must resolve to a regular file inside the repository; replace the link or choose an in-repository regular file";

/// Loads the repository-controlled implicit suppression policy after proving
/// that its resolved target remains inside `root`.
///
/// A missing ordinary path still means "no accepted debt". Existing paths
/// that cannot be resolved safely, escape through a link/reparse chain, or do
/// not name a regular file fail closed before their contents are read.
pub fn load_suppressions_for_root_at(
    root: &Path,
    relative_path: &Path,
) -> Result<Vec<SuppressionEntry>, Vec<String>> {
    let Some(path) = confined_repository_file(root, relative_path)? else {
        return Ok(Vec::new());
    };
    let text = std::fs::read_to_string(path).map_err(|err| {
        vec![format!(
            "failed to read suppression policy `{}`: {err}",
            relative_path.display()
        )]
    })?;
    let (entries, violations) = parse_suppressions_manifest(&text);
    if violations.is_empty() {
        Ok(entries)
    } else {
        let display = relative_path.display().to_string();
        Err(violations
            .into_iter()
            .map(|violation| violation.replacen(SUPPRESSIONS_PATH, &display, 1))
            .collect())
    }
}

fn confined_repository_file(
    root: &Path,
    relative_path: &Path,
) -> Result<Option<PathBuf>, Vec<String>> {
    let canonical_root = std::fs::canonicalize(root).map_err(|_| confinement_refusal())?;
    if !canonical_root.is_dir() {
        return Err(confinement_refusal());
    }

    let mut components = Vec::<OsString>::new();
    for component in relative_path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(value) => components.push(value.to_os_string()),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(confinement_refusal());
            }
        }
    }
    if components.is_empty() {
        return Err(confinement_refusal());
    }

    let mut candidate = canonical_root.clone();
    for (index, component) in components.iter().enumerate() {
        candidate.push(component);
        match std::fs::symlink_metadata(&candidate) {
            Ok(_) => {}
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(confinement_refusal()),
        }

        let resolved = std::fs::canonicalize(&candidate).map_err(|_| confinement_refusal())?;
        if !resolved.starts_with(&canonical_root) {
            return Err(confinement_refusal());
        }
        let metadata = std::fs::metadata(&resolved).map_err(|_| confinement_refusal())?;
        let is_final = index + 1 == components.len();
        if is_final {
            return metadata
                .is_file()
                .then_some(resolved)
                .map(Some)
                .ok_or_else(confinement_refusal);
        }
        if !metadata.is_dir() {
            return Err(confinement_refusal());
        }
        candidate = resolved;
    }

    Err(confinement_refusal())
}

fn confinement_refusal() -> Vec<String> {
    vec![CONFINEMENT_REFUSAL.to_string()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    const VALID_POLICY: &str = "schema_version = 1\n\n[[suppressions]]\nkind = \"exposure_gap\"\nfinding_id = \"probe:inside\"\nreason = \"reviewed\"\nowner = \"team\"\n";
    const OUTSIDE_SENTINEL: &str = "RIPR_OUTSIDE_SUPPRESSION_SENTINEL_14310";

    fn temp_root(name: &str) -> Result<PathBuf, String> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|err| err.to_string())?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ripr-suppression-confinement-{name}-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).map_err(|err| err.to_string())?;
        Ok(root)
    }

    fn write_policy(root: &Path, relative: &Path, text: &str) -> Result<(), String> {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        std::fs::write(path, text).map_err(|err| err.to_string())
    }

    #[test]
    fn direct_in_repository_policy_loads_and_missing_policy_stays_empty() -> Result<(), String> {
        let root = temp_root("direct")?;
        let relative = Path::new(".ripr/suppressions.toml");
        write_policy(&root, relative, VALID_POLICY)?;
        assert_eq!(
            load_suppressions_for_root_at(&root, relative)
                .map_err(|errors| errors.join("\n"))?
                .len(),
            1
        );
        assert!(
            load_suppressions_for_root_at(&root, Path::new(".ripr/missing.toml"))
                .map_err(|errors| errors.join("\n"))?
                .is_empty()
        );
        std::fs::remove_dir_all(root).map_err(|err| err.to_string())?;
        Ok(())
    }

    #[test]
    fn directory_and_parent_traversal_fail_with_the_fixed_refusal() -> Result<(), String> {
        let root = temp_root("invalid-kind")?;
        std::fs::create_dir_all(root.join(".ripr/policy.toml"))
            .map_err(|err| err.to_string())?;
        for relative in [
            Path::new(".ripr/policy.toml"),
            Path::new("../outside.toml"),
        ] {
            let errors = load_suppressions_for_root_at(&root, relative)
                .err()
                .ok_or("unsafe path must fail closed")?;
            assert_eq!(errors, vec![CONFINEMENT_REFUSAL.to_string()]);
        }
        std::fs::remove_dir_all(root).map_err(|err| err.to_string())?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn in_repository_symlink_to_regular_file_remains_supported() -> Result<(), String> {
        use std::os::unix::fs::symlink;

        let root = temp_root("inside-link")?;
        write_policy(&root, Path::new("policies/accepted.toml"), VALID_POLICY)?;
        std::fs::create_dir_all(root.join(".ripr")).map_err(|err| err.to_string())?;
        symlink(
            Path::new("../policies/accepted.toml"),
            root.join(".ripr/suppressions.toml"),
        )
        .map_err(|err| err.to_string())?;
        assert_eq!(
            load_suppressions_for_root_at(&root, Path::new(".ripr/suppressions.toml"))
                .map_err(|errors| errors.join("\n"))?
                .len(),
            1
        );
        std::fs::remove_dir_all(root).map_err(|err| err.to_string())?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn outside_symlink_is_refused_before_outside_bytes_are_read() -> Result<(), String> {
        use std::os::unix::fs::symlink;

        let sandbox = temp_root("outside-link")?;
        let root = sandbox.join("repo");
        std::fs::create_dir_all(root.join(".ripr")).map_err(|err| err.to_string())?;
        let outside = sandbox.join("outside.toml");
        std::fs::write(&outside, format!("owner = {OUTSIDE_SENTINEL}\n"))
            .map_err(|err| err.to_string())?;
        symlink(&outside, root.join(".ripr/suppressions.toml"))
            .map_err(|err| err.to_string())?;

        let error = load_suppressions_for_root_at(&root, Path::new(".ripr/suppressions.toml"))
            .err()
            .ok_or("outside link must fail closed")?
            .join("\n");
        assert_eq!(error, CONFINEMENT_REFUSAL);
        assert!(!error.contains(OUTSIDE_SENTINEL), "{error}");
        assert!(!error.contains(&outside.display().to_string()), "{error}");
        std::fs::remove_dir_all(sandbox).map_err(|err| err.to_string())?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn dangling_looping_and_nested_outside_links_fail_closed() -> Result<(), String> {
        use std::os::unix::fs::symlink;

        let sandbox = temp_root("bad-links")?;
        let root = sandbox.join("repo");
        std::fs::create_dir_all(root.join(".ripr")).map_err(|err| err.to_string())?;

        symlink(
            root.join("missing.toml"),
            root.join(".ripr/dangling.toml"),
        )
        .map_err(|err| err.to_string())?;
        symlink("loop-b.toml", root.join(".ripr/loop-a.toml"))
            .map_err(|err| err.to_string())?;
        symlink("loop-a.toml", root.join(".ripr/loop-b.toml"))
            .map_err(|err| err.to_string())?;

        let outside_dir = sandbox.join("outside-dir");
        std::fs::create_dir_all(&outside_dir).map_err(|err| err.to_string())?;
        std::fs::write(
            outside_dir.join("suppressions.toml"),
            format!("owner = {OUTSIDE_SENTINEL}\n"),
        )
        .map_err(|err| err.to_string())?;
        symlink(&outside_dir, root.join("linked-policy-dir"))
            .map_err(|err| err.to_string())?;

        for relative in [
            Path::new(".ripr/dangling.toml"),
            Path::new(".ripr/loop-a.toml"),
            Path::new("linked-policy-dir/suppressions.toml"),
        ] {
            let error = load_suppressions_for_root_at(&root, relative)
                .err()
                .ok_or("unsafe link must fail closed")?
                .join("\n");
            assert_eq!(error, CONFINEMENT_REFUSAL);
            assert!(!error.contains(OUTSIDE_SENTINEL), "{error}");
        }

        std::fs::remove_dir_all(sandbox).map_err(|err| err.to_string())?;
        Ok(())
    }
}

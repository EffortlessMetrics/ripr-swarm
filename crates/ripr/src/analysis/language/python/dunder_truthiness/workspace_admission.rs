//! Physical isolation admission for this small truthiness capability only.
//! This does not replace the general Python walker or infer namespace effects.

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_ENTRIES: usize = 512;
const MAX_DEPTH: usize = 16;
const IMPORT_ARTIFACTS: [&str; 9] = ["pyc", "pyo", "pyd", "so", "dll", "zip", "egg", "whl", "pth"];

/// Exactly two nonempty Python sources in the current bounded physical view.
/// Generated/excluded paths are inspected too. Empty package initializers are
/// inert; any omitted/error/opaque state withholds this capability's credit.
pub(super) fn closed_sources(
    root: &Path,
    sources: &HashMap<PathBuf, String>,
    incomplete: bool,
) -> Option<[PathBuf; 2]> {
    if incomplete {
        return None;
    }
    let root_type = fs::symlink_metadata(root).ok()?.file_type();
    if !root_type.is_dir() || root_type.is_symlink() {
        return None;
    }
    let mut pending = vec![(PathBuf::new(), 0usize)];
    let mut seen = 1usize;
    let mut python = Vec::new();
    while let Some((relative, depth)) = pending.pop() {
        if depth > MAX_DEPTH {
            return None;
        }
        for entry in fs::read_dir(root.join(&relative)).ok()? {
            let entry = entry.ok()?;
            seen += 1;
            if seen > MAX_ENTRIES {
                return None;
            }
            let name = entry.file_name();
            if !name.to_str()?.is_ascii() {
                return None;
            }
            let path = relative.join(&name);
            let kind = entry.file_type().ok()?;
            if kind.is_symlink() {
                return None;
            }
            if kind.is_dir() {
                pending.push((path, depth + 1));
                continue;
            }
            if !kind.is_file() {
                return None;
            }
            let extension = path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            let extension = extension.to_ascii_lowercase();
            if IMPORT_ARTIFACTS.contains(&extension.as_str()) {
                return None;
            }
            if extension != "py" {
                continue;
            }
            let metadata = fs::symlink_metadata(entry.path()).ok()?;
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return None;
            }
            if name.to_str()?.eq_ignore_ascii_case("__init__.py") && metadata.len() == 0 {
                continue;
            }
            python.push(path);
            if python.len() > 2 {
                return None;
            }
        }
    }
    python.sort();
    let [first, second] = python.as_slice() else {
        return None;
    };
    for path in [first, second] {
        let expected = sources.get(path)?;
        if !expected.is_ascii() {
            return None;
        }
        let absolute = root.join(path);
        let metadata = fs::symlink_metadata(&absolute).ok()?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return None;
        }
        let limit = u64::try_from(expected.len().checked_add(1)?).ok()?;
        let mut actual = Vec::new();
        fs::File::open(absolute)
            .ok()?
            .take(limit)
            .read_to_end(&mut actual)
            .ok()?;
        if actual != expected.as_bytes() {
            return None;
        }
    }
    Some([first.clone(), second.clone()])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    type Check = Result<(), Box<dyn std::error::Error>>;
    static NEXT: AtomicUsize = AtomicUsize::new(0);

    struct Workspace(PathBuf, HashMap<PathBuf, String>);

    impl Workspace {
        fn new() -> Result<Self, Box<dyn std::error::Error>> {
            let root = std::env::temp_dir().join(format!(
                "ripr-dunder-audit-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(std::env::temp_dir())?;
            fs::create_dir(&root)?;
            let sources = HashMap::from([
                (
                    PathBuf::from("src/money.py"),
                    "class Money: pass\n".to_string(),
                ),
                (
                    PathBuf::from("tests/test_money.py"),
                    "def test_money(): pass\n".to_string(),
                ),
            ]);
            for (path, source) in &sources {
                let absolute = root.join(path);
                fs::create_dir_all(absolute.parent().ok_or("source must have parent")?)?;
                fs::write(absolute, source)?;
            }
            Ok(Self(root, sources))
        }

        fn admitted(&self) -> bool {
            closed_sources(&self.0, &self.1, false).is_some()
        }
    }

    impl Drop for Workspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn physical_pair_and_empty_package_initializers_are_admitted() -> Check {
        let workspace = Workspace::new()?;
        fs::write(workspace.0.join("src/__init__.py"), "")?;
        assert!(workspace.admitted());
        assert!(closed_sources(&workspace.0, &workspace.1, true).is_none());
        Ok(())
    }

    #[test]
    fn every_third_python_source_is_refused_without_discovery_filters() -> Check {
        for name in [
            "tests/test_patch.py",
            "tests/test_patch_pb2.py",
            "vendor/patch.py",
            ".hidden/patch.PY",
        ] {
            let workspace = Workspace::new()?;
            let extra = workspace.0.join(name);
            fs::create_dir_all(extra.parent().ok_or("extra source needs parent")?)?;
            fs::write(extra, "# source outside the admitted pair\n")?;
            assert!(!workspace.admitted(), "{name}");
        }
        Ok(())
    }

    #[test]
    fn local_import_artifacts_and_changed_source_bytes_are_refused() -> Check {
        for extension in IMPORT_ARTIFACTS {
            let workspace = Workspace::new()?;
            fs::write(
                workspace.0.join(format!("src/__init__.{extension}")),
                b"opaque",
            )?;
            assert!(!workspace.admitted(), "{extension}");
        }
        let workspace = Workspace::new()?;
        fs::write(workspace.0.join("src/money.py"), "class Other: pass\n")?;
        assert!(!workspace.admitted());
        assert!(closed_sources(&workspace.0.join("missing"), &workspace.1, false).is_none());
        Ok(())
    }

    #[test]
    fn entry_and_depth_limits_refuse_unknown_frontiers() -> Check {
        let workspace = Workspace::new()?;
        for number in 0..MAX_ENTRIES {
            fs::write(workspace.0.join(format!("item-{number}")), "")?;
        }
        assert!(!workspace.admitted());
        let workspace = Workspace::new()?;
        let deep = (0..=MAX_DEPTH).fold(workspace.0.clone(), |path, _| path.join("deep"));
        fs::create_dir_all(deep)?;
        assert!(!workspace.admitted());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_roots_and_entries_are_refused() -> Check {
        let workspace = Workspace::new()?;
        std::os::unix::fs::symlink("src/money.py", workspace.0.join("alias.py"))?;
        assert!(!workspace.admitted());
        let alias = workspace.0.join("root-alias");
        std::os::unix::fs::symlink(&workspace.0, &alias)?;
        assert!(closed_sources(&alias, &workspace.1, false).is_none());
        Ok(())
    }
}

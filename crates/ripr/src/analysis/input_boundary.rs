//! Optional run-scoped filesystem boundary for the native diff output set.
//! Single-format analysis retains its existing filesystem behavior.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};

static ACTIVE: AtomicUsize = AtomicUsize::new(0);
static GUARDS: Mutex<Vec<Weak<State>>> = Mutex::new(Vec::new());

struct State {
    root: PathBuf,
    requested_root: PathBuf,
    escaped: AtomicBool,
}

pub(crate) struct Boundary(Arc<State>);

impl Boundary {
    pub(crate) fn new(root: &Path) -> Result<Self, String> {
        let state = Arc::new(State {
            root: root.canonicalize().map_err(|e| e.to_string())?,
            requested_root: std::path::absolute(root).map_err(|e| e.to_string())?,
            escaped: AtomicBool::new(false),
        });
        let mut guards = GUARDS
            .lock()
            .map_err(|_poisoned| "output-set boundary lock failed")?;
        guards.retain(|guard| guard.strong_count() > 0);
        guards.push(Arc::downgrade(&state));
        ACTIVE.fetch_add(1, Ordering::SeqCst);
        Ok(Self(state))
    }

    pub(crate) fn verify(&self) -> Result<(), String> {
        if self.0.escaped.load(Ordering::SeqCst) {
            Err("--output-dir cannot bind module reads outside the root or under Git metadata/Cargo target directories; use standalone formats for this input".into())
        } else {
            Ok(())
        }
    }
}

impl Drop for Boundary {
    fn drop(&mut self) {
        ACTIVE.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Called by the existing module-read owner, including Rayon workers and
/// absent-file probes. No parallel syntax scan or semantic decision is added.
pub(crate) fn observe_module_read(root: &Path, file: &Path, outside: bool) {
    if ACTIVE.load(Ordering::SeqCst) == 0 {
        return;
    }
    let mut guards = match GUARDS.lock() {
        Ok(guards) => guards,
        Err(poisoned) => {
            for state in poisoned.into_inner().iter().filter_map(Weak::upgrade) {
                state.escaped.store(true, Ordering::SeqCst);
            }
            return;
        }
    };
    guards.retain(|guard| guard.strong_count() > 0);
    let requested_root = std::path::absolute(root);
    let canonical_root = root.canonicalize();
    let observed_file = resolved_probe_path(file);
    for state in guards.iter().filter_map(Weak::upgrade) {
        let matches = requested_root
            .as_ref()
            .is_ok_and(|path| path == &state.requested_root || path == &state.root)
            || canonical_root
                .as_ref()
                .is_ok_and(|path| path == &state.root);
        // An unavailable current directory prevents attributing a relative
        // observation. Refuse active runs, without retaining process-wide state.
        if !matches && requested_root.is_ok() {
            continue;
        }
        let escaped = outside
            || canonical_root
                .as_ref()
                .map_or(true, |path| path != &state.root)
            || observed_file.as_ref().map_or(true, |path| {
                let Ok(relative) = path.strip_prefix(&state.root) else {
                    return true;
                };
                let mut parent = state.root.clone();
                relative.components().any(|component| {
                    let skipped = matches!(component, std::path::Component::ParentDir)
                        || component_is_named(component.as_os_str(), ".git", cfg!(windows))
                        || (component_is_named(component.as_os_str(), "target", cfg!(windows))
                            && parent.join("Cargo.toml").is_file());
                    parent.push(component);
                    skipped
                })
            });
        if escaped {
            state.escaped.store(true, Ordering::SeqCst);
        }
    }
}

/// Resolve aliases in the existing prefix while retaining absent module probes.
/// Other resolution failures remain failures; no unreadable prefix is admitted.
fn resolved_probe_path(file: &Path) -> std::io::Result<PathBuf> {
    let mut prefix = std::path::absolute(file)?;
    let mut absent = Vec::new();
    loop {
        match prefix.canonicalize() {
            Ok(mut resolved) => {
                for component in absent.iter().rev() {
                    resolved.push(component);
                }
                return Ok(resolved);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let Some(name) = prefix.file_name() else {
                    return Err(error);
                };
                absent.push(name.to_os_string());
                if !prefix.pop() {
                    return Err(error);
                }
            }
            Err(error) => return Err(error),
        }
    }
}

fn component_is_named(component: &std::ffi::OsStr, name: &str, windows: bool) -> bool {
    component.to_str().is_some_and(|value| {
        if windows {
            value.eq_ignore_ascii_case(name)
        } else {
            value == name
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture(PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn fixture(label: &str) -> Result<Fixture, String> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "ripr-fanout-{label}-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir(&path).map_err(|e| e.to_string())?;
        Ok(Fixture(path))
    }

    #[test]
    fn fanout_boundary_matches_windows_exclusion_aliases() {
        assert!(component_is_named(
            std::ffi::OsStr::new(".GIT"),
            ".git",
            true
        ));
        assert!(component_is_named(
            std::ffi::OsStr::new("TARGET"),
            "target",
            true
        ));
        assert!(!component_is_named(
            std::ffi::OsStr::new(".GIT"),
            ".git",
            false
        ));
        assert!(!component_is_named(
            std::ffi::OsStr::new("targeted"),
            "target",
            true
        ));
    }

    #[test]
    fn fanout_boundary_observes_worker_reads_and_isolates_roots() -> Result<(), String> {
        let first = fixture("first")?;
        let second = fixture("second")?;
        let boundary = Boundary::new(&first.0)?;
        let unrelated = Boundary::new(&second.0)?;
        std::thread::scope(|scope| {
            scope
                .spawn(|| observe_module_read(&first.0, &second.0.join("external.rs"), true))
                .join()
                .map_err(|_panic| "boundary worker failed".to_string())
        })?;
        assert!(boundary.verify().is_err());
        unrelated.verify()?;
        Ok(())
    }

    #[test]
    fn fanout_boundary_rejects_only_exclusions_below_selected_root() -> Result<(), String> {
        for excluded in [".git/private.rs", "target/generated.rs"] {
            let fixture = fixture("exclusions")?;
            std::fs::write(fixture.0.join("Cargo.toml"), "[workspace]")
                .map_err(|e| e.to_string())?;
            let root = fixture.0.join("target/input");
            std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
            std::fs::write(root.join("Cargo.toml"), "[workspace]").map_err(|e| e.to_string())?;
            let boundary = Boundary::new(&root)?;
            observe_module_read(&root, &root.join("lib.rs"), false);
            assert!(
                boundary.verify().is_ok(),
                "outer Cargo target must not taint selected root"
            );
            observe_module_read(&root, &root.join(excluded), false);
            assert!(
                boundary.verify().is_err(),
                "{excluded} must be outside custody"
            );
        }
        Ok(())
    }
    #[test]
    fn fanout_boundary_failure_is_scoped_to_active_root_lifecycle() -> Result<(), String> {
        let broken = fixture("broken-lifecycle")?;
        let stable = fixture("stable-lifecycle")?;
        let boundary = Boundary::new(&broken.0)?;
        let concurrent = Boundary::new(&stable.0)?;
        std::fs::remove_dir(&broken.0).map_err(|e| e.to_string())?;
        observe_module_read(&broken.0, &broken.0.join("missing.rs"), false);
        assert!(
            boundary.verify().is_err(),
            "failed observation must refuse its run"
        );
        let concurrent_result = concurrent.verify();
        drop(boundary);
        drop(concurrent);
        let fresh = Boundary::new(&stable.0)?;
        fresh.verify()?;
        concurrent_result?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn fanout_boundary_resolves_root_alias_and_absent_probes() -> Result<(), String> {
        let physical = fixture("physical")?;
        let aliases = fixture("aliases")?;
        let alias = aliases.0.join("root");
        std::os::unix::fs::symlink(&physical.0, &alias).map_err(|e| e.to_string())?;
        std::fs::write(physical.0.join("lib.rs"), "").map_err(|e| e.to_string())?;
        let boundary = Boundary::new(&alias)?;
        observe_module_read(&alias, &alias.join("lib.rs"), false);
        observe_module_read(&alias, &alias.join("absent/module.rs"), false);
        boundary.verify()?;
        observe_module_read(&alias, &alias.join(".git/absent.rs"), false);
        assert!(
            boundary.verify().is_err(),
            "alias must retain excluded-directory refusal"
        );
        Ok(())
    }
}

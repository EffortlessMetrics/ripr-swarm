# Independent review experiment

```rust
//! Optional run-scoped filesystem boundary for the native diff output set.
//! Single-format analysis retains its existing filesystem behavior.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};

static ACTIVE: AtomicUsize = AtomicUsize::new(0);
static FAILED: AtomicBool = AtomicBool::new(false);
static GUARDS: Mutex<Vec<Weak<State>>> = Mutex::new(Vec::new());

struct State {
    root: PathBuf,
    escaped: AtomicBool,
}

pub(crate) struct Boundary(Arc<State>);

impl Boundary {
    pub(crate) fn new(root: &Path) -> Result<Self, String> {
        let state = Arc::new(State {
            root: root.canonicalize().map_err(|e| e.to_string())?,
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
        if FAILED.load(Ordering::SeqCst) || self.0.escaped.load(Ordering::SeqCst) {
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
    let canonical_root = match root.canonicalize() {
        Ok(root) => root,
        Err(_) => {
            FAILED.store(true, Ordering::SeqCst);
            return;
        }
    };
    let absolute_file = match std::path::absolute(file) {
        Ok(file) => file,
        Err(_) => {
            FAILED.store(true, Ordering::SeqCst);
            return;
        }
    };
    let excluded = match absolute_file.strip_prefix(&canonical_root) {
        Ok(relative) => {
            let mut parent = canonical_root.clone();
            relative.components().any(|component| {
                let skipped = component_is_named(component.as_os_str(), ".git", cfg!(windows))
                    || (component_is_named(component.as_os_str(), "target", cfg!(windows))
                        && parent.join("Cargo.toml").is_file());
                parent.push(component);
                skipped
            })
        }
        Err(_) => true,
    };
    if !outside && !excluded {
        return;
    }
    match GUARDS.lock() {
        Ok(guards) => {
            for state in guards.iter().filter_map(Weak::upgrade) {
                if state.root == canonical_root {
                    state.escaped.store(true, Ordering::SeqCst);
                }
            }
        }
        Err(_) => FAILED.store(true, Ordering::SeqCst),
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
}

```

//! Project Python interpreter selection for the printed verify command and
//! the doctor pytest probe (#13617).
//!
//! The verify command is `python -m pytest <node>` run from the selected
//! root. When the project carries a virtualenv (`.venv` or `venv`), that
//! interpreter is the one that has the project's dependencies, so both the
//! printed command and doctor use it. Detection is purely static: doctor and
//! check never execute code from the checkout to learn this.

use std::path::Path;

/// Virtualenv directories searched, in preference order.
const VENV_DIRS: [&str; 2] = [".venv", "venv"];

/// What the project virtualenv says about pytest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum VenvPytest {
    /// No usable project virtualenv; the PATH `python` is the interpreter.
    NoVenv,
    /// The venv interpreter has pytest installed (version when readable).
    Present(Option<String>),
    /// A venv interpreter exists but pytest is not installed in it.
    Missing,
}

/// Root-relative POSIX path of the project virtualenv interpreter, when one
/// exists. Windows keeps the PATH interpreter: the rooted verify command is
/// not rendered there.
pub(crate) fn project_venv_python(root: &Path) -> Option<&'static str> {
    if cfg!(windows) {
        return None;
    }
    [".venv/bin/python", "venv/bin/python"]
        .into_iter()
        .find(|rel| root.join(rel).exists())
}

/// Whether the project virtualenv has pytest installed, read from its
/// `site-packages` listing without running the interpreter.
pub(crate) fn project_venv_pytest(root: &Path) -> VenvPytest {
    let Some(python) = project_venv_python(root) else {
        return VenvPytest::NoVenv;
    };
    let venv = python.split('/').next().unwrap_or(VENV_DIRS[0]);
    let lib = root.join(venv).join("lib");
    let mut site_dirs: Vec<std::path::PathBuf> = std::fs::read_dir(&lib)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path().join("site-packages"))
        .filter(|path| path.is_dir())
        .collect();
    site_dirs.sort();
    for site in site_dirs {
        let Ok(entries) = std::fs::read_dir(&site) else {
            continue;
        };
        let mut installed = false;
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if let Some(version) = name
                .strip_prefix("pytest-")
                .and_then(|rest| rest.strip_suffix(".dist-info"))
            {
                return VenvPytest::Present(Some(version.to_string()));
            }
            installed |= name == "pytest" || name == "_pytest";
        }
        if installed {
            return VenvPytest::Present(None);
        }
    }
    VenvPytest::Missing
}

/// The confidence label for a rooted verify command. A venv that lacks
/// pytest cannot run the command, so the declared label is not kept.
pub(crate) fn rooted_verify_confidence<'a>(root: &Path, declared: &'a str) -> &'a str {
    match project_venv_pytest(root) {
        VenvPytest::Missing => "low",
        VenvPytest::NoVenv | VenvPytest::Present(_) => declared,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(venv: &str, site_entries: &[&str]) -> std::io::Result<tempfile_dir::Dir> {
        let dir = tempfile_dir::Dir::new()?;
        let site = dir.path().join(venv).join("lib/python3.13/site-packages");
        std::fs::create_dir_all(&site)?;
        std::fs::create_dir_all(dir.path().join(venv).join("bin"))?;
        std::fs::write(dir.path().join(venv).join("bin/python"), "")?;
        for entry in site_entries {
            std::fs::create_dir_all(site.join(entry))?;
        }
        Ok(dir)
    }

    #[cfg(not(windows))]
    #[test]
    fn venv_with_pytest_is_present_with_version() -> std::io::Result<()> {
        let dir = fixture(".venv", &["pytest-9.2.0.dist-info", "_pytest"])?;
        assert_eq!(project_venv_python(dir.path()), Some(".venv/bin/python"));
        assert_eq!(
            project_venv_pytest(dir.path()),
            VenvPytest::Present(Some("9.2.0".to_string()))
        );
        assert_eq!(rooted_verify_confidence(dir.path(), "high"), "high");
        Ok(())
    }

    #[cfg(not(windows))]
    #[test]
    fn venv_without_pytest_is_missing_and_not_high() -> std::io::Result<()> {
        let dir = fixture("venv", &["pip", "iniconfig"])?;
        assert_eq!(project_venv_python(dir.path()), Some("venv/bin/python"));
        assert_eq!(project_venv_pytest(dir.path()), VenvPytest::Missing);
        assert_eq!(rooted_verify_confidence(dir.path(), "high"), "low");
        Ok(())
    }

    #[test]
    fn no_venv_keeps_path_interpreter_and_declared_confidence() -> std::io::Result<()> {
        let dir = tempfile_dir::Dir::new()?;
        assert_eq!(project_venv_python(dir.path()), None);
        assert_eq!(project_venv_pytest(dir.path()), VenvPytest::NoVenv);
        assert_eq!(rooted_verify_confidence(dir.path(), "high"), "high");
        Ok(())
    }

    mod tempfile_dir {
        use std::path::{Path, PathBuf};
        pub(super) struct Dir(PathBuf);
        impl Dir {
            pub(super) fn new() -> std::io::Result<Self> {
                static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
                let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let path = std::env::temp_dir()
                    .join(format!("ripr-python-env-{}-{n}", std::process::id()));
                let _ = std::fs::remove_dir_all(&path);
                std::fs::create_dir_all(&path)?;
                Ok(Self(path))
            }
            pub(super) fn path(&self) -> &Path {
                &self.0
            }
        }
        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }
}

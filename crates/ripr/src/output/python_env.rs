//! Static facts about a project virtualenv (`.venv` or `venv`) for the doctor
//! pytest probe (#13617).
//!
//! Doctor always probes the PATH `python -m pytest` that the printed verify
//! command runs. These facts only explain a PATH failure, for example by
//! suggesting that the user activate a venv that has pytest. Choosing the venv
//! interpreter for the printed command itself is #13801. Detection is purely
//! static: doctor never executes code from the checkout to learn this.

use std::path::Path;

/// What the project virtualenv says about pytest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum VenvPytest {
    /// No usable project virtualenv; the PATH `python` is the interpreter.
    NoVenv,
    /// The venv interpreter has pytest installed (version when readable).
    Present(Option<String>),
    /// A venv interpreter exists but pytest is not installed in it.
    Missing,
    /// The venv exists but its layout cannot be read statically (system
    /// site-packages included, ambiguous or absent `lib/python3.X`).
    Unknown,
}

/// Root-relative POSIX path of the project virtualenv interpreter, when one
/// exists. Windows keeps the PATH interpreter.
pub(crate) fn project_venv_python(root: &Path) -> Option<&'static str> {
    if cfg!(windows) {
        return None;
    }
    [".venv/bin/python", "venv/bin/python"]
        .into_iter()
        .find(|rel| root.join(rel).exists())
}

/// `pytest-<digit>...dist-info` yields the version; `pytest-cov` and other
/// distributions do not match.
fn pytest_dist_info_version(name: &str) -> Option<&str> {
    let version = name.strip_prefix("pytest-")?.strip_suffix(".dist-info")?;
    version
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_digit())
        .then_some(version)
}

/// Whether the project virtualenv has pytest installed, read from its
/// `site-packages` listing and `pyvenv.cfg` without running the interpreter.
pub(crate) fn project_venv_pytest(root: &Path) -> VenvPytest {
    let Some(python) = project_venv_python(root) else {
        return VenvPytest::NoVenv;
    };
    let venv = root.join(python.split('/').next().unwrap_or(".venv"));
    let cfg = std::fs::read_to_string(venv.join("pyvenv.cfg")).unwrap_or_default();
    let mut cfg_version: Option<String> = None;
    for line in cfg.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        if key == "include-system-site-packages" && value.eq_ignore_ascii_case("true") {
            return VenvPytest::Unknown;
        }
        if key == "version" || key == "version_info" {
            let mut parts = value.split('.');
            if let (Some(major), Some(minor)) = (parts.next(), parts.next()) {
                cfg_version = Some(format!("python{major}.{minor}"));
            }
        }
    }
    let mut libs: Vec<std::path::PathBuf> = std::fs::read_dir(venv.join("lib"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("python3"))
        .map(|e| e.path())
        .collect();
    libs.sort();
    let lib = match (libs.len(), &cfg_version) {
        (1, _) => libs.pop(),
        (_, Some(want)) => libs.into_iter().find(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy() == want.as_str())
        }),
        _ => None,
    };
    let Some(lib) = lib else {
        return VenvPytest::Unknown;
    };
    let Ok(entries) = std::fs::read_dir(lib.join("site-packages")) else {
        return VenvPytest::Unknown;
    };
    let mut installed = false;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if let Some(version) = pytest_dist_info_version(name) {
            return VenvPytest::Present(Some(version.to_string()));
        }
        installed |= name == "pytest";
    }
    if installed {
        VenvPytest::Present(None)
    } else {
        VenvPytest::Missing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(windows))]
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
    fn write_cfg(dir: &tempfile_dir::Dir, venv: &str, text: &str) -> std::io::Result<()> {
        std::fs::write(dir.path().join(venv).join("pyvenv.cfg"), text)
    }

    #[cfg(not(windows))]
    #[test]
    fn venv_with_pytest_is_present_with_version() -> std::io::Result<()> {
        let dir = fixture(".venv", &["pytest-9.2.0.dist-info", "pytest"])?;
        assert_eq!(project_venv_python(dir.path()), Some(".venv/bin/python"));
        assert_eq!(
            project_venv_pytest(dir.path()),
            VenvPytest::Present(Some("9.2.0".to_string()))
        );
        Ok(())
    }

    #[cfg(not(windows))]
    #[test]
    fn venv_without_pytest_is_missing_and_pytest_cov_is_not_pytest() -> std::io::Result<()> {
        let dir = fixture("venv", &["pip", "pytest-cov-5.0.0.dist-info", "_pytest"])?;
        assert_eq!(project_venv_python(dir.path()), Some("venv/bin/python"));
        assert_eq!(project_venv_pytest(dir.path()), VenvPytest::Missing);
        Ok(())
    }

    #[cfg(not(windows))]
    #[test]
    fn system_site_packages_and_ambiguous_layout_are_unknown() -> std::io::Result<()> {
        let dir = fixture(".venv", &["pip"])?;
        write_cfg(&dir, ".venv", "include-system-site-packages = true\n")?;
        assert_eq!(project_venv_pytest(dir.path()), VenvPytest::Unknown);
        let dir = fixture(".venv", &["pip"])?;
        std::fs::create_dir_all(dir.path().join(".venv/lib/python3.12/site-packages"))?;
        assert_eq!(project_venv_pytest(dir.path()), VenvPytest::Unknown);
        write_cfg(&dir, ".venv", "version = 3.13.1\n")?;
        assert_eq!(project_venv_pytest(dir.path()), VenvPytest::Missing);
        Ok(())
    }

    #[test]
    fn no_venv_keeps_path_interpreter() -> std::io::Result<()> {
        let dir = tempfile_dir::Dir::new()?;
        assert_eq!(project_venv_python(dir.path()), None);
        assert_eq!(project_venv_pytest(dir.path()), VenvPytest::NoVenv);
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

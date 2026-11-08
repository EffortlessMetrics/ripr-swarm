//! Isolated launch context and owned fixture cleanup (#4510).

use std::fs;
use std::path::{Path, PathBuf};

#[cfg(unix)]
pub(crate) const CHILD_PATH: &str = "/usr/bin:/bin";
#[cfg(not(unix))]
pub(crate) const CHILD_PATH: &str = "";

pub(crate) const ROOT_TOKEN: &str = "<case-root>";

pub(crate) const CHILD_OUTPUT_BYTES: usize = 16 * 1024 * 1024;

const GIT_REDIRECT_ENV: [&str; 3] = ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE"];

pub(crate) fn removed_env() -> Vec<String> {
    let mut names: Vec<String> = std::env::vars()
        .map(|(name, _)| name)
        .filter(|name| name.starts_with("RIPR_"))
        .collect();
    names.sort();
    names.extend(GIT_REDIRECT_ENV.iter().map(|name| (*name).to_string()));
    names
}

pub(crate) fn environment_boundary() -> Vec<String> {
    vec![
        format!("child PATH is exactly `{CHILD_PATH}`; the binary is launched by absolute path"),
        "HOME, TMPDIR and XDG_{CACHE,CONFIG,DATA}_HOME point inside the owned case root"
            .to_string(),
        "NO_COLOR=1 and RUST_BACKTRACE=0".to_string(),
        "every RIPR_* variable present in the parent environment is removed from the child"
            .to_string(),
        "GIT_CEILING_DIRECTORIES is the case root's parent, and GIT_DIR, GIT_WORK_TREE and GIT_INDEX_FILE are removed, so no repository above the case root is discovered"
            .to_string(),
        "other inherited variables are not scrubbed".to_string(),
    ]
}

pub(crate) struct OwnedFixtureRoot {
    path: PathBuf,
}

impl OwnedFixtureRoot {
    pub(crate) fn new(parent: &Path, id: &str) -> Result<Self, String> {
        let path = parent.join(id);
        if path.exists() {
            return Err(format!(
                "owned fixture root {} already exists; refusing to reuse it",
                path.display()
            ));
        }
        fs::create_dir_all(&path)
            .map_err(|error| format!("failed to create {}: {error}", path.display()))?;
        Ok(Self { path })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn tokenize(&self, text: &str) -> String {
        text.replace(self.path.to_string_lossy().as_ref(), ROOT_TOKEN)
    }
}

pub(crate) fn child_env_for_root(root: &Path) -> Result<Vec<(String, String)>, String> {
    let home = root.join(".harness-home");
    let tmp = home.join("tmp");
    fs::create_dir_all(&tmp)
        .map_err(|error| format!("failed to create {}: {error}", tmp.display()))?;
    let ceiling = root
        .canonicalize()
        .ok()
        .and_then(|owned| {
            owned
                .parent()
                .map(|parent| parent.to_string_lossy().to_string())
        })
        .ok_or_else(|| format!("failed to resolve the parent of {}", root.display()))?;
    Ok(vec![
        ("PATH".to_string(), CHILD_PATH.to_string()),
        ("HOME".to_string(), home.to_string_lossy().to_string()),
        ("TMPDIR".to_string(), tmp.to_string_lossy().to_string()),
        (
            "XDG_CACHE_HOME".to_string(),
            home.join("cache").to_string_lossy().to_string(),
        ),
        (
            "XDG_CONFIG_HOME".to_string(),
            home.join("config").to_string_lossy().to_string(),
        ),
        (
            "XDG_DATA_HOME".to_string(),
            home.join("data").to_string_lossy().to_string(),
        ),
        ("GIT_CEILING_DIRECTORIES".to_string(), ceiling),
        ("NO_COLOR".to_string(), "1".to_string()),
        ("RUST_BACKTRACE".to_string(), "0".to_string()),
    ])
}

pub(crate) fn cleanup(root: &Path) -> String {
    cleanup_with(root, &|path| fs::remove_dir_all(path))
}

pub(crate) fn cleanup_with(root: &Path, remove: &dyn Fn(&Path) -> std::io::Result<()>) -> String {
    match remove(root) {
        Ok(()) if !root.exists() => "removed".to_string(),
        Ok(()) => "retained_after_remove".to_string(),
        Err(error) if !root.exists() && error.kind() == std::io::ErrorKind::NotFound => {
            "removed".to_string()
        }
        Err(error) => format!("retained ({error})"),
    }
}

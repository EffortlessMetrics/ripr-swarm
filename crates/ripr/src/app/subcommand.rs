//! Shared argv and repo-root helpers for `app/` subcommands ported from xtask.

use std::path::{Path, PathBuf};

/// Placeholder shown in empty-value errors for a flag value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NonEmptyPlaceholder {
    Path,
    Value,
}

impl NonEmptyPlaceholder {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Path => "<path>",
            Self::Value => "<value>",
        }
    }
}

/// Require a non-whitespace value immediately after `flag` in `args[index]`.
pub(crate) fn non_empty_arg<'a>(
    command: &str,
    args: &'a [String],
    index: usize,
    flag: &str,
    placeholder: NonEmptyPlaceholder,
) -> Result<&'a str, String> {
    let Some(value) = args.get(index) else {
        return Err(format!("missing value for {flag}; pass a value after it"));
    };
    if value.trim().is_empty() {
        return Err(format!(
            "{command} {flag} requires a non-empty value; pass `{flag} {}`",
            placeholder.as_str()
        ));
    }
    Ok(value)
}

/// Current working directory as the analyzed repository root (no extra hint).
pub(crate) fn invocation_repo_root() -> Result<PathBuf, String> {
    std::env::current_dir().map_err(|err| format!("failed to determine working directory: {err}"))
}

/// Current working directory with the standard "run ripr from an existing directory" hint.
pub(crate) fn invocation_repo_root_with_directory_hint() -> Result<PathBuf, String> {
    std::env::current_dir().map_err(|err| {
        format!("failed to determine working directory: {err}; run ripr from an existing directory")
    })
}

/// Resolve a relative selected root once against the invocation directory.
pub(crate) fn resolve_selected_root(root: &Path) -> Result<PathBuf, String> {
    if root.is_absolute() {
        return Ok(root.to_path_buf());
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(root))
        .map_err(|err| format!(
            "failed to determine working directory: {err}; run ripr from an existing directory or pass an absolute `--root`"
        ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;
    use std::sync::{Mutex, MutexGuard};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn env_lock() -> MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|poison| poison.into_inner())
    }

    #[test]
    fn non_empty_arg_rejects_missing_value() {
        let args = vec!["--root".to_string()];
        let err = non_empty_arg(
            "pr-evidence",
            &args,
            1,
            "--root",
            NonEmptyPlaceholder::Value,
        )
        .unwrap_err();
        assert_eq!(
            err,
            "missing value for --root; pass a value after it".to_string()
        );
    }

    #[test]
    fn non_empty_arg_rejects_whitespace_only() {
        let args = vec!["--comments".to_string(), "   ".to_string()];
        let err = non_empty_arg(
            "annotations",
            &args,
            1,
            "--comments",
            NonEmptyPlaceholder::Path,
        )
        .unwrap_err();
        assert_eq!(
            err,
            "annotations --comments requires a non-empty value; pass `--comments <path>`"
                .to_string()
        );
    }

    #[test]
    fn non_empty_arg_accepts_trimmed_content() {
        let args = vec!["--head".to_string(), "  HEAD  ".to_string()];
        let value = non_empty_arg(
            "impacted-evidence",
            &args,
            1,
            "--head",
            NonEmptyPlaceholder::Value,
        )
        .expect("non-empty");
        assert_eq!(value, "  HEAD  ");
    }

    #[test]
    fn resolve_selected_root_keeps_absolute() -> Result<(), String> {
        let absolute = if cfg!(windows) {
            PathBuf::from(r"C:\repo")
        } else {
            PathBuf::from("/tmp/repo")
        };
        assert_eq!(resolve_selected_root(&absolute)?, absolute);
        Ok(())
    }

    #[test]
    fn resolve_selected_root_anchors_relative_to_cwd() -> Result<(), String> {
        let _lock = env_lock();
        let temp = env::temp_dir().join(format!(
            "ripr-subcommand-root-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&temp).map_err(|err| format!("mkdir: {err}"))?;
        env::set_current_dir(&temp).map_err(|err| format!("chdir: {err}"))?;
        let resolved = resolve_selected_root(Path::new("nested"))?;
        assert_eq!(resolved, temp.join("nested"));
        Ok(())
    }

    #[test]
    fn invocation_repo_root_returns_cwd() -> Result<(), String> {
        let _lock = env_lock();
        let temp = env::temp_dir().join(format!(
            "ripr-subcommand-cwd-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&temp).map_err(|err| format!("mkdir: {err}"))?;
        env::set_current_dir(&temp).map_err(|err| format!("chdir: {err}"))?;
        assert_eq!(invocation_repo_root()?, temp);
        assert_eq!(invocation_repo_root_with_directory_hint()?, temp);
        Ok(())
    }
}

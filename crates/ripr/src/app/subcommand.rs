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

    fn with_temp_cwd<T>(run: impl FnOnce(&Path) -> Result<T, String>) -> Result<T, String> {
        let _cwd = crate::testing::cwd_lock::hold_cwd();
        let temp = env::temp_dir().join(format!(
            "ripr-subcommand-cwd-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&temp).map_err(|err| format!("mkdir: {err}"))?;
        let previous = env::current_dir().map_err(|err| format!("cwd: {err}"))?;
        let result = (|| {
            env::set_current_dir(&temp).map_err(|err| format!("chdir: {err}"))?;
            run(&temp)
        })();
        let _ = env::set_current_dir(&previous);
        result
    }

    #[test]
    fn non_empty_arg_rejects_missing_value() -> Result<(), String> {
        let args = vec!["--root".to_string()];
        match non_empty_arg(
            "pr-evidence",
            &args,
            1,
            "--root",
            NonEmptyPlaceholder::Value,
        ) {
            Err(err) if err == "missing value for --root; pass a value after it" => Ok(()),
            other => Err(format!("unexpected result: {other:?}")),
        }
    }

    #[test]
    fn non_empty_arg_rejects_whitespace_only() -> Result<(), String> {
        let args = vec!["--comments".to_string(), "   ".to_string()];
        match non_empty_arg(
            "annotations",
            &args,
            1,
            "--comments",
            NonEmptyPlaceholder::Path,
        ) {
            Err(err)
                if err
                    == "annotations --comments requires a non-empty value; pass `--comments <path>`" =>
            {
                Ok(())
            }
            other => Err(format!("unexpected result: {other:?}")),
        }
    }

    #[test]
    fn non_empty_arg_accepts_trimmed_content() -> Result<(), String> {
        let args = vec!["--head".to_string(), "  HEAD  ".to_string()];
        let value = non_empty_arg(
            "impacted-evidence",
            &args,
            1,
            "--head",
            NonEmptyPlaceholder::Value,
        )
        .map_err(|err| format!("expected non-empty value: {err}"))?;
        if value != "  HEAD  " {
            return Err(format!("unexpected value: {value:?}"));
        }
        Ok(())
    }

    #[test]
    fn resolve_selected_root_keeps_absolute() -> Result<(), String> {
        let absolute = env::temp_dir().join("ripr-subcommand-absolute-root");
        if resolve_selected_root(&absolute)? != absolute {
            return Err("absolute root must be returned unchanged".to_string());
        }
        Ok(())
    }

    #[test]
    fn resolve_selected_root_anchors_relative_to_cwd() -> Result<(), String> {
        with_temp_cwd(|temp| {
            let resolved = resolve_selected_root(Path::new("nested"))?;
            if resolved != temp.join("nested") {
                return Err(format!(
                    "expected {}, got {}",
                    temp.join("nested").display(),
                    resolved.display()
                ));
            }
            Ok(())
        })
    }

    #[test]
    fn invocation_repo_root_returns_cwd() -> Result<(), String> {
        with_temp_cwd(|temp| {
            if invocation_repo_root()? != *temp {
                return Err("invocation_repo_root must match temp cwd".to_string());
            }
            if invocation_repo_root_with_directory_hint()? != *temp {
                return Err(
                    "invocation_repo_root_with_directory_hint must match temp cwd".to_string(),
                );
            }
            Ok(())
        })
    }
}

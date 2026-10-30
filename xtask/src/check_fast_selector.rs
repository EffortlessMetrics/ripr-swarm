//! Identity of the committed diff used to select local fast checks.
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Selection {
    pub(crate) base: String,
    pub(crate) head: String,
    pub(crate) merge_base: String,
    pub(crate) files: Vec<String>,
}

impl Selection {
    pub(crate) fn identity_line(&self) -> String {
        format!(
            "Selector identity: origin/main={} HEAD={} merge-base={}",
            self.base, self.head, self.merge_base
        )
    }

    pub(crate) fn diagnostic(&self) -> String {
        format!(
            "{}\nChanged files: {}\nRef freshness: not checked (local refs; run `git fetch origin main` and retry if this basis is unexpected).\nScope: committed diff selects checks; gates inspect the live worktree.\n",
            self.identity_line(),
            self.files.len()
        )
    }

    pub(crate) fn validate_report(&self, report: &str) -> Result<(), String> {
        let identities = report
            .lines()
            .filter(|line| line.starts_with("Selector identity:"))
            .collect::<Vec<_>>();
        if identities == [self.identity_line()] {
            Ok(())
        } else {
            Err(format!(
                "expected exactly one `{}`; observed {identities:?}",
                self.identity_line()
            ))
        }
    }
}

pub(crate) fn discover(root: &Path) -> Result<Selection, String> {
    let base = git_commit(
        root,
        &["rev-parse", "--verify", "refs/remotes/origin/main^{commit}"],
    )?;
    let head = git_commit(root, &["rev-parse", "--verify", "HEAD^{commit}"])?;
    let merge_base = git_commit(root, &["merge-base", &base, &head])?;
    // Never rediscover symbolic refs for the inventory: a fetch/commit between
    // commands must not attach paths from another basis to these identities.
    let files = crate::git_changed_files(root, &format!("{merge_base}..{head}"))?;
    Ok(Selection {
        base,
        head,
        merge_base,
        files,
    })
}

fn git_commit(root: &Path, args: &[&str]) -> Result<String, String> {
    let args = args
        .iter()
        .map(|arg| (*arg).to_string())
        .collect::<Vec<_>>();
    let output =
        crate::run::capture_output_in_dir("git", &args, root, "check-fast selector identity")?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed ({}): {}; check the local refs and run `git fetch origin main` if needed",
            args.join(" "),
            output.status,
            output.stderr.trim()
        ));
    }
    let value = output.stdout.trim();
    if !matches!(value.len(), 40 | 64) || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!(
            "git {} did not return one commit identity: {value:?}",
            args.join(" ")
        ));
    }
    Ok(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::discover;

    #[test]
    fn native_selection_preserves_divergence_detached_dirty_and_empty_workflows()
    -> Result<(), String> {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ripr-selector-compatibility-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        let git = |args: &[&str]| -> Result<String, String> {
            let args = args
                .iter()
                .map(|arg| (*arg).to_string())
                .collect::<Vec<_>>();
            let output = crate::run::capture_output_in_dir(
                "git",
                &args,
                &root,
                "selector compatibility fixture Git",
            )?;
            if output.status.success() {
                Ok(output.stdout.trim().to_string())
            } else {
                Err(format!("fixture Git {args:?}: {}", output.stderr))
            }
        };
        let commit = |message: &str| -> Result<String, String> {
            git(&["add", "--", "docs/guide.md"])?;
            git(&["-c", "commit.gpgsign=false", "commit", "-m", message])?;
            git(&["rev-parse", "HEAD"])
        };
        let result = (|| {
            git(&["init", "--initial-branch=main"])?;
            git(&["config", "user.name", "Selector fixture"])?;
            git(&["config", "user.email", "selector@example.invalid"])?;
            std::fs::create_dir_all(root.join("docs")).map_err(|error| error.to_string())?;
            std::fs::write(root.join("docs/guide.md"), "base")
                .map_err(|error| error.to_string())?;
            let base = commit("base")?;
            git(&["update-ref", "refs/remotes/origin/main", &base])?;
            std::fs::write(root.join("docs/guide.md"), "candidate")
                .map_err(|error| error.to_string())?;
            let head = commit("candidate")?;
            let saved = discover(&root)?;
            assert_eq!(saved.base, base);
            assert_eq!(saved.head, head);
            assert_eq!(saved.merge_base, base);
            assert_eq!(saved.files, ["docs/guide.md"]);

            git(&["checkout", "--detach", &base])?;
            git(&[
                "-c",
                "commit.gpgsign=false",
                "commit",
                "--allow-empty",
                "-m",
                "upstream",
            ])?;
            let upstream = git(&["rev-parse", "HEAD"])?;
            git(&["update-ref", "refs/remotes/origin/main", &upstream])?;
            git(&["checkout", "--detach", &head])?;
            let diverged = discover(&root)?;
            assert_eq!(diverged.base, upstream);
            assert_eq!(diverged.head, head);
            assert_eq!(diverged.merge_base, base);
            assert_eq!(diverged.files, saved.files);
            assert_ne!(
                diverged, saved,
                "base movement must be observable even with identical paths"
            );

            std::fs::write(root.join("docs/guide.md"), "local edit")
                .map_err(|error| error.to_string())?;
            std::fs::write(root.join("untracked.txt"), "untracked")
                .map_err(|error| error.to_string())?;
            assert_eq!(
                discover(&root)?,
                diverged,
                "local authoring stays supported"
            );
            git(&["add", "--", "docs/guide.md"])?;
            assert_eq!(discover(&root)?, diverged, "staged edits stay supported");

            std::fs::write(root.join("docs/guide.md"), "base")
                .map_err(|error| error.to_string())?;
            let reverted = commit("revert net change")?;
            let empty = discover(&root)?;
            assert_eq!(empty.head, reverted);
            assert_ne!(empty.head, empty.merge_base);
            assert!(
                empty.files.is_empty(),
                "a valid net-empty diff stays supported"
            );
            let diagnostic = empty.diagnostic();
            for expected in [
                format!("origin/main={upstream}"),
                format!("HEAD={reverted}"),
                format!("merge-base={base}"),
                "Changed files: 0".to_string(),
                "Ref freshness: not checked".to_string(),
                "gates inspect the live worktree".to_string(),
            ] {
                assert!(diagnostic.contains(&expected), "{diagnostic}");
            }
            git(&["checkout", "--detach", &base])?;
            let behind = discover(&root)?;
            assert_eq!(behind.base, upstream);
            assert_eq!(behind.head, base);
            assert_eq!(behind.merge_base, base);
            assert!(behind.files.is_empty(), "behind-only is a valid selector");
            Ok(())
        })();
        let cleanup = std::fs::remove_dir_all(&root).map_err(|error| error.to_string());
        result.and(cleanup)
    }
}

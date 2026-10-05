use super::{CheckInput, Mode};
use crate::agent::loop_commands::{bound_root, shell_arg};
use std::path::Path;

/// Copy-pasteable sibling commands for a selected finding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FindingNavigation {
    explain_prefix: String,
    context_prefix: String,
    list_prefix: String,
    stub_prefix: String,
}

impl FindingNavigation {
    pub(crate) fn legacy() -> Self {
        Self {
            explain_prefix: "ripr explain".to_string(),
            context_prefix: "ripr context".to_string(),
            list_prefix: "ripr check".to_string(),
            stub_prefix: "ripr agent stub".to_string(),
        }
    }

    pub(crate) fn explain_command(&self, selector: &str) -> String {
        format!("{} {}", self.explain_prefix, shell_arg(selector))
    }

    pub(crate) fn context_command(&self, selector: &str) -> String {
        format!("{} --at {}", self.context_prefix, shell_arg(selector))
    }

    /// The one-step `ripr agent stub` route from a finding location to a
    /// runnable test (#5355). It reads the working tree at the same root.
    pub(crate) fn stub_command(&self, file: &str, line: usize) -> String {
        format!(
            "{} --at {}",
            self.stub_prefix,
            shell_arg(&format!("{file}:{line}"))
        )
    }

    /// The `ripr check --json` command that lists finding ids for the same
    /// scope (root, base or diff, `--worktree`, mode), so a selector miss
    /// recovers against the analysis it asked about. Only meaningful for a
    /// fresh scope: `ripr check` has no `--from`.
    pub(crate) fn list_command(&self) -> String {
        format!("{} --json", self.list_prefix)
    }
}

/// The drill-in guidance the human surfaces print (#4321). The block must
/// never vanish silently: every check, including a `--worktree` run without
/// an artifact, prints sibling commands that replay its own input identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FindingDrillIn {
    /// Copy-pasteable `ripr explain` / `ripr context` commands that preserve
    /// this run's input identity (`--worktree` included).
    Commands(FindingNavigation),
}

/// Build sibling commands that preserve the input identity needed to replay a
/// finding. An artifact is authoritative for its diff source; otherwise the
/// explicit diff or base is carried forward.
pub(crate) fn finding_navigation(
    input: &CheckInput,
    artifact_path: Option<&Path>,
    mode_explicit: bool,
) -> FindingNavigation {
    finding_navigation_with_worktree(input, artifact_path, mode_explicit, false)
}

/// [`finding_navigation`] for a run that may have analyzed the working tree:
/// `worktree` carries `--worktree` forward so the drill-in commands see the
/// same uncommitted edits as the check that listed the finding.
pub(crate) fn finding_navigation_with_worktree(
    input: &CheckInput,
    artifact_path: Option<&Path>,
    mode_explicit: bool,
    worktree: bool,
) -> FindingNavigation {
    let args = navigation_args(input, artifact_path, mode_explicit, worktree);
    // `ripr check` has no `--from`: the listing always re-runs the original
    // scope, even when the drill-in commands replay a written artifact.
    let list_args = navigation_args(input, None, mode_explicit, worktree);
    FindingNavigation {
        explain_prefix: format!("ripr explain {args}"),
        context_prefix: format!("ripr context {args}"),
        list_prefix: format!("ripr check {list_args}"),
        stub_prefix: format!(
            "ripr agent stub --root {}",
            shell_arg(&bound_root(&input.root.display().to_string()))
        ),
    }
}

fn navigation_args(
    input: &CheckInput,
    artifact_path: Option<&Path>,
    mode_explicit: bool,
    worktree: bool,
) -> String {
    // The drill-in is pasted after the listing, often from another directory,
    // so it names the repository the check resolved, not the typed relative
    // spelling (#3948).
    let mut args = vec![format!(
        "--root {}",
        shell_arg(&bound_root(&input.root.display().to_string()))
    )];

    if let Some(artifact_path) = artifact_path {
        args.push(format!(
            "--from {}",
            shell_arg(&artifact_path.display().to_string())
        ));
    } else if let Some(diff_file) = input.diff_file.as_deref() {
        args.push(format!(
            "--diff {}",
            shell_arg(&diff_file.display().to_string())
        ));
    } else {
        if let Some(base) = input.base.as_deref() {
            args.push(format!("--base {}", shell_arg(base)));
        }
        if worktree {
            args.push("--worktree".to_string());
        }
    }

    if mode_explicit || input.mode != Mode::Draft {
        args.push(format!("--mode {}", shell_arg(input.mode.as_str())));
    }
    if !input.include_unchanged_tests {
        args.push("--no-unchanged-tests".to_string());
    }
    if let Some(perl_facts_path) = input.perl_facts_path.as_deref() {
        args.push(format!(
            "--perl-facts {}",
            shell_arg(&perl_facts_path.display().to_string())
        ));
    }
    if let Some(suppression_policy) = input.suppression_policy.as_deref() {
        args.push(format!(
            "--suppression-policy {}",
            shell_arg(&suppression_policy.display().to_string())
        ));
    }

    args.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// The `--root` value a drill-in prints for a typed root: the resolved
    /// repository, quoted for the shell.
    fn bound(root: &str) -> String {
        shell_arg(&bound_root(root))
    }

    #[test]
    fn finding_navigation_preserves_diff_and_quotes_dynamic_values() {
        let input = CheckInput {
            root: PathBuf::from("repo root"),
            diff_file: Some(PathBuf::from("change set.diff")),
            ..CheckInput::default()
        };
        let navigation = finding_navigation(&input, None, false);

        let root = bound("repo root");
        assert_eq!(
            navigation.explain_command("probe:src/lib.rs:error_path:abc123"),
            format!(
                "ripr explain --root {root} --diff 'change set.diff' probe:src/lib.rs:error_path:abc123"
            )
        );
        assert_eq!(
            navigation.context_command("probe:src/lib.rs:error_path:abc123"),
            format!(
                "ripr context --root {root} --diff 'change set.diff' --at probe:src/lib.rs:error_path:abc123"
            )
        );
    }

    #[test]
    fn finding_navigation_prefers_artifact_identity_over_diff_source() {
        let input = CheckInput {
            root: PathBuf::from("repo"),
            diff_file: Some(PathBuf::from("old.diff")),
            mode: Mode::Ready,
            ..CheckInput::default()
        };
        let navigation = finding_navigation(&input, Some(Path::new("saved artifact.json")), false);

        assert_eq!(
            navigation.explain_command("probe:id"),
            format!(
                "ripr explain --root {} --from 'saved artifact.json' --mode ready probe:id",
                bound("repo")
            )
        );
    }

    /// `check --worktree` lists findings from uncommitted edits; its drill-in
    /// commands must analyze the same edits, not the committed history.
    #[test]
    fn finding_navigation_carries_worktree_scope_after_the_base() {
        let input = CheckInput {
            base: Some("HEAD".to_string()),
            ..CheckInput::default()
        };
        let navigation = finding_navigation_with_worktree(&input, None, false, true);
        assert_eq!(
            navigation.explain_command("src/calc.py:5"),
            format!(
                "ripr explain --root {} --base HEAD --worktree src/calc.py:5",
                bound(".")
            )
        );
        assert_eq!(
            navigation.context_command("src/calc.py:5"),
            format!(
                "ripr context --root {} --base HEAD --worktree --at src/calc.py:5",
                bound(".")
            )
        );
        // A selector miss lists ids from the same worktree scope.
        assert_eq!(
            navigation.list_command(),
            format!(
                "ripr check --root {} --base HEAD --worktree --json",
                bound(".")
            )
        );
        // An artifact already records the worktree diff; `--from` wins.
        let from_artifact =
            finding_navigation_with_worktree(&input, Some(Path::new("wt.json")), false, true);
        assert_eq!(
            from_artifact.explain_command("probe:id"),
            format!("ripr explain --root {} --from wt.json probe:id", bound("."))
        );
        // `ripr check` has no `--from`: the listing re-runs the worktree scope.
        assert_eq!(
            from_artifact.list_command(),
            format!(
                "ripr check --root {} --base HEAD --worktree --json",
                bound(".")
            )
        );
    }

    #[test]
    fn finding_navigation_preserves_explicit_draft_mode() {
        // #3952: the default carries no base, so name the base explicitly;
        // the test pins mode preservation, not the old default.
        let input = CheckInput {
            base: Some("origin/main".to_string()),
            ..CheckInput::default()
        };
        let navigation = finding_navigation(&input, None, true);

        assert_eq!(
            navigation.explain_command("probe:id"),
            format!(
                "ripr explain --root {} --base origin/main --mode draft probe:id",
                bound(".")
            )
        );
    }

    /// #3948: a relative typed root must not be repeated as typed. Pasted from
    /// another directory it would name whatever sits at that relative path
    /// there, so every drill-in (explain, context, list, stub) carries the
    /// resolved repository.
    #[test]
    fn finding_navigation_binds_a_relative_root_for_every_drill_in() {
        let input = CheckInput {
            root: PathBuf::from("nested/repo"),
            base: Some("HEAD".to_string()),
            ..CheckInput::default()
        };
        let navigation = finding_navigation(&input, None, false);
        let resolved = bound_root("nested/repo");
        assert!(
            Path::new(&resolved).is_absolute(),
            "bound root must be absolute: {resolved}"
        );
        let root_arg = format!("--root {}", shell_arg(&resolved));
        for command in [
            navigation.explain_command("probe:id"),
            navigation.context_command("probe:id"),
            navigation.list_command(),
            navigation.stub_command("src/lib.rs", 5),
        ] {
            assert!(
                command.contains(&root_arg),
                "drill-in must carry the resolved root: {command}"
            );
            assert!(
                !command.contains("--root nested/repo"),
                "drill-in repeated the typed relative root: {command}"
            );
        }
    }
}

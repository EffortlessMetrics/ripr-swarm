// Keep the helper self-tests in one integration binary.
include!("common/fixture_git.rs");

mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static FIXTURE_GIT_COUNTER: AtomicU64 = AtomicU64::new(0);

    /// Fixture root that removes itself on every exit path, so a failing
    /// test never leaks its temp directory.
    struct TempDirGuard(PathBuf);

    impl std::ops::Deref for TempDirGuard {
        type Target = Path;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn unique_root(label: &str) -> Result<TempDirGuard, String> {
        let root = std::env::temp_dir().join(format!(
            "ripr-common-fixture-git-{label}-{}-{}",
            std::process::id(),
            FIXTURE_GIT_COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&root)
            .map_err(|error| format!("create {} failed: {error}", root.display()))?;
        Ok(TempDirGuard(root))
    }

    #[test]
    fn resolves_the_subcommand_past_global_options() -> Result<(), String> {
        if fixture_subcommand(&["-c", "init.templateDir=", "init", "-q"]) != Some("init") {
            return Err("global -c pair must resolve to the init subcommand".to_string());
        }
        if !is_retryable_checkout(&["checkout", "--detach", "deadbeef"]) {
            return Err("detached checkout must stay retryable".to_string());
        }
        if is_retryable_checkout(&["checkout", "-b", "probe-branch"]) {
            return Err("branch creation must never retry".to_string());
        }
        Ok(())
    }

    #[test]
    fn fixture_git_cannot_leave_background_maintenance_running() -> Result<(), String> {
        let root = unique_root("no-background")?;
        fixture_git_ok(&root, &["init", "-q", "-b", "main"])?;
        // A repository that asks for background maintenance still gets none:
        // the per-invocation configuration outranks the repository's.
        fixture_git_ok(&root, &["config", "maintenance.auto", "true"])?;
        fixture_git_ok(&root, &["config", "gc.auto", "1"])?;
        for (key, want) in [("maintenance.auto", "false"), ("gc.auto", "0")] {
            let effective = fixture_git_output(&root, &["config", "--get", key])?;
            if effective.trim() != want {
                return Err(format!(
                    "fixture git resolves {key} to {:?}, want {want:?}",
                    effective.trim()
                ));
            }
        }
        // The commit is the step that detaches maintenance. Every fixture
        // subcommand is built by one function, so the commit command itself
        // must carry the same pairs and no inherited `-c` channel.
        let commit = fixture_git_command(&root, &["commit", "-qm", "fixture"]);
        let env = |name: &str| {
            commit
                .get_envs()
                .find(|(key, _)| *key == std::ffi::OsStr::new(name))
                .map(|(_, value)| value.map(|value| value.to_string_lossy().into_owned()))
        };
        let pairs = [
            ("GIT_CONFIG_COUNT", "2"),
            ("GIT_CONFIG_KEY_0", "maintenance.auto"),
            ("GIT_CONFIG_VALUE_0", "false"),
            ("GIT_CONFIG_KEY_1", "gc.auto"),
            ("GIT_CONFIG_VALUE_1", "0"),
        ];
        for (name, want) in pairs {
            if env(name) != Some(Some(want.to_string())) {
                return Err(format!(
                    "commit command carries {name}={:?}, want {want:?}",
                    env(name)
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn fixture_setup_preserves_utf8_bytes_in_its_own_worktree() -> Result<(), String> {
        let root = unique_root("bytes with spaces")?;
        fixture_git_ok(&root, &["init", "-q", "-b", "main"])?;
        fixture_git_ok(&root, &["config", "user.name", "Fixture"])?;
        fixture_git_ok(&root, &["config", "user.email", "fixture@example.invalid"])?;
        let contents = "corpus: café, İstanbul, 日本語\n";
        std::fs::write(root.join("corpus.txt"), contents)
            .map_err(|error| format!("write corpus fixture: {error}"))?;
        fixture_git_ok(&root, &["add", "corpus.txt"])?;
        fixture_git_ok(&root, &["commit", "-q", "-m", "corpus fixture"])?;
        let stored = fixture_git_output(&root, &["show", "HEAD:corpus.txt"])?;
        assert_eq!(stored, contents);
        let top = fixture_git_output(&root, &["rev-parse", "--show-toplevel"])?;
        assert_eq!(
            std::fs::canonicalize(top.trim()).map_err(|error| error.to_string())?,
            std::fs::canonicalize(&root.0).map_err(|error| error.to_string())?
        );
        Ok(())
    }

    #[test]
    fn fixture_setup_keeps_explicit_local_and_command_configuration() -> Result<(), String> {
        let root = unique_root("explicit-config")?;
        fixture_git_ok(&root, &["init", "-q"])?;
        fixture_git_ok(&root, &["config", "fixture.setting", "local"])?;
        assert_eq!(
            fixture_git_output(&root, &["config", "--get", "fixture.setting"])?,
            "local\n"
        );
        assert_eq!(
            fixture_git_output(
                &root,
                &[
                    "-c",
                    "fixture.setting=command",
                    "config",
                    "--get",
                    "fixture.setting"
                ]
            )?,
            "command\n"
        );
        // Isolating default global ignores must retain explicit fixture
        // choices, including a command-line override of the local file.
        std::fs::write(root.join("local.ignore"), "local.txt\n")
            .map_err(|error| error.to_string())?;
        std::fs::write(root.join("command.ignore"), "command.txt\n")
            .map_err(|error| error.to_string())?;
        fixture_git_ok(&root, &["config", "core.excludesFile", "local.ignore"])?;
        assert_eq!(
            fixture_git_output(&root, &["check-ignore", "local.txt"])?,
            "local.txt\n"
        );
        assert_eq!(
            fixture_git_output(
                &root,
                &[
                    "-c",
                    "core.excludesFile=command.ignore",
                    "check-ignore",
                    "command.txt"
                ]
            )?,
            "command.txt\n"
        );
        // System-attribute isolation must not discard explicitly selected
        // attribute files, including a command-line override of the local one.
        std::fs::write(
            root.join("local.attributes"),
            "corpus.txt fixture-layer=local\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(
            root.join("command.attributes"),
            "corpus.txt fixture-layer=command\n",
        )
        .map_err(|error| error.to_string())?;
        fixture_git_ok(
            &root,
            &["config", "core.attributesFile", "local.attributes"],
        )?;
        assert_eq!(
            fixture_git_output(&root, &["check-attr", "fixture-layer", "--", "corpus.txt"])?,
            "corpus.txt: fixture-layer: local\n"
        );
        assert_eq!(
            fixture_git_output(
                &root,
                &[
                    "-c",
                    "core.attributesFile=command.attributes",
                    "check-attr",
                    "fixture-layer",
                    "--",
                    "corpus.txt",
                ]
            )?,
            "corpus.txt: fixture-layer: command\n"
        );
        Ok(())
    }

    #[test]
    fn fixture_setup_disables_system_attributes() -> Result<(), String> {
        let root = unique_root("system-attributes")?;
        // Establish that this Git can report an enabled system-attributes
        // path. A missing capability must not masquerade as isolation. Use a
        // separate child with the guard explicitly disabled and the same
        // bounded spawn/drain owner; never change the parent or system files.
        let args = ["var", "GIT_ATTR_SYSTEM"];
        let mut control = fixture_git_command(&root, &args);
        control.env("GIT_ATTR_NOSYSTEM", "0");
        let output = match run_git_command_deadline(&root, &args, FIXTURE_GIT_DEADLINE, control)? {
            FixtureGitOutcome::Finished(output) => output,
            FixtureGitOutcome::TimedOut(error) => return Err(error),
        };
        if !output.status.success() || String::from_utf8_lossy(&output.stdout).trim().is_empty() {
            return Err(format!(
                "positive control did not identify the system attributes path: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let variables = fixture_git_output(&root, &["var", "-l"])?;
        assert!(
            !variables
                .lines()
                .any(|line| line.starts_with("GIT_ATTR_SYSTEM=")),
            "fixture child still enables system attributes: {variables}"
        );
        Ok(())
    }

    #[test]
    fn fixture_setup_reports_stable_english_diagnostics() -> Result<(), String> {
        let root = unique_root("diagnostic")?;
        let error = fixture_git_output(&root, &["not-a-fixture-subcommand"])
            .err()
            .ok_or("invalid git command unexpectedly succeeded")?;
        assert!(error.contains("is not a git command"), "{error}");
        Ok(())
    }

    #[test]
    fn zero_deadline_reports_timeouts_without_retrying_state() -> Result<(), String> {
        let root = unique_root("timeout")?;
        match fixture_git_ok_with_deadline(
            &root,
            &["-c", "init.templateDir=", "init"],
            Duration::ZERO,
        ) {
            Ok(()) => return Err("zero-deadline init unexpectedly succeeded".to_string()),
            // Both attempts refused before spawning: init is retried, and the
            // retry is refused the same way.
            Err(error) if error.contains("retry timed out") && error.contains("(not spawned)") => {}
            Err(error) => {
                return Err(format!("unexpected init error shape: {error}"));
            }
        }
        // A commit is never re-run. Its pre-commit HEAD probe is refused
        // before spawning, and that refusal propagates, so the commit itself
        // never runs. When the runner still spawned on a zero deadline, this
        // outcome depended on whether each git exited before the first poll
        // (#3742), and hosted CI hit the init retry's lock failure on #4014.
        match fixture_git_ok_with_deadline(&root, &["commit", "-m", "x"], Duration::ZERO) {
            Ok(()) => Err("zero-deadline commit unexpectedly succeeded".to_string()),
            Err(error)
                if error.contains("probe fixture HEAD") && error.contains("(not spawned)") =>
            {
                Ok(())
            }
            Err(error) => Err(format!("unexpected commit error shape: {error}")),
        }
    }
}

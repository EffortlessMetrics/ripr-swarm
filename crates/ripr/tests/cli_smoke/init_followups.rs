use super::{replay_printed_ripr, run_command, run_git, unique_temp_workspace};
use std::path::Path;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// A retry can exit successfully in a valid decoy directory. Observe the
/// selected files and the untouched decoy, rather than only command spelling.
#[test]
fn init_refusal_retries_replay_the_selected_root() -> TestResult {
    let scratch = unique_temp_workspace("init-refusal-retry");
    let selected = scratch.join("selected répo it's");
    let decoy = scratch.join("decoy");
    let config = "[analysis]\nmode = \"deep\"\n";
    let workflow = "name: Existing\n";
    let result: TestResult = (|| {
        for root in [&selected, &decoy] {
            std::fs::create_dir_all(root.join(".github/workflows"))?;
            std::fs::write(root.join("ripr.toml"), config)?;
            std::fs::write(root.join(".github/workflows/ripr.yml"), workflow)?;
        }
        let absolute = selected.to_str().ok_or("fixture root must be UTF-8")?;
        for (cwd, root_args) in [
            (&decoy, vec!["--root", absolute]),
            (&scratch, vec!["--root", "selected répo it's"]),
            (&selected, vec![]),
        ] {
            for ci in [false, true] {
                for dry_run in [false, true] {
                    std::fs::write(selected.join("ripr.toml"), config)?;
                    std::fs::write(selected.join(".github/workflows/ripr.yml"), workflow)?;
                    let mut args = vec!["init"];
                    args.extend(&root_args);
                    if ci {
                        args.extend(["--ci", "github"]);
                    }
                    if dry_run {
                        args.push("--dry-run");
                    }
                    let refused = run_command(env!("CARGO_BIN_EXE_ripr"), Some(cwd), &args)?;
                    assert_eq!(refused.status.code(), Some(2), "{refused:?}");
                    assert_eq!(std::fs::read_to_string(selected.join("ripr.toml"))?, config);
                    assert_eq!(
                        std::fs::read_to_string(selected.join(".github/workflows/ripr.yml"))?,
                        workflow
                    );
                    let stderr = String::from_utf8(refused.stderr)?;
                    let commands: Vec<_> = stderr
                        .split('`')
                        .filter(|text| text.starts_with("ripr init"))
                        .collect();
                    assert!(!commands.is_empty(), "{stderr}");
                    for command in &commands {
                        std::fs::write(selected.join("ripr.toml"), config)?;
                        if !ci {
                            // The config-preserving alternative has a new workflow to write.
                            let path = selected.join(".github/workflows/ripr.yml");
                            if path.exists() {
                                std::fs::remove_file(path)?;
                            }
                        }
                        let replay = replay_printed_ripr(command, &decoy)?;
                        assert!(replay.status.success(), "{command}: {replay:?}");
                        let after = std::fs::read_to_string(selected.join("ripr.toml"))?;
                        if command.contains("--dry-run") || command.contains("--ci github") {
                            assert_eq!(after, config, "{command}");
                        } else {
                            assert!(after.contains("mode = \"draft\""), "{command}: {after}");
                        }
                        if command.contains("--ci github") {
                            let after = std::fs::read_to_string(
                                selected.join(".github/workflows/ripr.yml"),
                            )?;
                            assert!(after.contains("name: RIPR"), "{command}: {after}");
                        }
                        assert_eq!(std::fs::read_to_string(decoy.join("ripr.toml"))?, config);
                        assert_eq!(
                            std::fs::read_to_string(decoy.join(".github/workflows/ripr.yml"))?,
                            workflow
                        );
                    }
                    assert_eq!(commands.len(), if ci { 1 } else { 3 }, "{stderr}");
                }
            }
        }
        Ok(())
    })();
    std::fs::remove_dir_all(&scratch)?;
    result
}

#[cfg(unix)]
#[test]
fn init_refusal_omits_retries_for_an_unrepresentable_root() -> TestResult {
    use std::os::unix::ffi::OsStringExt;
    let scratch = unique_temp_workspace("init-refusal-lossless");
    let selected = scratch.join(std::ffi::OsString::from_vec(b"selected-\xff".to_vec()));
    let result: TestResult = (|| {
        std::fs::create_dir_all(selected.join(".github/workflows"))?;
        std::fs::write(selected.join("ripr.toml"), "[analysis]\n")?;
        std::fs::write(
            selected.join(".github/workflows/ripr.yml"),
            "name: Existing\n",
        )?;
        for args in [vec!["init"], vec!["init", "--ci", "github"]] {
            let refused = run_command(env!("CARGO_BIN_EXE_ripr"), Some(&selected), &args)?;
            assert_eq!(refused.status.code(), Some(2), "{refused:?}");
            let stderr = String::from_utf8(refused.stderr)?;
            assert!(stderr.contains("already exists"), "{stderr}");
            assert!(stderr.contains("commands omitted"), "{stderr}");
            assert!(stderr.contains("UTF-8"), "{stderr}");
            assert!(!stderr.contains("`ripr init"), "{stderr}");
        }
        Ok(())
    })();
    std::fs::remove_dir_all(&scratch)?;
    result
}

fn rust_repo(root: &Path) -> TestResult {
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"init-followups\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )?;
    std::fs::write(root.join("src/lib.rs"), "pub fn answer() -> u8 { 15 }\n")?;
    std::fs::write(root.join(".gitignore"), "/target/\n")?;
    run_git(root, &["init", "-q", "-b", "main"])?;
    run_git(root, &["add", "."])?;
    run_git(
        root,
        &[
            "-c",
            "user.name=RIPR test",
            "-c",
            "user.email=ripr@example.invalid",
            "commit",
            "-qm",
            "initial Rust project",
        ],
    )?;
    std::fs::write(root.join("src/lib.rs"), "pub fn answer() -> u8 { 17 }\n")?;
    Ok(())
}

/// Both commands can succeed against a valid decoy. Pin the configured
/// repository and its mode/changed value, so success or `--root .` is not
/// enough. Absolute, relative and implicit init roots must survive a later
/// paste from a different directory, including an apostrophe and non-ASCII.
#[test]
fn init_followups_replay_the_selected_root_from_foreign_cwd() -> TestResult {
    let scratch = unique_temp_workspace("init-followups");
    let selected = scratch.join("selected répo it's");
    let decoy = scratch.join("decoy");
    let empty = scratch.join("empty");
    let result: TestResult = (|| {
        rust_repo(&selected)?;
        rust_repo(&decoy)?;
        std::fs::create_dir_all(&empty)?;
        std::fs::write(decoy.join("ripr.toml"), "[analysis]\nmode = \"instant\"\n")?;
        let selected_arg = selected.display().to_string();
        let config_path = selected
            .join("ripr.toml")
            .display()
            .to_string()
            .replace('\\', "/");
        for (cwd, args) in [
            (
                &decoy,
                vec!["init", "--root", selected_arg.as_str(), "--force"],
            ),
            (
                &scratch,
                vec!["init", "--root", "selected répo it's", "--force"],
            ),
            (&selected, vec!["init", "--force"]),
        ] {
            let init = run_command(env!("CARGO_BIN_EXE_ripr"), Some(cwd), &args)?;
            assert!(init.status.success(), "{init:?}");
            assert!(selected.join("ripr.toml").is_file());
            let stdout = String::from_utf8(init.stdout)?;
            let commands: Vec<_> = stdout
                .split('`')
                .filter(|text| text.starts_with("ripr doctor") || text.starts_with("ripr check"))
                .collect();
            assert_eq!(commands.len(), 2, "{stdout}");
            for command in commands {
                let replay = replay_printed_ripr(command, &decoy)?;
                assert!(replay.status.success(), "{command}: {replay:?}");
                let text = String::from_utf8(replay.stdout)?;
                if command.starts_with("ripr doctor") {
                    assert!(
                        text.contains(&format!("Config path: {config_path}")),
                        "{text}"
                    );
                    assert!(text.contains("Analysis mode default: draft"), "{text}");
                } else {
                    assert!(text.contains("\nmode: draft\n"), "{text}");
                    assert!(text.contains("Changed behavior: 17"), "{text}");
                    assert!(
                        text.contains(&format!("root: {}", selected_arg.replace('\\', "/"))),
                        "{text}"
                    );
                }
                let replay = replay_printed_ripr(command, &empty)?;
                assert!(replay.status.success(), "{command}: {replay:?}");
            }
        }
        // The plausible root-dot substitute exits 0 in the decoy yet must
        // remain distinguishable by the literal policy and location pins.
        for command in ["ripr doctor --root .", "ripr check --root ."] {
            let replay = replay_printed_ripr(command, &decoy)?;
            assert!(replay.status.success(), "{replay:?}");
            let text = String::from_utf8(replay.stdout)?;
            assert!(!text.contains(&config_path), "{text}");
            assert!(text.contains("instant"), "{text}");
        }
        Ok(())
    })();
    std::fs::remove_dir_all(&scratch)?;
    result
}

#[cfg(unix)]
fn raw_subject_and_sentinel(
    scratch: &Path,
) -> Result<(std::path::PathBuf, std::path::PathBuf), Box<dyn std::error::Error>> {
    use std::os::unix::ffi::OsStringExt;
    let selected = scratch.join(std::ffi::OsString::from_vec(
        b"selected-\xff r\xc3\xa9po".to_vec(),
    ));
    // Literal U+FFFD spells a different valid repository. Do not derive this
    // oracle from the product's rendered command or path conversion helper.
    let sentinel = scratch.join("selected-\u{fffd} répo");
    rust_repo(&selected)?;
    rust_repo(&sentinel)?;
    std::fs::write(
        selected.join("src/lib.rs"),
        "pub fn answer() -> u8 { 43 }\n",
    )?;
    std::fs::write(
        sentinel.join("src/lib.rs"),
        "pub fn answer() -> u8 { 73 }\n",
    )?;
    std::fs::write(
        sentinel.join("ripr.toml"),
        "[analysis]\nmode = \"instant\"\n",
    )?;
    Ok((selected, sentinel))
}

/// UTF-8 argv can select a byte-invalid implicit cwd. Successful config
/// writes must not manufacture portable commands targeting a lossy decoy.
#[cfg(unix)]
#[test]
fn init_omits_followups_for_an_unrepresentable_implicit_root() -> TestResult {
    let scratch = unique_temp_workspace("init-lossless-root");
    let result: TestResult = (|| {
        let (selected, sentinel) = raw_subject_and_sentinel(&scratch)?;
        let init = run_command(
            env!("CARGO_BIN_EXE_ripr"),
            Some(&selected),
            &["init", "--root", "."],
        )?;
        assert!(init.status.success(), "{init:?}");
        assert!(selected.join("ripr.toml").is_file());
        // Both repositories are healthy and independently distinguishable.
        for (root, mode, answer) in [(&selected, "draft", 43), (&sentinel, "instant", 73)] {
            let doctor = run_command(
                env!("CARGO_BIN_EXE_ripr"),
                Some(root),
                &["doctor", "--root", "."],
            )?;
            assert!(doctor.status.success(), "{doctor:?}");
            let text = String::from_utf8(doctor.stdout)?;
            assert!(
                text.contains(&format!("Analysis mode default: {mode}")),
                "{text}"
            );
            let check = run_command(
                env!("CARGO_BIN_EXE_ripr"),
                Some(root),
                &["check", "--root", "."],
            )?;
            assert!(check.status.success(), "{check:?}");
            let text = String::from_utf8(check.stdout)?;
            assert!(text.contains(&format!("\nmode: {mode}\n")), "{text}");
            assert!(
                text.contains(&format!("Changed behavior: {answer}")),
                "{text}"
            );
        }
        let stdout = String::from_utf8(init.stdout)?;
        let commands: Vec<_> = stdout
            .split('`')
            .filter(|text| text.starts_with("ripr doctor") || text.starts_with("ripr check"))
            .collect();
        // Reach the old failure: commands can exit 0 on the sentinel, while
        // the literal selected policy/behavior expectation still rejects it.
        for command in &commands {
            let replay = replay_printed_ripr(command, &sentinel)?;
            assert!(replay.status.success(), "{command}: {replay:?}");
            let text = String::from_utf8(replay.stdout)?;
            if command.starts_with("ripr doctor") {
                assert!(
                    text.contains("Analysis mode default: draft"),
                    "printed follow-up inspected the wrong repository: {text}"
                );
            } else {
                assert!(
                    text.contains("\nmode: draft\n"),
                    "printed follow-up inspected the wrong repository: {text}"
                );
                assert!(text.contains("Changed behavior: 43"), "{text}");
            }
        }
        assert!(
            commands.is_empty(),
            "an unrepresentable root must not produce executable follow-ups: {stdout}"
        );
        assert!(
            !stdout.contains("ripr doctor") && !stdout.contains("ripr check"),
            "{stdout}"
        );
        assert!(
            stdout.contains("Init completed; follow-up commands omitted"),
            "{stdout}"
        );
        assert!(
            stdout.contains("UTF-8 parent") && stdout.contains("UTF-8 alias"),
            "{stdout}"
        );
        assert!(!stdout.contains("(PowerShell) ripr"), "{stdout}");
        // An absent replacement path also needs the truthful omission, rather
        // than printing a command that will fail later with exit 2.
        std::fs::remove_dir_all(&sentinel)?;
        let init = run_command(
            env!("CARGO_BIN_EXE_ripr"),
            Some(&selected),
            &["init", "--root", ".", "--force"],
        )?;
        assert!(init.status.success(), "{init:?}");
        assert!(selected.join("ripr.toml").is_file());
        let stdout = String::from_utf8(init.stdout)?;
        assert!(stdout.contains("follow-up commands omitted"), "{stdout}");
        assert!(
            !stdout.contains("`ripr doctor") && !stdout.contains("`ripr check"),
            "{stdout}"
        );
        Ok(())
    })();
    std::fs::remove_dir_all(&scratch)?;
    result
}

/// A UTF-8 absolute alias is a lossless command spelling even when its
/// physical target contains invalid bytes. Do not blanket-refuse such roots.
#[cfg(unix)]
#[test]
fn init_followups_replay_a_utf8_alias_to_a_non_utf8_root() -> TestResult {
    let scratch = unique_temp_workspace("init-lossless-alias");
    let result: TestResult = (|| {
        let (selected, sentinel) = raw_subject_and_sentinel(&scratch)?;
        let alias = scratch.join("alias répo it's");
        std::os::unix::fs::symlink(&selected, &alias)?;
        let alias_arg = alias.to_str().ok_or("test alias must be UTF-8")?;
        let init = run_command(
            env!("CARGO_BIN_EXE_ripr"),
            Some(&sentinel),
            &["init", "--root", alias_arg],
        )?;
        assert!(init.status.success(), "{init:?}");
        assert!(selected.join("ripr.toml").is_file());
        let stdout = String::from_utf8(init.stdout)?;
        let commands: Vec<_> = stdout
            .split('`')
            .filter(|text| text.starts_with("ripr doctor") || text.starts_with("ripr check"))
            .collect();
        assert_eq!(commands.len(), 2, "{stdout}");
        for command in commands {
            let replay = replay_printed_ripr(command, &sentinel)?;
            assert!(replay.status.success(), "{command}: {replay:?}");
            let text = String::from_utf8(replay.stdout)?;
            if command.starts_with("ripr doctor") {
                assert!(text.contains("Analysis mode default: draft"), "{text}");
                assert!(!text.contains("Analysis mode default: instant"), "{text}");
            } else {
                assert!(text.contains("\nmode: draft\n"), "{text}");
                assert!(text.contains("Changed behavior: 43"), "{text}");
                assert!(!text.contains("Changed behavior: 73"), "{text}");
            }
        }
        Ok(())
    })();
    std::fs::remove_dir_all(&scratch)?;
    result
}

/// A byte-invalid producer cwd need not make the selected target invalid.
/// Resolve actual traversal rather than dropping `..` across a symlink.
#[cfg(unix)]
#[test]
fn init_followups_replay_utf8_targets_from_a_non_utf8_cwd() -> TestResult {
    use std::os::unix::ffi::OsStringExt;
    let scratch = unique_temp_workspace("init-lossless-parent");
    let result: TestResult = (|| {
        let parent = scratch.join("selected parent répo");
        let child = parent.join(std::ffi::OsString::from_vec(b"child-\xff".to_vec()));
        let physical = scratch.join("physical");
        let traversed = physical.join("repo");
        let lexical = child.join("repo");
        let decoy = scratch.join("decoy");
        for (root, answer, mode) in [
            (&parent, 311, "draft"),
            (&traversed, 412, "draft"),
            (&lexical, 789, "instant"),
            (&decoy, 999, "instant"),
        ] {
            rust_repo(root)?;
            std::fs::write(
                root.join("src/lib.rs"),
                format!("pub fn answer() -> u16 {{ {answer} }}\n"),
            )?;
            if mode == "instant" {
                std::fs::write(root.join("ripr.toml"), "[analysis]\nmode = \"instant\"\n")?;
            }
            // Healthy subjects use independent literal policy/value pins.
            let check = run_command(
                env!("CARGO_BIN_EXE_ripr"),
                Some(root),
                &["check", "--root", "."],
            )?;
            assert!(check.status.success(), "{check:?}");
            let text = String::from_utf8(check.stdout)?;
            assert!(text.contains(&format!("\nmode: {mode}\n")), "{text}");
            assert!(
                text.contains(&format!("Changed behavior: {answer}")),
                "{text}"
            );
        }
        std::fs::create_dir_all(physical.join("anchor"))?;
        std::os::unix::fs::symlink(physical.join("anchor"), child.join("link"))?;
        for (root_arg, selected, answer) in
            [("..", &parent, 311), ("link/../repo", &traversed, 412)]
        {
            let init = run_command(
                env!("CARGO_BIN_EXE_ripr"),
                Some(&child),
                &["init", "--root", root_arg],
            )?;
            assert!(init.status.success(), "{init:?}");
            assert!(selected.join("ripr.toml").is_file());
            let stdout = String::from_utf8(init.stdout)?;
            let commands: Vec<_> = stdout
                .split('`')
                .filter(|text| text.starts_with("ripr doctor") || text.starts_with("ripr check"))
                .collect();
            assert_eq!(
                commands.len(),
                2,
                "a lossless selected target exists: {stdout}"
            );
            for command in commands {
                let replay = replay_printed_ripr(command, &decoy)?;
                assert!(replay.status.success(), "{command}: {replay:?}");
                let text = String::from_utf8(replay.stdout)?;
                if command.starts_with("ripr doctor") {
                    assert!(text.contains("Analysis mode default: draft"), "{text}");
                    assert!(
                        text.contains(&format!(
                            "Config path: {}",
                            selected.join("ripr.toml").display()
                        )),
                        "{text}"
                    );
                } else {
                    assert!(text.contains("\nmode: draft\n"), "{text}");
                    assert!(
                        text.contains(&format!("Changed behavior: {answer}")),
                        "{text}"
                    );
                    assert!(!text.contains("Changed behavior: 789"), "{text}");
                    assert!(!text.contains("Changed behavior: 999"), "{text}");
                }
            }
        }
        Ok(())
    })();
    std::fs::remove_dir_all(&scratch)?;
    result
}

/// Observe the destination of the printed write command: a successful refresh
/// against the caller would otherwise look indistinguishable from success.
fn workflow_refresh_replays_selected_root(doctor: bool) -> TestResult {
    let scratch = unique_temp_workspace("workflow-refresh-root");
    let selected = scratch.join("selected répo it's");
    let decoy = scratch.join("decoy");
    let config = "[analysis]\nmode = \"deep\"\n";
    let sentinel = "name: caller sentinel\n";
    let result: TestResult = (|| {
        rust_repo(&selected)?;
        rust_repo(&decoy)?;
        for root in [&selected, &decoy] {
            std::fs::write(root.join("ripr.toml"), config)?;
            std::fs::create_dir_all(root.join(".github/workflows"))?;
        }
        let absolute = selected.to_str().ok_or("UTF-8 fixture root")?;
        for (cwd, root_args) in [
            (&decoy, vec!["--root", absolute]),
            (&scratch, vec!["--root", "selected répo it's"]),
            (&selected, vec![]),
        ] {
            for preview in [false, true] {
                let mut args = vec![if doctor { "doctor" } else { "init" }];
                args.extend(&root_args);
                if doctor {
                    // Both historical forms must get a usable refresh route.
                    let old_workflow = if preview {
                        "run: cargo install ripr --locked\n"
                    } else {
                        "run: cargo install ripr --version 0.0.1 --locked\n"
                    };
                    std::fs::write(selected.join(".github/workflows/ripr.yml"), old_workflow)?;
                } else {
                    args.extend(["--ci", "github", "--force"]);
                    if preview {
                        args.push("--dry-run");
                    }
                }
                let output = run_command(env!("CARGO_BIN_EXE_ripr"), Some(cwd), &args)?;
                assert!(output.status.success(), "{output:?}");
                let text = String::from_utf8(if doctor { output.stdout } else { output.stderr })?;
                if !doctor && !text.contains("is not released") {
                    // A released generator legitimately has no upgrade warning.
                    // Pin that state to the real self-pinned template instead
                    // of accepting a silently missing development warning.
                    let template = if preview {
                        run_command(env!("CARGO_BIN_EXE_ripr"), Some(cwd), &args)?.stdout
                    } else {
                        std::fs::read(selected.join(".github/workflows/ripr.yml"))?
                    };
                    assert!(
                        String::from_utf8(template)?
                            .contains(&format!("version={}\n", env!("CARGO_PKG_VERSION")))
                    );
                    continue;
                }
                let command = text
                    .split('`')
                    .find(|part| part.starts_with("ripr init"))
                    .ok_or_else(|| format!("missing refresh command: {text}"))?;
                std::fs::write(
                    selected.join(".github/workflows/ripr.yml"),
                    "name: selected sentinel\n",
                )?;
                std::fs::write(decoy.join(".github/workflows/ripr.yml"), sentinel)?;
                let refreshed = replay_printed_ripr(command, &decoy)?;
                assert!(refreshed.status.success(), "{command}: {refreshed:?}");
                let after = std::fs::read_to_string(selected.join(".github/workflows/ripr.yml"))?;
                assert!(
                    after.starts_with("name: RIPR\n"),
                    "selected workflow unchanged: {command}: {after}"
                );
                assert!(text.contains("(PowerShell) `ripr init"), "{text}");
                assert!(!text.contains("\\u{0a}(PowerShell)"), "{text}");
                assert_eq!(
                    std::fs::read_to_string(decoy.join(".github/workflows/ripr.yml"))?,
                    sentinel
                );
                for root in [&selected, &decoy] {
                    assert_eq!(std::fs::read_to_string(root.join("ripr.toml"))?, config);
                }
            }
        }
        Ok(())
    })();
    std::fs::remove_dir_all(&scratch)?;
    result
}

#[test]
fn init_upgrade_warning_refresh_replays_selected_root() -> TestResult {
    workflow_refresh_replays_selected_root(false)
}

#[test]
fn doctor_workflow_warning_refresh_replays_selected_root() -> TestResult {
    workflow_refresh_replays_selected_root(true)
}

#[cfg(unix)]
#[test]
fn workflow_warning_omits_refresh_for_unrepresentable_root() -> TestResult {
    use std::os::unix::ffi::OsStringExt;
    let scratch = unique_temp_workspace("workflow-refresh-lossless");
    let root = scratch.join(std::ffi::OsString::from_vec(b"selected-\xff".to_vec()));
    let result: TestResult = (|| {
        rust_repo(&root)?;
        let init = run_command(
            env!("CARGO_BIN_EXE_ripr"),
            Some(&root),
            &["init", "--ci", "github"],
        )?;
        assert!(init.status.success(), "{init:?}");
        let stderr = String::from_utf8(init.stderr)?;
        if stderr.contains("is not released") {
            assert!(stderr.contains("refresh command omitted"), "{stderr}");
            assert!(stderr.contains("UTF-8"), "{stderr}");
            assert!(!stderr.contains("`ripr init"), "{stderr}");
        }
        std::fs::write(
            root.join(".github/workflows/ripr.yml"),
            "run: cargo install ripr --locked\n",
        )?;
        let doctor = run_command(env!("CARGO_BIN_EXE_ripr"), Some(&root), &["doctor"])?;
        assert!(doctor.status.success(), "{doctor:?}");
        let stdout = String::from_utf8(doctor.stdout)?;
        let line = stdout
            .lines()
            .find(|line| line.contains("Generated workflow:"))
            .ok_or_else(|| format!("missing advisory: {stdout}"))?;
        assert!(line.contains("refresh command omitted"), "{line}");
        assert!(line.contains("UTF-8"), "{line}");
        assert!(!line.contains("`ripr init"), "{line}");
        Ok(())
    })();
    std::fs::remove_dir_all(&scratch)?;
    result
}

use super::{replay_printed_ripr, run_command, run_git, unique_temp_workspace};
use std::path::Path;

type TestResult = Result<(), Box<dyn std::error::Error>>;

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

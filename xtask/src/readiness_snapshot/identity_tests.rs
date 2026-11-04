use std::fs;
use std::path::PathBuf;

use super::identity::{collect_identity_inputs_for_commit, git_rev_parse, git_worktree_clean};

struct Fixture {
    root: PathBuf,
    previous: PathBuf,
    _guard: crate::CwdWriteGuard<'static>,
}

impl Fixture {
    fn new() -> Result<Self, String> {
        let guard = crate::acquire_test_cwd_write_guard();
        let previous = std::env::current_dir().map_err(|e| e.to_string())?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let suffix = if cfg!(unix) { " " } else { "" };
        let root = std::env::temp_dir().join(format!(
            "rehearsal-identity-{}-{stamp}{suffix}",
            std::process::id()
        ));
        fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let fixture = Self {
            root,
            previous,
            _guard: guard,
        };
        fixture.git(&["init", "-q"])?;
        for (path, text) in [
            ("Cargo.lock", "committed-lock\n"),
            ("Cargo.toml", "[workspace.package]\nversion = \"0.11.0\"\n"),
            (
                "crates/ripr/Cargo.toml",
                "[package]\nversion.workspace = true\n",
            ),
            ("rust-toolchain.toml", "[toolchain]\nchannel = \"1.95.0\"\n"),
            ("editors/vscode/package.json", "{\"version\":\"0.11.0\"}\n"),
            ("editors/vscode/package-lock.json", "{}\n"),
        ] {
            let file = fixture.root.join(path);
            fs::create_dir_all(file.parent().ok_or("fixture path has no parent")?)
                .map_err(|e| e.to_string())?;
            fs::write(file, text).map_err(|e| e.to_string())?;
        }
        fixture.git(&["add", "."])?;
        fixture.git(&[
            "-c",
            "user.name=Rehearsal Test",
            "-c",
            "user.email=rehearsal@example.invalid",
            "commit",
            "-qm",
            "fixture",
        ])?;
        std::env::set_current_dir(&fixture.root).map_err(|e| e.to_string())?;
        Ok(fixture)
    }

    fn git(&self, args: &[&str]) -> Result<String, String> {
        let args = args.iter().map(|v| (*v).to_string()).collect::<Vec<_>>();
        let output = crate::run::capture_output_in_dir(
            "git",
            &args,
            &self.root,
            "rehearsal identity fixture",
        )?;
        if output.status.success() {
            Ok(output.stdout.trim().to_string())
        } else {
            Err(format!("fixture git failed: {}", output.stderr))
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.previous);
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn rehearsal_identity_rejects_index_masks_and_uses_committed_bytes() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let head = git_rev_parse("HEAD")?;
    let identity = collect_identity_inputs_for_commit(&fixture.root, &head)?;
    assert!(git_worktree_clean()?);
    for flag in ["assume-unchanged", "skip-worktree"] {
        fixture.git(&["update-index", &format!("--{flag}"), "Cargo.lock"])?;
        fs::write(fixture.root.join("Cargo.lock"), "different-lock\n")
            .map_err(|e| e.to_string())?;
        assert!(
            fixture
                .git(&[
                    "status",
                    "--porcelain",
                    "--untracked-files=all",
                    "--ignore-submodules=none"
                ])?
                .is_empty()
        );
        assert!(git_worktree_clean().is_err(), "accepted index mask {flag}");
        std::env::set_current_dir(fixture.root.join("crates/ripr")).map_err(|e| e.to_string())?;
        assert!(
            git_worktree_clean().is_err(),
            "member invocation accepted root index mask {flag}"
        );
        assert!(
            super::rehearsal_plan::run(&["--sha", "HEAD", "--plan", "--json"].map(str::to_string))
                .is_err()
        );
        std::env::set_current_dir(&fixture.root).map_err(|e| e.to_string())?;
        assert_eq!(
            collect_identity_inputs_for_commit(&fixture.root, &head)?,
            identity
        );
        assert!(
            super::rehearsal_plan::run(&["--sha", "HEAD", "--plan", "--json"].map(str::to_string))
                .is_err()
        );
        fixture.git(&["update-index", &format!("--no-{flag}"), "Cargo.lock"])?;
        fs::write(fixture.root.join("Cargo.lock"), "committed-lock\n")
            .map_err(|e| e.to_string())?;
    }
    assert!(git_worktree_clean()?);
    Ok(())
}

#[cfg(unix)]
#[test]
fn rehearsal_identity_does_not_execute_configured_helpers() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let monitor_marker = fixture.root.join(".git/fsmonitor-marker");
    fs::write(
        fixture.root.join(".git/fsmonitor.sh"),
        "#!/bin/sh\nprintf helper > .git/fsmonitor-marker\nprintf 'token\\0'\n",
    )
    .map_err(|e| e.to_string())?;
    fixture.git(&["config", "core.fsmonitor", "sh .git/fsmonitor.sh"])?;
    assert!(git_worktree_clean()?);
    assert!(!monitor_marker.exists(), "planning executed fsmonitor");
    fixture.git(&["config", "--unset", "core.fsmonitor"])?;
    fs::write(
        fixture.root.join(".gitattributes"),
        "Cargo.lock filter=rehearsal\n",
    )
    .map_err(|e| e.to_string())?;
    fixture.git(&["add", ".gitattributes"])?;
    fixture.git(&[
        "-c",
        "user.name=Rehearsal Test",
        "-c",
        "user.email=rehearsal@example.invalid",
        "commit",
        "-qm",
        "attributes",
    ])?;
    for key in ["filter.rehearsal.clean", "filter.rehearsal.process"] {
        fixture.git(&["config", key, "sh .git/clean.sh"])?;
        fs::write(
            fixture.root.join(".git/clean.sh"),
            "#!/bin/sh\nprintf helper > .git/clean-marker\ncat\n",
        )
        .map_err(|e| e.to_string())?;
        fs::write(fixture.root.join("Cargo.lock"), "changed-lock\n").map_err(|e| e.to_string())?;
        assert!(git_worktree_clean().is_err(), "accepted helper {key}");
        assert!(
            !fixture.root.join(".git/clean-marker").exists(),
            "planning executed {key}"
        );
        fixture.git(&["config", "--unset", key])?;
    }
    Ok(())
}

#[test]
fn rehearsal_identity_rejects_gitlinks_before_recursive_status() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let head = git_rev_parse("HEAD")?;
    fixture.git(&[
        "update-index",
        "--add",
        "--cacheinfo",
        &format!("160000,{head},nested"),
    ])?;
    let error = git_worktree_clean().err().ok_or("accepted a gitlink")?;
    assert!(error.contains("gitlink"));
    Ok(())
}

#[test]
fn rehearsal_identity_ignores_replacement_objects() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let head = git_rev_parse("HEAD")?;
    let identity = collect_identity_inputs_for_commit(&fixture.root, &head)?;
    let original = fixture.git(&["rev-parse", "HEAD:Cargo.lock"])?;
    fs::write(fixture.root.join("Cargo.lock"), "replacement-lock\n").map_err(|e| e.to_string())?;
    let replacement = fixture.git(&["hash-object", "-w", "Cargo.lock"])?;
    fixture.git(&["replace", &original, &replacement])?;
    assert_eq!(
        fixture.git(&["show", "HEAD:Cargo.lock"])?,
        "replacement-lock"
    );
    assert_eq!(
        collect_identity_inputs_for_commit(&fixture.root, &head)?,
        identity
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn rehearsal_identity_missing_blob_cannot_invoke_promisor_transport() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let head = git_rev_parse("HEAD")?;
    let blob = fixture.git(&["rev-parse", "HEAD:Cargo.lock"])?;
    fixture.git(&["config", "remote.origin.url", "ext::sh .git/transport.sh"])?;
    fixture.git(&["config", "remote.origin.promisor", "true"])?;
    fixture.git(&["config", "protocol.ext.allow", "always"])?;
    fs::write(
        fixture.root.join(".git/transport.sh"),
        "#!/bin/sh\nprintf helper > .git/transport-marker\nexit 1\n",
    )
    .map_err(|e| e.to_string())?;
    fs::remove_file(
        fixture
            .root
            .join(".git/objects")
            .join(&blob[..2])
            .join(&blob[2..]),
    )
    .map_err(|e| e.to_string())?;
    assert!(collect_identity_inputs_for_commit(&fixture.root, &head).is_err());
    assert!(
        !fixture.root.join(".git/transport-marker").exists(),
        "identity read invoked transport"
    );
    Ok(())
}

#[test]
fn rehearsal_identity_rejects_same_size_preserved_mtime_changes() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let path = fixture.root.join("Cargo.lock");
    let original = fs::read(&path).map_err(|e| e.to_string())?;
    let old = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_200_000_000);
    let set_time = || -> Result<(), String> {
        fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .map_err(|e| e.to_string())?
            .set_times(fs::FileTimes::new().set_modified(old))
            .map_err(|e| e.to_string())
    };
    set_time()?;
    fixture.git(&["update-index", "--refresh"])?;
    fixture.git(&["config", "core.trustctime", "false"])?;
    let mut changed = original.clone();
    let first = changed.first_mut().ok_or("empty fixture lockfile")?;
    *first = if *first == b'!' { b'#' } else { b'!' };
    fs::write(&path, changed).map_err(|e| e.to_string())?;
    set_time()?;
    assert!(
        fixture
            .git(&[
                "status",
                "--porcelain",
                "--untracked-files=all",
                "--ignore-submodules=none"
            ])?
            .is_empty()
    );
    assert!(
        !git_worktree_clean()?,
        "accepted changed bytes hidden by restored mtime"
    );
    assert!(
        super::rehearsal_plan::run(&["--sha", "HEAD", "--plan", "--json"].map(str::to_string))
            .is_err()
    );
    Ok(())
}

#[test]
fn rehearsal_identity_trace_environment_does_not_write() -> Result<(), String> {
    if std::env::var_os("RIPR_REHEARSAL_TRACE_CHILD").is_some() {
        let _guard = crate::acquire_test_cwd_read_guard();
        assert!(git_worktree_clean()?);
        return Ok(());
    }
    let fixture = Fixture::new()?;
    let config = fixture.root.join(".git/trace-config");
    let config_text = config.to_str().ok_or("trace config path is not UTF-8")?;
    let mut markers = Vec::new();
    for key in ["normalTarget", "eventTarget", "perfTarget"] {
        let path = fixture.root.join(format!(".git/trace-config-{key}"));
        fixture.git(&[
            "config",
            "--file",
            config_text,
            &format!("trace2.{key}"),
            path.to_str().ok_or("trace path is not UTF-8")?,
        ])?;
        markers.push(path);
    }
    let base = vec![
        ("RIPR_REHEARSAL_TRACE_CHILD".to_string(), "1".to_string()),
        ("GIT_CONFIG_GLOBAL".to_string(), config_text.to_string()),
        ("GIT_CONFIG_NOSYSTEM".to_string(), "1".to_string()),
    ];
    let mut traced = base.clone();
    for key in [
        "GIT_TRACE",
        "GIT_TRACE_SETUP",
        "GIT_TRACE_PERFORMANCE",
        "GIT_TRACE2",
        "GIT_TRACE2_EVENT",
        "GIT_TRACE2_PERF",
        "GIT_TRACE_PACK_ACCESS",
        "GIT_TRACE_PACKET",
        "GIT_TRACE_CURL",
        "GIT_REDIRECT_STDOUT",
        "GIT_REDIRECT_STDERR",
        "GIT_REDIRECT_STDIN",
    ] {
        let path = fixture.root.join(format!(".git/trace-env-{key}"));
        traced.push((
            key.to_string(),
            path.to_str().ok_or("trace path is not UTF-8")?.to_string(),
        ));
        markers.push(path);
    }
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let args = [
        "--exact",
        "readiness_snapshot::identity_tests::rehearsal_identity_trace_environment_does_not_write",
        "--nocapture",
    ]
    .map(str::to_string);
    // Exercise both inherited environment targets and configuration-only fallback.
    for environment in [&traced, &base] {
        let envs = environment
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect::<Vec<_>>();
        let output = crate::run::capture_output_in_dir_with_envs(
            executable.to_str().ok_or("test executable is not UTF-8")?,
            &args,
            &fixture.root,
            "trace guard child",
            &envs,
            &[],
        )?;
        assert!(
            output.status.success(),
            "{} {}",
            output.stdout,
            output.stderr
        );
        assert!(
            output.stdout.contains("1 passed"),
            "trace child selected no test"
        );
        assert!(
            markers.iter().all(|path| !path.exists()),
            "Git probe wrote a trace/redirection target"
        );
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn rehearsal_identity_checks_owner_execute_mode_without_stat_trust() -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new()?;
    fixture.git(&["config", "core.filemode", "false"])?;
    fs::set_permissions(
        fixture.root.join("Cargo.lock"),
        fs::Permissions::from_mode(0o744),
    )
    .map_err(|e| e.to_string())?;
    assert!(fixture.git(&["status", "--porcelain"])?.is_empty());
    assert!(!git_worktree_clean()?);
    Ok(())
}

#[test]
fn rehearsal_identity_hashes_every_batch() -> Result<(), String> {
    let fixture = Fixture::new()?;
    for index in 0..130 {
        fs::write(
            fixture.root.join(format!("batch-{index:03}")),
            format!("source-{index}\n"),
        )
        .map_err(|e| e.to_string())?;
    }
    fixture.git(&["add", "."])?;
    fixture.git(&[
        "-c",
        "user.name=Rehearsal Test",
        "-c",
        "user.email=rehearsal@example.invalid",
        "commit",
        "-qm",
        "batch fixture",
    ])?;
    let head = git_rev_parse("HEAD")?;
    assert!(super::source_tree::tracked_bytes_match(
        &fixture.root,
        &head
    )?);
    fs::write(fixture.root.join("batch-129"), "changed\n").map_err(|e| e.to_string())?;
    assert!(!super::source_tree::tracked_bytes_match(
        &fixture.root,
        &head
    )?);
    Ok(())
}

#[cfg(unix)]
#[test]
fn rehearsal_identity_rejects_symlink_parents() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let original = fixture.root.join("crates/ripr");
    let moved = fixture.root.join(".git/source-dir");
    fs::rename(&original, &moved).map_err(|e| e.to_string())?;
    std::os::unix::fs::symlink(&moved, &original).map_err(|e| e.to_string())?;
    let error = git_worktree_clean()
        .err()
        .ok_or("accepted a symlink parent")?;
    assert!(error.contains("symlink"));
    Ok(())
}

#[cfg(unix)]
#[test]
fn rehearsal_identity_refuses_split_index_without_freshening_it() -> Result<(), String> {
    let fixture = Fixture::new()?;
    fixture.git(&["update-index", "--split-index"])?;
    let shared = PathBuf::from(fixture.git(&["rev-parse", "--shared-index-path"])?);
    let shared = if shared.is_absolute() {
        shared
    } else {
        fixture.root.join(shared)
    };
    let old = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_200_000_000);
    fs::File::open(&shared)
        .map_err(|e| e.to_string())?
        .set_times(fs::FileTimes::new().set_modified(old))
        .map_err(|e| e.to_string())?;
    assert!(git_worktree_clean().is_err());
    assert_eq!(
        fs::metadata(&shared)
            .map_err(|e| e.to_string())?
            .modified()
            .map_err(|e| e.to_string())?,
        old
    );
    Ok(())
}

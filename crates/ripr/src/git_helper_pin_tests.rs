//! #14242: programs named by a repository's own git configuration must not
//! run during ripr's read-only git calls. Each case plants the helper, shows
//! plain git runs it (so the fixture is a real stimulus), then shows the
//! shared `git_command` path does not.

use super::*;
use std::os::unix::fs::PermissionsExt;

struct Repo {
    root: PathBuf,
    /// Outside the work tree, so the marker never shows up in `git status`.
    marker: PathBuf,
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
        let _ = std::fs::remove_file(&self.marker);
    }
}

fn plain_git(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .output()
        .map_err(|err| format!("git {args:?}: {err}"))?;
    if !output.status.success() {
        return Err(format!("git {args:?} failed: {output:?}"));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn repo(label: &str) -> Result<Repo, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    let name = format!("ripr-git-helper-{label}-{}-{stamp}", std::process::id());
    let repo = Repo {
        root: std::env::temp_dir().join(&name),
        marker: std::env::temp_dir().join(format!("{name}.ran")),
    };
    std::fs::create_dir_all(&repo.root).map_err(|err| format!("create root: {err}"))?;
    plain_git(&repo.root, &["init", "-q"])?;
    std::fs::write(repo.root.join("lib.rs"), "fn a() {}\n").map_err(|err| err.to_string())?;
    plain_git(&repo.root, &["add", "lib.rs"])?;
    plain_git(
        &repo.root,
        &[
            "-c",
            "user.name=ripr",
            "-c",
            "user.email=ripr@example.invalid",
            "-c",
            "commit.gpgSign=false",
            "commit",
            "-qm",
            "seed",
        ],
    )?;
    Ok(repo)
}

/// Plant `filter.<driver>.clean` on `lib.rs` and leave the file stat-dirty at
/// its committed size, so git must run the clean filter to compare it.
fn plant_clean_filter(repo: &Repo, driver: &str) -> Result<(), String> {
    let program = format!("touch '{}'; cat", repo.marker.display());
    plain_git(
        &repo.root,
        &["config", &format!("filter.{driver}.clean"), &program],
    )?;
    std::fs::create_dir_all(repo.root.join(".git/info")).map_err(|err| err.to_string())?;
    std::fs::write(
        repo.root.join(".git/info/attributes"),
        format!("lib.rs filter={driver}\n"),
    )
    .map_err(|err| err.to_string())?;
    std::fs::write(repo.root.join("lib.rs"), "fn b() {}\n").map_err(|err| err.to_string())
}

fn take_marker(repo: &Repo) -> bool {
    let ran = repo.marker.exists();
    let _ = std::fs::remove_file(&repo.marker);
    ran
}

#[test]
fn repository_clean_filter_program_does_not_run() -> Result<(), String> {
    let repo = repo("clean")?;
    plant_clean_filter(&repo, "planted")?;

    for args in [
        vec!["status", "--porcelain"],
        vec!["ls-files", "-m"],
        vec!["diff", "--name-only", "HEAD"],
    ] {
        plain_git(&repo.root, &args)?;
        assert!(
            take_marker(&repo),
            "control: plain git {args:?} runs the filter"
        );
        let listed = run_git(&repo.root, &args)?;
        assert!(!take_marker(&repo), "ripr git {args:?} ran the filter");
        assert!(
            listed.contains("lib.rs"),
            "{args:?} lost the edit: {listed}"
        );
    }
    Ok(())
}

/// The pins are cached per root; a driver planted after the first call on
/// that root is still seen, because `.git/config` changed.
#[test]
fn clean_filter_planted_after_the_first_call_does_not_run() -> Result<(), String> {
    let repo = repo("late")?;
    run_git(&repo.root, &["status", "--porcelain"])?;
    plant_clean_filter(&repo, "late")?;
    let status = run_git(&repo.root, &["status", "--porcelain"])?;
    assert!(!take_marker(&repo), "ripr git status ran the late filter");
    assert!(status.contains("lib.rs"), "{status}");
    Ok(())
}

/// `git -c` cannot spell a key whose driver name contains `=`, so the call is
/// refused before git reads the worktree.
#[test]
fn clean_filter_with_an_unspellable_driver_name_is_refused() -> Result<(), String> {
    let repo = repo("equals")?;
    plant_clean_filter(&repo, "a=b")?;
    plain_git(&repo.root, &["status", "--porcelain"])?;
    assert!(take_marker(&repo), "control: plain git runs the filter");

    let error = match run_git(&repo.root, &["status", "--porcelain"]) {
        Ok(status) => return Err(format!("status was not refused: {status}")),
        Err(error) => error.to_string(),
    };
    assert!(!take_marker(&repo), "refused call still ran the filter");
    assert!(error.contains(REFUSED_FILTER_CONFIG_FLAG), "{error}");
    Ok(())
}

#[test]
fn repository_gpg_program_does_not_run_on_log() -> Result<(), String> {
    let repo = repo("gpg")?;
    let program = repo.root.join(".git/planted-gpg");
    std::fs::write(
        &program,
        format!("#!/bin/sh\ntouch '{}'\nexit 1\n", repo.marker.display()),
    )
    .map_err(|err| err.to_string())?;
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755))
        .map_err(|err| err.to_string())?;

    // A commit object with a `gpgsig` header: git verifies it by running
    // `gpg.program` whenever signatures are shown.
    let tree = plain_git(&repo.root, &["rev-parse", "HEAD^{tree}"])?;
    let parent = plain_git(&repo.root, &["rev-parse", "HEAD"])?;
    let object = repo.root.join(".git/planted-commit");
    std::fs::write(
        &object,
        format!(
            "tree {tree}\nparent {parent}\nauthor r <r@example.invalid> 1 +0000\n\
             committer r <r@example.invalid> 1 +0000\n\
             gpgsig -----BEGIN PGP SIGNATURE-----\n \n iQEzBAABCAAdFiEE\n \
             -----END PGP SIGNATURE-----\n\nsigned\n"
        ),
    )
    .map_err(|err| err.to_string())?;
    let commit = plain_git(
        &repo.root,
        &["hash-object", "-t", "commit", "-w", ".git/planted-commit"],
    )?;
    plain_git(&repo.root, &["update-ref", "HEAD", &commit])?;
    plain_git(&repo.root, &["config", "log.showSignature", "true"])?;
    let program = program.display().to_string();
    plain_git(&repo.root, &["config", "gpg.program", &program])?;

    let args = ["log", "-1", "--format=%H", "HEAD"];
    plain_git(&repo.root, &args)?;
    assert!(
        take_marker(&repo),
        "control: plain git log runs gpg.program"
    );
    let logged = run_git(&repo.root, &args)?;
    assert!(!take_marker(&repo), "ripr git log ran gpg.program");
    assert_eq!(logged, commit);
    Ok(())
}

fn listing(entries: &[(&str, &str, &str)]) -> Vec<u8> {
    let mut out = Vec::new();
    for (scope, key, value) in entries {
        out.extend_from_slice(scope.as_bytes());
        out.push(0);
        out.extend_from_slice(key.as_bytes());
        out.push(b'\n');
        out.extend_from_slice(value.as_bytes());
        out.push(0);
    }
    out
}

#[test]
fn user_scope_filter_definitions_survive_and_repository_ones_are_reset() {
    let stdout = listing(&[
        ("system", "filter.lfs.clean", "git-lfs clean -- %f"),
        ("global", "filter.lfs.process", "git-lfs filter-process"),
        ("global", "filter.lfs.required", "true"),
        ("local", "filter.lfs.clean", "planted-clean"),
        ("local", "filter.lfs.required", "true"),
        ("local", "filter.planted.process", "planted-process"),
        ("worktree", "filter.planted.required", "true"),
        ("command", "filter.cmd.smudge", "from-command-line"),
    ]);
    assert_eq!(
        filter_pins_from_listing(&stdout, true),
        RepositoryFilterPins::Overrides(vec![
            "filter.lfs.clean=git-lfs clean -- %f".to_string(),
            "filter.lfs.required=true".to_string(),
            "filter.planted.process=".to_string(),
            "filter.planted.required=false".to_string(),
        ])
    );
}

#[test]
fn listing_edge_shapes_fail_closed() {
    assert_eq!(
        filter_pins_from_listing(b"", true),
        RepositoryFilterPins::Overrides(Vec::new())
    );
    // An unknown scope is the repository's.
    assert_eq!(
        filter_pins_from_listing(&listing(&[("mystery", "filter.x.clean", "run")]), true),
        RepositoryFilterPins::Overrides(vec!["filter.x.clean=".to_string()])
    );
    // A scope with no entry after it is a truncated listing.
    assert_eq!(
        filter_pins_from_listing(b"local\0", true),
        RepositoryFilterPins::Refused
    );
    assert_eq!(
        filter_pins_from_listing(&listing(&[("local", "filter.a=b.clean", "run")]), true),
        RepositoryFilterPins::Refused
    );
    // A key that is not UTF-8 cannot be passed back as an argument.
    assert_eq!(
        filter_pins_from_listing(b"local\0filter.\xff.clean\nrun\0", true),
        RepositoryFilterPins::Refused
    );
    // Git without `--show-scope`: every definition is reset.
    assert_eq!(
        filter_pins_from_listing(
            b"filter.lfs.clean\ngit-lfs clean\0filter.lfs.required\0",
            false
        ),
        RepositoryFilterPins::Overrides(vec![
            "filter.lfs.clean=".to_string(),
            "filter.lfs.required=false".to_string(),
        ])
    );
}

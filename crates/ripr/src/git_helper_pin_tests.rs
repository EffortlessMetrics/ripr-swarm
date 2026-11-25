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
        // A failing assertion keeps the fixture for inspection.
        if !std::thread::panicking() {
            let _ = std::fs::remove_dir_all(&self.root);
            let _ = std::fs::remove_file(&self.marker);
        }
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

/// The listing is read for every call, so a driver planted after the first
/// call on that root is still reset.
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
    let entries: Vec<_> = entries
        .iter()
        .map(|(scope, key, value)| (*scope, *key, Some(*value)))
        .collect();
    listing_with_valueless(&entries)
}

/// `git config --show-scope -z` output; a `None` value is a key written with
/// no value, which git prints with no line feed after the key.
fn listing_with_valueless(entries: &[(&str, &str, Option<&str>)]) -> Vec<u8> {
    let mut out = Vec::new();
    for (scope, key, value) in entries {
        out.extend_from_slice(scope.as_bytes());
        out.push(0);
        out.extend_from_slice(key.as_bytes());
        if let Some(value) = value {
            out.push(b'\n');
            out.extend_from_slice(value.as_bytes());
        }
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

#[test]
fn probe_exit_states_fail_closed_when_the_listing_is_unknown() {
    let empty = RepositoryFilterPins::Overrides(Vec::new());
    let one = listing(&[("local", "filter.x.clean", "run")]);
    assert_eq!(
        filter_pins_for_probe_exit(Some(0), &one, true),
        Some(RepositoryFilterPins::Overrides(vec![
            "filter.x.clean=".to_string()
        ]))
    );
    assert_eq!(
        filter_pins_for_probe_exit(Some(1), b"", true),
        Some(empty.clone())
    );
    // Old git: retry unscoped once; the unscoped answer is final.
    assert_eq!(filter_pins_for_probe_exit(Some(129), b"", true), None);
    assert_eq!(
        filter_pins_for_probe_exit(Some(129), b"", false),
        Some(empty.clone())
    );
    assert_eq!(
        filter_pins_for_probe_exit(Some(128), b"", true),
        Some(empty)
    );
    // Ended by a signal: no exit code, so nothing is known about the listing.
    for with_scope in [true, false] {
        assert_eq!(
            filter_pins_for_probe_exit(None, &one, with_scope),
            Some(RepositoryFilterPins::Refused)
        );
    }
}

/// A valueless boolean is true to git and an empty one is false, so a
/// trusted valueless `required` must come back as `true`.
#[test]
fn trusted_valueless_required_stays_required() {
    let stdout = listing_with_valueless(&[
        ("global", "filter.x.clean", Some("trusted-clean")),
        ("global", "filter.x.required", None),
        ("local", "filter.x.clean", Some("planted-clean")),
        ("local", "filter.x.required", Some("false")),
        ("local", "filter.y.required", None),
        ("local", "filter.y.clean", None),
    ]);
    assert_eq!(
        filter_pins_from_listing(&stdout, true),
        RepositoryFilterPins::Overrides(vec![
            "filter.x.clean=trusted-clean".to_string(),
            "filter.x.required=true".to_string(),
            "filter.y.clean=".to_string(),
            "filter.y.required=false".to_string(),
        ])
    );
}

/// Real git: a trusted clean filter that fails and is marked required by a
/// valueless key must fail ripr's call the way it fails plain git. The pin
/// the listing used to produce (`required=`) hashes raw content instead.
#[test]
fn trusted_valueless_required_filter_still_refuses_raw_content() -> Result<(), String> {
    let repo = repo("valueless")?;
    std::fs::create_dir_all(repo.root.join(".git/info")).map_err(|err| err.to_string())?;
    std::fs::write(
        repo.root.join(".git/info/attributes"),
        "lib.rs filter=strict\n",
    )
    .map_err(|err| err.to_string())?;
    // The user's own definition, read through `GIT_CONFIG_GLOBAL`.
    let global = repo.marker.with_extension("gitconfig");
    std::fs::write(
        &global,
        "[filter \"strict\"]\n\tclean = false\n\trequired\n",
    )
    .map_err(|err| err.to_string())?;
    plain_git(&repo.root, &["config", "filter.strict.required", "false"])?;

    let with_global = |mut command: Command| -> Result<Output, String> {
        command
            .env("GIT_CONFIG_GLOBAL", &global)
            .stdin(Stdio::null())
            .output()
            .map_err(|err| err.to_string())
    };
    let listed = with_global(fixture_git_command(
        &repo.root,
        &[
            "config",
            "--show-scope",
            "--includes",
            "-z",
            "--get-regexp",
            FILTER_CONFIG_PATTERN,
        ],
    ))?;
    let pins = filter_pins_from_listing(&listed.stdout, true);
    let args = ["hash-object", "--path=lib.rs", "--stdin"];
    let pinned = with_global(git_command_with_filter_pins(&repo.root, &args, &pins))?;
    let emptied = with_global(git_command_with_filter_pins(
        &repo.root,
        &args,
        &RepositoryFilterPins::Overrides(vec!["filter.strict.required=".to_string()]),
    ))?;
    let _ = std::fs::remove_file(&global);

    assert_eq!(
        pins,
        RepositoryFilterPins::Overrides(vec!["filter.strict.required=true".to_string()]),
        "listing: {:?}",
        String::from_utf8_lossy(&listed.stdout)
    );
    assert!(
        emptied.status.success(),
        "control: an empty `required` hashes raw content: {emptied:?}"
    );
    assert!(
        !pinned.status.success(),
        "the trusted required filter was made optional: {pinned:?}"
    );
    Ok(())
}

/// `GIT_CONFIG` redirects only `git config`, so an inherited value would
/// blind the listing while other calls still read the repository.
#[test]
fn inherited_git_config_selector_is_removed_from_every_call() {
    let command = fixture_git_command(Path::new("."), &["status"]);
    let removed = command
        .get_envs()
        .any(|(key, value)| key == "GIT_CONFIG" && value.is_none());
    assert!(removed, "GIT_CONFIG must be removed from the child");
}

/// A filter that arrives through an included file leaves `.git/config`
/// untouched; the next call must still reset it.
#[test]
fn clean_filter_added_to_an_included_file_does_not_run() -> Result<(), String> {
    let repo = repo("include")?;
    let included = repo.root.join(".git").join("extra.config");
    std::fs::write(&included, "").map_err(|err| err.to_string())?;
    plain_git(&repo.root, &["config", "include.path", "extra.config"])?;
    run_git(&repo.root, &["status", "--porcelain"])?;

    std::fs::create_dir_all(repo.root.join(".git/info")).map_err(|err| err.to_string())?;
    std::fs::write(
        repo.root.join(".git/info/attributes"),
        "lib.rs filter=inc\n",
    )
    .map_err(|err| err.to_string())?;
    std::fs::write(
        &included,
        format!(
            "[filter \"inc\"]\n\tclean = \"touch '{}'; cat\"\n",
            repo.marker.display()
        ),
    )
    .map_err(|err| err.to_string())?;
    std::fs::write(repo.root.join("lib.rs"), "fn d() {}\n").map_err(|err| err.to_string())?;

    plain_git(&repo.root, &["status", "--porcelain"])?;
    assert!(
        take_marker(&repo),
        "control: plain git runs the included filter"
    );
    let status = run_git(&repo.root, &["status", "--porcelain"])?;
    assert!(
        !take_marker(&repo),
        "ripr git status ran the included filter"
    );
    assert!(status.contains("lib.rs"), "{status}");
    Ok(())
}

/// A zero deadline starts no listing; a real deadline starts one inside it.
#[test]
fn filter_listing_runs_inside_the_callers_deadline() -> Result<(), String> {
    let repo = repo("budget")?;
    let probes = || TEST_FILTER_PROBE_SPAWNS.with(std::cell::Cell::get);

    let before = (probes(), test_process_spawns());
    let zero = run_git_output_with_deadline(&repo.root, &["status"], Some(Duration::ZERO));
    assert!(zero.is_err_and(|err| err.is_git_invocation_timeout()));
    assert_eq!(
        (probes(), test_process_spawns()),
        before,
        "a zero deadline spawned a process"
    );

    let (_, remaining) = git_command_within(&repo.root, &["status"], Some(Duration::from_secs(30)));
    assert_eq!(probes(), before.0 + 1, "a real deadline lists once");
    assert!(remaining.is_some_and(|left| left < Duration::from_secs(30)));

    // A budget too small for any listing is refused, never unpinned.
    assert_eq!(
        probe_repository_filter_pins(&repo.root, Some(Duration::from_nanos(1))),
        RepositoryFilterPins::Refused
    );
    Ok(())
}

/// #15101: a status or diff call that names no submodule option still keeps
/// git out of a submodule's working tree; an explicit caller choice, a path
/// after `--`, and other subcommands are left as given.
#[test]
fn submodule_examining_calls_get_pointers_only() {
    let cases: [(&[&str], &[&str]); 7] = [
        (
            &["status", "--porcelain", "--", "."],
            &["status", SUBMODULE_POINTERS_ONLY, "--porcelain", "--", "."],
        ),
        (
            &["-c", "core.quotePath=true", "diff", "HEAD"],
            &[
                "-c",
                "core.quotePath=true",
                "diff",
                SUBMODULE_POINTERS_ONLY,
                "HEAD",
            ],
        ),
        (
            &["--literal-pathspecs", "diff-files", "-z"],
            &[
                "--literal-pathspecs",
                "diff-files",
                SUBMODULE_POINTERS_ONLY,
                "-z",
            ],
        ),
        (
            &["diff-index", "HEAD", "--", "--ignore-submodules=all"],
            &[
                "diff-index",
                SUBMODULE_POINTERS_ONLY,
                "HEAD",
                "--",
                "--ignore-submodules=all",
            ],
        ),
        (
            &["diff", "--ignore-submodules=all", "HEAD"],
            &["diff", "--ignore-submodules=all", "HEAD"],
        ),
        (&["log", "-1", "HEAD"], &["log", "-1", "HEAD"]),
        (&["-C", "status", "ls-files"], &["-C", "status", "ls-files"]),
    ];
    for (args, expected) in cases {
        assert_eq!(with_submodule_pointers_only(args), expected, "{args:?}");
    }
}

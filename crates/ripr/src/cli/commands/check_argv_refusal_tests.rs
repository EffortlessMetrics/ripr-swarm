//! Argument-validation refusals, exit-code decisions and error messages of
//! `ripr check` that no lib test pinned: the fan-out/`--committed`/`--path`/
//! `--write-artifact`/`--gap-ledger` conflict matrix, the timeout environment
//! parsers, help detection and the machine refusal documents. Process-level
//! stderr/exit behavior stays with the `tests/` integration suites.

use super::super::tests::{args, unique_command_test_dir};
use super::*;
use crate::testing::{cwd_lock, fixture_git};

struct TempRoot(PathBuf);

impl TempRoot {
    fn new(label: &str) -> Result<Self, String> {
        let root = unique_command_test_dir(label);
        std::fs::create_dir_all(root.join("src")).map_err(|err| err.to_string())?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"argv_refusal_probe\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .map_err(|err| err.to_string())?;
        std::fs::write(root.join("src/lib.rs"), "pub fn one() -> u8 { 1 }\n")
            .map_err(|err| err.to_string())?;
        Ok(Self(root))
    }

    fn arg(&self) -> String {
        self.0.to_string_lossy().into_owned()
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct CwdRestore(PathBuf);

impl Drop for CwdRestore {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.0);
    }
}

/// Run `check --root <root> <extra...>` and return its refusal text.
fn refusal(root: &TempRoot, extra: &[&str]) -> Result<String, String> {
    let mut argv = vec!["--root".to_string(), root.arg()];
    argv.extend(extra.iter().map(|value| value.to_string()));
    match check(&argv) {
        Err(message) => Ok(message),
        Ok(()) => Err(format!("{extra:?} must be refused, but check succeeded")),
    }
}

#[test]
fn output_dir_is_refused_when_named_twice() {
    assert_eq!(
        check(&args(&["--output-dir", "first", "--output-dir", "second"])),
        Err("pass --output-dir once".to_string())
    );
}

#[test]
fn output_dir_refuses_every_conflicting_interface_before_touching_disk() -> Result<(), String> {
    let destination = unique_command_test_dir("fanout-conflict");
    let destination_arg = destination.to_string_lossy().into_owned();
    let cases: [&[&str]; 12] = [
        &[],
        &["--diff", "-"],
        &["--diff", "change.diff", "--json"],
        &["--diff", "change.diff", "--format", "human"],
        &["--diff", "change.diff", "--worktree"],
        &["--diff", "change.diff", "--committed"],
        &["--diff", "change.diff", "--candidate-tree", "abc"],
        &["--diff", "change.diff", "--candidate-base", "main"],
        &["--diff", "change.diff", "--gap-ledger", "ledger.json"],
        &["--diff", "change.diff", "--write-artifact", "saved.json"],
        &[
            "--diff",
            "change.diff",
            "--suppression-policy",
            "policy.toml",
        ],
        &["--diff", "-", "--json"],
    ];
    for extra in cases {
        let mut argv = vec!["--output-dir".to_string(), destination_arg.clone()];
        argv.extend(extra.iter().map(|value| value.to_string()));
        let Err(error) = check(&argv) else {
            return Err(format!("{extra:?} with --output-dir must be refused"));
        };
        let want = "--output-dir requires a named --diff file and cannot be combined with \
                    --format/--json, --worktree/--committed, --candidate-tree/--candidate-base, \
                    --gap-ledger, --write-artifact or --suppression-policy";
        if error != want {
            return Err(format!(
                "{extra:?} refusal must name the conflict, got {error}"
            ));
        }
    }
    if destination.exists() {
        return Err("a refused fan-out must not create its destination".to_string());
    }
    Ok(())
}

#[test]
fn explicit_root_that_is_not_a_directory_is_refused_by_name() -> Result<(), String> {
    let root = TempRoot::new("root-is-file")?;
    let file_arg = root.0.join("src/lib.rs").to_string_lossy().into_owned();
    let missing_arg = root.0.join("no-such-dir").to_string_lossy().into_owned();
    for bad in [missing_arg, file_arg] {
        let Err(error) = check(&["--root".to_string(), bad.clone()]) else {
            return Err(format!("--root {bad} must be refused"));
        };
        if !(error.contains(&format!("check root {bad} is not a directory"))
            && error.contains("pass the directory that contains the workspace")
            && !error.contains("failed to run"))
        {
            return Err(format!("root refusal must name the path, got {error}"));
        }
    }
    Ok(())
}

#[test]
fn committed_refuses_each_conflicting_diff_source() -> Result<(), String> {
    let root = TempRoot::new("committed-conflicts")?;
    let cases: [(&[&str], &str); 3] = [
        (
            &["--committed", "--worktree"],
            "check --committed and --worktree select different diff sources; pass one",
        ),
        (
            &["--committed", "--diff", "change.diff"],
            "check --committed cannot be combined with --diff",
        ),
        (
            &[
                "--committed",
                "--candidate-tree",
                "0000000000000000000000000000000000000000",
            ],
            "check --committed cannot be combined with --candidate-tree",
        ),
    ];
    for (extra, want) in cases {
        let error = refusal(&root, extra)?;
        if error != want {
            return Err(format!("{extra:?}: expected `{want}`, got `{error}`"));
        }
    }
    Ok(())
}

#[test]
fn committed_refuses_repo_scoped_formats_and_gap_ledger_after_config_load() -> Result<(), String> {
    let root = TempRoot::new("committed-repo-scope")?;
    let ledger_arg = root.0.join("ledger.json").to_string_lossy().into_owned();
    let error = refusal(&root, &["--committed", "--gap-ledger", &ledger_arg])?;
    let want = "check --committed selects a diff source; --gap-ledger renders a supplied ledger and reads no diff";
    if error != want {
        return Err(format!("gap-ledger conflict must be named, got {error}"));
    }
    for format in ["repo-exposure-json", "repo-badge-json"] {
        let error = refusal(&root, &["--committed", "--format", format])?;
        let want = format!(
            "check --committed selects a diff source; --format {format} is repo-scoped and reads no diff"
        );
        if error != want {
            return Err(format!("{format}: expected `{want}`, got `{error}`"));
        }
    }
    Ok(())
}

#[test]
fn path_and_exclude_are_refused_where_they_would_claim_an_unapplied_bound() -> Result<(), String> {
    let root = TempRoot::new("scope-refusals")?;
    for flag in ["--path", "--exclude"] {
        let error = refusal(
            &root,
            &[flag, "src/lib.rs", "--format", "repo-exposure-json"],
        )?;
        if !(error
            .contains("--path/--exclude narrow a diff and do not apply to the repo-scoped format")
            && error.contains("use a diff-scoped format such as --format json"))
        {
            return Err(format!(
                "{flag} on a repo format must be refused, got {error}"
            ));
        }
        for sink in [
            ["--write-artifact", "saved.json"],
            ["--gap-ledger", "ledger.json"],
        ] {
            let error = refusal(&root, &[flag, "src/lib.rs", sink[0], sink[1]])?;
            if !(error.starts_with(
                "--path/--exclude cannot be combined with --write-artifact, --gap-ledger or --output-dir",
            ) && error.contains("save the scoped result from --format json instead"))
            {
                return Err(format!(
                    "{flag} with {} must be refused, got {error}",
                    sink[0]
                ));
            }
        }
    }
    if root.0.join("saved.json").exists() {
        return Err("a refused scoped artifact must not be written".to_string());
    }
    let error = refusal(&root, &["--exclude", "../escape"])?;
    if !(error.contains("--exclude pattern \"../escape\"") && error.contains("`..` segment")) {
        return Err(format!(
            "a malformed exclude pattern must be named, got {error}"
        ));
    }
    Ok(())
}

#[test]
fn suppression_policy_is_refused_on_formats_that_cannot_apply_it() -> Result<(), String> {
    let root = TempRoot::new("suppression-refusals")?;
    for format in [
        "sarif",
        "badge-json",
        "repo-exposure-json",
        "repo-badge-json",
    ] {
        let error = refusal(
            &root,
            &["--suppression-policy", "policy.toml", "--format", format],
        )?;
        if !(error.starts_with(
            "--suppression-policy applies to the findings-based check formats (human, human-full, json, github)",
        ) && error.ends_with("it is not yet supported for SARIF, badge, or repo formats"))
        {
            return Err(format!(
                "{format}: refusal must name the limitation, got {error}"
            ));
        }
    }
    Ok(())
}

#[test]
fn write_artifact_refuses_inputs_it_cannot_record() -> Result<(), String> {
    let root = TempRoot::new("artifact-refusals")?;
    let saved = root.0.join("saved.json");
    let saved_arg = saved.to_string_lossy().into_owned();
    let shown = saved.display().to_string();
    let cases: [(Vec<&str>, String); 4] = [
        (
            vec!["--write-artifact", &saved_arg, "--diff", "-"],
            format!(
                "--write-artifact {shown} cannot be combined with --diff -: stdin bytes are not retained in the artifact identity; save stdin to a named diff file, then pass --diff <path> with --write-artifact"
            ),
        ),
        (
            vec![
                "--write-artifact",
                &saved_arg,
                "--gap-ledger",
                "ledger.json",
            ],
            format!(
                "--write-artifact {shown} cannot be combined with --gap-ledger: the artifact records a findings-based check run"
            ),
        ),
        (
            vec![
                "--write-artifact",
                &saved_arg,
                "--format",
                "repo-exposure-json",
            ],
            format!(
                "--write-artifact {shown} records a diff-scoped findings run; --format repo-exposure-json is repo-scoped and produces no such artifact"
            ),
        ),
        (
            vec![
                "--write-artifact",
                &saved_arg,
                "--format",
                "repo-badge-json",
            ],
            format!(
                "--write-artifact {shown} records a diff-scoped findings run; --format repo-badge-json is repo-scoped and produces no such artifact"
            ),
        ),
    ];
    for (extra, want) in &cases {
        let error = refusal(&root, extra)?;
        if &error != want {
            return Err(format!("{extra:?}: expected `{want}`, got `{error}`"));
        }
    }
    if saved.exists() {
        return Err("a refused artifact request must not write the artifact".to_string());
    }
    Ok(())
}

#[test]
fn gap_ledger_is_limited_to_badge_formats_and_names_unreadable_ledgers() -> Result<(), String> {
    let root = TempRoot::new("gap-ledger")?;
    let ledger = root.0.join("ledger.json");
    let ledger_arg = ledger.to_string_lossy().into_owned();
    let want = "check --gap-ledger is only supported with repo-badge-* formats";
    let error = refusal(&root, &["--gap-ledger", &ledger_arg, "--format", "json"])?;
    if error != want {
        return Err(format!("non-badge format must be refused, got {error}"));
    }
    let error = refusal(&root, &["--gap-ledger", &ledger_arg])?;
    if error != want {
        return Err(format!("the default format must be refused, got {error}"));
    }
    let error = refusal(
        &root,
        &["--gap-ledger", &ledger_arg, "--format", "repo-badge-json"],
    )?;
    if !error.starts_with(&format!("failed to read gap ledger {}", ledger.display())) {
        return Err(format!("a missing ledger must be named, got {error}"));
    }
    std::fs::write(&ledger, "not json").map_err(|err| err.to_string())?;
    let error = refusal(
        &root,
        &[
            "--gap-ledger",
            &ledger_arg,
            "--format",
            "repo-badge-shields",
        ],
    )?;
    if error.is_empty() || error == want {
        return Err(format!(
            "a malformed ledger needs its own message, got {error:?}"
        ));
    }
    Ok(())
}

#[test]
fn repo_scope_diff_input_must_be_a_readable_file() -> Result<(), String> {
    let root = TempRoot::new("repo-scope-diff")?;
    let missing_diff = root.0.join("missing.diff");
    let missing_arg = missing_diff.to_string_lossy().into_owned();
    let error = refusal(
        &root,
        &["--format", "repo-exposure-json", "--diff", &missing_arg],
    )?;
    if !(error.contains(&format!(
        "check: --diff {} is not a readable file",
        missing_diff.display()
    )) && error.contains("or omit --diff for repo-scoped analysis"))
    {
        return Err(format!("a missing --diff must be named, got {error}"));
    }
    Ok(())
}

#[test]
fn repo_scope_validation_decides_per_input_shape() -> Result<(), String> {
    let root = TempRoot::new("repo-scope-shapes")?;
    let mut input = CheckInput {
        root: root.0.clone(),
        diff_file: Some(PathBuf::from("-")),
        ..CheckInput::default()
    };
    // The stdin sentinel is never a file path.
    validate_repo_scope_diff_inputs(&input, true)?;
    // A present diff wins over base validation.
    input.diff_file = Some(root.0.join("src/lib.rs"));
    input.base = Some("no-such-ref".to_string());
    validate_repo_scope_diff_inputs(&input, true)?;
    // Without an explicit --base the recorded default is never probed.
    input.diff_file = None;
    validate_repo_scope_diff_inputs(&input, false)?;
    // An explicit flag with no recorded value has nothing to probe.
    input.base = None;
    validate_repo_scope_diff_inputs(&input, true)?;
    Ok(())
}

#[test]
fn repo_scope_base_must_resolve_to_a_commit() -> Result<(), String> {
    let root = TempRoot::new("repo-scope-base")?;
    fixture_git::fixture_git_ok(&root.0, &["init", "--initial-branch=main"])?;
    fixture_git::fixture_git_ok(&root.0, &["config", "user.email", "r@e.in"])?;
    fixture_git::fixture_git_ok(&root.0, &["config", "user.name", "r"])?;
    fixture_git::fixture_git_ok(&root.0, &["add", "."])?;
    fixture_git::fixture_git_ok(&root.0, &["commit", "-qm", "base"])?;
    let mut input = CheckInput {
        root: root.0.clone(),
        base: Some("main".to_string()),
        git_timeout: Some(std::time::Duration::from_secs(30)),
        ..CheckInput::default()
    };
    validate_repo_scope_diff_inputs(&input, true)?;
    input.base = Some("no-such-ref".to_string());
    let Err(error) = validate_repo_scope_diff_inputs(&input, true) else {
        return Err("an unresolvable base must be refused".to_string());
    };
    let want = format!(
        "check: base revision \"no-such-ref\" does not resolve to a commit in {}",
        root.0.display()
    );
    if error.to_string() != want {
        return Err(format!("expected `{want}`, got `{error}`"));
    }
    // The same refusal surfaces through the CLI.
    let cli = refusal(
        &root,
        &["--format", "repo-exposure-json", "--base", "no-such-ref"],
    )?;
    if cli != want {
        return Err(format!("the CLI must carry the same refusal, got {cli}"));
    }
    Ok(())
}

#[test]
fn check_timeout_environment_is_a_fallback_and_fails_closed() -> Result<(), String> {
    assert_eq!(
        check_timeout_from_env(false, Ok("30".to_string())),
        Ok(Some(Some(std::time::Duration::from_secs(30))))
    );
    assert_eq!(
        check_timeout_from_env(false, Ok("0".to_string())),
        Ok(Some(None))
    );
    assert_eq!(
        check_timeout_from_env(false, Err(std::env::VarError::NotPresent)),
        Ok(None)
    );
    // An explicit --timeout wins, so a bad environment value is not read.
    assert_eq!(
        check_timeout_from_env(true, Ok("invalid".to_string())),
        Ok(None)
    );
    for value in ["soon", "-1", "", "99999999999999999999"] {
        let error = check_timeout_from_env(false, Ok(value.to_string()))
            .err()
            .ok_or(format!("RIPR_CHECK_TIMEOUT={value:?} should fail closed"))?;
        assert_eq!(
            error,
            format!("RIPR_CHECK_TIMEOUT requires a non-negative integer (seconds); got {value:?}")
        );
    }
    let error = check_timeout_from_env(false, Ok(u64::MAX.to_string()))
        .err()
        .ok_or("an overflowing deadline should fail closed")?;
    assert_eq!(
        error,
        format!(
            "RIPR_CHECK_TIMEOUT is too large for the platform deadline; got {} seconds",
            u64::MAX
        )
    );
    assert_eq!(
        check_timeout_from_env(
            false,
            Err(std::env::VarError::NotUnicode(std::ffi::OsString::from(
                "x"
            )))
        ),
        Err("RIPR_CHECK_TIMEOUT must be valid UTF-8".to_string())
    );
    Ok(())
}

#[test]
fn timeout_flags_name_their_own_source_when_refused() -> Result<(), String> {
    assert_eq!(
        parse_check_timeout_from("--timeout", "7"),
        Ok(Some(std::time::Duration::from_secs(7)))
    );
    assert_eq!(parse_check_timeout_from("--timeout", "0"), Ok(None));
    let error = check(&args(&["--timeout", "soon"]))
        .err()
        .ok_or("a non-numeric --timeout must fail closed")?;
    assert_eq!(
        error,
        "--timeout requires a non-negative integer (seconds); got \"soon\""
    );
    let error = parse_git_timeout("18446744073709551615")
        .err()
        .ok_or("an overflowing --git-timeout must fail closed")?;
    assert_eq!(
        error,
        "--git-timeout is too large for the platform deadline; got 18446744073709551615 seconds"
    );
    Ok(())
}

#[test]
fn help_request_wins_over_values_but_never_consumes_an_option_value() {
    let help = |values: &[&str]| help_request(&args(values));
    assert_eq!(help(&["--help"]), Some(false));
    assert_eq!(help(&["-h"]), Some(false));
    assert_eq!(help(&["--help", "--all"]), Some(true));
    assert_eq!(help(&["--all", "-h"]), Some(true));
    // `--all` alone is not a request; it is an unknown argument.
    assert_eq!(help(&["--all"]), None);
    assert_eq!(help(&[]), None);
    // A free-text option value that looks like help is the value.
    assert_eq!(help(&["--base", "--help"]), None);
    assert_eq!(help(&["--diff", "-h"]), None);
    assert_eq!(help(&["--diff", "--all"]), None);
    // An enumerated value never looks like a flag, so help still wins.
    assert_eq!(help(&["--mode", "--help"]), Some(false));
    assert_eq!(help(&["--format", "-h"]), Some(false));
    // Help after a consumed value is still honored.
    assert_eq!(help(&["--base", "main", "--help"]), Some(false));
}

#[test]
fn bare_all_is_an_unknown_check_argument_not_a_help_modifier() {
    assert_eq!(
        check(&args(&["--all"])),
        Err("unknown check argument \"--all\". Run `ripr check --help --all`.".to_string())
    );
}

#[test]
fn invalid_mode_and_format_values_are_refused_before_analysis() -> Result<(), String> {
    for argv in [["--mode", "turbo"], ["--format", "yaml"]] {
        let error = check(&args(&argv))
            .err()
            .ok_or(format!("{argv:?} must be refused"))?;
        assert!(error.contains(argv[1]), "{argv:?}: {error}");
    }
    Ok(())
}

#[test]
fn machine_formats_get_a_parseable_refusal_document_and_human_formats_stay_prose()
-> Result<(), String> {
    let input = CheckInput {
        root: PathBuf::from("."),
        ..CheckInput::default()
    };
    let error = CoreError::repository_root_unusable("synthetic refusal");
    for format in [
        OutputFormat::Json,
        OutputFormat::Sarif,
        OutputFormat::Github,
        OutputFormat::BadgeJson,
        OutputFormat::RepoExposureJson,
        OutputFormat::AgentSeamPacketsJson,
    ] {
        let Some(document) = machine_refusal_document(&input, format, &error) else {
            return Err(format!("{format:?} must carry a refusal document"));
        };
        let document = document?;
        if document.trim().is_empty() {
            return Err(format!("{format:?} refusal document must not be empty"));
        }
        if !matches!(format, OutputFormat::Github)
            && serde_json::from_str::<serde_json::Value>(&document).is_err()
        {
            return Err(format!("{format:?} refusal must be JSON, got {document}"));
        }
    }
    for format in [OutputFormat::Human, OutputFormat::HumanFull] {
        if machine_refusal_document(&input, format, &error).is_some() {
            return Err(format!("{format:?} must stay prose-only"));
        }
    }
    Ok(())
}

#[test]
fn refuse_check_returns_the_unchanged_prose_for_human_formats() {
    let input = CheckInput::default();
    assert_eq!(
        refuse_check(&input, OutputFormat::Human, "plain refusal".to_string()),
        "plain refusal"
    );
}

#[test]
fn implicit_root_rebinds_to_the_enclosing_workspace_from_a_member_directory() -> Result<(), String>
{
    let _cwd = cwd_lock::hold_cwd();
    let root = TempRoot::new("implicit-rebind")?;
    std::fs::write(root.0.join("Cargo.toml"), "[workspace]\nmembers = []\n")
        .map_err(|err| err.to_string())?;
    let member = root.0.join("crates/inner");
    std::fs::create_dir_all(&member).map_err(|err| err.to_string())?;
    let original = std::env::current_dir().map_err(|err| err.to_string())?;
    let _restore = CwdRestore(original);
    let canonical_root = std::fs::canonicalize(&root.0).map_err(|err| err.to_string())?;

    std::env::set_current_dir(&member).map_err(|err| err.to_string())?;
    let mut input = CheckInput::default();
    resolve_implicit_workspace_root(&mut input)?;
    if input.root != canonical_root {
        return Err(format!(
            "expected rebind to {}, got {}",
            canonical_root.display(),
            input.root.display()
        ));
    }

    std::env::set_current_dir(&root.0).map_err(|err| err.to_string())?;
    let mut at_root = CheckInput::default();
    let before = at_root.root.clone();
    resolve_implicit_workspace_root(&mut at_root)?;
    if at_root.root != before {
        return Err(format!(
            "running at the workspace root must not rebind, got {}",
            at_root.root.display()
        ));
    }
    Ok(())
}

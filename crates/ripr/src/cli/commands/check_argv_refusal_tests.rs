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
        let guard = Self(unique_command_test_dir(label));
        let root = &guard.0;
        std::fs::create_dir_all(root.join("src")).map_err(|err| err.to_string())?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"argv_refusal_probe\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .map_err(|err| err.to_string())?;
        std::fs::write(root.join("src/lib.rs"), "pub fn one() -> u8 { 1 }\n")
            .map_err(|err| err.to_string())?;
        Ok(guard)
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
                    --gap-ledger, --write-artifact or --suppression-policy; pass a named `--diff <file>` \
                    and drop the other flags";
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
fn committed_refuses_repo_scoped_formats_and_gap_ledger() -> Result<(), String> {
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
fn a_bare_argument_is_read_as_a_path_scope() -> Result<(), String> {
    // #14307: the bare path reaches the --path refusal for a repo-scoped
    // format, so it was parsed as a scope rather than refused as unknown.
    let root = TempRoot::new("bare-path-scope")?;
    let bare = refusal(&root, &["src/lib.rs", "--format", "repo-exposure-json"])?;
    let flag = refusal(
        &root,
        &["--path", "src/lib.rs", "--format", "repo-exposure-json"],
    )?;
    if bare != flag || !bare.contains("--path/--exclude narrow a diff") {
        return Err(format!("bare: {bare}\nflag: {flag}"));
    }
    // A dash-led token is still an unknown argument, not a path.
    let dashed = refusal(&root, &["-x"])?;
    if !dashed.contains("unknown check argument \"-x\"") {
        return Err(format!("-x must stay unknown, got {dashed}"));
    }
    Ok(())
}

#[test]
fn path_and_exclude_are_refused_where_they_would_claim_an_unapplied_bound() -> Result<(), String> {
    let root = TempRoot::new("scope-refusals")?;
    let saved_arg = root.0.join("saved.json").to_string_lossy().into_owned();
    let ledger_arg = root.0.join("ledger.json").to_string_lossy().into_owned();
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
            ["--write-artifact", saved_arg.as_str()],
            ["--gap-ledger", ledger_arg.as_str()],
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
        "agent",
        "badge-json",
        "repo-exposure-json",
        "repo-badge-json",
    ] {
        let error = refusal(
            &root,
            &["--suppression-policy", "policy.toml", "--format", format],
        )?;
        if !(error.starts_with(
            "--suppression-policy applies to the findings-based check formats (human, human-full, json, json-compact, github, missed-txt)",
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
fn suppression_policy_is_not_refused_on_missed_txt() -> Result<(), String> {
    let root = TempRoot::new("suppression-missed-txt")?;
    let mut argv = vec!["--root".to_string(), root.arg()];
    argv.extend(
        [
            "--suppression-policy",
            "policy.toml",
            "--format",
            "missed-txt",
        ]
        .iter()
        .map(|value| value.to_string()),
    );
    if let Err(error) = check(&argv)
        && error.contains("applies to the findings-based check formats")
    {
        return Err(format!("missed-txt must accept the flag, got {error}"));
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
    if !error.starts_with("invalid JSON: ") {
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
        crate::output::path::human_path(&root.0)
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
            "RIPR_CHECK_TIMEOUT is too large for the platform deadline; got {} seconds; pass a smaller number of seconds",
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
        Err("RIPR_CHECK_TIMEOUT must be valid UTF-8; set it to a whole number of seconds, or unset it".to_string())
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
        "--git-timeout is too large for the platform deadline; got 18446744073709551615 seconds; pass a smaller number of seconds"
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
fn machine_formats_pin_refusal_identity_and_human_formats_stay_prose() -> Result<(), String> {
    let input = CheckInput {
        root: PathBuf::from("."),
        ..CheckInput::default()
    };
    let error = CoreError::repository_root_unusable("synthetic refusal");
    for format in [
        OutputFormat::Json,
        OutputFormat::Sarif,
        OutputFormat::Github,
        OutputFormat::MissedTxt,
        OutputFormat::BadgeJson,
        OutputFormat::RepoExposureJson,
        OutputFormat::AgentSeamPacketsJson,
        OutputFormat::Agent,
    ] {
        let Some(document) = machine_refusal_document(&input, format, &error) else {
            return Err(format!("{format:?} must carry a refusal document"));
        };
        let document = document?;
        if matches!(format, OutputFormat::MissedTxt) {
            assert_eq!(
                document,
                "# ripr run refused: run_status=repository_root_unusable; repair_route=analysis/repository-root; synthetic refusal\n"
            );
            continue;
        }
        if matches!(format, OutputFormat::Agent) {
            assert_eq!(
                document,
                "#ripr agent static-advisory refused run_status=repository_root_unusable repair_route=analysis/repository-root\nstop=synthetic refusal\nverify=refused|outcome=refused|findings=0|exposed=0|reason=repository_root_unusable\n"
            );
            continue;
        }
        if matches!(format, OutputFormat::Github) {
            assert_eq!(
                document,
                "::error title=ripr refusal::run refused: run_status=repository_root_unusable; repair_route=analysis/repository-root; synthetic refusal\n"
            );
            continue;
        }
        let value: serde_json::Value =
            serde_json::from_str(&document).map_err(|err| format!("{format:?}: {err}"))?;
        match format {
            OutputFormat::Json => {
                for pointer in [
                    "/analysis_scope/run_status",
                    "/analysis_scope/basis",
                    "/analysis_scope/limitation",
                    "/run_limitations/0/category",
                ] {
                    assert_eq!(
                        value.pointer(pointer),
                        Some(&serde_json::json!("repository_root_unusable")),
                        "{format:?} {pointer}"
                    );
                }
                assert_eq!(
                    value["analysis_scope"]["repair_route"],
                    "analysis/repository-root"
                );
                assert_eq!(value["analysis_scope"]["downstream_consumable"], false);
                assert_eq!(value["findings"], serde_json::json!([]));
            }
            OutputFormat::Sarif => {
                let run = &value["runs"][0];
                let invocation = &run["invocations"][0];
                let notice = &invocation["toolConfigurationNotifications"][0];
                assert_eq!(value["version"], "2.1.0");
                assert_eq!(notice["properties"]["identity"], "repository_root_unusable");
                assert_eq!(
                    notice["properties"]["repair_route"],
                    "analysis/repository-root"
                );
                assert_eq!(invocation["executionSuccessful"], false);
                assert_eq!(run["properties"]["run_status"], "refused");
                assert_eq!(run["results"], serde_json::json!([]));
            }
            OutputFormat::BadgeJson => {
                assert_eq!(value["message"], "refused: repository_root_unusable");
                assert_eq!(value["status"], "warn");
                assert_eq!(value["color"], "lightgrey");
            }
            OutputFormat::RepoExposureJson | OutputFormat::AgentSeamPacketsJson => {
                assert_eq!(value["run_status"], "refused");
                assert_eq!(
                    value["limitations"][0]["category"],
                    "repository_root_unusable"
                );
                assert_eq!(
                    value["limitations"][0]["repair_route"],
                    "analysis/repository-root"
                );
                if matches!(format, OutputFormat::RepoExposureJson) {
                    assert_eq!(value["seams"], serde_json::json!([]));
                    assert_eq!(value["metrics"]["seams_total"], 0);
                } else {
                    assert_eq!(value["packets"], serde_json::json!([]));
                    assert_eq!(value["packets_total"], 0);
                    assert!(value.get("next").is_none());
                }
            }
            _ => return Err(format!("unexpected machine format {format:?}")),
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

// ── #12417: the repair-lifecycle discovery gate ─────────────────────────────

/// A directory no project encloses, for the unanchored-gate controls. `TMPDIR`
/// can sit inside a checkout (an anchored root, where the gate is silent), so
/// fall back to the process temp dir and `/tmp`, and fail loudly if none is
/// free of markers: a silent early return would leave the gate unchecked.
/// Unix-only: the tests that use it are (the temp parent must hold no
/// manifest or repo).
#[cfg(unix)]
fn marker_free_temp_base() -> Result<PathBuf, String> {
    let anchored = |dir: &Path| {
        dir.ancestors().any(|ancestor| {
            [
                "Cargo.toml",
                ".git",
                "package.json",
                "pyproject.toml",
                "setup.py",
                "setup.cfg",
            ]
            .iter()
            .any(|name| ancestor.join(name).exists())
        })
    };
    let candidates = [
        std::env::var_os("TMPDIR").map(PathBuf::from),
        Some(std::env::temp_dir()),
        Some(PathBuf::from("/tmp")),
    ];
    candidates
        .into_iter()
        .flatten()
        .filter_map(|dir| dir.canonicalize().ok())
        .find(|dir| !anchored(dir))
        .ok_or_else(|| {
            "no temp directory outside every project was found for the unanchored control"
                .to_string()
        })
}

/// A marker-free directory the guard removes on drop.
#[cfg(unix)]
struct UnanchoredDir(PathBuf);

#[cfg(unix)]
impl UnanchoredDir {
    fn new(label: &str) -> Result<Self, String> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let dir =
            marker_free_temp_base()?.join(format!("ripr-{label}-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
        Ok(Self(dir))
    }
}

#[cfg(unix)]
impl Drop for UnanchoredDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// #12417: the repair-lifecycle gate refuses an unanchored directory with the
/// discovery cause, while the ordinary resolution keeps retaining it
/// (check/pilot/context/rerun behavior) and an explicit `--root` skips the
/// gate. Unix-only like the other implicit-root controls (the temp parent
/// must hold no manifest or repo).
#[cfg(unix)]
#[test]
fn unanchored_gate_refuses_while_the_plain_walk_retains_and_explicit_root_skips()
-> Result<(), String> {
    let _cwd = cwd_lock::hold_cwd();
    let guard = UnanchoredDir::new("gate-refusal")?;
    let original = std::env::current_dir().map_err(|err| err.to_string())?;
    let _restore = CwdRestore(original);
    std::env::set_current_dir(&guard.0).map_err(|err| err.to_string())?;

    let Some(cause) = unanchored_implicit_root_cause() else {
        return Err("an unanchored directory must produce a refusal cause".to_string());
    };
    if !cause.contains("no workspace, package or Git repository was found") {
        return Err(format!("cause must name the discovery outcome: {cause}"));
    }

    // The gate itself: refuse, naming the cause.
    let error = resolve_implicit_command_root_refusing(&mut PathBuf::from("."), false);
    if error != Err(cause.clone()) {
        return Err(format!(
            "the gate must refuse with its cause, got {error:?}"
        ));
    }

    // check/pilot/context/rerun keep retain-start: the plain walk answers Ok
    // and leaves the root as typed.
    let mut retained = PathBuf::from(".");
    resolve_implicit_command_root(&mut retained, false)
        .map_err(|error| format!("the plain walk must keep retain-start: {error}"))?;
    if retained != Path::new(".") {
        return Err(format!(
            "retain-start keeps the root, got {}",
            retained.display()
        ));
    }

    // An explicit --root skips the gate.
    let mut explicit = PathBuf::from(".");
    resolve_implicit_command_root_refusing(&mut explicit, true)
        .map_err(|error| format!("an explicit --root must skip the gate: {error}"))?;
    if explicit != Path::new(".") {
        return Err(format!(
            "an explicit root is kept as typed, got {}",
            explicit.display()
        ));
    }
    Ok(())
}

/// #12417, #12480: a standalone `package.json` project without Git or Cargo
/// still reads as unanchored to `resolve_project_root`, but the gate keeps it
/// anchored: it is a project.
#[cfg(unix)]
#[test]
fn a_plain_package_manifest_keeps_the_gate_anchored() -> Result<(), String> {
    let _cwd = cwd_lock::hold_cwd();
    let guard = UnanchoredDir::new("gate-plain-package")?;
    std::fs::write(guard.0.join("package.json"), "{\"name\":\"shop\"}")
        .map_err(|err| err.to_string())?;
    let original = std::env::current_dir().map_err(|err| err.to_string())?;
    let _restore = CwdRestore(original);
    std::env::set_current_dir(&guard.0).map_err(|err| err.to_string())?;

    // The walk itself still finds no anchor; the ordinary manifest is what
    // keeps the gate silent.
    if !matches!(resolve_project_root(Path::new(".")), Ok(None)) {
        return Err("a plain package.json without workspaces must not anchor the walk".to_string());
    }
    if let Some(cause) = unanchored_implicit_root_cause() {
        return Err(format!(
            "a plain package project must not be refused: {cause}"
        ));
    }
    let mut root = PathBuf::from(".");
    resolve_implicit_command_root_refusing(&mut root, false)
        .map_err(|error| format!("the gate must keep a plain package project: {error}"))?;
    Ok(())
}

/// #14499: every machine-readable `check` format carries its own refusal
/// document (never zero bytes), each naming the typed identity, and each
/// JSON-shaped one parsing as JSON with the family's refused marker.
#[test]
fn every_machine_format_carries_a_refusal_document() -> Result<(), String> {
    let input = CheckInput {
        root: PathBuf::from("."),
        ..CheckInput::default()
    };
    let error = CoreError::repository_root_unusable("synthetic refusal");
    let mut covered = 0;
    for name in OutputFormat::accepted_cli_names() {
        let format = OutputFormat::parse_cli_name(name).ok_or(name)?;
        let document = machine_refusal_document(&input, format, &error);
        // `github` is a CI log format, not machine-readable for progress
        // purposes, but its annotation is still a refusal document.
        if !format.is_machine_readable() && !matches!(format, OutputFormat::Github) {
            if document.is_some() {
                return Err(format!("{name} is prose: its refusal stays on stderr"));
            }
            continue;
        }
        let document = document.ok_or(format!("{name} must carry a refusal document"))??;
        covered += 1;
        assert!(
            document.contains("repository_root_unusable"),
            "{name}: {document}"
        );
        if !(format.is_json_request()
            || matches!(
                format,
                OutputFormat::Sarif
                    | OutputFormat::SarifCompact
                    | OutputFormat::RepoSarif
                    | OutputFormat::RepoSarifCompact
                    | OutputFormat::BadgeShields
                    | OutputFormat::BadgePlusShields
                    | OutputFormat::RepoBadgeShields
                    | OutputFormat::RepoBadgePlusShields
            ))
        {
            continue;
        }
        let value: serde_json::Value =
            serde_json::from_str(&document).map_err(|err| format!("{name}: {err}"))?;
        // Each family's own refused marker, not any one of them.
        let (pointer, expected) = match format {
            OutputFormat::Json
            | OutputFormat::JsonCompact
            | OutputFormat::JsonTerse
            | OutputFormat::CheckFindingsJson => {
                ("/analysis_scope/run_status", "repository_root_unusable")
            }
            OutputFormat::Sarif
            | OutputFormat::SarifCompact
            | OutputFormat::RepoSarif
            | OutputFormat::RepoSarifCompact => ("/runs/0/properties/run_status", "refused"),
            OutputFormat::BadgeJson
            | OutputFormat::BadgeShields
            | OutputFormat::BadgePlusJson
            | OutputFormat::BadgePlusShields
            | OutputFormat::RepoBadgeJson
            | OutputFormat::RepoBadgeShields
            | OutputFormat::RepoBadgePlusJson
            | OutputFormat::RepoBadgePlusShields => {
                ("/message", "refused: repository_root_unusable")
            }
            _ => ("/run_status", "refused"),
        };
        assert_eq!(
            value.pointer(pointer),
            Some(&serde_json::json!(expected)),
            "{name} {pointer}: {value}"
        );
        if matches!(
            format,
            OutputFormat::BadgeJson
                | OutputFormat::BadgePlusJson
                | OutputFormat::RepoBadgeJson
                | OutputFormat::RepoBadgePlusJson
        ) {
            assert_eq!(value["status"], "warn", "{name}");
            assert_eq!(value["color"], "lightgrey", "{name}");
        }
        match format {
            OutputFormat::BadgeShields | OutputFormat::RepoBadgeShields => {
                assert_eq!(value["label"], "ripr", "{name}");
                assert_eq!(value["color"], "lightgrey", "{name}");
                assert_eq!(value.as_object().map(|map| map.len()), Some(4), "{name}");
            }
            OutputFormat::BadgePlusShields | OutputFormat::RepoBadgePlusShields => {
                assert_eq!(value["label"], "ripr+", "{name}");
                assert_eq!(value.as_object().map(|map| map.len()), Some(4), "{name}");
            }
            OutputFormat::BadgePlusJson => {
                assert_eq!(value["kind"], "ripr_plus", "{name}");
                assert_eq!(value["scope"], "diff", "{name}");
            }
            OutputFormat::RepoBadgeJson | OutputFormat::RepoBadgePlusJson => {
                assert_eq!(value["scope"], "repo", "{name}");
                assert_eq!(value["basis"], "canonical_actionable_gap", "{name}");
            }
            OutputFormat::RepoSarif | OutputFormat::RepoSarifCompact => {
                let run = &value["runs"][0];
                assert_eq!(run["properties"]["scope"], "repo_seam", "{name}");
                assert_eq!(run["results"], serde_json::json!([]), "{name}");
                assert_eq!(
                    run["invocations"][0]["toolConfigurationNotifications"][0]["properties"]["identity"],
                    "repository_root_unusable",
                    "{name}"
                );
            }
            OutputFormat::RepoSeamsJson => {
                assert_eq!(value["seams"], serde_json::json!([]), "{name}");
                assert!(value.get("artifact").is_none(), "{name}");
                assert_eq!(
                    value["limitations"][0]["repair_route"],
                    "analysis/repository-root"
                );
            }
            OutputFormat::RepoExposureSummaryJson => {
                assert_eq!(value["format"], "repo-exposure-summary-json");
                assert_eq!(value["top_files"], serde_json::json!([]));
                assert_eq!(value["metrics"]["unsuppressed_exposure_gaps"], 0);
            }
            OutputFormat::JsonTerse => assert_eq!(value["rendering"], "terse"),
            OutputFormat::CheckFindingsJson => {
                assert!(value.get("artifact").is_none(), "{name}");
                assert_eq!(value["findings"], serde_json::json!([]));
            }
            _ => {}
        }
    }
    // 23 names with a refusal document, `agent` (#14452) included.
    assert_eq!(covered, 23, "machine formats with a refusal document");
    Ok(())
}

/// #14499: the check-findings-json refusal is not a verify snapshot; the
/// artifact validator rejects it rather than reading an empty, whole run.
#[test]
fn check_findings_json_refusal_is_rejected_as_a_verify_artifact() -> Result<(), String> {
    let input = CheckInput {
        root: PathBuf::from("."),
        ..CheckInput::default()
    };
    let error = CoreError::repository_root_unusable("synthetic refusal");
    let document = machine_refusal_document(&input, OutputFormat::CheckFindingsJson, &error)
        .ok_or("check-findings-json must carry a refusal document")??;
    let verdict =
        crate::agent::artifact::validate_check_findings_artifact(&input.root, &document, "--from");
    assert!(
        verdict.is_err(),
        "a refusal must not validate as a snapshot"
    );
    Ok(())
}

/// #14499 review: a scope-guard refusal keeps its `diff_scope_oversized`
/// identity in every JSON-family format, not only `json`.
#[test]
fn json_family_refusals_keep_the_scope_guard_identity() -> Result<(), String> {
    let input = CheckInput {
        root: PathBuf::from("."),
        ..CheckInput::default()
    };
    let error = CoreError::message(
        "diff_scope_oversized: 2301 changed Rust lines across 42 Rust files exceed \
         RIPR_MAX_DIFF_CHANGED_LINES=2000",
    );
    for format in [
        OutputFormat::Json,
        OutputFormat::JsonCompact,
        OutputFormat::JsonTerse,
        OutputFormat::CheckFindingsJson,
    ] {
        let document = machine_refusal_document(&input, format, &error)
            .ok_or(format!("{format:?} must carry a refusal document"))??;
        let value: serde_json::Value =
            serde_json::from_str(&document).map_err(|err| format!("{format:?}: {err}"))?;
        assert_eq!(
            value["analysis_scope"]["run_status"], "diff_scope_oversized",
            "{format:?}: {value}"
        );
        assert_eq!(
            value["analysis_scope"]["repair_route"], "analysis/diff-scope-budget",
            "{format:?}"
        );
    }
    Ok(())
}

/// #14499 review: the refused exposure summary keeps every aggregate,
/// breakdown and limit field of the normal summary shape.
#[test]
fn refused_exposure_summary_keeps_the_normal_summary_shape() -> Result<(), String> {
    let input = CheckInput {
        root: PathBuf::from("."),
        ..CheckInput::default()
    };
    let error = CoreError::repository_root_unusable("synthetic refusal");
    let document = machine_refusal_document(&input, OutputFormat::RepoExposureSummaryJson, &error)
        .ok_or("summary must carry a refusal document")??;
    let value: serde_json::Value =
        serde_json::from_str(&document).map_err(|err| err.to_string())?;
    for pointer in [
        "/metrics/grip_class",
        "/reason_breakdown/actionability",
        "/reason_breakdown/gap_state",
        "/reason_breakdown/seam_kind",
        "/reason_breakdown/grip_class",
        "/limits/top_files_limit",
        "/limits/top_files_total",
        "/limits/top_files_truncated",
    ] {
        assert!(
            value.pointer(pointer).is_some(),
            "missing {pointer}: {value}"
        );
    }
    assert_eq!(value["limits"]["top_files_total"], 0);
    assert_eq!(value["run_status"], "refused");
    Ok(())
}

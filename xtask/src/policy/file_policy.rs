use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Duration;

use crate::run::{COMMAND_DIAGNOSTIC_DISCLOSURE, CommandDiagnostic, TimedOutput};
use crate::{
    FilePolicyHost, FilePolicyTestCommand, FixKind, PolicyDisclosure, PolicyReportSpec,
    capture_output_with_timeout, collect_files, finish_policy_report_with_disclosures,
    is_cargo_test_command, is_file_policy_candidate, is_non_rust_programming_candidate,
    matches_any_glob, non_rust_programming_retention_reason, normalize_path,
    read_file_policy_allowlist, read_file_policy_test_commands,
};

const TEST_COVERED_BY_LIST_TIMEOUT: Duration = Duration::from_mins(5);
/// Compile budget, separate from listing. A cold `cargo test --list` spends
/// almost all of its wall clock building; that must not be charged against
/// the list cap or reported as an unresolved `covered_by` pointer (#4141).
const TEST_COVERED_BY_BUILD_TIMEOUT: Duration = Duration::from_mins(30);
const COVERED_BY_INSTRUMENT_PREFIX: &str = "covered_by_instrument_timeout:";
/// Infrastructure state: the spawn timed out while Cargo was blocked on a
/// build/package lock held by another process (#8793). Still an instrument
/// failure, never a policy violation and never a pass.
const COVERED_BY_LOCK_WAIT_PREFIX: &str = "covered_by_lock_wait_timeout:";
/// Cargo prints this line while it waits on a lock another process holds.
const CARGO_LOCK_WAIT_MARKER: &str = "waiting for file lock";
/// A spawn that timed out while only waiting on a cargo lock is retried this
/// many times, so time spent queued behind another process's build does not
/// consume the enumeration budget. The bound keeps the gate fail-closed.
const LOCK_WAIT_RETRIES: usize = 6;
/// Total wall-clock ceiling for one lock-wait retry sequence (#8939). The
/// attempt bound alone allows 7 spawns of the 30-minute build timeout (about
/// 3.5h). The sum of the lock-wait attempts' recorded durations is checked
/// after each attempt, so the sequence may overshoot by at most one spawn
/// timeout. 90 minutes lets three build-length lock waits clear and no more.
const LOCK_WAIT_CEILING: Duration = Duration::from_mins(90);
/// First word of the history line recording that the ceiling stopped retrying.
const LOCK_WAIT_CEILING_NOTE_PREFIX: &str = "xtask: lock-wait ceiling";
const INSTRUMENT_FAILURE_PHRASE: &str = "instrument timeout, not an unresolved pointer";

/// True when `error` is a `check-file-policy` instrument failure (timeout or
/// lock wait), as opposed to a policy violation. `precommit` uses this to run
/// its remaining checks and still fail at the end.
pub(crate) fn is_covered_by_instrument_failure(error: &str) -> bool {
    // Anchored to the first line, which the validator formats itself. Later
    // lines embed arbitrary cargo stdout/stderr, which must not be able to
    // turn a real failure into an infrastructure one.
    error.lines().next().is_some_and(|first| {
        first.contains(&format!(
            "enumeration did not finish ({INSTRUMENT_FAILURE_PHRASE})"
        ))
    })
}

/// The lock line, only while cargo was still blocked on it: cargo keeps the
/// line after it acquires the lock and goes on to compile, so a lock line
/// followed by any other output is a stale mention, not a lock wait.
fn lock_wait_note(stderr: &str) -> Option<&str> {
    stderr
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .filter(|line| line.contains(CARGO_LOCK_WAIT_MARKER))
}

/// Run `capture`, retrying while the only observed cause of a timeout is a
/// cargo lock wait. Any other timeout, exhausted lock-wait retries, or an
/// exhausted [`LOCK_WAIT_CEILING`] (summed from the recorded attempt
/// durations, so no clock is read) is returned as a timed-out output and stays
/// a failure.
fn capture_past_lock_wait(
    spawn: &CoveredBySpawn,
    capture: &mut impl FnMut(&CoveredBySpawn) -> Result<TimedOutput, String>,
) -> Result<TimedOutput, String> {
    let mut attempts = 0;
    let mut waited = Duration::ZERO;
    loop {
        let mut output = capture(spawn)?;
        let lock_wait = output.timed_out && lock_wait_note(&output.stderr).is_some();
        if lock_wait {
            waited = waited.saturating_add(output.duration);
        }
        let ceiling_exhausted = lock_wait && waited >= LOCK_WAIT_CEILING;
        if lock_wait && !ceiling_exhausted && attempts < LOCK_WAIT_RETRIES {
            attempts += 1;
            continue;
        }
        // History lines are kept ahead of cargo's own output so the last-line
        // lock check above is unaffected.
        if ceiling_exhausted {
            output.stderr = format!(
                "{LOCK_WAIT_CEILING_NOTE_PREFIX} of {LOCK_WAIT_CEILING:?} exhausted after {waited:?} blocked on a cargo lock\n{}",
                output.stderr
            );
        }
        if attempts > 0 {
            output.stderr = format!(
                "xtask: retried {attempts} time(s) while blocked on a cargo lock (final attempt took {:?})\n{}",
                output.duration, output.stderr
            );
        }
        return Ok(output);
    }
}

/// Validate the repository's non-Rust file policy and write its standard
/// report. The parser and shared path predicates remain in `main.rs` until
/// their other policy/report consumers can move in a later slice.
pub(crate) fn check_file_policy() -> Result<(), String> {
    let policy_path = "policy/non-rust-allowlist.toml";
    let allowlist = read_file_policy_allowlist(policy_path)?;
    let mut diagnostics = Vec::new();
    let coverage = match validate_test_covered_by_recorded(
        policy_path,
        &read_file_policy_test_commands(policy_path)?,
        &mut diagnostics,
    ) {
        Ok(coverage) => coverage,
        Err(error) => {
            return file_policy_failure_report(
                "coverage enumeration",
                &error,
                &diagnostics,
                |body| crate::write_report("file-policy.md", body),
            );
        }
    };
    let mut violations = Vec::new();

    let files = match collect_files(Path::new(".")) {
        Ok(files) => files,
        Err(error) => {
            return file_policy_failure_report("file enumeration", &error, &diagnostics, |body| {
                crate::write_report("file-policy.md", body)
            });
        }
    };
    for path in files {
        let normalized = normalize_path(&path);
        if !is_file_policy_candidate(&normalized) {
            continue;
        }
        if normalized.ends_with(".rs") {
            continue;
        }
        if !matches_any_glob(&allowlist, &normalized) {
            violations.push(format!(
                "unapproved non-Rust programming/declarative file: {normalized}\n  preferred: implement automation in Rust/xtask or add a policy allowlist entry with owner and reason"
            ));
            continue;
        }
        if is_non_rust_programming_candidate(&normalized)
            && non_rust_programming_retention_reason(&normalized).is_none()
        {
            violations.push(format!(
                "non-Rust programming file lacks a keep-non-Rust retention rule: {normalized}\n  preferred: convert implementation/test automation to Rust/xtask unless the file is bound to an approved non-Rust runtime surface"
            ));
        }
    }

    finish_policy_report_with_disclosures(
        PolicyReportSpec {
            report_file: "file-policy.md",
            check: "check-file-policy",
            why_it_matters: "Rust and xtask are the default implementation surface so repo automation stays typed, tested, and reviewable.",
            fix_kind: FixKind::PolicyExceptionRequired,
            recommended_fixes: &[
                "Move implementation or automation logic into Rust/xtask.",
                "If the file belongs to an approved surface, add an allowlist entry with owner and reason.",
            ],
            rerun_command: "cargo xtask check-file-policy",
            exception_template: Some(
                "policy/non-rust-allowlist.toml entry:\n[[allow]]\nglob = \"path/**/*.ext\"\nkind = \"surface_kind\"\nowner = \"team/area\"\nsurface = \"docs|editor|fixtures|policy|rust|ci\"\nclassification = \"production|test|tooling|generated|config|docs|fixture|metadata\"\nreason = \"why this must remain non-Rust or declarative\"\ncovered_by = [\"cargo xtask check-file-policy\"]",
            ),
        },
        &violations,
        &[PolicyDisclosure {
            heading: "Test-valued coverage applicability".to_string(),
            intro: "Applicable selectors must enumerate nonzero tests. Enumeration is not test execution. Host-inapplicable selectors are not enumerated and confer no coverage on this host.".to_string(),
            items: coverage.iter().map(TestCoverageObservation::render).collect(),
        }, PolicyDisclosure {
            heading: "Cargo child command diagnostics".to_string(),
            intro: format!("{COMMAND_DIAGNOSTIC_DISCLOSURE} Capture scope: actual covered-by warmup/list calls only. Reused listings, skipped warmups and host-inapplicable selectors add no executed rows."),
            items: diagnostics.iter().map(CommandDiagnostic::render).collect(),
        }],
    )
}

fn file_policy_failure_report(
    phase: &str,
    error: &str,
    diagnostics: &[CommandDiagnostic],
    mut write: impl FnMut(&str) -> Result<(), String>,
) -> Result<(), String> {
    // Preserve the original enumerator failure, including instrument failures,
    // without routing it through a renderer that calls it a policy violation.
    let body = format!(
        "# check-file-policy\n\nStatus: stopped during {phase}\n\nOriginal failure:\n```text\n{error}\n```\n\n## Cargo child command diagnostics\n\n{COMMAND_DIAGNOSTIC_DISCLOSURE}\n\n{}\n",
        diagnostics
            .iter()
            .map(|row| format!("- {}", row.render()))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    match write(&body) {
        Ok(()) => Err(error.to_string()),
        Err(report_error) => Err(format!(
            "{error}; failed to publish current file-policy report: {report_error}"
        )),
    }
}

#[derive(Debug)]
struct TestCoverageObservation {
    selector: FilePolicyTestCommand,
    host: FilePolicyHost,
    // None is explicitly inapplicable, never a successful empty enumeration.
    selected: Option<Vec<String>>,
}

impl TestCoverageObservation {
    fn render(&self) -> String {
        let declared = self.selector.host.map_or("all", FilePolicyHost::name);
        let status = match &self.selected {
            Some(tests) => format!(
                "applicable; selected={}; tests=[{}]",
                tests.len(),
                tests.join(", ")
            ),
            None => "not_applicable; selected=not_enumerated".to_string(),
        };
        format!(
            "line {}; host={}; declared={declared}; {status}; `{}`",
            self.selector.line,
            self.host.name(),
            self.selector.command
        )
    }
}

#[cfg(test)]
fn validate_test_covered_by(
    path: &str,
    commands: &[FilePolicyTestCommand],
) -> Result<Vec<TestCoverageObservation>, String> {
    validate_test_covered_by_recorded(path, commands, &mut Vec::new())
}

fn validate_test_covered_by_recorded(
    path: &str,
    commands: &[FilePolicyTestCommand],
    diagnostics: &mut Vec<CommandDiagnostic>,
) -> Result<Vec<TestCoverageObservation>, String> {
    let host = FilePolicyHost::current()?;
    // Cargo serializes builds on the target-dir lock, so the per-selector
    // preparations below compile one package at a time. One combined build
    // lets Cargo schedule them together; it only pre-fills the cache.
    if let Some(args) = combined_preparation_args(commands, host) {
        let captured = capture_output_with_timeout(
            "cargo",
            &args,
            &[],
            TEST_COVERED_BY_BUILD_TIMEOUT,
            "test-valued covered_by combined build",
        );
        diagnostics.push(CommandDiagnostic::captured("cargo", &args, &captured));
    }
    validate_test_covered_by_recorded_with(path, commands, host, diagnostics, |spawn| {
        capture_output_with_timeout("cargo", &spawn.args, &[], spawn.timeout, spawn.description)
    })
}

fn validate_test_covered_by_recorded_with(
    path: &str,
    commands: &[FilePolicyTestCommand],
    host: FilePolicyHost,
    diagnostics: &mut Vec<CommandDiagnostic>,
    mut capture: impl FnMut(&CoveredBySpawn) -> Result<TimedOutput, String>,
) -> Result<Vec<TestCoverageObservation>, String> {
    let mut warmed = BTreeSet::new();
    validate_test_covered_by_with(path, commands, host, |args| {
        let spawns = enumeration_spawns(args);
        let warmup = spawns.len() == 2;
        let mut listed = None;
        for (index, spawn) in spawns.into_iter().enumerate() {
            let key = (index == 0 && warmup)
                .then(|| preparation_key(&spawn.args))
                .flatten();
            if key.as_ref().is_some_and(|key| warmed.contains(key)) {
                continue;
            }
            // One diagnostic row per attempt, so the time spent in earlier
            // lock-wait attempts is not lost from the report.
            let output = capture_past_lock_wait(&spawn, &mut |attempt| {
                let captured = capture(attempt);
                diagnostics.push(CommandDiagnostic::captured(
                    "cargo",
                    &attempt.args,
                    &captured,
                ));
                captured
            })?;
            if output.timed_out || !output.status.is_some_and(|status| status.success()) {
                return Ok(enumeration_result(output, spawn.timeout));
            }
            if let Some(key) = key {
                warmed.insert(key);
            }
            listed = Some(output);
        }
        let output = listed
            .ok_or_else(|| "test-valued covered_by enumeration produced no spawn".to_string())?;
        Ok(enumeration_result(output, TEST_COVERED_BY_LIST_TIMEOUT))
    })
}

/// One `cargo test --no-run` covering every applicable selector whose build
/// is only `-p`, `--bin`, `--test` and lock flags. Selectors with any other
/// shape are left to their own preparation. This is a cache fill, not
/// evidence: a failure here is ignored and each selector still builds,
/// lists and fails under its own spawn.
fn combined_preparation_args(
    commands: &[FilePolicyTestCommand],
    host: FilePolicyHost,
) -> Option<Vec<String>> {
    let mut packages = BTreeSet::new();
    let mut bins = BTreeSet::new();
    let mut tests = BTreeSet::new();
    let mut merged = 0usize;
    for selector in commands {
        if selector.host.is_some_and(|declared| declared != host)
            || !is_cargo_test_command(&selector.command)
        {
            continue;
        }
        let words = selector.command.split_whitespace().skip(2);
        let mut command_packages = BTreeSet::new();
        let mut command_bins = BTreeSet::new();
        let mut command_tests = BTreeSet::new();
        let mut words = words.peekable();
        let mut mergeable = true;
        while let Some(word) = words.next() {
            let target = match word {
                "-p" | "--package" => Some(&mut command_packages),
                "--bin" => Some(&mut command_bins),
                "--test" => Some(&mut command_tests),
                _ => None,
            };
            match target {
                Some(set) => match words.next() {
                    Some(value) if !value.starts_with('-') => {
                        set.insert(value.to_string());
                    }
                    _ => mergeable = false,
                },
                // A test-name filter or a lock flag does not change the build.
                // Positionals only filter which tests run, never what `--no-run`
                // compiles; a selector whose build a positional could change
                // must not reach this arm.
                None if matches!(word, "--locked" | "--offline") || !word.starts_with('-') => {}
                None => mergeable = false,
            }
        }
        if mergeable && !command_packages.is_empty() {
            packages.extend(command_packages);
            bins.extend(command_bins);
            tests.extend(command_tests);
            merged += 1;
        }
    }
    if merged < 2 {
        return None;
    }
    let mut args = vec!["test".to_string(), "--no-run".to_string()];
    for (flag, values) in [("-p", packages), ("--bin", bins), ("--test", tests)] {
        for value in values {
            args.push(flag.to_string());
            args.push(value);
        }
    }
    Some(args)
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct PreparationKey {
    args: Vec<String>,
    cwd: std::path::PathBuf,
    environment: BTreeMap<std::ffi::OsString, std::ffi::OsString>,
}

fn preparation_key(args: &[String]) -> Option<PreparationKey> {
    Some(PreparationKey {
        args: preparation_args_identity(args),
        cwd: std::env::current_dir().ok()?,
        environment: std::env::vars_os().collect(),
    })
}

/// Cargo passes TESTNAME to the harness; it does not change a no-run build.
/// Recognize only unambiguous option forms here. Unknown or malformed argv
/// keeps its exact identity, so Cargo still decides acceptance. Spawn argv
/// and every native listing remain unchanged. This is invocation-local
/// preparation reuse, not acceptance of a persistent compilation artifact.
fn preparation_args_identity(args: &[String]) -> Vec<String> {
    const VALUES: &[&str] = &[
        "-p",
        "--package",
        "--exclude",
        "--bin",
        "--example",
        "--test",
        "--bench",
        "-F",
        "--features",
        "--target",
        "--profile",
        "--manifest-path",
        "--target-dir",
        "-j",
        "--jobs",
        "--config",
        "--color",
        "--message-format",
        "-Z",
    ];
    const FLAGS: &[&str] = &[
        "--no-run",
        "--no-fail-fast",
        "--workspace",
        "--all",
        "--lib",
        "--bins",
        "--examples",
        "--tests",
        "--benches",
        "--all-targets",
        "--all-features",
        "--no-default-features",
        "-r",
        "--release",
        "--locked",
        "--offline",
        "--frozen",
        "-q",
        "--quiet",
        "-v",
        "--verbose",
        "--keep-going",
    ];
    if args.first().map(String::as_str) != Some("test") || args.iter().any(|arg| arg.contains('\0'))
    {
        return args.to_vec();
    }
    let mut filter = None;
    let mut index = 1;
    while index < args.len() {
        let arg = args[index].as_str();
        if VALUES.contains(&arg) {
            let Some(value) = args.get(index + 1) else {
                return args.to_vec();
            };
            if value.is_empty() || value.starts_with('-') {
                return args.to_vec();
            }
            index += 2;
            continue;
        }
        if FLAGS.contains(&arg)
            || arg.split_once('=').is_some_and(|(name, value)| {
                name.starts_with("--") && VALUES.contains(&name) && !value.is_empty()
            })
        {
            index += 1;
            continue;
        }
        if arg.is_empty() || arg.starts_with('-') || filter.replace(index).is_some() {
            return args.to_vec();
        }
        index += 1;
    }
    let mut identity = args.to_vec();
    if let Some(index) = filter {
        identity.remove(index);
    }
    identity
}

/// One cargo spawn used to prove a test-valued `covered_by` pointer.
struct CoveredBySpawn {
    args: Vec<String>,
    timeout: Duration,
    description: &'static str,
}

/// Warm a cold compile under the build budget, then list under the list cap.
///
/// Ordinary `cargo test` pointers warm with `--no-run`. `cargo test --doc`
/// rejects `--no-run` (`can't skip running doc tests with --no-run`), and a
/// second `cargo test --doc -- --list` still runs `rustdoc --test` after the
/// library is fresh. That pointer is therefore enumerated once, under the
/// compile budget. A second spawn would put the doctest compile back inside
/// the five-minute list cap (#4141).
fn enumeration_spawns(args: &[String]) -> Vec<CoveredBySpawn> {
    if let Some(build_args) = build_args_before_list(args) {
        vec![
            CoveredBySpawn {
                args: build_args,
                timeout: TEST_COVERED_BY_BUILD_TIMEOUT,
                description: "test-valued covered_by build",
            },
            CoveredBySpawn {
                args: args.to_vec(),
                timeout: TEST_COVERED_BY_LIST_TIMEOUT,
                description: "test-valued covered_by enumeration",
            },
        ]
    } else {
        vec![CoveredBySpawn {
            args: args.to_vec(),
            timeout: TEST_COVERED_BY_BUILD_TIMEOUT,
            description: "test-valued covered_by doc enumeration",
        }]
    }
}

/// Build args for `cargo test … --no-run`, or `None` when Cargo rejects
/// that combination. `cargo test --doc --no-run` is an error, so a
/// documentation-test pointer is not given a `--no-run` spawn.
/// [`enumeration_spawns`] enumerates that pointer once under the build
/// budget. Cargo recompiles doctests on every listing, so a follow-up list
/// would charge that compile against the list cap.
fn build_args_before_list(args: &[String]) -> Option<Vec<String>> {
    let mut build_args = Vec::new();
    for arg in args {
        if arg == "--" {
            break;
        }
        if arg == "--doc" {
            return None;
        }
        build_args.push(arg.clone());
    }
    build_args.push("--no-run".to_string());
    Some(build_args)
}

fn enumeration_result(output: TimedOutput, timeout: Duration) -> (bool, String, String) {
    let status = output
        .status
        .map(|status| status.to_string())
        .unwrap_or_else(|| "not available".to_string());
    let timeout_note = if output.timed_out {
        match lock_wait_note(&output.stderr) {
            Some(line) => {
                let ceiling = output
                    .stderr
                    .lines()
                    .take(2)
                    .find(|history| history.starts_with(LOCK_WAIT_CEILING_NOTE_PREFIX))
                    .map(|history| format!("{history}; "))
                    .unwrap_or_default();
                format!(
                    "{COVERED_BY_LOCK_WAIT_PREFIX} timed out after {timeout:?} blocked on a cargo lock held by another process ({}); {ceiling}",
                    line.trim()
                )
            }
            None => format!("{COVERED_BY_INSTRUMENT_PREFIX} timed out after {timeout:?}; "),
        }
    } else {
        String::new()
    };
    let stderr = format!(
        "{timeout_note}status: {status}\n{}",
        output.stderr.trim_end()
    );
    (
        output.status.is_some_and(|status| status.success()) && !output.timed_out,
        output.stdout,
        stderr,
    )
}

fn validate_test_covered_by_with(
    path: &str,
    commands: &[FilePolicyTestCommand],
    host: FilePolicyHost,
    mut enumerate: impl FnMut(&[String]) -> Result<(bool, String, String), String>,
) -> Result<Vec<TestCoverageObservation>, String> {
    let mut observations = Vec::new();
    // This synchronous invocation keeps one execution context. Reuse raw
    // successful output by exact argv, retaining each selector's validation.
    let mut enumerated = BTreeMap::<Vec<String>, (bool, String, String)>::new();
    for selector in commands {
        let FilePolicyTestCommand { line, command, .. } = selector;
        if !is_cargo_test_command(command) {
            return Err(format!(
                "{path}:{line} unsupported test-valued `covered_by`: {command}"
            ));
        }
        if selector.host.is_some_and(|declared| declared != host) {
            observations.push(TestCoverageObservation {
                selector: selector.clone(),
                host,
                selected: None,
            });
            continue;
        }
        let words = command.split_whitespace().skip(2);
        let mut args = vec!["test".to_string()];
        args.extend(words.map(ToString::to_string));
        args.extend([
            "--".to_string(),
            "--list".to_string(),
            "--format".to_string(),
            "terse".to_string(),
        ]);
        let (success, stdout, stderr) = match enumerated.get(&args) {
            Some(output) => output.clone(),
            None => enumerate(&args)
                .map_err(|error| format!("{path}:{line} enumerate `{command}`: {error}"))?,
        };
        if stderr.starts_with(COVERED_BY_INSTRUMENT_PREFIX)
            || stderr.starts_with(COVERED_BY_LOCK_WAIT_PREFIX)
        {
            return Err(format!(
                "{path}:{line} test-valued `covered_by` enumeration did not finish ({INSTRUMENT_FAILURE_PHRASE}): `{command}`\nstdout: {stdout}\nstderr: {stderr}"
            ));
        }
        if !success {
            return Err(format!(
                "{path}:{line} test-valued `covered_by` could not be enumerated: `{command}`\nstdout: {stdout}\nstderr: {stderr}"
            ));
        }
        let selected: Vec<String> = stdout
            .lines()
            .filter_map(|line| line.strip_suffix(": test").map(str::to_string))
            .collect();
        if selected.is_empty() {
            return Err(format!(
                "{path}:{line} test-valued `covered_by` selects zero tests: `{command}`"
            ));
        }
        enumerated.entry(args).or_insert((success, stdout, stderr));
        observations.push(TestCoverageObservation {
            selector: selector.clone(),
            host,
            selected: Some(selected),
        });
    }
    Ok(observations)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::COVERED_BY_INSTRUMENT_PREFIX;
    use super::COVERED_BY_LOCK_WAIT_PREFIX;
    use super::CoveredBySpawn;
    use super::INSTRUMENT_FAILURE_PHRASE;
    use super::LOCK_WAIT_CEILING;
    use super::LOCK_WAIT_CEILING_NOTE_PREFIX;
    use super::LOCK_WAIT_RETRIES;
    use super::TEST_COVERED_BY_BUILD_TIMEOUT;
    use super::TEST_COVERED_BY_LIST_TIMEOUT;
    use super::build_args_before_list;
    use super::capture_past_lock_wait;
    use super::enumeration_result;
    use super::enumeration_spawns;
    use super::is_covered_by_instrument_failure;
    use super::validate_test_covered_by;
    use super::validate_test_covered_by_with;
    use crate::run::{CommandDiagnostic, TimedOutput};
    use crate::{FilePolicyHost, FilePolicyTestCommand, is_cargo_test_command};

    fn argv(command: &str) -> Vec<String> {
        command.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn covered_by_preparation_identity_preserves_option_values() {
        for options in [
            "-p xtask --bin xtask --locked --offline",
            "--package=xtask --bin=xtask --features=alpha --profile=test",
            "--package xtask --features alpha --target beta --profile gamma",
            "--manifest-path alpha --target-dir beta --config gamma --jobs 2",
            "--workspace --exclude alpha --lib --all-features --frozen",
            "-p alpha -F beta -j 2 --release --no-default-features",
        ] {
            let expected = argv(&format!("test {options} --no-run"));
            for command in [
                format!("test {options} alpha --no-run"),
                format!("test beta {options} --no-run"),
            ] {
                assert_eq!(super::preparation_args_identity(&argv(&command)), expected);
            }
        }
    }

    #[test]
    fn covered_by_combined_preparation_merges_only_plain_build_shapes() {
        let commands = [
            common(1, "cargo test -p xtask --bin xtask alpha"),
            common(2, "cargo test -p ripr --locked --test mcp_stdio"),
            common(3, "cargo test -p ripr --test causal_delta_fixture beta"),
            common(4, "cargo test --workspace"),
            common(5, "cargo test -p ripr --doc"),
            common(6, "cargo test -p windows_only --test w"),
        ];
        let mut windows = commands.to_vec();
        windows[5].host = Some(FilePolicyHost::Windows);
        assert_eq!(
            super::combined_preparation_args(&windows, FilePolicyHost::Unix),
            Some(argv(
                "test --no-run -p ripr -p xtask --bin xtask --test causal_delta_fixture --test mcp_stdio"
            ))
        );
        assert_eq!(
            super::combined_preparation_args(&commands[..1], FilePolicyHost::Unix),
            None,
            "a single selector needs no combined build"
        );
        assert_eq!(
            super::combined_preparation_args(
                &[
                    common(1, "cargo test --bin xtask alpha"),
                    common(2, "cargo test --test mcp_stdio"),
                ],
                FilePolicyHost::Unix,
            ),
            None,
            "selectors without -p cannot be merged"
        );
    }

    #[test]
    fn covered_by_preparation_identity_declines_ambiguous_forms() {
        for command in [
            "test -pxtask alpha --no-run",
            "test -Falpha beta --no-run",
            "test --unknown alpha --no-run",
            "test --package --bin xtask alpha --no-run",
            "test --package --no-run",
            "test alpha beta --no-run",
            "test --features= alpha --no-run",
            "test --doc alpha --no-run",
            "test alpha\0beta --no-run",
            "test a\u{1f}b --unknown --no-run",
            "test a b --unknown --no-run",
        ] {
            let args = argv(command);
            assert_eq!(super::preparation_args_identity(&args), args);
        }
        let one = super::preparation_key(&argv("test a\u{1f}b --unknown --no-run"));
        let two = super::preparation_key(&argv("test a b --unknown --no-run"));
        assert!(one != two, "argument boundaries must not alias");
    }

    #[test]
    fn covered_by_launch_invalid_filter_keeps_its_preparation_refusal() -> Result<(), String> {
        let commands = [
            common(1, "cargo test -p xtask alpha"),
            common(2, "cargo test -p xtask beta\0"),
        ];
        let mut rows = Vec::new();
        let result = super::validate_test_covered_by_recorded_with(
            "policy.toml",
            &commands,
            FilePolicyHost::Unix,
            &mut rows,
            |spawn| {
                if spawn.args.iter().any(|arg| arg.contains('\0')) {
                    return Err(format!(
                        "refused NUL {}",
                        if spawn.args.iter().any(|arg| arg == "--no-run") {
                            "preparation"
                        } else {
                            "listing"
                        }
                    ));
                }
                Ok(timed_output(Some(status(0)), "alpha: test\n", "", false))
            },
        );
        assert!(result.is_err_and(|error| error.ends_with("refused NUL preparation")));
        assert_eq!(
            rows.len(),
            3,
            "healthy preparation/list and refused sibling preparation"
        );
        assert!(rows[2].argv.iter().any(|arg| arg == "--no-run"));
        assert_eq!(rows[2].outcome, "capture_unavailable");
        Ok(())
    }

    #[test]
    fn covered_by_preparation_keeps_compilation_requests_distinct() -> Result<(), String> {
        for (left, right) in [
            ("-p one", "-p two"),
            ("--bin one", "--bin two"),
            ("--test one", "--test two"),
            ("--features one", "--features two"),
            ("--target one", "--target two"),
            ("--profile one", "--profile two"),
            ("--locked", "--offline"),
            ("--locked --offline", "--locked"),
            ("--config one", "--config two"),
            ("--target-dir one", "--target-dir two"),
        ] {
            let commands = [
                common(1, &format!("cargo test {left} alpha")),
                common(2, &format!("cargo test {right} beta")),
            ];
            let mut rows = Vec::new();
            let observations = super::validate_test_covered_by_recorded_with(
                "policy.toml",
                &commands,
                FilePolicyHost::Unix,
                &mut rows,
                |_| Ok(timed_output(Some(status(0)), "selected: test\n", "", false)),
            )?;
            assert_eq!(observations.len(), 2);
            assert_eq!(
                rows.len(),
                4,
                "distinct preparation requests: {left}, {right}"
            );
        }
        Ok(())
    }

    #[test]
    fn covered_by_preparation_reuse_is_local_and_failure_stops_siblings() -> Result<(), String> {
        let commands = [
            common(1, "cargo test -p xtask alpha"),
            common(2, "cargo test -p xtask beta"),
        ];
        for failure in ["exit", "timeout", "status", "capture"] {
            let mut rows = Vec::new();
            let result = super::validate_test_covered_by_recorded_with(
                "policy.toml",
                &commands,
                FilePolicyHost::Unix,
                &mut rows,
                |_| {
                    if failure == "capture" {
                        return Err("refused capture".to_string());
                    }
                    Ok(timed_output(
                        if failure == "status" {
                            None
                        } else {
                            Some(status(if failure == "exit" { 1 } else { 0 }))
                        },
                        "partial: test\n",
                        "partial diagnostic",
                        failure == "timeout",
                    ))
                },
            );
            assert!(result.is_err(), "{failure} preparation was accepted");
            assert_eq!(
                rows.len(),
                1,
                "failed preparation must stop before any listing or sibling"
            );
        }
        for _ in 0..2 {
            let mut rows = Vec::new();
            super::validate_test_covered_by_recorded_with(
                "policy.toml",
                &commands,
                FilePolicyHost::Unix,
                &mut rows,
                |_| Ok(timed_output(Some(status(0)), "selected: test\n", "", false)),
            )?;
            assert_eq!(rows.len(), 3, "each invocation needs its own preparation");
        }
        Ok(())
    }

    #[test]
    fn covered_by_sibling_filters_share_only_successful_preparation() -> Result<(), String> {
        let commands = [
            common(
                11,
                "cargo test -p xtask --bin xtask --locked --offline alpha",
            ),
            common(
                12,
                "cargo test -p xtask --bin xtask --locked --offline beta",
            ),
            common(
                13,
                "cargo test -p xtask --bin xtask --locked --offline alpha",
            ),
        ];
        let mut rows = Vec::new();
        let mut calls = Vec::new();
        let observations = super::validate_test_covered_by_recorded_with(
            "policy.toml",
            &commands,
            FilePolicyHost::Unix,
            &mut rows,
            |spawn| {
                calls.push(spawn.args.clone());
                let subject = if spawn.args.iter().any(|arg| arg == "beta") {
                    "beta"
                } else {
                    "alpha"
                };
                Ok(timed_output(
                    Some(status(0)),
                    &format!("{subject}: test\n"),
                    "",
                    false,
                ))
            },
        )?;
        assert_eq!(
            calls.len(),
            3,
            "one preparation and both distinct native listings"
        );
        assert_eq!(rows.len(), calls.len());
        assert!(calls[0].contains(&"--no-run".to_string()));
        assert!(
            calls[0].contains(&"alpha".to_string()),
            "spawn argv stays original"
        );
        for (call, subject) in calls[1..].iter().zip(["alpha", "beta"]) {
            assert!(call.contains(&"--list".to_string()));
            assert!(call.contains(&subject.to_string()));
        }
        assert_eq!(observations.len(), 3);
        for ((observation, command), (line, subject)) in
            observations
                .iter()
                .zip(&commands)
                .zip([(11, "alpha"), (12, "beta"), (13, "alpha")])
        {
            assert_eq!(
                observation.selected.as_deref(),
                Some([subject.to_string()].as_slice())
            );
            assert_eq!(
                observation.render(),
                format!(
                    "line {line}; host=unix; declared=all; applicable; selected=1; tests=[{subject}]; `{}`",
                    command.command
                )
            );
        }
        Ok(())
    }

    #[test]
    fn covered_by_diagnostics_count_only_actual_spawns_and_preserve_order() -> Result<(), String> {
        let commands = [
            common(1, "cargo test selected"),
            common(2, "cargo test selected"),
            common(3, "cargo test selected -- --ignored"),
            FilePolicyTestCommand {
                host: Some(FilePolicyHost::Windows),
                ..common(4, "cargo test foreign")
            },
        ];
        let mut rows = Vec::new();
        let mut calls = Vec::new();
        let observations = super::validate_test_covered_by_recorded_with(
            "policy.toml",
            &commands,
            FilePolicyHost::Unix,
            &mut rows,
            |spawn| {
                calls.push(spawn.args.clone());
                Ok(timed_output(Some(status(0)), "selected: test\n", "", false))
            },
        )?;
        assert_eq!(observations.len(), 4);
        assert_eq!(calls.len(), 3); // one warmup, two distinct listings
        assert_eq!(
            rows.iter().map(|row| row.argv.clone()).collect::<Vec<_>>(),
            calls
        );
        assert!(rows[0].argv.iter().any(|arg| arg == "--no-run"));
        assert!(
            rows[1..]
                .iter()
                .all(|row| row.argv.iter().any(|arg| arg == "--list"))
        );
        assert!(observations[3].selected.is_none());
        Ok(())
    }

    #[test]
    fn covered_by_diagnostics_survive_interrupted_and_zero_subject_enumeration()
    -> Result<(), String> {
        for fail_at in [0, 1] {
            for kind in ["exit", "timeout", "status", "capture", "zero"] {
                let mut rows = Vec::new();
                let mut calls = 0;
                let result = super::validate_test_covered_by_recorded_with(
                    "policy.toml",
                    &[common(1, "cargo test selected")],
                    FilePolicyHost::Unix,
                    &mut rows,
                    |_| {
                        let current = calls;
                        calls += 1;
                        if current != fail_at {
                            return Ok(timed_output(
                                Some(status(0)),
                                "selected: test\n",
                                "",
                                false,
                            ));
                        }
                        match kind {
                            "capture" => Err("capture unavailable".to_string()),
                            "exit" => Ok(timed_output(
                                Some(status(7)),
                                "selected: test\n",
                                "failed",
                                false,
                            )),
                            "timeout" => Ok(timed_output(
                                Some(status(0)),
                                "selected: test\n",
                                "partial",
                                true,
                            )),
                            "status" => {
                                Ok(timed_output(None, "selected: test\n", "partial", false))
                            }
                            _ => Ok(timed_output(Some(status(0)), "0 tests\n", "", false)),
                        }
                    },
                );
                // A zero-subject warmup is not a listing; only listing output
                // owns the selector denominator, so its later list may pass.
                if kind == "zero" && fail_at == 0 {
                    assert!(result.is_ok());
                    assert_eq!(rows.len(), 2);
                } else {
                    assert!(result.is_err(), "{kind} at {fail_at} passed");
                    assert_eq!(rows.len(), fail_at + 1);
                }
                assert_eq!(rows.len(), calls);
            }
        }
        Ok(())
    }

    #[test]
    fn covered_by_failure_keeps_diagnostics_and_report_io_error() {
        let row = CommandDiagnostic::captured(
            "cargo",
            &["test".to_string()],
            &Err("capture".to_string()),
        );
        let original = "instrument timeout, not an unresolved pointer";
        let mut written = String::new();
        let error =
            super::file_policy_failure_report("coverage enumeration", original, &[row], |body| {
                written = body.to_string();
                Err("injected diagnostic I/O".to_string())
            });
        assert!(error.is_err_and(
            |error| error.contains(original) && error.contains("injected diagnostic I/O")
        ));
        assert!(written.contains("capture_unavailable") && !written.contains("Status: pass"));
    }

    fn common(line: usize, command: &str) -> FilePolicyTestCommand {
        FilePolicyTestCommand {
            line,
            command: command.to_string(),
            host: None,
        }
    }

    #[cfg(windows)]
    fn status(code: u32) -> std::process::ExitStatus {
        use std::os::windows::process::ExitStatusExt;

        ExitStatusExt::from_raw(code)
    }

    #[cfg(unix)]
    fn status(code: i32) -> std::process::ExitStatus {
        use std::os::unix::process::ExitStatusExt;

        ExitStatusExt::from_raw(code << 8)
    }

    fn timed_output(
        status: Option<std::process::ExitStatus>,
        stdout: &str,
        stderr: &str,
        timed_out: bool,
    ) -> TimedOutput {
        TimedOutput {
            status,
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
            duration: Duration::ZERO,
            timed_out,
        }
    }

    #[test]
    fn test_covered_by_reuses_successful_argv_and_retains_each_observation() -> Result<(), String> {
        for host in [FilePolicyHost::Unix, FilePolicyHost::Windows] {
            let other = if host == FilePolicyHost::Unix {
                FilePolicyHost::Windows
            } else {
                FilePolicyHost::Unix
            };
            let commands = [
                FilePolicyTestCommand {
                    host: Some(other),
                    ..common(1, "cargo test shared")
                },
                common(2, "cargo test shared"),
                FilePolicyTestCommand {
                    host: Some(host),
                    ..common(3, "cargo\ttest  shared")
                },
                common(4, "cargo test distinct"),
                FilePolicyTestCommand {
                    host: Some(other),
                    ..common(5, "cargo test shared")
                },
                common(6, "cargo test shared"),
            ];
            let mut calls = Vec::new();
            let observations =
                validate_test_covered_by_with("policy.toml", &commands, host, |args| {
                    calls.push(args.to_vec());
                    Ok((
                        true,
                        format!("{}: test\n", args[1]),
                        "status: success".to_string(),
                    ))
                })?;
            assert_eq!(
                calls,
                [
                    ["test", "shared", "--", "--list", "--format", "terse"]
                        .map(str::to_string)
                        .to_vec(),
                    ["test", "distinct", "--", "--list", "--format", "terse"]
                        .map(str::to_string)
                        .to_vec(),
                ]
            );
            assert_eq!(observations.len(), commands.len());
            for (index, observation) in observations.iter().enumerate() {
                let selector = &commands[index];
                assert_eq!(observation.selector.line, selector.line);
                assert_eq!(observation.selector.command, selector.command);
                assert_eq!(observation.selector.host, selector.host);
                assert_eq!(observation.host, host);
                let status = if index == 0 || index == 4 {
                    assert!(observation.selected.is_none());
                    "not_applicable; selected=not_enumerated".to_string()
                } else {
                    let subject = if index == 3 { "distinct" } else { "shared" };
                    assert_eq!(
                        observation.selected.as_deref(),
                        Some([subject.to_string()].as_slice())
                    );
                    format!("applicable; selected=1; tests=[{subject}]")
                };
                assert_eq!(
                    observation.render(),
                    format!(
                        "line {}; host={}; declared={}; {status}; `{}`",
                        selector.line,
                        host.name(),
                        selector.host.map_or("all", FilePolicyHost::name),
                        selector.command
                    )
                );
            }
        }
        Ok(())
    }

    #[test]
    fn test_covered_by_argv_identity_preserves_argument_boundaries() -> Result<(), String> {
        let commands = [
            common(1, "cargo test a\u{1f}b"),
            common(2, "cargo test a b"),
        ];
        let mut calls = Vec::new();
        let observations = validate_test_covered_by_with(
            "policy.toml",
            &commands,
            FilePolicyHost::Unix,
            |args| {
                calls.push(args.to_vec());
                let subject = if calls.len() == 1 {
                    "one_argument"
                } else {
                    "two_arguments"
                };
                Ok((true, format!("{subject}: test\n"), String::new()))
            },
        )?;
        assert_eq!(calls.len(), 2);
        assert_ne!(calls[0], calls[1]);
        assert_eq!(calls[0].join("\u{1f}"), calls[1].join("\u{1f}"));
        assert_eq!(
            observations[0].selected.as_deref(),
            Some(["one_argument".to_string()].as_slice())
        );
        assert_eq!(
            observations[1].selected.as_deref(),
            Some(["two_arguments".to_string()].as_slice())
        );
        Ok(())
    }

    #[test]
    fn test_covered_by_reuse_does_not_cross_invocations() -> Result<(), String> {
        let commands = [
            common(8, "cargo test shared"),
            common(9, "cargo test shared"),
        ];
        let mut calls = 0;
        for subject in ["first", "second"] {
            let observations = validate_test_covered_by_with(
                "policy.toml",
                &commands,
                FilePolicyHost::Unix,
                |_| {
                    calls += 1;
                    Ok((true, format!("{subject}: test\n"), String::new()))
                },
            )?;
            for observation in observations {
                assert_eq!(observation.selected, Some(vec![subject.to_string()]));
            }
        }
        assert_eq!(calls, 2);
        let failed =
            validate_test_covered_by_with("policy.toml", &commands, FilePolicyHost::Unix, |_| {
                calls += 1;
                Err("fresh spawn failure".to_string())
            });
        assert_eq!(calls, 3);
        assert_eq!(
            failed.err().as_deref(),
            Some("policy.toml:8 enumerate `cargo test shared`: fresh spawn failure")
        );
        Ok(())
    }

    #[test]
    fn test_covered_by_reuse_preserves_later_rejections() {
        let commands = [
            common(2, "cargo test good"),
            common(3, "cargo test good"),
            common(17, "cargo test bad"),
        ];
        let cases = [
            (Ok((true, "not_a_test: benchmark\n".to_string(), String::new())),
             "policy.toml:17 test-valued `covered_by` selects zero tests: `cargo test bad`".to_string()),
            (Ok((false, "partial: test\n".to_string(), "status: exit 1".to_string())),
             "policy.toml:17 test-valued `covered_by` could not be enumerated: `cargo test bad`\nstdout: partial: test\n\nstderr: status: exit 1".to_string()),
            (Ok((true, "partial: test\n".to_string(), format!("{COVERED_BY_INSTRUMENT_PREFIX} timed out"))),
             format!("policy.toml:17 test-valued `covered_by` enumeration did not finish (instrument timeout, not an unresolved pointer): `cargo test bad`\nstdout: partial: test\n\nstderr: {COVERED_BY_INSTRUMENT_PREFIX} timed out")),
            (Err("spawn failed".to_string()), "policy.toml:17 enumerate `cargo test bad`: spawn failed".to_string()),
        ];
        for (failure, expected) in cases {
            let mut calls = 0;
            let result = validate_test_covered_by_with(
                "policy.toml",
                &commands,
                FilePolicyHost::Unix,
                |args| {
                    calls += 1;
                    if args[1] == "good" {
                        Ok((true, "good: test\n".to_string(), String::new()))
                    } else {
                        failure.clone()
                    }
                },
            );
            assert_eq!(result.err(), Some(expected));
            assert_eq!(calls, 2);
        }
    }

    #[test]
    fn test_covered_by_output_mapping_fails_closed_with_partial_diagnostics() -> Result<(), String>
    {
        let failed_status = status(1);
        let failed_status_text = failed_status.to_string();
        let cases = [
            (
                timed_output(
                    Some(failed_status),
                    "selected_case: test\n",
                    "compiler stderr",
                    false,
                ),
                (format!("status: {failed_status_text}"), "compiler stderr"),
            ),
            (
                timed_output(
                    Some(status(0)),
                    "selected_case: test\n",
                    "partial stderr",
                    true,
                ),
                ("timed out after 300s".to_string(), "partial stderr"),
            ),
            (
                timed_output(None, "selected_case: test\n", "spawn stderr", false),
                ("status: not available".to_string(), "spawn stderr"),
            ),
        ];

        for (output, (expected_status, expected_stderr)) in cases {
            let (success, stdout, stderr) =
                enumeration_result(output, TEST_COVERED_BY_LIST_TIMEOUT);
            if success || stdout != "selected_case: test\n" || !stderr.contains(&expected_status) {
                return Err(format!(
                    "enumeration output mapping was not fail-closed: success={success}, stdout={stdout:?}, stderr={stderr:?}"
                ));
            }
            if !stderr.contains(expected_stderr) {
                return Err(format!(
                    "enumeration stderr payload was lost: expected={expected_stderr:?}, actual={stderr:?}"
                ));
            }
        }

        Ok(())
    }

    #[test]
    fn test_covered_by_classification_is_token_aware() -> Result<(), String> {
        for command in ["cargo test", "cargo\ttest -p xtask", "cargo\ntest filtered"] {
            if !is_cargo_test_command(command) {
                return Err(format!(
                    "cargo-test command was not classified: {command:?}"
                ));
            }
        }
        for command in ["cargo testable", "cargo check", "xcargo test"] {
            if is_cargo_test_command(command) {
                return Err(format!("non-test command was classified: {command:?}"));
            }
        }
        Ok(())
    }

    #[test]
    fn test_covered_by_host_selection_retains_common_and_discloses_inapplicable()
    -> Result<(), String> {
        let commands = [
            common(1, "cargo test common_case"),
            FilePolicyTestCommand {
                host: Some(FilePolicyHost::Unix),
                ..common(2, "cargo test unix_case")
            },
            FilePolicyTestCommand {
                host: Some(FilePolicyHost::Windows),
                ..common(3, "cargo test windows_case")
            },
        ];
        for host in [FilePolicyHost::Unix, FilePolicyHost::Windows] {
            let mut enumerated = Vec::new();
            let observations =
                validate_test_covered_by_with("policy.toml", &commands, host, |args| {
                    let subject = args.get(1).ok_or("missing test filter")?.clone();
                    enumerated.push(subject.clone());
                    Ok((true, format!("{subject}: test\n"), String::new()))
                })?;
            let applicable = format!("{}_case", host.name());
            assert_eq!(enumerated, ["common_case", applicable.as_str()]);
            assert_eq!(observations.len(), 3);
            assert_eq!(
                observations[0].selected.as_deref(),
                Some(["common_case".to_string()].as_slice())
            );
            for observation in observations {
                let rendered = observation.render();
                assert!(rendered.contains(&format!("host={}", host.name())));
                if observation
                    .selector
                    .host
                    .is_some_and(|declared| declared != host)
                {
                    assert!(observation.selected.is_none());
                    assert!(rendered.contains("not_applicable; selected=not_enumerated;"));
                    assert!(!rendered.contains("tests=["));
                } else {
                    assert!(rendered.contains("applicable; selected=1; tests=["));
                }
            }
        }
        Ok(())
    }

    #[test]
    fn test_covered_by_empty_applicable_host_selector_is_rejected() -> Result<(), String> {
        for host in [FilePolicyHost::Unix, FilePolicyHost::Windows] {
            for declared in [None, Some(host)] {
                let commands = [FilePolicyTestCommand {
                    host: declared,
                    ..common(1, "cargo test nonexistent_subject")
                }];
                let result = validate_test_covered_by_with("policy.toml", &commands, host, |_| {
                    Ok((true, "0 tests, 0 benchmarks\n".to_string(), String::new()))
                });
                match result {
                    Err(error) if error.contains("selects zero tests") => {}
                    other => {
                        return Err(format!("empty applicable selector did not fail: {other:?}"));
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn test_covered_by_unknown_host_family_is_rejected() {
        assert_eq!(FilePolicyHost::parse("unix"), Ok(FilePolicyHost::Unix));
        assert_eq!(
            FilePolicyHost::parse("windows"),
            Ok(FilePolicyHost::Windows)
        );
        for unsupported in ["", "linux", "Windows", "wasm", "unix,windows"] {
            assert_eq!(
                FilePolicyHost::parse(unsupported),
                Err(format!(
                    "unsupported file-policy host family `{unsupported}`"
                ))
            );
        }
    }

    #[test]
    fn test_covered_by_requires_nonzero_successful_enumeration() -> Result<(), String> {
        let commands = [common(7, "cargo test -p xtask missing-filter")];
        let empty =
            validate_test_covered_by_with("policy.toml", &commands, FilePolicyHost::Unix, |_| {
                Ok((true, String::new(), String::new()))
            });
        let failed =
            validate_test_covered_by_with("policy.toml", &commands, FilePolicyHost::Unix, |_| {
                Ok((false, String::new(), "instrument failed".to_string()))
            });
        let nonzero =
            validate_test_covered_by_with("policy.toml", &commands, FilePolicyHost::Unix, |_| {
                Ok((true, "selected_case: test\n".to_string(), String::new()))
            });
        if empty.is_err() && failed.is_err() && nonzero.is_ok() {
            Ok(())
        } else {
            Err("test-valued covered_by did not fail closed on its denominator".to_string())
        }
    }

    #[test]
    fn test_covered_by_production_wrapper_enumerates_through_cargo() -> Result<(), String> {
        // End-to-end pin on the production wrapper (not the injected
        // closure): the args construction, bounded capture, and status
        // mapping all run for real, and an existing test filter enumerates
        // successfully.
        let commands = [common(
            12,
            "cargo test -p xtask test_covered_by_classification_is_token_aware",
        )];
        validate_test_covered_by("policy.toml", &commands).map(|_| ())
    }

    #[test]
    fn test_covered_by_production_wrapper_preserves_cargo_stderr() -> Result<(), String> {
        let commands = [common(17, "cargo test -p package-that-does-not-exist-3528")];
        let error = match validate_test_covered_by("policy.toml", &commands) {
            Ok(_) => return Err("failed Cargo enumeration unexpectedly passed".to_string()),
            Err(error) => error,
        };

        if error.contains("did not match any packages")
            && error.contains("stderr:")
            && error.contains("status:")
        {
            Ok(())
        } else {
            Err(format!("Cargo enumeration diagnostics were lost: {error}"))
        }
    }

    #[test]
    fn test_covered_by_preserves_enumeration_diagnostics() -> Result<(), String> {
        let commands = [common(19, "cargo test -p xtask missing-filter")];
        let error = match validate_test_covered_by_with(
            "policy.toml",
            &commands,
            FilePolicyHost::Unix,
            |_| {
                Ok((
                    false,
                    "compiler stdout".to_string(),
                    "runner stderr".to_string(),
                ))
            },
        ) {
            Ok(_) => return Err("failed enumeration did not remain fail-closed".to_string()),
            Err(error) => error,
        };

        if error.contains("cargo test -p xtask missing-filter")
            && error.contains("compiler stdout")
            && error.contains("runner stderr")
        {
            Ok(())
        } else {
            Err(format!("enumeration diagnostics were lost: {error}"))
        }
    }

    #[test]
    fn test_covered_by_build_args_stop_before_the_list_harness() -> Result<(), String> {
        let args = [
            "test",
            "-p",
            "xtask",
            "some_filter",
            "--",
            "--list",
            "--format",
            "terse",
        ]
        .map(str::to_string);
        let build = build_args_before_list(&args)
            .ok_or("ordinary cargo test lost its separate build step")?;
        let expected = ["test", "-p", "xtask", "some_filter", "--no-run"].map(str::to_string);
        if build != expected {
            return Err(format!("build args drifted: {build:?}"));
        }
        let spawns = enumeration_spawns(&args);
        if spawns.len() != 2
            || spawns[0].args != expected
            || spawns[0].timeout != TEST_COVERED_BY_BUILD_TIMEOUT
            || spawns[1].args != args
            || spawns[1].timeout != TEST_COVERED_BY_LIST_TIMEOUT
        {
            return Err(
                "ordinary pointer no longer warms with --no-run before the list cap".to_string(),
            );
        }
        Ok(())
    }

    #[test]
    fn test_covered_by_doc_tests_do_not_append_no_run() -> Result<(), String> {
        let args = [
            "test",
            "--workspace",
            "--doc",
            "--",
            "--list",
            "--format",
            "terse",
        ]
        .map(str::to_string);
        if build_args_before_list(&args).is_none() {
            Ok(())
        } else {
            Err("cargo test --doc must not be combined with --no-run".to_string())
        }
    }

    #[test]
    fn test_covered_by_doc_pointer_is_enumerated_once_under_the_build_budget() -> Result<(), String>
    {
        let args = [
            "test",
            "--workspace",
            "--doc",
            "--",
            "--list",
            "--format",
            "terse",
        ]
        .map(str::to_string);
        let spawns = enumeration_spawns(&args);
        if spawns.len() != 1 {
            return Err(format!(
                "doc pointer spawned twice, which recompiles doctests ({})",
                spawns.len()
            ));
        }
        if spawns[0].timeout != TEST_COVERED_BY_BUILD_TIMEOUT {
            return Err(
                "doc enumeration was charged against the list cap instead of the build budget"
                    .to_string(),
            );
        }
        if spawns[0].args.iter().any(|arg| arg == "--no-run") {
            return Err("doc enumeration appended --no-run, which Cargo rejects".to_string());
        }
        if spawns[0].args != args {
            return Err("doc enumeration changed the list command".to_string());
        }
        if spawns[0].description != "test-valued covered_by doc enumeration" {
            return Err("doc enumeration is no longer the spawn that counts tests".to_string());
        }
        Ok(())
    }

    #[test]
    fn test_covered_by_instrument_timeout_is_not_an_unresolved_pointer() -> Result<(), String> {
        let commands = [common(4, "cargo test -p xtask slow_filter")];
        let error = match validate_test_covered_by_with(
            "policy.toml",
            &commands,
            FilePolicyHost::Unix,
            |_| {
                Ok((
                    false,
                    String::new(),
                    format!("{COVERED_BY_INSTRUMENT_PREFIX} timed out after 300s"),
                ))
            },
        ) {
            Ok(_) => {
                return Err("instrument timeout unexpectedly passed".to_string());
            }
            Err(error) => error,
        };
        if error.contains("not an unresolved pointer") && !error.contains("selects zero tests") {
            Ok(())
        } else {
            Err(format!(
                "instrument timeout was reported as an unresolved pointer: {error}"
            ))
        }
    }

    fn lock_wait_output(timed_out: bool) -> TimedOutput {
        timed_output(
            None,
            "",
            "    Blocking waiting for file lock on build directory\n",
            timed_out,
        )
    }

    fn lock_spawn() -> CoveredBySpawn {
        CoveredBySpawn {
            args: vec!["test".to_string()],
            timeout: TEST_COVERED_BY_LIST_TIMEOUT,
            description: "test",
        }
    }

    fn timed_for(
        millis: u64,
        status: Option<std::process::ExitStatus>,
        stdout: &str,
        stderr: &str,
        timed_out: bool,
    ) -> TimedOutput {
        let mut output = timed_output(status, stdout, stderr, timed_out);
        output.duration = Duration::from_millis(millis);
        output
    }

    #[test]
    fn test_covered_by_lock_wait_attempts_are_all_recorded_with_their_durations()
    -> Result<(), String> {
        let commands = [
            common(1, "cargo test -p xtask slow_filter"),
            common(2, "cargo test -p xtask other_filter"),
        ];
        let lock = "    Blocking waiting for file lock on build directory\n";
        let mut script = vec![
            (
                "slow_filter",
                "--no-run",
                timed_for(1100, None, "", lock, true),
            ),
            (
                "slow_filter",
                "--no-run",
                timed_for(2200, None, "", lock, true),
            ),
            (
                "slow_filter",
                "--no-run",
                timed_for(3300, Some(status(0)), "", "", false),
            ),
            (
                "slow_filter",
                "--list",
                timed_for(4400, Some(status(0)), "slow_case: test\n", "", false),
            ),
            // The second selector shares the first's warmed build, so it only
            // lists.
            (
                "other_filter",
                "--list",
                timed_for(600, Some(status(0)), "other_case: test\n", "", false),
            ),
        ]
        .into_iter();
        let mut rows = Vec::new();
        let observations = super::validate_test_covered_by_recorded_with(
            "policy.toml",
            &commands,
            FilePolicyHost::Unix,
            &mut rows,
            |spawn| {
                let (filter, flag, output) = script
                    .next()
                    .ok_or_else(|| "unexpected extra spawn".to_string())?;
                if !spawn.args.iter().any(|arg| arg == filter)
                    || !spawn.args.iter().any(|arg| arg == flag)
                {
                    return Err(format!("unexpected argv {:?}", spawn.args));
                }
                Ok(output)
            },
        )?;
        if script.next().is_some() {
            return Err("scripted spawns were not all consumed".to_string());
        }
        let selected = observations
            .iter()
            .map(|observation| observation.selected.clone())
            .collect::<Vec<_>>();
        if selected
            != [
                Some(vec!["slow_case".to_string()]),
                Some(vec!["other_case".to_string()]),
            ]
        {
            return Err(format!("selectors were not kept distinct: {selected:?}"));
        }
        let elapsed = rows.iter().filter_map(|row| row.elapsed_ms).sum::<u128>();
        if rows.len() != 5 || elapsed != 11_600 {
            return Err(format!(
                "every attempt must be recorded: {} rows, {elapsed} ms",
                rows.len()
            ));
        }
        let first_selector = rows
            .iter()
            .take(4)
            .filter_map(|row| row.elapsed_ms)
            .sum::<u128>();
        if first_selector != 11_000 {
            return Err(format!("lock-wait attempts lost time: {first_selector}"));
        }
        Ok(())
    }

    #[test]
    fn test_covered_by_lock_wait_does_not_consume_the_budget_but_stays_bounded()
    -> Result<(), String> {
        // A lock wait that clears within the retry bound completes.
        let mut calls = 0;
        let output = capture_past_lock_wait(&lock_spawn(), &mut |_| {
            calls += 1;
            Ok(if calls <= 3 {
                lock_wait_output(true)
            } else {
                timed_output(Some(status(0)), "case: test\n", "", false)
            })
        })?;
        if output.timed_out || calls != 4 {
            return Err(format!("lock wait was not retried to completion: {calls}"));
        }
        // Negative experiment: a lock that never clears still fails, distinctly.
        let mut calls = 0;
        let output = capture_past_lock_wait(&lock_spawn(), &mut |_| {
            calls += 1;
            Ok(lock_wait_output(true))
        })?;
        if !output.timed_out || calls != LOCK_WAIT_RETRIES + 1 {
            return Err(format!("unbounded or swallowed lock wait: {calls}"));
        }
        let (success, _, stderr) = enumeration_result(output, TEST_COVERED_BY_LIST_TIMEOUT);
        if success || !stderr.starts_with(COVERED_BY_LOCK_WAIT_PREFIX) {
            return Err(format!("lock-wait timeout was not distinct: {stderr}"));
        }
        // Negative experiment: a real timeout with no lock wait is not retried.
        let mut calls = 0;
        let output = capture_past_lock_wait(&lock_spawn(), &mut |_| {
            calls += 1;
            Ok(timed_output(None, "", "Compiling x\n", true))
        })?;
        let (_, _, stderr) = enumeration_result(output, TEST_COVERED_BY_LIST_TIMEOUT);
        if calls != 1 || !stderr.starts_with(COVERED_BY_INSTRUMENT_PREFIX) {
            return Err(format!("real timeout was retried or mislabeled: {calls}"));
        }
        // Negative experiment: a stale lock line followed by compile output is
        // not a lock wait, so it is neither retried nor labeled as one.
        let mut calls = 0;
        let output = capture_past_lock_wait(&lock_spawn(), &mut |_| {
            calls += 1;
            Ok(timed_output(
                None,
                "",
                "    Blocking waiting for file lock on build directory\n   Compiling xtask v0.1.0\n",
                true,
            ))
        })?;
        let (_, _, stderr) = enumeration_result(output, TEST_COVERED_BY_LIST_TIMEOUT);
        if calls != 1 || !stderr.starts_with(COVERED_BY_INSTRUMENT_PREFIX) {
            return Err(format!(
                "stale lock line was retried or mislabeled: {calls}: {stderr}"
            ));
        }
        // Retry history is kept in the report text.
        let output = capture_past_lock_wait(&lock_spawn(), &mut |_| Ok(lock_wait_output(true)))?;
        if !output.stderr.starts_with("xtask: retried 6 time(s)") {
            return Err(format!("retry history missing: {}", output.stderr));
        }
        // Both timeouts surface as instrument failures through the validator.
        let commands = [common(4, "cargo test -p xtask slow_filter")];
        let error = match validate_test_covered_by_with(
            "policy.toml",
            &commands,
            FilePolicyHost::Unix,
            |_| {
                Ok((
                    false,
                    String::new(),
                    format!("{COVERED_BY_LOCK_WAIT_PREFIX} timed out"),
                ))
            },
        ) {
            Ok(_) => return Err("lock-wait timeout unexpectedly passed".to_string()),
            Err(error) => error,
        };
        if !is_covered_by_instrument_failure(&error) {
            return Err(format!("lock wait not an instrument failure: {error}"));
        }
        // Negative experiment: the phrase quoted in embedded cargo output of a
        // genuine failure does not make it an instrument failure.
        let spoofed =
            validate_test_covered_by_with("policy.toml", &commands, FilePolicyHost::Unix, |_| {
                Ok((
                    false,
                    String::new(),
                    format!("error: {INSTRUMENT_FAILURE_PHRASE}"),
                ))
            });
        match spoofed {
            Err(error) if !is_covered_by_instrument_failure(&error) => {}
            other => return Err(format!("spoofed phrase read as instrument: {other:?}")),
        }
        if is_covered_by_instrument_failure("policy.toml:9 selects zero tests") {
            return Err("unresolved pointer read as instrument failure".to_string());
        }
        Ok(())
    }

    #[test]
    fn test_covered_by_lock_wait_ceiling_stops_retrying_and_names_the_ceiling() -> Result<(), String>
    {
        let attempt = LOCK_WAIT_CEILING / 3 + Duration::from_secs(1);
        // Lock waits whose recorded durations sum past the ceiling stop early,
        // well before the attempt bound.
        let mut calls = 0;
        let output = capture_past_lock_wait(&lock_spawn(), &mut |_| {
            calls += 1;
            let mut output = lock_wait_output(true);
            output.duration = attempt;
            Ok(output)
        })?;
        if calls != 3 || calls > LOCK_WAIT_RETRIES {
            return Err(format!("ceiling did not stop the retries: {calls}"));
        }
        let (success, _, stderr) = enumeration_result(output, TEST_COVERED_BY_LIST_TIMEOUT);
        let ceiling = format!("{LOCK_WAIT_CEILING:?}");
        if success
            || !stderr.starts_with(COVERED_BY_LOCK_WAIT_PREFIX)
            || !stderr.contains(&format!("{LOCK_WAIT_CEILING_NOTE_PREFIX} of {ceiling}"))
        {
            return Err(format!("ceiling stop did not name the ceiling: {stderr}"));
        }
        // Still an instrument failure through the validator, never a pass.
        let commands = [common(4, "cargo test -p xtask slow_filter")];
        let error = match validate_test_covered_by_with(
            "policy.toml",
            &commands,
            FilePolicyHost::Unix,
            |_| Ok((false, String::new(), stderr.clone())),
        ) {
            Ok(_) => return Err("ceiling stop unexpectedly passed".to_string()),
            Err(error) => error,
        };
        if !is_covered_by_instrument_failure(&error) {
            return Err(format!("ceiling stop not an instrument failure: {error}"));
        }
        // A sum landing exactly on the ceiling is exhausted: the comparison is
        // inclusive.
        let mut calls = 0;
        let output = capture_past_lock_wait(&lock_spawn(), &mut |_| {
            calls += 1;
            let mut output = lock_wait_output(true);
            output.duration = LOCK_WAIT_CEILING / 3;
            Ok(output)
        })?;
        if calls != 3 || !output.stderr.contains(LOCK_WAIT_CEILING_NOTE_PREFIX) {
            return Err(format!("exact-ceiling sum was not exhausted: {calls}"));
        }
        // A sequence below the ceiling is still retried to completion.
        let mut calls = 0;
        let output = capture_past_lock_wait(&lock_spawn(), &mut |_| {
            calls += 1;
            Ok(if calls <= 2 {
                let mut output = lock_wait_output(true);
                output.duration = LOCK_WAIT_CEILING / 4;
                output
            } else {
                timed_output(Some(status(0)), "case: test\n", "", false)
            })
        })?;
        if output.timed_out || calls != 3 || output.stderr.contains(LOCK_WAIT_CEILING_NOTE_PREFIX) {
            return Err(format!("sub-ceiling wait was cut short: {calls}"));
        }
        // Negative experiment: a long real (non-lock) timeout is not a lock
        // wait, is not retried, and is not labeled as the ceiling.
        let mut calls = 0;
        let output = capture_past_lock_wait(&lock_spawn(), &mut |_| {
            calls += 1;
            let mut output = timed_output(None, "", "Compiling x\n", true);
            output.duration = LOCK_WAIT_CEILING * 2;
            Ok(output)
        })?;
        let (_, _, stderr) = enumeration_result(output, TEST_COVERED_BY_LIST_TIMEOUT);
        if calls != 1
            || !stderr.starts_with(COVERED_BY_INSTRUMENT_PREFIX)
            || stderr.contains(LOCK_WAIT_CEILING_NOTE_PREFIX)
        {
            return Err(format!("real timeout changed behavior: {calls}: {stderr}"));
        }
        Ok(())
    }

    #[test]
    fn test_covered_by_zero_tests_stay_unresolved_and_slow_success_passes() -> Result<(), String> {
        let commands = [common(9, "cargo test -p xtask missing_pointer")];
        let unresolved = match validate_test_covered_by_with(
            "policy.toml",
            &commands,
            FilePolicyHost::Unix,
            |_| Ok((true, String::new(), String::new())),
        ) {
            Ok(_) => return Err("zero-test enumeration unexpectedly passed".to_string()),
            Err(error) => error,
        };
        if !unresolved.contains("selects zero tests") || unresolved.contains("instrument timeout") {
            return Err(format!(
                "unresolved pointer was not kept distinct: {unresolved}"
            ));
        }
        let slow_but_complete =
            validate_test_covered_by_with("policy.toml", &commands, FilePolicyHost::Unix, |_| {
                Ok((true, "slow_case: test\n".to_string(), String::new()))
            });
        if slow_but_complete.is_ok() {
            Ok(())
        } else {
            Err(format!(
                "a completed enumeration failed only because it was slow: {slow_but_complete:?}"
            ))
        }
    }
}

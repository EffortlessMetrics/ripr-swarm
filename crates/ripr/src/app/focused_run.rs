//! Caller-supplied focused test result for a repair attempt (#13203 slice B).
//!
//! ripr does not run `cargo test` (ADR 0022 keeps execution outside ripr).
//! The caller runs the focused test and hands the libtest events to the after
//! phase inside a small envelope. This module only validates and normalizes
//! that claim: it must be well formed, bound to the attempt id and the
//! repository HEAD the after phase evaluates, name the attempt's selected test
//! file, and contain at least one test that ran (`ok` or `failed`) and that is
//! an executable test defined in that file (or, for an inline proposal, inside
//! the governed module). Anything else is refused and nothing is recorded.
//!
//! The record states what was checked and what was not: ripr validated shape,
//! binding and file membership; it did not observe the run.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

pub(crate) const FOCUSED_RUN_SCHEMA_VERSION: &str = "0.1";
pub(crate) const FOCUSED_RUN_INPUT_KIND: &str = "focused_test_result";
pub(crate) const FOCUSED_RUN_RECORD_KIND: &str = "focused_test_run_record";
pub(crate) const FOCUSED_RUN_ARTIFACT_FILE: &str = "focused-test-result.json";
/// Hard cap on events read from a caller result, so a hostile file cannot
/// make validation unbounded.
const MAX_EVENTS: usize = 200_000;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FocusedRunInput {
    schema_version: String,
    kind: String,
    attempt_id: String,
    head: String,
    /// The test file the focused run executed (`cargo test --test <stem>` for
    /// an integration file, or the production file for an inline module).
    test_file: String,
    /// Libtest JSON events, one object per element.
    libtest_events: Vec<Value>,
}

/// What the attempt authority knows and the caller's claim must match.
pub(crate) struct FocusedRunScope<'a> {
    pub(crate) attempt_id: &'a str,
    /// The repository HEAD the after phase evaluates.
    pub(crate) head: &'a str,
    /// The attempt's selected test file (the proposal's file for a proposal).
    pub(crate) file: &'a str,
    /// Current text of that file, for membership of the ran tests.
    pub(crate) file_source: &'a str,
    /// The governed inline module name when the target is an inline module.
    pub(crate) inline_module: Option<&'a str>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct FocusedRunRecord {
    schema_version: &'static str,
    kind: &'static str,
    attempt_id: String,
    head: String,
    file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    module: Option<String>,
    /// Unique tests that ran in the file or module, sorted.
    ran_tests: Vec<String>,
    passed: usize,
    failed: usize,
    /// Tests that ran but are not defined in the file or module; not counted.
    uncounted_ran: usize,
    claim: &'static str,
}

impl FocusedRunRecord {
    pub(crate) fn counted(&self) -> usize {
        self.ran_tests.len()
    }

    pub(crate) fn render(&self) -> Result<Vec<u8>, String> {
        let mut bytes = serde_json::to_vec_pretty(self)
            .map_err(|error| format!("serialize focused test run record failed: {error}; rerun `ripr agent repair --phase after --test-result FILE`"))?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    pub(crate) fn summary(&self) -> String {
        format!(
            "{} test(s) ran in `{}` ({} passed, {} failed)",
            self.counted(),
            self.file,
            self.passed,
            self.failed
        )
    }
}

/// Validate a caller-supplied focused result against the attempt's scope.
/// Every refusal names the cause and the recovery; nothing is recorded.
pub(crate) fn validate_focused_run(
    raw: &[u8],
    scope: &FocusedRunScope<'_>,
) -> Result<FocusedRunRecord, String> {
    let input: FocusedRunInput = serde_json::from_slice(raw).map_err(|error| {
        format!(
            "the focused test result is not a valid `{FOCUSED_RUN_INPUT_KIND}` document ({error}); expected schema_version, kind, attempt_id, head, test_file and libtest_events; rebuild the result and pass it with `--test-result FILE`"
        )
    })?;
    if input.schema_version != FOCUSED_RUN_SCHEMA_VERSION || input.kind != FOCUSED_RUN_INPUT_KIND {
        return Err(format!(
            "the focused test result must be schema_version `{FOCUSED_RUN_SCHEMA_VERSION}` kind `{FOCUSED_RUN_INPUT_KIND}`; got `{}` / `{}`; pass a result with those fields using `--test-result FILE`",
            input.schema_version, input.kind
        ));
    }
    if input.attempt_id != scope.attempt_id {
        return Err(format!(
            "the focused test result is bound to attempt `{}`, not this attempt `{}`; run the focused test for this attempt and rebuild the result",
            input.attempt_id, scope.attempt_id
        ));
    }
    if input.head != scope.head {
        return Err(format!(
            "the focused test result is bound to HEAD {}, but the after phase evaluates HEAD {}; rerun the focused test at the current HEAD",
            input.head, scope.head
        ));
    }
    if input.test_file != scope.file {
        return Err(format!(
            "the focused test result ran `{}`, but this attempt's selected test file is `{}`; run that file's tests and pass the events with `--test-result FILE`",
            input.test_file, scope.file
        ));
    }
    if input.libtest_events.len() > MAX_EVENTS {
        return Err(format!(
            "the focused test result carries more than {MAX_EVENTS} events; run only the focused test and pass its events with `--test-result FILE`"
        ));
    }

    let defined = executable_test_names(scope.file, scope.file_source)?;
    let name_scope = name_scope(scope);
    let mut ran_in_scope = BTreeSet::new();
    let mut failed_in_scope = BTreeSet::new();
    let mut uncounted = BTreeSet::new();
    for (index, event) in input.libtest_events.iter().enumerate() {
        let object = event.as_object().ok_or_else(|| {
            format!("libtest_events[{index}] is not a JSON object; pass libtest JSON event objects from `cargo test -- --format json -Z unstable-options`")
        })?;
        let kind = object
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("libtest_events[{index}] has no string `type`; pass libtest JSON event objects from `cargo test -- --format json -Z unstable-options`"))?;
        if kind != "test" {
            // Suite and bench events carry no per-test outcome.
            continue;
        }
        let outcome = object
            .get("event")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("libtest_events[{index}] (test) has no string `event`; pass libtest JSON event objects from `cargo test -- --format json -Z unstable-options`"))?;
        let name = object
            .get("name")
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty())
            .ok_or_else(|| format!("libtest_events[{index}] (test) has no `name`; pass libtest JSON event objects from `cargo test -- --format json -Z unstable-options`"))?;
        match outcome {
            "started" | "ignored" | "timeout" => {}
            "ok" | "failed" => {
                if name_is_in_scope(name, &defined, &name_scope) {
                    ran_in_scope.insert(name.to_string());
                    if outcome == "failed" {
                        failed_in_scope.insert(name.to_string());
                    }
                } else {
                    uncounted.insert(name.to_string());
                }
            }
            other => {
                return Err(format!(
                    "libtest_events[{index}] has unknown test event `{other}`; expected started, ok, failed, ignored or timeout; pass the events from `cargo test -- --format json -Z unstable-options`"
                ));
            }
        }
    }
    if ran_in_scope.is_empty() {
        let place = match scope.inline_module {
            Some(module) => format!("the `{module}` module of `{}`", scope.file),
            None => format!("`{}`", scope.file),
        };
        return Err(format!(
            "no test ran in {place}: the result is not counted; run the focused test (for example `cargo test --test <stem>` or `cargo test <module>::`) and pass its libtest JSON events"
        ));
    }
    let failed = failed_in_scope.len();
    Ok(FocusedRunRecord {
        schema_version: FOCUSED_RUN_SCHEMA_VERSION,
        kind: FOCUSED_RUN_RECORD_KIND,
        attempt_id: scope.attempt_id.to_string(),
        head: scope.head.to_string(),
        file: scope.file.to_string(),
        module: scope.inline_module.map(str::to_string),
        passed: ran_in_scope.len() - failed,
        failed,
        uncounted_ran: uncounted.len(),
        ran_tests: ran_in_scope.into_iter().collect(),
        claim: "caller-supplied; ripr validated shape, attempt and HEAD binding, and file membership; ripr did not run or observe the tests",
    })
}

/// A ran test counts only when its final path segment names an executable
/// test defined in the file, and, for an inline module, when its path passes
/// through that module.
/// The libtest path prefix a test inside `file`'s inline module `module` must
/// carry: the file's own module path, then the module. `None` when the file's
/// module path cannot be derived (a bin target, a non-`src` file, a nested
/// `tests/` file); nothing is then counted rather than guessed. A `#[path]`
/// module is not detectable from the file alone and stays a caller's claim.
fn inline_scope_path(file: &str, module: &str) -> Option<Vec<String>> {
    let parts: Vec<&str> = file.split('/').collect();
    let mut path: Vec<String> = Vec::new();
    if let Some(at) = parts.iter().position(|part| *part == "src") {
        let rest = &parts[at + 1..];
        let (last, dirs) = rest.split_last()?;
        if dirs.first() == Some(&"bin") {
            return None;
        }
        let stem = last.strip_suffix(".rs")?;
        path.extend(dirs.iter().map(|dir| dir.to_string()));
        if !(dirs.is_empty() && (stem == "lib" || stem == "main")) && stem != "mod" {
            path.push(stem.to_string());
        }
    } else {
        // An integration-test binary's crate root: `tests/<name>.rs`.
        match parts.as_slice() {
            [.., "tests", last] if last.ends_with(".rs") => {}
            _ => return None,
        }
    }
    path.push(module.to_string());
    Some(path)
}

enum NameScope {
    /// Integration file: the test's function name is the only attribution.
    AnyPath,
    /// Inline module: the libtest path must start with this prefix.
    Prefix(Vec<String>),
    /// Inline module whose path cannot be derived: nothing counts.
    Nothing,
}

fn name_scope(scope: &FocusedRunScope<'_>) -> NameScope {
    match scope.inline_module {
        None => NameScope::AnyPath,
        Some(module) => match inline_scope_path(scope.file, module) {
            Some(prefix) => NameScope::Prefix(prefix),
            None => NameScope::Nothing,
        },
    }
}

fn name_is_in_scope(name: &str, defined: &BTreeSet<String>, scope: &NameScope) -> bool {
    let mut segments: Vec<&str> = name.split("::").collect();
    let Some(function) = segments.pop() else {
        return false;
    };
    if !defined.contains(function) {
        return false;
    }
    match scope {
        NameScope::Prefix(prefix) => {
            segments.len() >= prefix.len()
                && segments.iter().zip(prefix).all(|(a, b)| *a == b.as_str())
        }
        NameScope::AnyPath => true,
        NameScope::Nothing => false,
    }
}

fn executable_test_names(file: &str, source: &str) -> Result<BTreeSet<String>, String> {
    crate::analysis::executable_test_function_names(std::path::Path::new(file), source)
        .map_err(|error| format!("read the test functions of `{file}` failed: {error}; fix the file so it parses, then rerun `ripr agent repair --phase after --test-result FILE`"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "#[test]\nfn adds() {}\n\n#[test]\nfn subtracts() {}\n\nfn helper() {}\n";

    fn scope<'a>(inline_module: Option<&'a str>) -> FocusedRunScope<'a> {
        FocusedRunScope {
            attempt_id: "attempt-1",
            head: "abc123",
            file: "tests/math.rs",
            file_source: SOURCE,
            inline_module,
        }
    }

    fn document(attempt: &str, head: &str, file: &str, events: Value) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "schema_version": "0.1",
            "kind": "focused_test_result",
            "attempt_id": attempt,
            "head": head,
            "test_file": file,
            "libtest_events": events,
        }))
        .unwrap_or_default()
    }

    fn refusal(raw: &[u8], scope: &FocusedRunScope) -> String {
        match validate_focused_run(raw, scope) {
            Ok(record) => format!("unexpectedly accepted: {record:?}"),
            Err(error) => error,
        }
    }

    fn ok(name: &str) -> Value {
        serde_json::json!({"type": "test", "event": "ok", "name": name})
    }

    #[test]
    fn a_ran_test_defined_in_the_file_is_counted() {
        let raw = document(
            "attempt-1",
            "abc123",
            "tests/math.rs",
            serde_json::json!([ok("adds")]),
        );
        let record = validate_focused_run(&raw, &scope(None));
        assert_eq!(
            record.as_ref().map(FocusedRunRecord::counted),
            Ok(1),
            "{record:?}"
        );
    }

    #[test]
    fn zero_in_scope_tests_are_not_counted() {
        let events = serde_json::json!([
            {"type": "test", "event": "started", "name": "adds"},
            {"type": "test", "event": "ignored", "name": "subtracts"},
            ok("elsewhere"),
            ok("helper"),
        ]);
        let raw = document("attempt-1", "abc123", "tests/math.rs", events);
        let error = refusal(&raw, &scope(None));
        assert!(error.contains("no test ran"), "{error}");
    }

    #[test]
    fn duplicates_do_not_pad_the_count() {
        let raw = document(
            "attempt-1",
            "abc123",
            "tests/math.rs",
            serde_json::json!([ok("adds"), ok("adds"), ok("adds")]),
        );
        let record = validate_focused_run(&raw, &scope(None));
        assert_eq!(
            record.as_ref().map(FocusedRunRecord::counted),
            Ok(1),
            "{record:?}"
        );
    }

    #[test]
    fn another_attempt_head_file_or_shape_is_refused() {
        let events = serde_json::json!([ok("adds")]);
        for (raw, needle) in [
            (
                document("other", "abc123", "tests/math.rs", events.clone()),
                "bound to attempt",
            ),
            (
                document("attempt-1", "def456", "tests/math.rs", events.clone()),
                "bound to HEAD",
            ),
            (
                document("attempt-1", "abc123", "tests/other.rs", events.clone()),
                "selected test file",
            ),
            (b"not json".to_vec(), "not a valid"),
            (
                br#"{"schema_version":"0.1","kind":"focused_test_result"}"#.to_vec(),
                "not a valid",
            ),
            (
                document(
                    "attempt-1",
                    "abc123",
                    "tests/math.rs",
                    serde_json::json!([42]),
                ),
                "not a JSON object",
            ),
            (
                document(
                    "attempt-1",
                    "abc123",
                    "tests/math.rs",
                    serde_json::json!([{"type": "test", "event": "maybe", "name": "adds"}]),
                ),
                "unknown test event",
            ),
        ] {
            let error = refusal(&raw, &scope(None));
            assert!(error.contains(needle), "{needle}: {error}");
        }
    }

    #[test]
    fn an_inline_module_counts_only_tests_through_that_module() {
        let source =
            "fn production() {}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn adds() {}\n}\n";
        let inline = FocusedRunScope {
            attempt_id: "attempt-1",
            head: "abc123",
            file: "src/lib.rs",
            file_source: source,
            inline_module: Some("tests"),
        };
        let counted = document(
            "attempt-1",
            "abc123",
            "src/lib.rs",
            serde_json::json!([ok("tests::adds")]),
        );
        let record = validate_focused_run(&counted, &inline);
        assert_eq!(
            record.as_ref().map(FocusedRunRecord::counted),
            Ok(1),
            "{record:?}"
        );
        let wrong_module = document(
            "attempt-1",
            "abc123",
            "src/lib.rs",
            serde_json::json!([ok("other::adds"), ok("adds")]),
        );
        let error = refusal(&wrong_module, &inline);
        assert!(error.contains("no test ran"), "{error}");
    }

    fn inline_counted(file: &str, name: &str) -> Result<usize, String> {
        let source =
            "fn production() {}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn adds() {}\n}\n";
        let inline = FocusedRunScope {
            attempt_id: "attempt-1",
            head: "abc123",
            file,
            file_source: source,
            inline_module: Some("tests"),
        };
        let raw = document("attempt-1", "abc123", file, serde_json::json!([ok(name)]));
        validate_focused_run(&raw, &inline).map(|record| record.counted())
    }

    #[test]
    fn an_inline_module_is_attributed_through_the_files_own_module_path() {
        for (file, name) in [
            ("src/foo.rs", "foo::tests::adds"),
            ("src/a/b.rs", "a::b::tests::adds"),
            ("src/a/mod.rs", "a::tests::adds"),
            ("src/lib.rs", "tests::adds"),
            ("src/main.rs", "tests::adds"),
            ("crates/x/src/foo.rs", "foo::tests::adds"),
            ("tests/it.rs", "tests::adds"),
        ] {
            assert_eq!(inline_counted(file, name), Ok(1), "{file} {name}");
        }
        // A same-named module in another file does not count.
        for (file, name) in [
            ("src/foo.rs", "bar::tests::adds"),
            ("src/foo.rs", "tests::adds"),
            ("src/a/b.rs", "a::tests::adds"),
            ("src/lib.rs", "foo::tests::adds"),
        ] {
            let error = inline_counted(file, name).err().unwrap_or_default();
            assert!(error.contains("no test ran"), "{file} {name}: {error}");
        }
    }

    #[test]
    fn an_inline_module_whose_path_cannot_be_derived_counts_nothing() {
        for file in [
            "src/bin/tool.rs",
            "build.rs",
            "examples/demo.rs",
            "tests/common/mod.rs",
        ] {
            let error = inline_counted(file, "tests::adds")
                .err()
                .unwrap_or_default();
            assert!(error.contains("no test ran"), "{file}: {error}");
        }
    }

    #[test]
    fn a_timeout_event_is_not_terminal_and_never_counts_by_itself() {
        let timeout = serde_json::json!({"type": "test", "event": "timeout", "name": "adds"});
        let raw = document(
            "attempt-1",
            "abc123",
            "tests/math.rs",
            serde_json::json!([timeout.clone(), ok("adds")]),
        );
        let record = validate_focused_run(&raw, &scope(None));
        assert_eq!(
            record.as_ref().map(FocusedRunRecord::counted),
            Ok(1),
            "{record:?}"
        );
        let alone = document(
            "attempt-1",
            "abc123",
            "tests/math.rs",
            serde_json::json!([timeout]),
        );
        let error = refusal(&alone, &scope(None));
        assert!(error.contains("no test ran"), "{error}");
    }
}

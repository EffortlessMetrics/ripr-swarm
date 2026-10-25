//! Result.map payload forwarding and independent per-test observation (#9870).
use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const ORIGINAL: &str = "Ok(sign_extend(self.try_get_uint(nbytes)?, nbytes))";
const FORWARDED: &str = "self.try_get_uint(nbytes).map(|v| sign_extend(v, nbytes))";
const SOURCE: &str = include_str!(
    "../../../fixtures/rust-verdict-corpus/subjects/authored-result-map-callback/src/lib.rs.txt"
);
struct Subject(PathBuf);
impl Drop for Subject {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn analyze(tail: &str, tests: &str, extra: &str) -> Result<Value, String> {
    analyze_with_reader(tail, tests, extra, false, false)
}
fn analyze_with_reader(
    tail: &str,
    tests: &str,
    extra: &str,
    asynchronous: bool,
    ambiguous: bool,
) -> Result<Value, String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "ripr-result-map-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let subject = Subject(root);
    std::fs::create_dir_all(subject.0.join("src")).map_err(|e| e.to_string())?;
    std::fs::write(
        subject.0.join("Cargo.toml"),
        "[package]\nname='result_map_callback'\nversion='0.1.0'\nedition='2021'\n",
    )
    .map_err(|e| e.to_string())?;
    let production = SOURCE
        .split("#[cfg(test)]")
        .next()
        .ok_or("production source")?;
    let production = if asynchronous {
        production.replace("fn try_get_uint", "async fn try_get_uint")
    } else {
        production.to_string()
    };
    let production = if ambiguous {
        let declaration =
            "    async fn try_get_uint(&mut self, nbytes: usize) -> Result<u64, TryGetError>;\n";
        assert_eq!(production.matches(declaration).count(), 1);
        let production = production.replace(declaration, "");
        let boundary = "}\n\nimpl Buf for &[u8] {\n    async fn try_get_uint";
        assert_eq!(production.matches(boundary).count(), 1);
        production.replace(boundary, "    async fn try_get_uint(&mut self, nbytes: usize) -> Result<u64, TryGetError>; } impl Buf for &[u8] { async fn try_get_uint")
    } else {
        production
    };
    assert_eq!(production.matches(ORIGINAL).count(), 1);
    let source = format!(
        "{}\n{extra}\n#[cfg(test)] mod tests {{ use super::*; {tests} }}",
        production.replace(ORIGINAL, tail)
    );
    std::fs::write(subject.0.join("src/lib.rs"), source).map_err(|e| e.to_string())?;
    let line = production
        .lines()
        .position(|line| line.contains(ORIGINAL))
        .ok_or("changed tail")?
        + 1;
    let diff = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -{line},1 +{line},1 @@\n-        {ORIGINAL}\n+        {tail}\n"
    );
    let patch = subject.0.join("change.diff");
    std::fs::write(&patch, diff).map_err(|e| e.to_string())?;
    let report = check_workspace(CheckInput {
        root: subject.0.clone(),
        diff_file: Some(patch),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let json: Value = serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
        .map_err(|e| e.to_string())?;
    let matches = json["findings"]
        .as_array()
        .ok_or("findings")?
        .iter()
        .filter(|f| {
            f["source_currentness"] == "candidate_current"
                && f["probe"]["family"] == "return_value"
                && f["probe"]["expression"] == tail
        })
        .collect::<Vec<_>>();
    assert_eq!(matches.len(), 1, "one current changed return: {json}");
    Ok(matches[0].clone())
}
const GOOD: &str = "#[test] fn exact_ok() { let mut a = &[0xff, 0xff, 0xff][..]; assert_eq!(a.try_get_int(3), Ok(-1)); }";
const ERROR_ONLY: &str = "#[test] fn error_only() { let mut a = &[][..]; assert_eq!(a.try_get_int(3), Err(TryGetError)); }";
#[test]
fn result_map_forwards_the_exact_callback_payload() -> Result<(), String> {
    for tail in [
        FORWARDED,
        "self.try_get_uint(nbytes).map(|payload| { sign_extend(payload, nbytes) })",
    ] {
        let finding = analyze(tail, GOOD, "")?;
        assert_eq!(finding["ripr"]["propagate"]["state"], "yes", "{finding}");
        assert_eq!(finding["ripr"]["discriminate"]["state"], "yes", "{finding}");
        assert_eq!(finding["classification"], "exposed", "{finding}");
    }
    Ok(())
}
#[test]
fn result_map_refuses_discard_recompute_constant_and_changed_payloads() -> Result<(), String> {
    for tail in [
        "self.try_get_uint(nbytes).map(|v| { let _ = sign_extend(v, nbytes); -1 })",
        "self.try_get_uint(nbytes).map(|v| { let _ = sign_extend(v, nbytes); sign_extend(0xffffff, 3) })",
        "self.try_get_uint(nbytes).map(|_v| -1)",
        "self.try_get_uint(nbytes).map(|v| sign_extend(0, nbytes))",
        "self.try_get_uint(nbytes).map(|v| sign_extend(v, nbytes * 2))",
        "self.try_get_uint(nbytes).map_err(|v| v).map(|v| sign_extend(v, nbytes))",
        "self.try_get_uint(nbytes).map(|v| sign_extend(v, nbytes)).map(|_| -1)",
        "self.try_get_uint(nbytes + 1).map(|v| sign_extend(v, nbytes))",
        "self.try_get_uint(nbytes).map(|v| sign_extend(v + v, nbytes))",
        "self.try_get_uint(nbytes).map(|v| if true { sign_extend(v, nbytes) } else { -1 })",
        "self.try_get_uint(nbytes).map(move |v| sign_extend(v, nbytes))",
    ] {
        let finding = analyze(tail, GOOD, "")?;
        assert_ne!(finding["ripr"]["propagate"]["state"], "yes", "{finding}");
        assert_ne!(finding["classification"], "exposed", "{finding}");
    }
    Ok(())
}
#[test]
fn result_map_requires_an_ok_pin_from_the_observing_test() -> Result<(), String> {
    for test in [
        ERROR_ONLY.to_string(),
        "#[test] fn error_plus_unrelated_transform() { let mut a = &[][..]; assert_eq!(a.try_get_int(3), Err(TryGetError)); assert_eq!(sign_extend(0xffffff, 3), -1); }".to_string(),
        format!(
            "{ERROR_ONLY} #[test] fn merely_reaches() {{ let mut a = &[0xff, 0xff, 0xff][..]; let _ = a.try_get_int(3); }}"
        ),
        format!(
            "#[test] fn deferred() {{ let _f = || {{ let mut a = &[0xff, 0xff, 0xff][..]; assert_eq!(a.try_get_int(3), Ok(-1)); }}; }} {ERROR_ONLY}"
        ),
    ] {
        let finding = analyze(FORWARDED, &test, "")?;
        assert_eq!(finding["ripr"]["propagate"]["state"], "yes", "{finding}");
        assert_ne!(finding["ripr"]["discriminate"]["state"], "yes", "{finding}");
        assert_ne!(finding["classification"], "exposed", "{finding}");
    }
    let finding = analyze(FORWARDED, &format!("{GOOD} {ERROR_ONLY}"), "")?;
    assert_eq!(finding["classification"], "exposed", "{finding}");
    let related = finding["related_tests"].as_array().ok_or("related tests")?;
    let good = related
        .iter()
        .find(|test| test["name"] == "exact_ok")
        .ok_or("Ok observer")?;
    let error = related
        .iter()
        .find(|test| test["name"] == "error_only")
        .ok_or("Err observer")?;
    assert!(good["miss"].is_null(), "{finding}");
    assert_eq!(error["miss"], "observation_unconfirmed", "{finding}");
    Ok(())
}
#[test]
fn result_map_refuses_rebound_standard_names() -> Result<(), String> {
    let finding = analyze(
        FORWARDED,
        GOOD,
        "type Result<T, E> = core::result::Result<T, E>;",
    )?;
    assert_ne!(finding["ripr"]["propagate"]["state"], "yes", "{finding}");
    assert_ne!(finding["classification"], "exposed", "{finding}");
    Ok(())
}

#[test]
fn result_map_refuses_an_async_reader_with_a_discarding_future_map() -> Result<(), String> {
    let extension = "trait FutureMap { fn map<F: FnOnce(u64) -> i64>(self, ignored: F) -> Result<i64, TryGetError>; } impl<T: core::future::Future<Output = Result<u64, TryGetError>>> FutureMap for T { fn map<F: FnOnce(u64) -> i64>(self, _ignored: F) -> Result<i64, TryGetError> { Ok(-1) } }";
    let finding = analyze_with_reader(FORWARDED, GOOD, extension, true, false)?;
    assert_ne!(finding["ripr"]["propagate"]["state"], "yes", "{finding}");
    assert_ne!(finding["classification"], "exposed", "{finding}");
    Ok(())
}

#[test]
fn result_map_refuses_ambiguous_same_line_async_reader_declarations() -> Result<(), String> {
    let extension = "trait FutureMap { fn map<F: FnOnce(u64) -> i64>(self, ignored: F) -> Result<i64, TryGetError>; } impl<T: core::future::Future<Output = Result<u64, TryGetError>>> FutureMap for T { fn map<F: FnOnce(u64) -> i64>(self, _ignored: F) -> Result<i64, TryGetError> { Ok(-1) } }";
    let finding = analyze_with_reader(FORWARDED, GOOD, extension, true, true)?;
    assert_ne!(finding["ripr"]["propagate"]["state"], "yes", "{finding}");
    assert_ne!(finding["classification"], "exposed", "{finding}");
    Ok(())
}

#[test]
fn result_map_does_not_borrow_an_error_only_wrapper_pin() -> Result<(), String> {
    let extra = "fn entry() -> Result<i64, TryGetError> { let mut a = &[][..]; a.try_get_int(3) } fn expected_error() -> Result<i64, TryGetError> { Err(TryGetError) }";
    let finding = analyze(
        FORWARDED,
        "#[test] fn error_entry() { assert_eq!(entry(), expected_error()); }",
        extra,
    )?;
    assert_ne!(finding["ripr"]["discriminate"]["state"], "yes", "{finding}");
    assert_ne!(finding["classification"], "exposed", "{finding}");
    Ok(())
}

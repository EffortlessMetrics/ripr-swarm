//! Public-API controls for a loop accumulator whose update adds a called
//! function's value (#9478, RIPR-SPEC-0360 call-in-term):
//! `for x in xs { t += double(*x).await * 1; }`.
//!
//! The credit is read by folding the pinned literal run through the callee, so
//! each negative here is the credited twin with one thing changed: the pin, the
//! callee, the term or the owner shape.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str, diff: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-loop-call-term-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"loop-accumulator\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), lib)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), diff)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root })
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The one finding of `family` on `line` after `old` becomes `new` there.
/// `lib` holds the new text.
fn finding(
    lib: &str,
    line: usize,
    old: &str,
    new: &str,
    family: ProbeFamily,
) -> Result<Finding, String> {
    let diff = format!(
        "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -{line},1 +{line},1 @@
-{old}
+{new}
"
    );
    let repo = TempRepo::create(lib, &diff)?;
    let output = check_workspace(CheckInput {
        root: repo.root.clone(),
        base: None,
        diff_file: Some(repo.root.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let mut on_line = output
        .findings
        .iter()
        .filter(|finding| finding.probe.location.line == line && finding.probe.family == family);
    match (on_line.next(), on_line.next()) {
        (Some(finding), None) => Ok(finding.clone()),
        other => Err(format!(
            "premise: exactly one {family:?} finding on line {line}, got {other:?} in {:?}",
            output.findings
        )),
    }
}

const HELPERS: &str = "use std::future::Future;\n";

/// The polling helper, defined in the test module like the corpus subject's.
const BLOCK_ON: &str = "use std::pin::pin;
    use std::task::{Context, Poll, Waker};

    fn block_on<F: Future>(fut: F) -> F::Output {
        let mut fut = pin!(fut);
        let mut cx = Context::from_waker(Waker::noop());
        loop {
            if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
                return v;
            }
        }
    }";

/// The credited twin: an async owner awaiting `double` per element, edited
/// from `t += double(*x).await;` to `t += double(*x).await * 1;`.
const OWNER: &str = "async fn double(x: u32) -> u32 {
    x * 2
}

pub async fn sum_doubled(xs: &[u32]) -> u32 {
    let mut t = 0;
    for x in xs {
        t += double(*x).await * 1;
    }
    t
}
";

const PIN: &str = "assert_eq!(block_on(sum_doubled(&[1, 2, 3])), 12);";

/// The finding on the update line of `owner` (the line holding `t +=`),
/// pinned by `test_body`. The edit is `old` becoming the owner's own line.
fn update_finding(owner: &str, old: &str, test_body: &str) -> Result<Finding, String> {
    let lib = format!(
        "{HELPERS}
{owner}
#[cfg(test)]
mod tests {{
    use super::*;

    {BLOCK_ON}

    #[test]
    fn pinned() {{
        {test_body}
    }}
}}
"
    );
    let (at, text) = lib
        .lines()
        .enumerate()
        .find(|(_, text)| {
            ["t +=", "t -=", "t ="]
                .iter()
                .any(|prefix| text.trim_start().starts_with(prefix))
        })
        .ok_or_else(|| "premise: an update line".to_string())?;
    finding(&lib, at + 1, old, text, ProbeFamily::ReturnValue)
}

/// A pin ripr cannot read as separating the update is withheld as
/// `static_unknown`: never credited and never called a gap. A sync owner reads
/// `rust_pinned_shape_unresolved`; an `async` one keeps the executor-helper
/// limit (`rust_framework_oracle_unresolved`) its pin already carried.
fn require_not_credited(label: &str, found: &Finding) -> Result<(), String> {
    let limit = found.static_limit_kind.map(|kind| kind.as_str());
    if found.class != ExposureClass::StaticUnknown
        || !matches!(
            limit,
            Some("rust_pinned_shape_unresolved" | "rust_framework_oracle_unresolved")
        )
    {
        return Err(format!("{label} must be withheld, not credited: {found:?}"));
    }
    Ok(())
}

#[test]
fn an_awaited_call_scaled_by_a_literal_is_credited_through_a_run_that_folds_to_the_pin()
-> Result<(), String> {
    let found = update_finding(OWNER, "        t += double(*x).await;", PIN)?;
    assert_eq!(found.class, ExposureClass::Exposed, "{found:?}");
    Ok(())
}

#[test]
fn a_sync_call_without_a_scale_is_credited_the_same_way() -> Result<(), String> {
    let owner = "fn double(x: u32) -> u32 {
    x * 2
}

pub fn sum_doubled(xs: &[u32]) -> u32 {
    let mut t = 0;
    for x in xs {
        t += double(*x);
    }
    t
}
";
    let found = update_finding(
        owner,
        "        t = t + double(*x);",
        "assert_eq!(sum_doubled(&[1, 2, 3]), 12);",
    )?;
    assert_eq!(found.class, ExposureClass::Exposed, "{found:?}");
    Ok(())
}

#[test]
fn a_pin_that_the_callee_does_not_fold_to_is_not_credited() -> Result<(), String> {
    for (label, pin) in [
        (
            "a total the callee does not give",
            "assert_eq!(block_on(sum_doubled(&[1, 2, 3])), 13);",
        ),
        (
            "a single element",
            "assert_eq!(block_on(sum_doubled(&[5])), 10);",
        ),
        (
            "an empty input",
            "assert_eq!(block_on(sum_doubled(&[])), 0);",
        ),
        (
            "a zero total",
            "assert_eq!(block_on(sum_doubled(&[0, 0])), 0);",
        ),
    ] {
        let found = update_finding(OWNER, "        t += double(*x).await;", pin)?;
        require_not_credited(label, &found)?;
    }
    Ok(())
}

#[test]
fn a_callee_ripr_cannot_evaluate_is_not_credited() -> Result<(), String> {
    for (label, callee) in [
        (
            "a statement body",
            "async fn double(x: u32) -> u32 {\n    let y = x * 2;\n    y\n}",
        ),
        (
            "a nested call",
            "async fn double(x: u32) -> u32 {\n    twice(x)\n}\nfn twice(x: u32) -> u32 {\n    x * 2\n}",
        ),
        (
            "a generic callee",
            "async fn double<T>(x: u32) -> u32 {\n    x * 2\n}",
        ),
        (
            "a mutable parameter",
            "async fn double(mut x: u32) -> u32 {\n    x * 2\n}",
        ),
        (
            "a cast",
            "async fn double(x: u32) -> u32 {\n    (x as u64 * 2) as u32\n}",
        ),
        (
            "a different return type",
            "async fn double(x: u32) -> u64 {\n    2\n}",
        ),
    ] {
        let owner = OWNER.replacen("async fn double(x: u32) -> u32 {\n    x * 2\n}", callee, 1);
        let found = update_finding(&owner, "        t += double(*x).await;", PIN)?;
        require_not_credited(label, &found)?;
    }
    Ok(())
}

#[test]
fn a_callee_defined_twice_is_not_credited() -> Result<(), String> {
    let owner = format!(
        "{OWNER}\nmod other {{\n    pub async fn double(x: u32) -> u32 {{\n        x * 3\n    }}\n}}\n"
    );
    let found = update_finding(&owner, "        t += double(*x).await;", PIN)?;
    require_not_credited("two functions named double", &found)
}

#[test]
fn a_term_operator_the_pin_cannot_tell_apart_is_not_credited() -> Result<(), String> {
    // `id(x) * 2` and `id(x) + 2` agree at every element of `[2, 2]`.
    let owner = "fn id(x: u32) -> u32 {
    x
}

pub fn scaled(xs: &[u32]) -> u32 {
    let mut t = 0;
    for x in xs {
        t += id(*x) * 2;
    }
    t
}
";
    let found = update_finding(
        owner,
        "        t += id(*x);",
        "assert_eq!(scaled(&[2, 2]), 8);",
    )?;
    require_not_credited("a coinciding term operator", &found)?;
    // The same owner pinned on a run the operators do not share is credited.
    let found = update_finding(
        owner,
        "        t += id(*x);",
        "assert_eq!(scaled(&[1, 5]), 12);",
    )?;
    assert_eq!(found.class, ExposureClass::Exposed, "{found:?}");
    Ok(())
}

#[test]
fn an_overwriting_update_that_a_single_element_cannot_tell_apart_is_not_credited()
-> Result<(), String> {
    // `t = double(x)` gives the pinned 10 for `[5]`, as `t += double(x)` does.
    let found = update_finding(
        OWNER,
        "        t += double(*x).await;",
        "assert_eq!(block_on(sum_doubled(&[5])), 10);",
    )?;
    require_not_credited("a one-element run", &found)
}

#[test]
fn a_term_outside_the_shape_keeps_its_static_unknown_probe() -> Result<(), String> {
    for (label, line) in [
        ("a second operator", "t += double(*x).await * 2 + 1;"),
        ("a method call", "t += x.double().await;"),
        ("a qualified call", "t += self::double(*x).await;"),
        ("a second argument", "t += double(*x, 1).await;"),
        ("a computed argument", "t += double(*x + 1).await;"),
        ("a non-literal operand", "t += double(*x).await * k;"),
        ("a hexadecimal operand", "t += double(*x).await * 0x1;"),
    ] {
        let owner = OWNER.replace("t += double(*x).await * 1;", line);
        let lib = format!("{HELPERS}\n{owner}");
        let at = lib
            .lines()
            .position(|text| text.trim_start().starts_with("t +="))
            .map_or(0, |at| at + 1);
        let diff = format!(
            "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -{at},1 +{at},1 @@
-        t += double(*x).await;
+        {line}
"
        );
        let repo = TempRepo::create(&lib, &diff)?;
        let output = check_workspace(CheckInput {
            root: repo.root.clone(),
            base: None,
            diff_file: Some(repo.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        if output.findings.iter().any(|found| {
            found.probe.family == ProbeFamily::ReturnValue && found.probe.location.line == at
        }) {
            return Err(format!("{label} must not be a return_value probe"));
        }
    }
    Ok(())
}

#[test]
fn an_async_owner_with_a_plain_term_keeps_its_old_reading() -> Result<(), String> {
    let owner = "pub async fn total(xs: &[u32]) -> u32 {
    let mut t = 0;
    for x in xs {
        t += *x;
    }
    t
}
";
    let lib = format!("{HELPERS}\n{owner}");
    let at = lib
        .lines()
        .position(|text| text.trim_start().starts_with("t +="))
        .map_or(0, |at| at + 1);
    let diff = format!(
        "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -{at},1 +{at},1 @@
-        t = t + *x;
+        t += *x;
"
    );
    let repo = TempRepo::create(&lib, &diff)?;
    let output = check_workspace(CheckInput {
        root: repo.root.clone(),
        base: None,
        diff_file: Some(repo.root.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    if output.findings.iter().any(|found| {
        found.probe.family == ProbeFamily::ReturnValue && found.probe.location.line == at
    }) {
        return Err("an async owner without a call term stays outside the shape".to_string());
    }
    Ok(())
}

#[test]
fn a_parameter_that_shadows_the_callee_is_not_evaluated_as_the_function() -> Result<(), String> {
    // `one` the parameter hides `one` the function: the run [2, 0] folds to 2
    // through the function but the closure the test passes returns [2, 0].
    let owner = "fn one(x: u32) -> u32 {
    x - x + 1
}

pub fn total(items: &[u32], one: fn(u32) -> u32) -> u32 {
    let mut t = 0;
    for x in items {
        t += one(*x);
    }
    t
}
";
    let lib = format!(
        "{owner}
#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn pinned() {{
        assert_eq!(total(&[2, 0], |x| x), 2);
    }}
}}
"
    );
    let at = lib
        .lines()
        .position(|text| text.trim_start().starts_with("t +="))
        .map_or(0, |at| at + 1);
    let diff = format!(
        "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -{at},1 +{at},1 @@
-        t = t + one(*x);
+        t += one(*x);
"
    );
    let repo = TempRepo::create(&lib, &diff)?;
    let output = check_workspace(CheckInput {
        root: repo.root.clone(),
        base: None,
        diff_file: Some(repo.root.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    if output
        .findings
        .iter()
        .any(|found| found.class == ExposureClass::Exposed)
    {
        return Err(format!(
            "a shadowed callee must not be credited: {:?}",
            output.findings
        ));
    }
    Ok(())
}

#[test]
fn a_subtracting_update_on_a_signed_owner_is_credited_and_one_element_may_be() -> Result<(), String>
{
    let owner = "fn double(x: i32) -> i32 {
    x * 2
}

pub fn drained(xs: &[i32]) -> i32 {
    let mut t = 0;
    for x in xs {
        t -= double(*x);
    }
    t
}
";
    let found = update_finding(
        owner,
        "        t = t - double(*x);",
        "assert_eq!(drained(&[1, 2, 3]), -12);",
    )?;
    assert_eq!(found.class, ExposureClass::Exposed, "{found:?}");
    Ok(())
}

#[test]
fn a_run_whose_bitwise_or_agrees_with_the_sum_is_not_credited() -> Result<(), String> {
    // `2 | 4` and `2 + 4` are both 6, so `|=` survives a pin of 6.
    let owner = "fn id(x: u32) -> u32 {
    x
}

pub fn joined(xs: &[u32]) -> u32 {
    let mut t = 0;
    for x in xs {
        t += id(*x);
    }
    t
}
";
    let found = update_finding(
        owner,
        "        t = t + id(*x);",
        "assert_eq!(joined(&[2, 4]), 6);",
    )?;
    require_not_credited("a run where |= agrees", &found)?;
    let found = update_finding(
        owner,
        "        t = t + id(*x);",
        "assert_eq!(joined(&[3, 4, 5]), 12);",
    )?;
    assert_eq!(found.class, ExposureClass::Exposed, "{found:?}");
    Ok(())
}

#[test]
fn a_smoke_call_never_reports_a_gap_for_a_callee_ripr_did_not_read() -> Result<(), String> {
    let found = update_finding(
        OWNER,
        "        t += double(*x).await;",
        "let _ = block_on(sum_doubled(&[1, 2, 3]));",
    )?;
    if found.class == ExposureClass::WeaklyExposed {
        return Err(format!("an unread callee must not report a gap: {found:?}"));
    }
    Ok(())
}

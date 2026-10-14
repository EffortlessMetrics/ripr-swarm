//! End-to-end witness for RIPR-SPEC-0263 (#7132).
//!
//! A changed `v.push(x)` inside `if x > 10` runs only for inputs above 10.
//! When every related test calls the owner with a literal that leaves the
//! guard false, no mutant of the push executes, so an exact assertion over
//! the collection cannot reveal it: the side effect must not read exposed,
//! and the guard is named as the missing input. Any input that enters the
//! guard, or any input the reading cannot evaluate, keeps today's verdict.

use ripr::{
    CheckInput, CheckOutput, ExposureClass, Mode, OutputFormat, ProbeFamily, check_workspace,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const UNENTERED_PREFIX: &str = "No related test call enters guard";

fn source(guard: &str, push: &str) -> String {
    format!(
        "pub fn add(v: &mut Vec<u32>, x: u32) {{\n    if {guard} {{\n        {push}\n    }}\n}}\n"
    )
}

fn diff(guard: &str) -> String {
    format!(
        "diff --git a/src/lib.rs b/src/lib.rs\nindex 0000000..1111111 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,5 +1,5 @@\n pub fn add(v: &mut Vec<u32>, x: u32) {{\n     if {guard} {{\n-        v.push(x);\n+        v.push(x * 1);\n     }}\n }}\n"
    )
}

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(guard: &str, test_source: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "ripr-guarded-side-effect-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::create_dir_all(root.join("tests"))
            .map_err(|error| format!("create test directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"guarded-side-effect\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), source(guard, "v.push(x * 1);"))
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("tests/add.rs"), test_source)
            .map_err(|error| format!("write test failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), diff(guard))
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root })
    }

    fn check(&self) -> Result<CheckOutput, String> {
        check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The side-effect and call-deletion findings on the changed push.
fn push_findings(guard: &str, test_source: &str) -> Result<Vec<ripr::Finding>, String> {
    let repo = TempRepo::create(guard, test_source)?;
    let output = repo.check()?;
    if format!("{output:?}").contains("MalformedDiff") {
        return Err("the fixture diff must parse cleanly".to_string());
    }
    let findings = output
        .findings
        .iter()
        .filter(|finding| {
            matches!(
                finding.probe.family,
                ProbeFamily::SideEffect | ProbeFamily::CallDeletion
            ) && finding.probe.location.line == 3
        })
        .cloned()
        .collect::<Vec<_>>();
    if findings.is_empty() {
        let observed = output
            .findings
            .iter()
            .map(|finding| {
                format!(
                    "{}:{}:{}",
                    finding.probe.family.as_str(),
                    finding.probe.location.line,
                    finding.probe.expression
                )
            })
            .collect::<Vec<_>>()
            .join(" | ");
        return Err(format!(
            "no side-effect finding on the push; observed {observed}"
        ));
    }
    Ok(findings)
}

/// Whether the finding names an unentered guard anywhere: the structured
/// missing discriminator, the infection summary or an evidence line. The
/// negative rows assert all three are absent, so a renderer moving the text
/// cannot let them pass silently.
fn names_unentered_guard(finding: &ripr::Finding) -> bool {
    finding
        .activation
        .missing_discriminators
        .iter()
        .any(|fact| fact.reason.starts_with(UNENTERED_PREFIX))
        || finding.ripr.infect.summary.contains(UNENTERED_PREFIX)
        || finding
            .evidence
            .iter()
            .any(|line| line.contains(UNENTERED_PREFIX))
}

fn test_calling(calls: &str) -> String {
    format!(
        "use guarded_side_effect::add;\n\n#[test]\nfn adds() {{\n    let mut v = Vec::new();\n{calls}\n    assert_eq!(v, Vec::<u32>::new());\n}}\n"
    )
}

#[test]
fn a_literal_that_misses_the_guard_names_it_and_is_not_exposed() -> Result<(), String> {
    for guard in ["x > 10", "10 < x", "(x >= 11)"] {
        for finding in push_findings(guard, &test_calling("    add(&mut v, 4);"))? {
            assert_eq!(
                finding.class,
                ExposureClass::WeaklyExposed,
                "{guard}: {finding:?}"
            );
            assert!(
                finding
                    .activation
                    .missing_discriminators
                    .iter()
                    .any(|fact| fact.reason.starts_with(UNENTERED_PREFIX)),
                "{guard}: the guard must be a missing discriminator: {:?}",
                finding.activation.missing_discriminators
            );
            assert!(
                finding.ripr.infect.summary.starts_with(UNENTERED_PREFIX),
                "{guard}: the infection must name the guard: {:?}",
                finding.ripr.infect
            );
            // The repair is an input that enters the guard, not an assertion.
            let step = finding.recommended_next_step.as_deref().unwrap_or_default();
            assert!(
                step.starts_with("Add a related test input that enters `if "),
                "{guard}: {step}"
            );
        }
    }
    Ok(())
}

#[test]
fn an_input_that_enters_the_guard_keeps_the_exposed_reading() -> Result<(), String> {
    let test = "use guarded_side_effect::add;\n\n#[test]\nfn adds() {\n    let mut v = Vec::new();\n    add(&mut v, 11);\n    assert_eq!(v, vec![11]);\n}\n";
    let findings = push_findings("x > 10", test)?;
    assert!(
        findings
            .iter()
            .any(|finding| finding.class == ExposureClass::Exposed),
        "an entering input keeps exposed: {findings:?}"
    );
    assert!(
        findings
            .iter()
            .all(|finding| !names_unentered_guard(finding))
    );
    Ok(())
}

#[test]
fn one_entering_call_or_one_unread_input_names_no_guard() -> Result<(), String> {
    for calls in [
        // One call enters the guard, so the tests do run the push.
        "    add(&mut v, 4);\n    add(&mut v, 11);",
        // A let-bound literal that enters the guard.
        "    let n = 11;\n    add(&mut v, n);",
        // A computed input is not evaluated.
        "    let n = 2 + 2;\n    add(&mut v, n);",
        // A non-literal expression is not evaluated either.
        "    add(&mut v, 2 + 2);",
        // A mutable binding may change before the call.
        "    let mut n = 4;\n    n += 7;\n    add(&mut v, n);",
        // A shadowing `let` gives the call another value.
        "    let n = 4;\n    let n = n + 7;\n    add(&mut v, n);",
        // An inner block's binding shadows the one the reading would take.
        "    let n = 4;\n    {\n        let n = 11;\n        add(&mut v, n);\n    }",
        // A closure parameter of the same name shadows the binding.
        "    let n = 4;\n    let call = |n: u32| add(&mut v, n);\n    call(11);",
        // A nested `fn` resolves names without the test's locals.
        "    let n = 4;\n    fn n_of() -> u32 {\n        11\n    }\n    add(&mut v, n);\n    let _ = n_of();",
    ] {
        for finding in push_findings("x > 10", &test_calling(calls))? {
            assert!(
                !names_unentered_guard(&finding),
                "{calls}: no guard may be named: {:?}",
                finding.ripr.infect
            );
        }
    }
    Ok(())
}

#[test]
fn a_let_bound_literal_that_misses_the_guard_names_it() -> Result<(), String> {
    for calls in [
        "    let n = 4;\n    add(&mut v, n);",
        "    let n: u32 = 4;\n    add(&mut v, n);\n    add(&mut v, n);",
    ] {
        for finding in push_findings("x > 10", &test_calling(calls))? {
            assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{calls}");
            assert!(
                finding
                    .activation
                    .missing_discriminators
                    .iter()
                    .any(|fact| fact.reason.starts_with(UNENTERED_PREFIX)
                        && fact.reason.contains("`n = 4`")),
                "{calls}: {:?}",
                finding.activation.missing_discriminators
            );
        }
    }
    Ok(())
}

#[test]
fn a_named_const_bound_that_the_input_misses_names_the_guard() -> Result<(), String> {
    let test = test_calling("    add(&mut v, 4);");
    for prelude in [
        "pub const LIMIT: u32 = 10;\n",
        "/// The smallest value kept.\nconst LIMIT: u32 = 1_0;\n",
    ] {
        for finding in push_findings_with_prelude("x > LIMIT", &test, prelude)? {
            assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{prelude}");
            assert!(
                finding
                    .activation
                    .missing_discriminators
                    .iter()
                    .any(|fact| fact.reason.starts_with(UNENTERED_PREFIX)
                        && fact.reason.contains("`if x > LIMIT`")),
                "{prelude}: {:?}",
                finding.activation.missing_discriminators
            );
        }
    }
    Ok(())
}

#[test]
fn an_unreadable_guard_names_nothing() -> Result<(), String> {
    let test = test_calling("    add(&mut v, 4);");
    for guard in ["x > 10 && x < 100", "x + 1 > 10"] {
        for finding in push_findings(guard, &test)? {
            assert!(
                !names_unentered_guard(&finding),
                "{guard}: an unread guard names nothing: {:?}",
                finding.ripr.infect
            );
        }
    }
    for prelude in [
        // The input enters the named bound.
        "pub const LIMIT: u32 = 3;\n",
        // A computed or non-integer constant is not evaluated.
        "pub const LIMIT: u32 = 5 * 2;\n",
        // A `static` may be read through interior paths; only `const` reads.
        "pub static LIMIT: u32 = 10;\n",
        // Two definitions: which one the guard sees is not read.
        "pub const LIMIT: u32 = 10;\npub mod other {\n    pub const LIMIT: u32 = 3;\n}\n",
        // An imported name with no local `const` is not read.
        "mod limits {\n    pub const SMALL: u32 = 10;\n}\nuse limits::SMALL as LIMIT;\n",
        // A macro may declare items the owner sees.
        "macro_rules! limit {\n    () => {\n        pub const LIMIT: u32 = 10;\n    };\n}\nlimit!();\n",
    ] {
        for finding in push_findings_with_prelude("x > LIMIT", &test, prelude)? {
            assert!(
                !names_unentered_guard(&finding),
                "{prelude}: an unread bound names nothing: {:?}",
                finding.ripr.infect
            );
        }
    }
    Ok(())
}

/// [`push_findings`] with extra items before the owner. The diff's line
/// numbers move with them.
fn push_findings_with_prelude(
    guard: &str,
    test_source: &str,
    prelude: &str,
) -> Result<Vec<ripr::Finding>, String> {
    let repo = TempRepo::create(guard, test_source)?;
    let lines = prelude.lines().count();
    std::fs::write(
        repo.root.join("src/lib.rs"),
        format!("{prelude}{}", source(guard, "v.push(x * 1);")),
    )
    .map_err(|error| format!("write source failed: {error}"))?;
    let shifted = diff(guard).replace(
        "@@ -1,5 +1,5 @@",
        &format!("@@ -{0},5 +{0},5 @@", 1 + lines),
    );
    std::fs::write(repo.root.join("diff.patch"), shifted)
        .map_err(|error| format!("write diff failed: {error}"))?;
    let output = repo.check()?;
    let findings = output
        .findings
        .iter()
        .filter(|finding| {
            matches!(
                finding.probe.family,
                ProbeFamily::SideEffect | ProbeFamily::CallDeletion
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    if findings.is_empty() {
        return Err(format!("{guard}: no side-effect finding on the push"));
    }
    Ok(findings)
}

/// The side-effect findings on `changed_line` of a crate whose `src/lib.rs`
/// moves from `before` to `after` (one changed line), tested by
/// `test_source`.
fn custom_findings(
    before: &str,
    after: &str,
    changed_line: usize,
    test_source: &str,
) -> Result<Vec<ripr::Finding>, String> {
    let repo = TempRepo::create("x > 10", test_source)?;
    std::fs::write(repo.root.join("src/lib.rs"), after)
        .map_err(|error| format!("write source failed: {error}"))?;
    let old = before.lines().collect::<Vec<_>>();
    let new = after.lines().collect::<Vec<_>>();
    if old.len() != new.len() {
        return Err("the fixture must change exactly one line".to_string());
    }
    let mut body = String::new();
    for (old_line, new_line) in old.iter().zip(&new) {
        if old_line == new_line {
            body.push_str(&format!(" {old_line}\n"));
        } else {
            body.push_str(&format!("-{old_line}\n+{new_line}\n"));
        }
    }
    let diff = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\nindex 0000000..1111111 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,{0} +1,{0} @@\n{body}",
        old.len()
    );
    std::fs::write(repo.root.join("diff.patch"), diff)
        .map_err(|error| format!("write diff failed: {error}"))?;
    let output = repo.check()?;
    let findings = output
        .findings
        .iter()
        .filter(|finding| {
            matches!(
                finding.probe.family,
                ProbeFamily::SideEffect | ProbeFamily::CallDeletion
            ) && finding.probe.location.line == changed_line
        })
        .cloned()
        .collect::<Vec<_>>();
    if findings.is_empty() {
        return Err(format!("no side-effect finding on line {changed_line}"));
    }
    Ok(findings)
}

#[test]
fn recursion_a_same_line_else_or_a_helper_names_no_guard() -> Result<(), String> {
    // The owner calls itself after the guard: `walk(&mut v, 0)` reaches
    // depth 4, which enters `depth > 3`, although the test passes 0.
    let recursive = |push: &str| {
        format!(
            "pub fn walk(v: &mut Vec<u32>, depth: u32) {{\n    if depth > 3 {{\n        {push}\n    }}\n    if depth < 5 {{\n        walk(v, depth + 1);\n    }}\n}}\n"
        )
    };
    let walk_test = "use guarded_side_effect::walk;\n\n#[test]\nfn walks() {\n    let mut v = Vec::new();\n    walk(&mut v, 0);\n    assert_eq!(v, vec![4, 5]);\n}\n";
    // The changed push sits in an `else` closed on the guard's own line,
    // so input 4 does run it.
    let same_line_else = |push: &str| {
        format!(
            "pub fn add(v: &mut Vec<u32>, x: u32) {{\n    if x > 10 {{\n        v.clear();\n    }} else {{ {push} }}\n}}\n"
        )
    };
    // A helper passes 20, which enters the guard; only the direct call
    // passes 4.
    let helper_test = "use guarded_side_effect::add;\n\nfn add_large(v: &mut Vec<u32>) {\n    add(v, 20);\n}\n\n#[test]\nfn adds() {\n    let mut v = Vec::new();\n    add(&mut v, 4);\n    add_large(&mut v);\n    assert_eq!(v, vec![20]);\n}\n";
    // The owner calls a helper that calls the owner back with 50, which
    // enters the guard: only a transitive walk of the owner's calls sees it.
    let mutual = |push: &str| {
        format!(
            "pub fn add(v: &mut Vec<u32>, x: u32) {{\n    if x > 10 {{\n        {push}\n    }}\n    fixup(v);\n}}\n\nfn fixup(v: &mut Vec<u32>) {{\n    if v.is_empty() {{\n        add(v, 50);\n    }}\n}}\n"
        )
    };
    // A macro the owner invokes calls the owner back with 20, which enters
    // the guard; the macro's expansion is not read.
    let via_macro = |push: &str| {
        format!(
            "macro_rules! bump {{\n    ($v:expr) => {{\n        add($v, 20)\n    }};\n}}\n\npub fn add(v: &mut Vec<u32>, x: u32) {{\n    if x > 10 {{\n        {push}\n    }}\n    if x < 10 {{\n        bump!(v);\n    }}\n}}\n"
        )
    };
    // A typed receiver is not a call argument: `s.add(11, 4)` passes 11 as
    // `x`, which enters the guard. A regression control: the signature
    // reader already drops `self: Box<Self>`.
    let typed_receiver = |push: &str| {
        format!(
            "pub struct S {{\n    pub v: Vec<u32>,\n}}\n\nimpl S {{\n    pub fn add(self: Box<Self>, x: u32, y: u32) -> Vec<u32> {{\n        let mut v = self.v;\n        if x > 10 {{\n            {push}\n        }}\n        v\n    }}\n}}\n"
        )
    };
    let method_test = "use guarded_side_effect::S;\n\n#[test]\nfn adds() {\n    let s = Box::new(S { v: Vec::new() });\n    let v = s.add(11, 4);\n    assert_eq!(v, vec![11]);\n}\n";
    // A helper the owner calls invokes a macro that calls the owner back
    // with 20; the helper's body never names the owner.
    let helper_macro = |push: &str| {
        format!(
            "macro_rules! bump {{\n    ($v:expr) => {{\n        add($v, 20)\n    }};\n}}\n\npub fn add(v: &mut Vec<u32>, x: u32) {{\n    if x > 10 {{\n        {push}\n    }}\n    fixup(v);\n}}\n\nfn fixup(v: &mut Vec<u32>) {{\n    if v.is_empty() {{\n        bump!(v);\n    }}\n}}\n"
        )
    };
    // A test helper invokes a macro that passes 20 to the owner.
    let test_helper_macro = "use guarded_side_effect::add;\n\nmacro_rules! bump {\n    ($v:expr) => {\n        add($v, 20)\n    };\n}\n\nfn setup(v: &mut Vec<u32>) {\n    bump!(v);\n}\n\n#[test]\nfn adds() {\n    let mut v = Vec::new();\n    add(&mut v, 4);\n    setup(&mut v);\n    assert_eq!(v, vec![20]);\n}\n";
    // A renamed import calls the owner with 20 under another name.
    let renamed_test = "use guarded_side_effect::add;\nuse guarded_side_effect::add as push_big;\n\n#[test]\nfn adds() {\n    let mut v = Vec::new();\n    add(&mut v, 4);\n    push_big(&mut v, 20);\n    assert_eq!(v, vec![20]);\n}\n";
    // A stored function pointer calls the owner with 20 under another name.
    let stored = |push: &str, item: &str| format!("{}\n{item}\n", source("x > 10", push));
    let static_test =
        test_calling("    add(&mut v, 4);\n    guarded_side_effect::BIG(&mut v, 20);");
    let table_test =
        test_calling("    add(&mut v, 4);\n    guarded_side_effect::OPS[0](&mut v, 20);");
    let static_item = "pub static BIG: fn(&mut Vec<u32>, u32) = add;";
    let table_item = "pub const OPS: [fn(&mut Vec<u32>, u32); 1] = [add];";
    // The changed push follows a guard closed on its own line, so input 4
    // runs it.
    let after_closed_if = |push: &str| {
        format!("pub fn add(v: &mut Vec<u32>, x: u32) {{\n    if x > 10 {{}}\n    {push}\n}}\n")
    };
    // A local macro shadows a standard name and calls the owner back with
    // 20; spelling alone does not make it the standard macro (#7852).
    let shadowed = |push: &str, call: &str| {
        format!(
            "macro_rules! println {{\n    ($v:expr) => {{\n        add($v, 20)\n    }};\n}}\n\npub fn add(v: &mut Vec<u32>, x: u32) {{\n    if x > 10 {{\n        {push}\n    }}\n    if x < 10 {{\n        {call};\n    }}\n}}\n"
        )
    };
    // A path-qualified macro from another crate under a standard leaf name
    // (`helpers::println!`) is not the standard macro either.
    let qualified = |push: &str| {
        format!(
            "pub fn add(v: &mut Vec<u32>, x: u32) {{\n    if x > 10 {{\n        {push}\n    }}\n    if x < 10 {{\n        helpers::println!(v);\n    }}\n}}\n"
        )
    };
    // Production code reaches the owner with 20 without a call the test
    // shows: a closure in a static, a `Drop` impl, an operator impl.
    let closure_item = "pub static BIG: fn(&mut Vec<u32>, u32) = |v, x| add(v, x * 5);";
    let closure_test =
        test_calling("    add(&mut v, 4);\n    guarded_side_effect::BIG(&mut v, 4);");
    let drop_item = "pub struct G(pub Vec<u32>);\n\nimpl Drop for G {\n    fn drop(&mut self) {\n        add(&mut self.0, 20);\n    }\n}";
    let drop_test =
        test_calling("    add(&mut v, 4);\n    let _g = guarded_side_effect::G(Vec::new());");
    let operator_item = "pub struct W(pub Vec<u32>);\n\nimpl std::ops::AddAssign<u32> for W {\n    fn add_assign(&mut self, x: u32) {\n        add(&mut self.0, x);\n    }\n}";
    let operator_test = test_calling(
        "    add(&mut v, 4);\n    let mut w = guarded_side_effect::W(Vec::new());\n    w += 20;",
    );
    // A doctest on the owner passes 20 under `cargo test`.
    let doctest = |push: &str| {
        format!(
            "/// ```\n/// let mut v = Vec::new();\n/// guarded_side_effect::add(&mut v, 20);\n/// assert_eq!(v, vec![20]);\n/// ```\n{}",
            source("x > 10", push)
        )
    };
    // An included file may hold a test that passes 20.
    let included_test = format!(
        "include!(\"cases.inc\");\n\n{}",
        test_calling("    add(&mut v, 4);")
    );
    let cases = [
        (
            "doctest",
            doctest("v.push(x);"),
            doctest("v.push(x * 1);"),
            8,
            test_calling("    add(&mut v, 4);"),
        ),
        (
            "included test",
            source("x > 10", "v.push(x);"),
            source("x > 10", "v.push(x * 1);"),
            3,
            included_test,
        ),
        (
            "closure in a static",
            stored("v.push(x);", closure_item),
            stored("v.push(x * 1);", closure_item),
            3,
            closure_test,
        ),
        (
            "drop impl",
            stored("v.push(x);", drop_item),
            stored("v.push(x * 1);", drop_item),
            3,
            drop_test,
        ),
        (
            "operator impl",
            stored("v.push(x);", operator_item),
            stored("v.push(x * 1);", operator_item),
            3,
            operator_test,
        ),
        (
            "shadowed println",
            shadowed("v.push(x);", "println!(v)"),
            shadowed("v.push(x * 1);", "println!(v)"),
            9,
            test_calling("    add(&mut v, 4);"),
        ),
        (
            "qualified println",
            qualified("v.push(x);"),
            qualified("v.push(x * 1);"),
            3,
            test_calling("    add(&mut v, 4);"),
        ),
        (
            "after a closed if",
            after_closed_if("v.push(x);"),
            after_closed_if("v.push(x * 1);"),
            3,
            test_calling("    add(&mut v, 4);"),
        ),
        (
            "static pointer",
            stored("v.push(x);", static_item),
            stored("v.push(x * 1);", static_item),
            3,
            static_test,
        ),
        (
            "pointer table",
            stored("v.push(x);", table_item),
            stored("v.push(x * 1);", table_item),
            3,
            table_test,
        ),
        (
            "helper macro",
            helper_macro("v.push(x);"),
            helper_macro("v.push(x * 1);"),
            9,
            test_calling("    add(&mut v, 4);"),
        ),
        (
            "test helper macro",
            source("x > 10", "v.push(x);"),
            source("x > 10", "v.push(x * 1);"),
            3,
            test_helper_macro.to_string(),
        ),
        (
            "renamed import",
            source("x > 10", "v.push(x);"),
            source("x > 10", "v.push(x * 1);"),
            3,
            renamed_test.to_string(),
        ),
        (
            "macro recursion",
            via_macro("v.push(x);"),
            via_macro("v.push(x * 1);"),
            9,
            test_calling("    add(&mut v, 4);"),
        ),
        (
            "typed receiver",
            typed_receiver("v.push(x);"),
            typed_receiver("v.push(x * 1);"),
            9,
            method_test.to_string(),
        ),
        (
            "mutual recursion",
            mutual("v.push(x);"),
            mutual("v.push(x * 1);"),
            3,
            test_calling("    add(&mut v, 4);"),
        ),
        (
            "recursion",
            recursive("v.push(depth);"),
            recursive("v.push(depth * 1);"),
            3,
            walk_test.to_string(),
        ),
        (
            "same-line else",
            same_line_else("v.push(x);"),
            same_line_else("v.push(x * 1);"),
            4,
            test_calling("    add(&mut v, 4);"),
        ),
        (
            "helper",
            source("x > 10", "v.push(x);"),
            source("x > 10", "v.push(x * 1);"),
            3,
            helper_test.to_string(),
        ),
    ];
    for (label, before, after, line, test) in cases {
        for finding in custom_findings(&before, &after, line, &test)? {
            assert!(
                !names_unentered_guard(&finding),
                "{label}: no guard may be named: {:?}",
                finding.ripr.infect
            );
        }
    }
    // A real standard macro in the owner keeps the guard reading.
    let printing = |push: &str| {
        format!(
            "pub fn add(v: &mut Vec<u32>, x: u32) {{\n    if x > 10 {{\n        {push}\n    }}\n    if x < 10 {{\n        println!(\"small {{x}}\");\n        std::println!(\"small\");\n    }}\n}}\n"
        )
    };
    let printing_findings = custom_findings(
        &printing("v.push(x);"),
        &printing("v.push(x * 1);"),
        3,
        &test_calling("    add(&mut v, 4);"),
    )?;
    assert!(
        printing_findings.iter().any(names_unentered_guard),
        "standard println!: the guard must be named: {:?}",
        printing_findings
            .iter()
            .map(|finding| &finding.ripr.infect)
            .collect::<Vec<_>>()
    );
    // The typed-receiver row discriminates only if the method reading names
    // the guard when its argument does miss it: `s.add(4, 11)` passes 4.
    let missing_method_test = method_test.replace("s.add(11, 4)", "s.add(4, 11)");
    let method_findings = custom_findings(
        &typed_receiver("v.push(x);"),
        &typed_receiver("v.push(x * 1);"),
        9,
        &missing_method_test,
    )?;
    assert!(
        method_findings.iter().any(names_unentered_guard),
        "typed receiver with 4: the guard must be named: {:?}",
        method_findings
            .iter()
            .map(|finding| &finding.ripr.infect)
            .collect::<Vec<_>>()
    );
    Ok(())
}

#[test]
fn a_named_bound_the_owner_shadows_or_cannot_see_names_no_guard() -> Result<(), String> {
    let test = test_calling("    add(&mut v, 10);");
    for (before, after, line) in [
        // An owner-local `let LIMIT = 3;` shadows the file-level 10, so 4
        // enters the guard.
        (
            "pub const LIMIT: u32 = 10;\npub fn add(v: &mut Vec<u32>, x: u32) {\n    let LIMIT = 3;\n    if x > LIMIT {\n        v.push(x);\n    }\n}\n",
            "pub const LIMIT: u32 = 10;\npub fn add(v: &mut Vec<u32>, x: u32) {\n    let LIMIT = 3;\n    if x > LIMIT {\n        v.push(x * 1);\n    }\n}\n",
            5,
        ),
        // The owner sits in an inline module, whose `LIMIT` is not read.
        (
            "pub const LIMIT: u32 = 10;\npub use inner::add;\nmod inner {\n    use super::LIMIT;\n    pub fn add(v: &mut Vec<u32>, x: u32) {\n        if x > LIMIT {\n            v.push(x);\n        }\n    }\n}\n",
            "pub const LIMIT: u32 = 10;\npub use inner::add;\nmod inner {\n    use super::LIMIT;\n    pub fn add(v: &mut Vec<u32>, x: u32) {\n        if x > LIMIT {\n            v.push(x * 1);\n        }\n    }\n}\n",
            7,
        ),
        // A literal past the parameter's type wraps when the lint is
        // allowed: `260` is 4 as a `u8`, so 10 enters the guard.
        (
            "#![allow(overflowing_literals)]\npub const LIMIT: u8 = 260;\npub fn add(v: &mut Vec<u8>, x: u8) {\n    if x > LIMIT {\n        v.push(x);\n    }\n}\n",
            "#![allow(overflowing_literals)]\npub const LIMIT: u8 = 260;\npub fn add(v: &mut Vec<u8>, x: u8) {\n    if x > LIMIT {\n        v.push(x * 1);\n    }\n}\n",
            5,
        ),
        // A `cfg` may remove the const under test, leaving the glob's `MIN`.
        (
            "use std::u8::*;\n#[cfg(not(test))]\nconst MIN: u8 = 100;\npub fn add(v: &mut Vec<u8>, x: u8) {\n    if x > MIN {\n        v.push(x);\n    }\n}\n",
            "use std::u8::*;\n#[cfg(not(test))]\nconst MIN: u8 = 100;\npub fn add(v: &mut Vec<u8>, x: u8) {\n    if x > MIN {\n        v.push(x * 1);\n    }\n}\n",
            6,
        ),
        // A bound typed through an alias is not an integer type the reading
        // trusts.
        (
            "pub type Small = u32;\npub const LIMIT: Small = 10;\npub fn add(v: &mut Vec<u32>, x: u32) {\n    if x > LIMIT {\n        v.push(x);\n    }\n}\n",
            "pub type Small = u32;\npub const LIMIT: Small = 10;\npub fn add(v: &mut Vec<u32>, x: u32) {\n    if x > LIMIT {\n        v.push(x * 1);\n    }\n}\n",
            5,
        ),
    ] {
        for finding in custom_findings(before, after, line, &test)? {
            assert!(
                !names_unentered_guard(&finding),
                "{after}: no guard may be named: {:?}",
                finding.ripr.infect
            );
        }
    }
    Ok(())
}

//! The deterministic adversarial corpus (#4511, #4512, #4513). Every input is
//! generated from constants below, so the per-case digests are stable across
//! hosts and runs.

use std::time::Duration;

use super::model::{Family, Role};

/// Outer instrument deadline per row. A kill at this deadline is a
/// `nonterminal_deadline` row, never a pass.
pub(crate) const ROW_DEADLINE: Duration = Duration::from_mins(1);

pub(crate) const DIFF_FILE: &str = "change.diff";
pub(crate) const SUPPRESSION_POLICY_FILE: &str = "policy/suppressions.toml";

/// What the exact binary must show for a case.
#[derive(Clone, Debug)]
pub(crate) enum Expect {
    /// Full analysis: the changed file reports a finding and no limitation
    /// token appears.
    Supported,
    /// The run terminates and keeps the changed file's finding. This route
    /// projects no limitation for the unchanged file, so none is expected.
    TerminatesWithEvidence,
    /// The hostile unchanged file is declined with `token` while the changed
    /// file still reports a finding.
    TypedLimitation {
        token: &'static str,
        language_class: Option<&'static str>,
    },
    /// The pattern does not match the changed path: the finding stays visible.
    GlobNonmatch,
    /// The pattern matches the changed path: the finding is suppressed.
    GlobMatch,
    /// `generated_file_patterns` does not match: the file is analyzed.
    GeneratedNotExcluded,
    /// `generated_file_patterns` matches: the file is excluded as generated.
    GeneratedExcluded,
    /// A malformed selector is a policy error (exit 2, typed limitation), not
    /// a matcher result and not a clean run.
    PolicyRefused,
}

impl Expect {
    pub(crate) fn describe(&self) -> String {
        match self {
            Self::Supported => "full analysis, no limitation".to_string(),
            Self::TerminatesWithEvidence => {
                "terminates; changed-file finding kept; no limitation projected on this route"
                    .to_string()
            }
            Self::TypedLimitation { token, .. } => format!("typed limitation `{token}`"),
            Self::GlobNonmatch => "pattern does not match; finding stays visible".to_string(),
            Self::GlobMatch => "pattern matches; finding suppressed".to_string(),
            Self::GeneratedNotExcluded => "pattern does not match; file analyzed".to_string(),
            Self::GeneratedExcluded => "pattern matches; file excluded as generated".to_string(),
            Self::PolicyRefused => {
                "policy refused (exit 2, suppression_policy_invalid), no findings".to_string()
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Route {
    /// `ripr check --diff` with JSON output.
    Check,
    /// `ripr pilot` whole-repository analysis (the only route that reads
    /// every Rust file and discloses `rust_nesting_budget`).
    Pilot,
}

pub(crate) struct Case {
    pub(crate) id: String,
    pub(crate) route: Route,
    /// Workspace-relative file the route writes that the assessment reads.
    pub(crate) artifact: Option<String>,
    pub(crate) family: Family,
    pub(crate) role: Role,
    pub(crate) shape: &'static str,
    /// Workspace-relative path, bytes. Includes the diff.
    pub(crate) files: Vec<(String, Vec<u8>)>,
    /// Arguments after the binary name.
    pub(crate) args: Vec<String>,
    pub(crate) expect: Expect,
    /// The changed file that must keep its evidence.
    pub(crate) changed: String,
    /// The unchanged neighbour outside the diff, when the case has one.
    pub(crate) hostile: Option<String>,
}

fn check_args(extra: &[&str]) -> Vec<String> {
    let mut args: Vec<String> = [
        "check", "--root", ".", "--diff", DIFF_FILE, "--format", "json",
    ]
    .iter()
    .map(|arg| (*arg).to_string())
    .collect();
    args.extend(extra.iter().map(|arg| (*arg).to_string()));
    args
}

// ---------------------------------------------------------------- Rust ----

const RUST_CHANGED: &str = "src/lib.rs";
const RUST_HOSTILE: &str = "src/deep.rs";

fn rust_workspace(hostile: &str) -> Vec<(String, Vec<u8>)> {
    vec![
        (
            "Cargo.toml".to_string(),
            b"[package]\nname = \"termination_probe\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"
                .to_vec(),
        ),
        (
            RUST_CHANGED.to_string(),
            b"pub fn price(amount: i32) -> i32 {\n    amount - 1\n}\n".to_vec(),
        ),
        (
            "tests/price.rs".to_string(),
            b"use termination_probe::price;\n\n#[test]\nfn price_subtracts_one() {\n    assert_eq!(price(5), 4);\n}\n"
                .to_vec(),
        ),
        (RUST_HOSTILE.to_string(), hostile.as_bytes().to_vec()),
        (
            DIFF_FILE.to_string(),
            b"diff --git a/src/lib.rs b/src/lib.rs\nindex 0000000..1111111 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn price(amount: i32) -> i32 {\n-    amount\n+    amount - 1\n }\n"
                .to_vec(),
        ),
    ]
}

/// Generated Rust source for one nesting shape at depth `n`, defining `name`.
pub(crate) fn rust_shape(shape: &str, n: usize, name: &str) -> String {
    match shape {
        "nested_blocks" => format!(
            "pub fn {name}() -> i32 {{ {}1{} }}\n",
            "{".repeat(n),
            "}".repeat(n)
        ),
        "nested_if" => format!(
            "pub fn {name}() -> i32 {{ {}1{} }}\n",
            "if true { ".repeat(n),
            " } else { 0 }".repeat(n)
        ),
        "parentheses" => format!(
            "pub fn {name}() -> i32 {{ {}1{} }}\n",
            "(".repeat(n),
            ")".repeat(n)
        ),
        "closures" => format!("pub fn {name}() {{ let _f = {}1; }}\n", "|| ".repeat(n)),
        "prefix_operators" => format!("pub fn {name}(x: bool) -> bool {{ {}x }}\n", "!".repeat(n)),
        "generic_arguments" => format!(
            "pub type {name} = {}i32{};\n",
            "Vec<".repeat(n),
            ">".repeat(n)
        ),
        "else_if_chain" => {
            let mut text = format!("pub fn {name}(x: i32) -> i32 {{\n    if x == 0 {{ 0 }}");
            for index in 1..n {
                text.push_str(&format!(" else if x == {index} {{ {index} }}"));
            }
            text.push_str(" else { -1 }\n}\n");
            text
        }
        "binary_operator_chain" => {
            format!("pub fn {name}() -> i32 {{ 1{} }}\n", " + 1".repeat(n))
        }
        _ => String::new(),
    }
}

pub(crate) const RUST_SHAPES: [&str; 8] = [
    "nested_blocks",
    "nested_if",
    "parentheses",
    "closures",
    "prefix_operators",
    "generic_arguments",
    "else_if_chain",
    "binary_operator_chain",
];

/// Depths that sit past the accepted budget and, for the families the repair
/// notes measured, past the unrepaired abort point on a release build.
fn rust_over_budget_depth(shape: &str) -> usize {
    match shape {
        "nested_blocks" | "nested_if" | "generic_arguments" => 5_000,
        "parentheses" | "closures" => 10_000,
        "prefix_operators" => 20_000,
        "else_if_chain" => 24_000,
        "binary_operator_chain" => 70_000,
        _ => 0,
    }
}

fn rust_supported_depth(shape: &str) -> usize {
    match shape {
        "else_if_chain" => 128,
        "binary_operator_chain" => 512,
        "closures" | "generic_arguments" => 32,
        _ => 64,
    }
}

fn rust_lookalike_source() -> String {
    let brackets = "(".repeat(10_000);
    let braces = "{".repeat(10_000);
    format!(
        "pub fn deep() -> &'static str {{\n    // {brackets}\n    /* {braces} */\n    let _a = \"{brackets}\";\n    let _b = r#\"{braces}\"#;\n    let _c = '(';\n    let _d = \"{}\";\n    \"ok\"\n}}\n",
        "else if ".repeat(5_000)
    )
}

pub(crate) fn rust_cases() -> Vec<Case> {
    let mut supported = String::new();
    for shape in RUST_SHAPES {
        supported.push_str(&rust_shape(
            shape,
            rust_supported_depth(shape),
            &format!("supported_{shape}"),
        ));
    }
    let mut cases = Vec::new();
    let mut push = |id: String, role: Role, shape: &'static str, hostile: String| {
        for route in [Route::Check, Route::Pilot] {
            let expect = match (role, route) {
                (Role::Discriminator, Route::Check) => Expect::TerminatesWithEvidence,
                (Role::Discriminator, Route::Pilot) => Expect::TypedLimitation {
                    token: "rust_nesting_budget",
                    language_class: None,
                },
                _ => Expect::Supported,
            };
            cases.push(rust_case(&id, route, role, shape, &hostile, expect));
        }
    };
    push(
        "rust-supported-control".to_string(),
        Role::Control,
        "all_shapes_below_budget",
        supported,
    );
    for shape in RUST_SHAPES {
        push(
            format!("rust-{}-over-budget", shape.replace('_', "-")),
            Role::Discriminator,
            shape,
            rust_shape(shape, rust_over_budget_depth(shape), "deep"),
        );
    }
    push(
        "rust-comment-string-lookalike".to_string(),
        Role::Lookalike,
        "comment_string_char_raw_string",
        rust_lookalike_source(),
    );
    cases
}

fn rust_case(
    id: &str,
    route: Route,
    role: Role,
    shape: &'static str,
    hostile: &str,
    expect: Expect,
) -> Case {
    let (suffix, args, artifact) = route_parts(route);
    Case {
        id: format!("{id}.{suffix}"),
        route,
        artifact,
        family: Family::RustNesting,
        role,
        shape,
        files: rust_workspace(hostile),
        args,
        expect,
        changed: RUST_CHANGED.to_string(),
        hostile: Some(RUST_HOSTILE.to_string()),
    }
}

fn route_parts(route: Route) -> (&'static str, Vec<String>, Option<String>) {
    match route {
        Route::Check => ("check", check_args(&[]), None),
        Route::Pilot => (
            "pilot",
            ["pilot", "--root", ".", "--format", "json"]
                .iter()
                .map(|arg| (*arg).to_string())
                .collect(),
            Some("target/ripr/pilot/repo-exposure.json".to_string()),
        ),
    }
}

// -------------------------------------------------------------- Python ----

const PY_CHANGED: &str = "src/app.py";
const PY_HOSTILE: &str = "src/deep.py";

fn python_workspace(hostile: &str) -> Vec<(String, Vec<u8>)> {
    vec![
        (
            "pyproject.toml".to_string(),
            b"[project]\nname = \"termination_probe\"\n".to_vec(),
        ),
        (
            PY_CHANGED.to_string(),
            b"def price(amount):\n    return amount - 1\n".to_vec(),
        ),
        (
            "tests/test_app.py".to_string(),
            b"from src.app import price\n\n\ndef test_price_subtracts_one():\n    assert price(5) == 4\n"
                .to_vec(),
        ),
        (PY_HOSTILE.to_string(), hostile.as_bytes().to_vec()),
        (
            DIFF_FILE.to_string(),
            b"diff --git a/src/app.py b/src/app.py\nindex 0000000..1111111 100644\n--- a/src/app.py\n+++ b/src/app.py\n@@ -1,2 +1,2 @@\n def price(amount):\n-    return amount\n+    return amount - 1\n"
                .to_vec(),
        ),
    ]
}

pub(crate) fn python_shape(shape: &str, n: usize) -> String {
    match shape {
        "binary_operator_run" => format!("value = 0{}\n", " + 1".repeat(n)),
        "ternary_run" => format!("value = {}0\n", "1 if c else ".repeat(n)),
        "unary_not_run" => format!("value = {}True\n", "not ".repeat(n)),
        "unary_minus_run" => format!("value = {}1\n", "-".repeat(n)),
        "await_prefix_run" => format!("async def deep(g):\n    return {}g()\n", "await ".repeat(n)),
        "elif_chain" => {
            let mut text = String::from("def deep(x):\n    if x == 0:\n        return 0\n");
            for index in 1..n {
                text.push_str(&format!("    elif x == {index}:\n        return {index}\n"));
            }
            text.push_str("    return -1\n");
            text
        }
        _ => String::new(),
    }
}

pub(crate) const PYTHON_SHAPES: [&str; 6] = [
    "binary_operator_run",
    "ternary_run",
    "unary_not_run",
    "unary_minus_run",
    "await_prefix_run",
    "elif_chain",
];

fn python_supported_source() -> String {
    // Deep but inside both budgets: 100 operators, 50 unary/ternary/await,
    // 100 elif arms, and 100 levels of brackets (the bracket budget is 128).
    let mut text = String::new();
    for shape in PYTHON_SHAPES {
        let n = match shape {
            "binary_operator_run" | "elif_chain" => 100,
            _ => 50,
        };
        let source = python_shape(shape, n);
        text.push_str(&match shape {
            "binary_operator_run" => source.replacen("value", "binary_value", 1),
            "ternary_run" => source.replacen("value", "ternary_value", 1),
            "unary_not_run" => source.replacen("value", "not_value", 1),
            "unary_minus_run" => source.replacen("value", "minus_value", 1),
            "await_prefix_run" => source.replacen("deep", "deep_await", 1),
            _ => source,
        });
    }
    text.push_str(&format!(
        "bracket_value = {}1{}\n",
        "(".repeat(100),
        ")".repeat(100)
    ));
    text
}

fn python_lookalike_source() -> String {
    format!(
        "# {}\n# {}\n# {}\nTEXT_A = \"{}\"\nTEXT_B = \"{}\"\nTEXT_C = \"{}\"\nTEXT_D = \"\"\"\n{}\"\"\"\n\n\ndef ok():\n    return 1\n",
        " + 1".repeat(10_000),
        "(".repeat(10_000),
        "elif x: ".repeat(5_000),
        " + 1".repeat(10_000),
        "not ".repeat(10_000),
        "elif x: ".repeat(5_000),
        "elif x == 1:\n".repeat(5_000)
    )
}

pub(crate) fn python_cases() -> Vec<Case> {
    let mut cases = vec![
        python_case(
            "python-supported-control",
            Role::Control,
            "deep_but_supported",
            &python_supported_source(),
            Expect::Supported,
        ),
        python_case(
            "python-comment-string-lookalike",
            Role::Lookalike,
            "comment_string_operator_elif_lookalikes",
            &python_lookalike_source(),
            Expect::Supported,
        ),
    ];
    for shape in PYTHON_SHAPES {
        cases.push(python_case(
            &format!("python-{}-over-budget", shape.replace('_', "-")),
            Role::Discriminator,
            shape,
            &python_shape(shape, 30_000),
            Expect::TypedLimitation {
                token: "parse_budget",
                language_class: Some("language_scope_unsupported"),
            },
        ));
    }
    cases
}

fn python_case(id: &str, role: Role, shape: &'static str, hostile: &str, expect: Expect) -> Case {
    Case {
        id: id.to_string(),
        route: Route::Check,
        artifact: None,
        family: Family::PythonNesting,
        role,
        shape,
        files: python_workspace(hostile),
        args: check_args(&[]),
        expect,
        changed: PY_CHANGED.to_string(),
        hostile: Some(PY_HOSTILE.to_string()),
    }
}

pub(crate) fn all_cases() -> Vec<Case> {
    let mut cases = rust_cases();
    cases.extend(python_cases());
    cases.extend(glob_cases());
    cases
}

// --------------------------------------------------------------- globs ----

const PRICE_SOURCE: &[u8] = b"pub fn price(amount: i32) -> i32 {\n    amount - 1\n}\n";

fn glob_files(changed: &str, extra: Vec<(String, Vec<u8>)>) -> Vec<(String, Vec<u8>)> {
    let diff = format!(
        "diff --git a/{changed} b/{changed}\nindex 0000000..1111111 100644\n--- a/{changed}\n+++ b/{changed}\n@@ -1,3 +1,3 @@\n pub fn price(amount: i32) -> i32 {{\n-    amount\n+    amount - 1\n }}\n"
    );
    let mut files = vec![
        (
            "Cargo.toml".to_string(),
            b"[package]\nname = \"termination_probe\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"
                .to_vec(),
        ),
        (changed.to_string(), PRICE_SOURCE.to_vec()),
        (DIFF_FILE.to_string(), diff.into_bytes()),
    ];
    files.extend(extra);
    files
}

/// `*a*a...*a*b` is the classic backtracking trap: no `b` anywhere in the
/// path, so a recursive matcher explores every split of the `a` run.
fn many_star_pattern(prefix: &str, stars: usize, tail: &str) -> String {
    format!("{prefix}{}{tail}", "*a".repeat(stars))
}

fn long_a_file() -> String {
    format!("src/{}.rs", "a".repeat(40))
}

fn deep_segment_file() -> String {
    format!("src/{}x.rs", "a/".repeat(40))
}

fn many_segment_pattern() -> String {
    format!("src/{}zzz.rs", "**/a/".repeat(16))
}

fn suppression_case(id: &str, role: Role, pattern: &str, changed: &str, expect: Expect) -> Case {
    let policy = format!(
        "schema_version = 1\n\n[[suppressions]]\nkind = \"exposure_gap\"\npath = \"{pattern}\"\nreason = \"termination qualification selector\"\nowner = \"qualification\"\n"
    );
    Case {
        id: format!("glob-suppression-{id}"),
        route: Route::Check,
        artifact: None,
        family: Family::PathGlobSuppression,
        role,
        shape: "suppression_path_selector",
        files: glob_files(
            changed,
            vec![(SUPPRESSION_POLICY_FILE.to_string(), policy.into_bytes())],
        ),
        args: check_args(&["--suppression-policy", SUPPRESSION_POLICY_FILE]),
        expect,
        changed: changed.to_string(),
        hostile: None,
    }
}

fn generated_case(id: &str, role: Role, pattern: &str, changed: &str, expect: Expect) -> Case {
    let config = format!(
        "[languages]\nenabled = [\"rust\"]\n\n[languages.rust]\ngenerated_file_patterns = [\"{pattern}\"]\n"
    );
    Case {
        id: format!("glob-generated-{id}"),
        route: Route::Check,
        artifact: None,
        family: Family::PathGlobGeneratedFile,
        role,
        shape: "generated_file_pattern",
        files: glob_files(
            changed,
            vec![("ripr.toml".to_string(), config.into_bytes())],
        ),
        args: check_args(&[]),
        expect,
        changed: changed.to_string(),
        hostile: None,
    }
}

pub(crate) fn glob_cases() -> Vec<Case> {
    use Expect::{GeneratedExcluded, GeneratedNotExcluded, GlobMatch, GlobNonmatch};
    let lib = "src/lib.rs";
    let accented = "src/h\u{e9}llo.rs";
    let long_a = long_a_file();
    let deep = deep_segment_file();
    let mut cases = vec![
        suppression_case(
            "double-star-prefix",
            Role::Control,
            "src/**",
            lib,
            GlobMatch,
        ),
        suppression_case(
            "ordinary-negative",
            Role::Control,
            "tests/**",
            lib,
            GlobNonmatch,
        ),
        suppression_case("star-in-segment", Role::Control, "src/*.rs", lib, GlobMatch),
        suppression_case(
            "star-stays-in-segment",
            Role::Control,
            "*.rs",
            lib,
            GlobNonmatch,
        ),
        suppression_case(
            "question-one-char",
            Role::Control,
            "src/li?.rs",
            lib,
            GlobMatch,
        ),
        suppression_case(
            "question-wrong-length",
            Role::Control,
            "src/l?.rs",
            lib,
            GlobNonmatch,
        ),
        suppression_case(
            "zero-segment-double-star",
            Role::Control,
            "src/**/lib.rs",
            lib,
            GlobMatch,
        ),
        suppression_case(
            "leading-double-star",
            Role::Control,
            "**/lib.rs",
            lib,
            GlobMatch,
        ),
        suppression_case(
            "adjacent-double-star",
            Role::Control,
            "src/**/**/lib.rs",
            lib,
            GlobMatch,
        ),
        suppression_case(
            "leading-dot-segment",
            Role::Control,
            "./src/lib.rs",
            lib,
            GlobMatch,
        ),
        suppression_case(
            "inner-dot-segment",
            Role::Control,
            "src/./lib.rs",
            lib,
            GlobMatch,
        ),
        suppression_case(
            "empty-segment",
            Role::Control,
            "src//lib.rs",
            lib,
            GlobMatch,
        ),
        suppression_case(
            "double-star-inside-segment-is-char-level",
            Role::Control,
            "src/**lib.rs",
            lib,
            GlobMatch,
        ),
        suppression_case(
            "empty-pattern-is-a-policy-error",
            Role::Control,
            "",
            lib,
            Expect::PolicyRefused,
        ),
        suppression_case(
            "unicode-question-one-scalar",
            Role::Control,
            "src/h?llo.rs",
            accented,
            GlobMatch,
        ),
        suppression_case(
            "unicode-question-not-per-byte",
            Role::Control,
            "src/h??llo.rs",
            accented,
            GlobNonmatch,
        ),
        suppression_case(
            "many-star-nonmatch",
            Role::Discriminator,
            &many_star_pattern("src/", 16, "*b"),
            &long_a,
            GlobNonmatch,
        ),
        suppression_case(
            "many-star-match",
            Role::Discriminator,
            &many_star_pattern("src/", 16, "*.rs"),
            &long_a,
            GlobMatch,
        ),
        suppression_case(
            "many-segment-nonmatch",
            Role::Discriminator,
            &many_segment_pattern(),
            &deep,
            GlobNonmatch,
        ),
    ];
    cases.extend([
        generated_case(
            "star-in-segment",
            Role::Control,
            "src/api_*.rs",
            "src/api_v2.rs",
            GeneratedExcluded,
        ),
        generated_case(
            "slashless-matches-any-filename",
            Role::Control,
            "api_*.rs",
            "src/api_v2.rs",
            GeneratedExcluded,
        ),
        generated_case(
            "ordinary-negative",
            Role::Control,
            "lib/**/*.rs",
            "src/api_v2.rs",
            GeneratedNotExcluded,
        ),
        generated_case(
            "zero-segment-double-star",
            Role::Control,
            "src/**/api_v2.rs",
            "src/api_v2.rs",
            GeneratedExcluded,
        ),
        generated_case(
            "question-one-char",
            Role::Control,
            "src/api_v?.rs",
            "src/api_v2.rs",
            GeneratedExcluded,
        ),
        generated_case(
            "question-wrong-length",
            Role::Control,
            "src/api_v??.rs",
            "src/api_v2.rs",
            GeneratedNotExcluded,
        ),
        generated_case(
            "unicode-question-one-scalar",
            Role::Control,
            "src/h?llo.rs",
            accented,
            GeneratedExcluded,
        ),
        generated_case(
            "unicode-question-not-per-byte",
            Role::Control,
            "src/h??llo.rs",
            accented,
            GeneratedNotExcluded,
        ),
        generated_case(
            "many-star-nonmatch",
            Role::Discriminator,
            &many_star_pattern("src/", 16, "*b"),
            &long_a,
            GeneratedNotExcluded,
        ),
        generated_case(
            "many-segment-nonmatch",
            Role::Discriminator,
            &many_segment_pattern(),
            &deep,
            GeneratedNotExcluded,
        ),
    ]);
    cases
}

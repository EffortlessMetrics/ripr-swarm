//! #9496 / RIPR-SPEC-0246 rule 7: a synchronous mocha setup hook that calls
//! the changed owner turns a `weakly_exposed` side-effect finding into
//! `static_unknown` with stop reason `gap_evidence_unresolved`. Every negative
//! reaches the same ladder arm (a strong oracle that does not observe the
//! changed sink) and keeps `weakly_exposed`. Each test pins the parsed hooks
//! before the class.

use super::*;

const REGISTRY: &str = "const names: string[] = [];\n\nexport function register(name: string): number {\n  names.push(name);\n  return names.length;\n}\n\nexport function registered(): readonly string[] {\n  return names;\n}\n";
const HEAD: &str =
    "import { expect } from \"chai\";\nimport { register, registered } from \"../src/registry\";\n";
const TESTS: &str = "  it(\"holds the trimmed name registered in the hook\", () => {\n    expect(registered()).to.deep.equal([\"alpha\"]);\n  });\n\n  it(\"counts a new registration\", () => {\n    expect(register(\"beta\")).to.equal(2);\n  });\n";

fn hook_bodies(source: &str) -> Vec<Vec<String>> {
    extract_tests(Path::new("tests/registry.test.ts"), source)
        .iter()
        .map(|test| test.setup_hook_bodies.clone())
        .collect()
}

fn run_with_head(label: &str, source: &str) -> Result<Finding, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/registry.ts"), REGISTRY)?;
    ts_write_file(&root.join("tests/registry.test.ts"), source)?;
    let changed = ChangedFile {
        path: PathBuf::from("src/registry.ts"),
        added_lines: vec![crate::analysis::diff::ChangedLine {
            line: 4,
            text: "  names.push(name);".to_string(),
            new_side_line: 4,
        }],
        removed_lines: vec![crate::analysis::diff::ChangedLine {
            line: 4,
            text: "  names.push(name.trim());".to_string(),
            new_side_line: 4,
        }],
    };
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed],
    );
    let _ = std::fs::remove_dir_all(&root);
    result?
        .findings
        .into_iter()
        .find(|finding| finding.probe.location.line == 4)
        .ok_or_else(|| format!("{label}: expected a finding on the changed line"))
}

/// The registry suite with `hooks` written before the tests.
fn suite(hooks: &str) -> String {
    format!("{HEAD}\ndescribe(\"registry\", () => {{\n{hooks}\n{TESTS}}});\n")
}

fn assert_abstained(label: &str, source: &str) -> Result<(), String> {
    let finding = run_with_head(label, source)?;
    assert_eq!(
        finding.class,
        ExposureClass::StaticUnknown,
        "{label}: {:?}",
        finding.missing
    );
    assert_eq!(
        finding.stop_reasons,
        vec![crate::domain::StopReason::GapEvidenceUnresolved],
        "{label}"
    );
    assert!(
        finding
            .missing
            .iter()
            .any(|line| line.contains("setup hook calls `register`")),
        "{label}: {:?}",
        finding.missing
    );
    assert!(
        finding
            .recommended_next_step
            .as_deref()
            .is_some_and(|step| step.contains("no new assertion is needed")),
        "{label}: {:?}",
        finding.recommended_next_step
    );
    Ok(())
}

fn assert_gap_kept(label: &str, source: &str) -> Result<(), String> {
    let finding = run_with_head(label, source)?;
    assert_eq!(
        finding.class,
        ExposureClass::WeaklyExposed,
        "{label}: {:?}",
        finding.missing
    );
    assert!(finding.stop_reasons.is_empty(), "{label}");
    Ok(())
}

#[test]
fn a_before_hook_that_calls_the_owner_abstains() -> Result<(), String> {
    let source = suite("  before(() => {\n    register(\"  alpha \");\n  });\n");
    let bodies = hook_bodies(&source);
    assert_eq!(bodies.len(), 2, "{bodies:?}");
    assert!(
        bodies
            .iter()
            .all(|hooks| hooks == &vec!["register(\"  alpha \");".to_string()]),
        "{bodies:?}"
    );
    assert_abstained("hook-owner-before", &source)
}

#[test]
fn the_same_suite_without_a_hook_keeps_the_gap() -> Result<(), String> {
    let source = suite("");
    assert!(hook_bodies(&source).iter().all(Vec::is_empty));
    assert_gap_kept("hook-owner-none", &source)
}

#[test]
fn a_before_each_hook_and_an_enclosing_describe_hook_abstain() -> Result<(), String> {
    assert_abstained(
        "hook-owner-before-each",
        &suite("  beforeEach(function () {\n    register(\"  alpha \");\n  });\n"),
    )?;
    let nested = format!(
        "{HEAD}\ndescribe(\"outer\", () => {{\n  before(() => {{\n    register(\"  alpha \");\n  }});\n\n  describe(\"registry\", () => {{\n{TESTS}  }});\n}});\n"
    );
    assert!(hook_bodies(&nested).iter().all(|hooks| hooks.len() == 1));
    assert_abstained("hook-owner-outer-describe", &nested)
}

#[test]
fn a_hook_in_a_sibling_describe_keeps_the_gap() -> Result<(), String> {
    let source = format!(
        "{HEAD}\ndescribe(\"other\", () => {{\n  before(() => {{\n    register(\"  alpha \");\n  }});\n\n  it(\"is unrelated\", () => {{\n    expect(1).to.equal(1);\n  }});\n}});\n\ndescribe(\"registry\", () => {{\n{TESTS}}});\n"
    );
    let bodies = hook_bodies(&source);
    assert_eq!(bodies.len(), 3, "{bodies:?}");
    assert_eq!(bodies[0].len(), 1, "{bodies:?}");
    assert!(bodies[1..].iter().all(Vec::is_empty), "{bodies:?}");
    assert_gap_kept("hook-owner-sibling", &source)
}

#[test]
fn async_done_returning_and_expression_hooks_are_not_synchronous_setup() -> Result<(), String> {
    for (label, hook) in [
        (
            "async",
            "  before(async () => {\n    register(\"  alpha \");\n  });\n",
        ),
        (
            "done",
            "  before((done) => {\n    register(\"  alpha \");\n    done();\n  });\n",
        ),
        (
            "function-done",
            "  before(function (done) {\n    register(\"  alpha \");\n    done();\n  });\n",
        ),
        (
            "generator",
            "  before(function* () {\n    register(\"  alpha \");\n  });\n",
        ),
        (
            "returning",
            "  before(() => {\n    return register(\"  alpha \");\n  });\n",
        ),
        ("expression", "  before(() => register(\"  alpha \"));\n"),
    ] {
        let source = suite(hook);
        assert!(
            hook_bodies(&source).iter().all(Vec::is_empty),
            "{label}: {:?}",
            hook_bodies(&source)
        );
        assert_gap_kept(&format!("hook-owner-{label}"), &source)?;
    }
    Ok(())
}

#[test]
fn a_hook_that_does_not_call_the_owner_keeps_the_gap() -> Result<(), String> {
    for (label, hook) in [
        (
            "other-function",
            "  before(() => {\n    registered();\n  });\n",
        ),
        (
            "owner-name-in-longer-call",
            "  before(() => {\n    preregister(\"  alpha \");\n  });\n",
        ),
        (
            "member-of-other",
            "  before(() => {\n    other.register(\"  alpha \");\n  });\n",
        ),
        (
            "shadowed",
            "  before(() => {\n    const register = (name: string) => name;\n    register(\"  alpha \");\n  });\n",
        ),
        (
            "comment-and-string",
            "  before(() => {\n    // register(\"  alpha \");\n    const text = \"register('x')\";\n    /* register(\"y\") */\n    console.log(text);\n  });\n",
        ),
    ] {
        let source = suite(hook);
        assert_eq!(hook_bodies(&source).len(), 2, "{label}");
        assert!(
            hook_bodies(&source).iter().all(|hooks| hooks.len() == 1),
            "{label}: hook must be recorded so the owner gate decides"
        );
        assert_gap_kept(&format!("hook-owner-{label}"), &source)?;
    }
    Ok(())
}

#[test]
fn a_hook_the_file_declares_or_imports_elsewhere_is_not_the_runner_hook() -> Result<(), String> {
    let local = suite(
        "  function before(run: () => void) {\n    run();\n  }\n  before(() => {\n    register(\"  alpha \");\n  });\n",
    );
    assert!(hook_bodies(&local).iter().all(Vec::is_empty));
    assert_gap_kept("hook-owner-local-before", &local)?;
    let imported = format!(
        "import {{ before }} from \"./helpers\";\n{}",
        suite("  before(() => {\n    register(\"  alpha \");\n  });\n")
    );
    assert!(hook_bodies(&imported).iter().all(Vec::is_empty));
    assert_gap_kept("hook-owner-imported-before", &imported)?;
    // An explicit mocha import of the hook is the runner's hook.
    let from_mocha = format!(
        "import {{ before }} from \"mocha\";\n{}",
        suite("  before(() => {\n    register(\"  alpha \");\n  });\n")
    );
    assert!(
        hook_bodies(&from_mocha)
            .iter()
            .all(|hooks| hooks.len() == 1)
    );
    assert_abstained("hook-owner-mocha-import", &from_mocha)
}

#[test]
fn a_jest_or_vitest_file_is_not_a_mocha_file() -> Result<(), String> {
    let hook = "  before(() => {\n    register(\"  alpha \");\n  });\n";
    let vitest = format!(
        "import {{ describe, it, expect, before }} from \"vitest\";\n{}",
        suite(hook)
    );
    assert!(hook_bodies(&vitest).iter().all(Vec::is_empty));
    assert_gap_kept("hook-owner-vitest", &vitest)?;
    let jest_globals = format!("import {{ jest }} from \"@jest/globals\";\n{}", suite(hook));
    assert!(hook_bodies(&jest_globals).iter().all(Vec::is_empty));
    assert_gap_kept("hook-owner-jest-globals", &jest_globals)?;
    // A chai import alone does not outweigh a jest global in the file.
    let jest_global_use = format!("{}\njest.setTimeout(1000);\n", suite(hook));
    assert!(hook_bodies(&jest_global_use).iter().all(Vec::is_empty));
    assert_gap_kept("hook-owner-jest-global-use", &jest_global_use)?;
    // No mocha or chai import at all: nothing says which runner owns `before`.
    let bare = suite(hook).replace("import { expect } from \"chai\";\n", "");
    assert!(hook_bodies(&bare).iter().all(Vec::is_empty));
    Ok(())
}

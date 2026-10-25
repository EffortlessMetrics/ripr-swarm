//! Public-path controls for matches! assertion execution eligibility.
use ripr::{CheckInput, ExposureClass, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);
impl Scratch {
    fn create() -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-matches-eligibility-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
        Ok(Self(path))
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct Case {
    name: &'static str,
    attributes: &'static str,
    body: &'static str,
    matches: [ExposureClass; 2],
    equality: [ExposureClass; 2],
}
use ExposureClass::{Exposed, ReachableUnrevealed, StaticUnknown, WeaklyExposed};
const CASES: &[Case] = &[
    Case {
        name: "ordinary",
        attributes: "",
        body: r#"assert!(matches!(parse("bad"), Err(Fault::Bad)));"#,
        matches: [Exposed, Exposed],
        equality: [Exposed, Exposed],
    },
    Case {
        name: "ignored",
        attributes: "#[ignore]",
        body: r#"assert!(matches!(parse("bad"), Err(Fault::Bad)));"#,
        matches: [ReachableUnrevealed, ReachableUnrevealed],
        equality: [ReachableUnrevealed, ReachableUnrevealed],
    },
    Case {
        name: "reason_ignore",
        attributes: r#"#[ignore = "slow"]"#,
        body: r#"assert!(matches!(parse("bad"), Err(Fault::Bad)));"#,
        matches: [ReachableUnrevealed, ReachableUnrevealed],
        equality: [ReachableUnrevealed, ReachableUnrevealed],
    },
    Case {
        name: "active_cfg_ignore",
        attributes: "#[cfg_attr(test, ignore)]",
        body: r#"assert!(matches!(parse("bad"), Err(Fault::Bad)));"#,
        matches: [StaticUnknown, StaticUnknown],
        equality: [StaticUnknown, StaticUnknown],
    },
    Case {
        name: "inactive_cfg_ignore",
        attributes: "#[cfg_attr(not(test), ignore)]",
        body: r#"assert!(matches!(parse("bad"), Err(Fault::Bad)));"#,
        matches: [Exposed, Exposed],
        equality: [StaticUnknown, StaticUnknown],
    },
    Case {
        name: "broad_panic",
        attributes: "#[should_panic]",
        body: r#"assert!(matches!(parse("bad"), Err(Fault::Bad)), "broad panic"); panic!("broad panic");"#,
        matches: [ReachableUnrevealed, ReachableUnrevealed],
        equality: [ReachableUnrevealed, ReachableUnrevealed],
    },
    Case {
        name: "raw_broad_panic",
        attributes: r##"#[should_panic(expected = r#"broad panic"#)]"##,
        body: r#"assert!(matches!(parse("bad"), Err(Fault::Bad)), "broad panic"); panic!("broad panic");"#,
        matches: [ReachableUnrevealed, ReachableUnrevealed],
        equality: [ReachableUnrevealed, ReachableUnrevealed],
    },
    // These native tests genuinely distinguish Bad from Other. Preserve the
    // current conservative result rather than rejecting expected-panic tests.
    Case {
        name: "inverse_panic",
        attributes: "#[should_panic]",
        body: r#"assert!(matches!(parse("bad"), Err(Fault::Other)), "oracle mismatch");"#,
        matches: [ReachableUnrevealed, WeaklyExposed],
        equality: [ReachableUnrevealed, ReachableUnrevealed],
    },
    Case {
        name: "narrow_panic",
        attributes: r#"#[should_panic(expected = "oracle mismatch")]"#,
        body: r#"assert!(matches!(parse("bad"), Err(Fault::Other)), "oracle mismatch");"#,
        matches: [ReachableUnrevealed, WeaklyExposed],
        equality: [ReachableUnrevealed, ReachableUnrevealed],
    },
    Case {
        name: "qualified_ignore",
        attributes: "#[ignore]",
        body: r#"std::assert!(std::matches!(parse("bad"), Err(Fault::Bad)));"#,
        matches: [ReachableUnrevealed, ReachableUnrevealed],
        equality: [ReachableUnrevealed, ReachableUnrevealed],
    },
    Case {
        name: "qualified_ordinary",
        attributes: "",
        body: r#"std::assert!(std::matches!(parse("bad"), Err(Fault::Bad)));"#,
        matches: [Exposed, Exposed],
        equality: [Exposed, Exposed],
    },
    Case {
        name: "ignored_inverse_panic",
        attributes: "#[should_panic]\n#[ignore]",
        body: r#"assert!(matches!(parse("bad"), Err(Fault::Other)), "oracle mismatch");"#,
        matches: [ReachableUnrevealed, ReachableUnrevealed],
        equality: [ReachableUnrevealed, ReachableUnrevealed],
    },
];
fn source(case: &Case, equality: bool) -> String {
    let body = if equality {
        case.body
            .replace(
                "assert!(matches!(parse(\"bad\"), Err(Fault::Bad))",
                "assert_eq!(parse(\"bad\"), Err(Fault::Bad)",
            )
            .replace(
                "assert!(matches!(parse(\"bad\"), Err(Fault::Other))",
                "assert_eq!(parse(\"bad\"), Err(Fault::Other)",
            )
            .replace(
                "std::assert!(std::matches!(parse(\"bad\"), Err(Fault::Bad)))",
                "std::assert_eq!(parse(\"bad\"), Err(Fault::Bad))",
            )
    } else {
        case.body.to_string()
    };
    format!(
        "#[derive(Debug, PartialEq)]\nenum Fault {{ Bad, Other }}\nfn parse(_: &str) -> Result<(), Fault> {{ Err(Fault::Bad) }}\n#[test]\n{}\nfn parse_checks_bad() {{ {body} }}\n",
        case.attributes
    )
}
fn check(case: &Case, equality: bool) -> Result<[ExposureClass; 2], String> {
    check_source(case, &source(case, equality))
}
fn check_source(case: &Case, source: &str) -> Result<[ExposureClass; 2], String> {
    let scratch = Scratch::create()?;
    std::fs::create_dir(scratch.0.join("src")).map_err(|e| e.to_string())?;
    std::fs::write(
        scratch.0.join("Cargo.toml"),
        "[package]\nname=\"matches-control\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[workspace]\n",
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(scratch.0.join("src/lib.rs"), source).map_err(|e| e.to_string())?;
    let after = source.lines().nth(2).ok_or("missing owner")?;
    let before = after.replace("Fault::Bad", "Fault::Other");
    let diff = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -3 +3 @@\n-{before}\n+{after}\n"
    );
    std::fs::write(scratch.0.join("change.diff"), diff).map_err(|e| e.to_string())?;
    let report = check_workspace(CheckInput {
        root: scratch.0.clone(),
        diff_file: Some(scratch.0.join("change.diff")),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let mut classes = Vec::new();
    for family in [ProbeFamily::ErrorPath, ProbeFamily::ReturnValue] {
        let findings: Vec<_> = report
            .findings
            .iter()
            .filter(|f| f.probe.location.line == 3 && f.probe.family == family)
            .collect();
        assert_eq!(findings.len(), 1, "{}: {family:?}", case.name);
        classes.push(findings[0].class.clone());
    }
    classes
        .try_into()
        .map_err(|_| "missing paired findings".to_string())
}
#[test]
fn matches_return_and_error_assertions_require_execution_context() -> Result<(), String> {
    let mut failures = Vec::new();
    for case in CASES {
        let actual = check(case, false)?;
        if actual != case.matches {
            failures.push(format!(
                "{}: {actual:?}, expected {:?}",
                case.name, case.matches
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}
#[test]
fn equality_outcomes_are_the_current_source_baseline() -> Result<(), String> {
    for case in CASES {
        assert_eq!(check(case, true)?, case.equality, "{}", case.name);
    }
    Ok(())
}

#[test]
fn inverse_panic_matches_does_not_require_equality_derives() -> Result<(), String> {
    let case = CASES
        .iter()
        .find(|case| case.name == "inverse_panic")
        .ok_or("missing inverse control")?;
    let source = source(case, false).replace("#[derive(Debug, PartialEq)]", "");
    assert_eq!(check_source(case, &source)?, case.matches);
    Ok(())
}

#[test]
fn matches_honesty_projection_matches_public_analysis() -> Result<(), String> {
    let mut findings = Vec::new();
    for name in ["ignored", "broad_panic"] {
        let case = CASES
            .iter()
            .find(|case| case.name == name)
            .ok_or("missing honesty subject")?;
        for (family, class) in ["error_path", "return_value"]
            .into_iter()
            .zip(check(case, false)?)
        {
            findings.push(
                serde_json::json!({"case":name,"classification":class,"probe":{"family":family}}),
            );
        }
    }
    let actual = serde_json::json!({"kind":"matches_eligibility_test_projection","summary":{"findings":4},"findings":findings});
    let expected: serde_json::Value = serde_json::from_str(include_str!("../../../fixtures/evidence-promotion-honesty-corpus/reports/rust-matches-ineligible-context.json")).map_err(|e|e.to_string())?;
    assert_eq!(actual, expected);
    Ok(())
}

#[test]
fn qualified_matches_require_the_actual_standard_root() -> Result<(), String> {
    let case = Case {
        name: "rebound_standard_root",
        attributes: "",
        body: r#"
            mod fake {
                macro_rules! always_true { ($($tokens:tt)*) => { true }; }
                pub(crate) use always_true as matches;
                pub(crate) use ::std::assert;
            }
            use fake as std;
            std::assert!(std::matches!(parse("bad"), Err(Fault::Bad)));
        "#,
        matches: [ReachableUnrevealed, ReachableUnrevealed],
        equality: [ReachableUnrevealed, ReachableUnrevealed],
    };
    assert!(check(&case, false)?.iter().all(|class| *class != Exposed));
    Ok(())
}

#[test]
fn qualified_assertion_does_not_hide_a_real_matches_binding() -> Result<(), String> {
    let case = Case {
        name: "matches_definition_beside_standard_assertion",
        attributes: "",
        body: r#"
            macro_rules! matches { ($($tokens:tt)*) => { true }; }
            std::assert!(matches!(parse("bad"), Err(Fault::Bad)));
        "#,
        matches: [ReachableUnrevealed, ReachableUnrevealed],
        equality: [ReachableUnrevealed, ReachableUnrevealed],
    };
    assert!(check(&case, false)?.iter().all(|class| *class != Exposed));
    Ok(())
}

#[test]
fn expected_panic_matches_cannot_credit_an_unresolved_panic_source() -> Result<(), String> {
    let case = Case {
        name: "competing_panic_source",
        attributes: "#[should_panic]",
        body: r#"assert!(matches!(parse("bad"), Err(Fault::Bad)));"#,
        matches: [StaticUnknown, StaticUnknown],
        equality: [ReachableUnrevealed, ReachableUnrevealed],
    };
    let plain = source(&case, false);
    let controls = [
        plain.replace(
            "{ Err(Fault::Bad) }",
            "{ panic!(\"owner panic\"); Err(Fault::Bad) }",
        ),
        plain
            .replace(
                "#[test]",
                "fn input() -> &'static str { panic!(\"input panic\") }\n#[test]",
            )
            .replace("parse(\"bad\")", "parse(input())"),
        plain.replace(
            "#[test]",
            "impl Drop for Fault { fn drop(&mut self) { panic!(\"drop panic\") } }\n#[test]",
        ),
        plain.replace("-> Result<(), Fault>", "-> Outcome")
            .replace("#[test]", "enum Outcome { Err(Fault) }\nuse Outcome::Err;\nimpl Drop for Outcome { fn drop(&mut self) { panic!(\"wrapper panic\") } }\n#[test]"),
        plain.replace("Debug, PartialEq", "Debug, PartialEq, PanicDrop")
            .replace("#[test]", "use panic_drop::PanicDrop;\n#[test]"),
    ];
    for source in controls {
        assert_eq!(check_source(&case, &source)?, case.matches, "{source}");
    }
    Ok(())
}

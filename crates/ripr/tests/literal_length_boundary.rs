//! The retained #9637 string-byte-length boundary goes through real analysis.

use ripr::domain::StageState;
use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::sync::atomic::{AtomicU64, Ordering};

const RETAINED: &str =
    include_str!("../../../fixtures/rust-verdict-corpus/subjects/authored-tokens/src/flow.rs.txt");
const DIFF: &str =
    include_str!("../../../fixtures/rust-verdict-corpus/cases/tokens-fits-binding-predicate.diff");

fn finding(source: &str) -> Result<Finding, String> {
    finding_with_diff(source, DIFF)
}

fn finding_with_diff(source: &str, diff: &str) -> Result<Finding, String> {
    finding_with_extra(source, diff, None)
}

fn finding_with_extra(source: &str, diff: &str, extra: Option<&str>) -> Result<Finding, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "ripr-literal-length-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
    let result = (|| {
        std::fs::write(
            root.join("Cargo.toml"),
            format!(
                "{}\n[workspace]\n",
                include_str!(
                    "../../../fixtures/rust-verdict-corpus/subjects/authored-tokens/Cargo.toml"
                )
            ),
        )
        .map_err(|e| e.to_string())?;
        // Materialize the complete retained subject, not a proxy containing
        // only its edited file. Foreign enums/macros are part of the input.
        for (path, text) in [
            (
                "src/lib.rs",
                include_str!(
                    "../../../fixtures/rust-verdict-corpus/subjects/authored-tokens/src/lib.rs.txt"
                ),
            ),
            (
                "src/raw.rs",
                include_str!(
                    "../../../fixtures/rust-verdict-corpus/subjects/authored-tokens/src/raw.rs.txt"
                ),
            ),
            (
                "src/reach.rs",
                include_str!(
                    "../../../fixtures/rust-verdict-corpus/subjects/authored-tokens/src/reach.rs.txt"
                ),
            ),
            (
                "src/scan.rs",
                include_str!(
                    "../../../fixtures/rust-verdict-corpus/subjects/authored-tokens/src/scan.rs.txt"
                ),
            ),
            (
                "tests/public.rs",
                include_str!(
                    "../../../fixtures/rust-verdict-corpus/subjects/authored-tokens/tests/public.rs.txt"
                ),
            ),
        ] {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().ok_or("retained source has no parent")?)
                .map_err(|e| e.to_string())?;
            std::fs::write(path, text).map_err(|e| e.to_string())?;
        }
        if let Some(extra) = extra {
            use std::io::Write;
            std::fs::OpenOptions::new()
                .append(true)
                .open(root.join("src/lib.rs"))
                .and_then(|mut file| writeln!(file, "mod foreign;"))
                .map_err(|e| e.to_string())?;
            std::fs::write(root.join("src/foreign.rs"), extra).map_err(|e| e.to_string())?;
        }
        std::fs::write(
            root.join("doc.md"),
            "```rust\nlet _ = tokens::flow::fits(\"abc\", 2 + 1);\n```\n",
        )
        .map_err(|e| e.to_string())?;
        std::fs::write(root.join("src/flow.rs"), source).map_err(|e| e.to_string())?;
        std::fs::write(root.join("diff.patch"), diff).map_err(|e| e.to_string())?;
        let output = check_workspace(CheckInput {
            root: root.clone(),
            base: None,
            diff_file: Some(root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let mut findings = output
            .findings
            .into_iter()
            .filter(|finding| finding.probe.expression == "end <= width")
            .collect::<Vec<_>>();
        if findings.len() != 1 {
            return Err(format!(
                "expected one retargeted predicate, got {}",
                findings.len()
            ));
        }
        findings
            .pop()
            .ok_or_else(|| "retargeted predicate missing".into())
    })();
    let _ = std::fs::remove_dir_all(root);
    result
}

fn candidate() -> String {
    assert_eq!(RETAINED.matches("let end = text.len();").count(), 1);
    RETAINED.replace("let end = text.len();", "let end = text.as_bytes().len();")
}

#[test]
fn retained_literal_byte_length_row_is_credited() -> Result<(), String> {
    let finding = finding(&candidate())?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:#?}");
    assert!(
        finding
            .activation
            .observed_values
            .iter()
            .any(|fact| { fact.value == "end = 3" && fact.text.contains("as_bytes().len()") })
    );
    assert!(
        finding
            .activation
            .observed_values
            .iter()
            .all(|fact| !fact.value.contains(" == "))
    );
    assert!(
        finding
            .evidence
            .iter()
            .all(|evidence| !evidence.starts_with("binding_predicate_value_unresolved:"))
    );
    Ok(())
}

#[test]
fn byte_lengths_decode_utf8_and_escapes_without_counting_characters() -> Result<(), String> {
    for input in ["\"é\"", "\"\\u{e9}\"", "\"a\\n\"", "r#\"é\"#"] {
        let source = candidate().replace("fits(\"abc\", 3)", &format!("fits({input}, 2)"));
        assert_eq!(finding(&source)?.class, ExposureClass::Exposed, "{input}");
        let source = source.replace(
            &format!("assert!(fits({input}, 2))"),
            &format!("assert!(!fits({input}, 1))"),
        );
        assert_eq!(
            finding(&source)?.class,
            ExposureClass::InfectionUnknown,
            "{input}"
        );
        assert!(
            finding(&source)?
                .evidence
                .iter()
                .any(|evidence| evidence.starts_with("binding_predicate_value_unresolved:"))
        );
    }
    Ok(())
}

#[test]
fn distant_inputs_and_cross_row_values_stay_unknown() -> Result<(), String> {
    for (first, second) in [
        ("fits(\"a\", 3)", "fits(\"abcde\", 3)"),
        ("fits(\"ab\", 3)", "fits(\"abcd\", 2)"),
    ] {
        let source = candidate()
            .replace("fits(\"abc\", 3)", first)
            .replace("fits(\"abcd\", 3)", second);
        assert_eq!(
            finding(&source)?.class,
            ExposureClass::InfectionUnknown,
            "{first}, {second}"
        );
    }
    Ok(())
}

#[test]
fn a_same_line_discarded_row_cannot_lend_an_asserted_far_call_its_boundary() -> Result<(), String> {
    let source = candidate().replace(
        "assert!(fits(\"abc\", 3));",
        "let _ = fits(\"abc\", 3); assert!(fits(\"ab\", 3));",
    );
    let finding = finding(&source)?;
    assert_eq!(finding.ripr.infect.state, StageState::Yes);
    assert_eq!(finding.ripr.propagate.state, StageState::Yes);
    assert_eq!(finding.ripr.reveal.discriminate.state, StageState::Weak);
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:#?}");
    Ok(())
}

#[test]
fn a_boundary_call_in_an_assertion_message_cannot_supply_a_pin() -> Result<(), String> {
    let source = candidate().replace(
        "assert!(fits(\"abc\", 3));",
        "assert!(fits(\"ab\", 3), \"{}\", fits(\"abc\", 3));",
    );
    assert_ne!(finding(&source)?.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_hidden_owner_alias_keeps_the_literal_input_family_unknown() -> Result<(), String> {
    for hidden in [
        "let alias = fits; let _ = alias(\"abc\", 2 + 1);",
        "let alias = r#fits; let _ = alias(\"abc\", 2 + 1);",
    ] {
        let source = candidate().replace(
            "assert!(fits(\"abc\", 3));",
            &format!("assert!(fits(\"abc\", 3)); {hidden}"),
        );
        let finding = finding(&source)?;
        assert_eq!(
            finding.class,
            ExposureClass::InfectionUnknown,
            "{hidden}: {finding:#?}"
        );
    }
    let source = format!(
        "{}\nfn hidden() {{ let alias = fits; let _ = alias(\"abc\", 2 + 1); }}\n",
        candidate()
    );
    assert_eq!(finding(&source)?.class, ExposureClass::InfectionUnknown);
    Ok(())
}

#[test]
fn foreign_owner_references_and_unindexed_docs_keep_inputs_unknown() -> Result<(), String> {
    for extra in [
        "fn hidden() { let alias = crate::flow::fits; let _ = alias(\"abc\", 2 + 1); }",
        "use crate::flow::fits as renamed; fn hidden() { let _ = renamed(\"abc\", 2 + 1); }",
        "macro_rules! hidden { () => { crate::flow::fits(\"abc\", 2 + 1) }; } fn invoke() { hidden!(); }",
        "#![doc = include_str!(\"../doc.md\")]",
        "#![doc = concat!(\"```rust\\nlet _ = tokens::flow::\", \"fit\", \"s(\\\"abc\\\", 2 + 1);\\n```\")]",
    ] {
        let finding = finding_with_extra(&candidate(), DIFF, Some(extra))?;
        assert_eq!(finding.class, ExposureClass::InfectionUnknown, "{extra}");
        assert!(
            finding
                .activation
                .observed_values
                .iter()
                .all(|fact| { !fact.text.contains("via as_bytes().len()") }),
            "{extra}"
        );
    }
    Ok(())
}

#[test]
fn computed_arguments_and_unsupported_owner_expressions_stay_unknown() -> Result<(), String> {
    for argument in [
        "build(\"abc\")",
        "\"abc\".trim()",
        "if true { \"abc\" } else { \"\" }",
    ] {
        let source = candidate().replace("fits(\"abc\", 3)", &format!("fits({argument}, 3)"));
        let source = format!("{source}\nfn build(text: &str) -> &str {{ text }}\n");
        assert_eq!(
            finding(&source)?.class,
            ExposureClass::InfectionUnknown,
            "{argument}"
        );
    }
    for initializer in ["text.trim().as_bytes().len()", "text.as_bytes().len() + 0"] {
        let source = candidate().replace("text.as_bytes().len()", initializer);
        let diff = DIFF.replace("text.as_bytes().len()", initializer);
        assert_eq!(
            finding_with_diff(&source, &diff)?.class,
            ExposureClass::InfectionUnknown,
            "{initializer}"
        );
    }
    // #9731 resolves a literal `chars().count()` boundary on its own path:
    // infection is no longer unknown there, but this byte-length row must not
    // claim it and the finding must not become exposed from it.
    let initializer = "text.chars().count()";
    let source = candidate().replace("text.as_bytes().len()", initializer);
    let diff = DIFF.replace("text.as_bytes().len()", initializer);
    let finding = finding_with_diff(&source, &diff)?;
    assert_ne!(finding.class, ExposureClass::Exposed, "{initializer}");
    assert!(
        finding
            .activation
            .observed_values
            .iter()
            .all(|fact| !fact.text.contains("via as_bytes().len()")),
        "{initializer}"
    );
    Ok(())
}

#[test]
fn primitive_aliases_and_imports_cannot_supply_intrinsic_meaning() -> Result<(), String> {
    for declaration in [
        "type str = std::primitive::str;",
        "type r#str = std::primitive::str;",
        "type usize = std::primitive::usize;",
        "mod foreign { pub type Size = std::primitive::usize; } use foreign::Size as usize;",
        "use std::primitive::usize as r#usize;",
        "type bool = std::primitive::bool;",
    ] {
        let source = format!("{}\n{declaration}\n", candidate());
        assert_eq!(
            finding(&source)?.class,
            ExposureClass::InfectionUnknown,
            "{declaration}"
        );
    }
    let source = format!("{}\nuse std::primitive::usize;\n", candidate());
    // Primitive binding authority alone admits this builtin import. The
    // unchanged reference-completeness authority has a narrower
    // import grammar, so this context remains intentionally unsupported.
    assert_eq!(finding(&source)?.class, ExposureClass::InfectionUnknown);
    Ok(())
}

#[test]
fn an_unsupported_body_cannot_borrow_a_sibling_returned_tail() -> Result<(), String> {
    let source = candidate().replace(
        "let end = text.as_bytes().len();",
        "let end = text.as_bytes().len();\n    let _extra = width;",
    );
    let source = format!(
        "{source}\nfn sibling(text: &str, width: usize) -> bool {{ let end = text.as_bytes().len(); end <= width }}\n"
    );
    let finding = finding(&source)?;
    assert_eq!(finding.class, ExposureClass::InfectionUnknown);
    assert!(finding.flow_sinks.is_empty(), "{:?}", finding.flow_sinks);
    Ok(())
}

#[test]
fn same_named_foreign_owner_and_weak_oracle_do_not_get_credit() -> Result<(), String> {
    let source = candidate().replace(
        "assert!(fits(\"abc\", 3));",
        "assert!(fits(\"ab\", 3)); assert!(Other::fits(\"abc\", 3));",
    );
    let source = format!(
        "{source}\nstruct Other; impl Other {{ fn fits(_: &str, _: usize) -> bool {{ true }} }}\n"
    );
    assert_ne!(finding(&source)?.class, ExposureClass::Exposed);
    let source = candidate().replace(
        "assert!(fits(\"abc\", 3));",
        "assert!(fits(\"abc\", 3) || true);",
    );
    let finding = finding(&source)?;
    assert_eq!(finding.ripr.infect.state, StageState::Yes);
    assert_eq!(finding.ripr.propagate.state, StageState::Yes);
    assert_eq!(finding.ripr.reveal.discriminate.state, StageState::Weak);
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:#?}");
    Ok(())
}

#[test]
fn a_single_equal_row_or_a_conditionally_run_row_is_not_credited() -> Result<(), String> {
    // `<=` -> `>=` / `==` survives a lone equal row.
    let lone = candidate().replace("assert!(!fits(\"abcd\", 3));", "");
    assert_eq!(finding(&lone)?.class, ExposureClass::InfectionUnknown);
    for (from, to) in [
        (
            "assert!(!fits(\"abcd\", 3));",
            "if false { assert!(!fits(\"abcd\", 3)); }",
        ),
        (
            "assert!(fits(\"abc\", 3));",
            "if false { assert!(fits(\"abc\", 3)); }",
        ),
        (
            "assert!(!fits(\"abcd\", 3));",
            "return; assert!(!fits(\"abcd\", 3));",
        ),
        (
            "assert!(!fits(\"abcd\", 3));",
            "let f = || assert!(!fits(\"abcd\", 3));",
        ),
    ] {
        let source = candidate().replace(from, to);
        assert_ne!(finding(&source)?.class, ExposureClass::Exposed, "{to}");
    }
    Ok(())
}

#[test]
fn short_circuited_attributed_or_shadowed_rows_are_not_credited() -> Result<(), String> {
    // A row guarded by a short-circuit never has to run, a cfg-gated or
    // ignored row may not be compiled in, and a same-named nested `fn`
    // after the call can be the callee.
    for (from, to) in [
        (
            "assert!(!fits(\"abcd\", 3));",
            "assert!(true || !fits(\"abcd\", 3));",
        ),
        (
            "assert!(!fits(\"abcd\", 3));",
            "assert!(false && !fits(\"abcd\", 3));",
        ),
        (
            "assert!(!fits(\"abcd\", 3));",
            "assert!(!fits(\"abcd\", 3)); fn fits(_: &str, _: usize) -> bool { false }",
        ),
    ] {
        let source = candidate().replace(from, to);
        assert_ne!(finding(&source)?.class, ExposureClass::Exposed, "{to}");
    }
    Ok(())
}

#[test]
fn an_old_initializer_other_than_plain_len_is_not_assumed_equivalent() -> Result<(), String> {
    // `chars().count()` -> `as_bytes().len()` can change the value for
    // non-ASCII text; the literal rows were never evaluated against the old
    // initializer, so the byte-length route must not credit the edit.
    for old in [
        "text.chars().count()",
        "text.trim().len()",
        "text.len() + 0",
    ] {
        let source = candidate();
        let diff = DIFF.replace("text.len()", old);
        let finding = finding_with_diff(&source, &diff)?;
        assert!(
            finding
                .activation
                .observed_values
                .iter()
                .all(|fact| !fact.text.contains("via as_bytes().len()")),
            "{old}: {:?}",
            finding.class
        );
        assert_ne!(finding.class, ExposureClass::Exposed, "{old}");
    }
    Ok(())
}

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
            "[package]\nname = \"tokens\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[workspace]\n",
        )
        .map_err(|e| e.to_string())?;
        std::fs::write(root.join("src/lib.rs"), "mod flow;\n").map_err(|e| e.to_string())?;
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
    for initializer in [
        "text.trim().as_bytes().len()",
        "text.as_bytes().len() + 0",
        "text.chars().count()",
    ] {
        let source = candidate().replace("text.as_bytes().len()", initializer);
        let diff = DIFF.replace("text.as_bytes().len()", initializer);
        assert_eq!(
            finding_with_diff(&source, &diff)?.class,
            ExposureClass::InfectionUnknown,
            "{initializer}"
        );
    }
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
    assert_eq!(finding(&source)?.class, ExposureClass::Exposed);
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
    assert_ne!(finding(&source)?.class, ExposureClass::Exposed);
    Ok(())
}

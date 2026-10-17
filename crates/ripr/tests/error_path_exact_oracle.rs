//! Literal upstream stimulus: existing exact errors must not imply a new test.
#![cfg(feature = "lang-rust")]

use ripr::{CheckInput, CheckOutput, ExposureClass, Mode, OutputFormat, check_workspace};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const SOURCE: &str =
    include_str!("../../../fixtures/rust-verdict-corpus/subjects/rust-hex/src/lib.rs.txt");

struct Fixture(PathBuf);

impl Fixture {
    fn new(source: &str, before: &str, after: &str) -> Result<Self, Box<dyn std::error::Error>> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let fixture = Self(std::env::temp_dir().join(format!(
            "ripr-exact-error-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        )));
        std::fs::create_dir_all(fixture.0.join("src"))?;
        std::fs::write(
            fixture.0.join("Cargo.toml"),
            "[package]\nname='exact_error'\nversion='0.0.0'\nedition='2024'\n[workspace]\n[features]\ndefault=['std']\nstd=['alloc']\nalloc=[]\n",
        )?;
        let line = source
            .lines()
            .position(|line| line.trim() == before)
            .ok_or("missing stimulus")?
            + 1;
        assert_eq!(
            source.lines().filter(|line| line.trim() == before).count(),
            1
        );
        std::fs::write(
            fixture.0.join("src/lib.rs"),
            source.replacen(before, after, 1),
        )?;
        std::fs::write(
            fixture.0.join("input.diff"),
            format!(
                "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -{line} +{line} @@\n-{before}\n+{after}\n"
            ),
        )?;
        Ok(fixture)
    }

    fn check(&self) -> Result<CheckOutput, String> {
        check_workspace(CheckInput {
            root: self.0.clone(),
            diff_file: Some(self.0.join("input.diff")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn check_hex_source(source: &str) -> Result<CheckOutput, Box<dyn std::error::Error>> {
    let fixture = Fixture::new(
        source,
        "*byte = val(data, 2 * i)?;",
        "*byte = val(data, 2 * i)? ^ 0;",
    )?;
    std::fs::write(
        fixture.0.join("Cargo.toml"),
        include_str!("../../../fixtures/rust-verdict-corpus/subjects/rust-hex/Cargo.toml"),
    )?;
    let support = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/rust-verdict-corpus/subjects/rust-hex/src");
    for name in ["error", "serde"] {
        std::fs::copy(
            support.join(format!("{name}.rs.txt")),
            fixture.0.join(format!("src/{name}.rs")),
        )?;
    }
    Ok(fixture.check()?)
}

#[test]
fn literal_hex_callee_error_does_not_prescribe_an_already_existing_test()
-> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        SOURCE.lines().nth(331).map(str::trim),
        Some("*byte = val(data, 2 * i)?;")
    );
    for witness in [
        "pub fn test_invalid_char()",
        "pub fn test_from_hex_whitespace()",
        "FromHexError::InvalidHexCharacter { c: 'g', index: 3 }",
        "FromHexError::InvalidHexCharacter { c: ' ', index: 4 }",
    ] {
        assert!(
            SOURCE.contains(witness),
            "missing literal witness: {witness}"
        );
    }
    let output = check_hex_source(SOURCE)?;

    let anchored = output
        .findings
        .iter()
        .filter(|f| {
            f.probe.location.line == 332 && f.probe.family == ripr::domain::ProbeFamily::ErrorPath
        })
        .collect::<Vec<_>>();
    assert_eq!(
        anchored.len(),
        1,
        "literal stimulus must produce exactly one error probe"
    );
    assert!(
        output.findings.iter().any(|f| f.probe.location.line == 332
            && f.probe.family == ripr::domain::ProbeFamily::CallDeletion
            && f.class == ExposureClass::Exposed),
        "the existing caller-buffer observation must retain its credit"
    );
    for finding in anchored {
        assert!(
            matches!(
                finding.class,
                ExposureClass::PropagationUnknown | ExposureClass::StaticUnknown
            ),
            "{}: {:?}: {:?}",
            finding.probe.family.as_str(),
            finding.class,
            finding.recommended_next_step
        );
        assert_eq!(
            finding.ripr.propagate.state,
            ripr::domain::StageState::Unknown
        );
        assert_eq!(finding.flow_sinks.len(), 1);
        assert!(
            finding
                .flow_sinks
                .iter()
                .all(|sink| sink.kind == ripr::domain::FlowSinkKind::Unknown)
        );
        let step = finding
            .recommended_next_step
            .as_deref()
            .ok_or("missing inspection advice")?;
        assert!(!step.contains("Add a test input"), "{step}");
        assert!(
            !finding
                .missing
                .iter()
                .any(|s| s == "No exact error variant discriminator was detected")
        );
    }
    Ok(())
}

#[test]
fn exact_error_oracle_beyond_display_cap_keeps_the_propagation_limit()
-> Result<(), Box<dyn std::error::Error>> {
    // Extra successful calls do not remove the original exact error tests.
    // Vary name order as well as forcing the eight-row display window.
    for prefix in ["a", "z"] {
        let mut source = SOURCE.to_string();
        source.push_str("\n#[cfg(test)]\nmod additional_successes {\nuse super::*;\n");
        for i in 0..8 {
            source.push_str(&format!(
                "#[test]\nfn {prefix}{i}_success() {{ assert_eq!(decode_to_slice(b\"6b697769\", &mut [0u8;4]), Ok(())); }}\n"
            ));
        }
        source.push_str("}\n");
        let output = check_hex_source(&source)?;
        let errors = output
            .findings
            .iter()
            .filter(|f| {
                f.probe.location.line == 332
                    && f.probe.family == ripr::domain::ProbeFamily::ErrorPath
            })
            .collect::<Vec<_>>();
        assert_eq!(errors.len(), 1);
        let error = errors[0];
        assert_eq!(error.related_tests.len(), 8);
        assert!(error.related_tests_total() > error.related_tests.len());
        if prefix == "a" {
            assert!(
                error.related_tests.iter().all(|test| {
                    test.oracle_kind != ripr::domain::OracleKind::ExactErrorVariant
                })
            );
        }
        assert_eq!(
            error.class,
            ExposureClass::PropagationUnknown,
            "extra success test prefix {prefix}: {:?}",
            error.recommended_next_step
        );
        assert!(
            error
                .recommended_next_step
                .as_deref()
                .is_some_and(|step| !step.contains("Add a test input"))
        );
    }
    Ok(())
}

#[test]
fn explicit_missing_error_arm_keeps_its_prescription() -> Result<(), Box<dyn std::error::Error>> {
    let source = "#[derive(Debug, PartialEq)]
enum E { Old, Bad }
fn parse(flag: bool) -> Result<u8, E> {
    if flag {
        return Err(E::Old);
    }
    Ok(9)
}
#[test]
fn good() { assert_eq!(parse(false), Ok(9)); }
";
    let fixture = Fixture::new(source, "return Err(E::Old);", "return Err(E::Bad);")?;
    let output = fixture.check()?;

    let errors = output
        .findings
        .iter()
        .filter(|f| f.probe.family == ripr::domain::ProbeFamily::ErrorPath)
        .collect::<Vec<_>>();
    assert_eq!(errors.len(), 1);
    assert!(matches!(
        errors[0].class,
        ExposureClass::WeaklyExposed | ExposureClass::ReachableUnrevealed
    ));
    assert!(
        errors[0]
            .activation
            .missing_discriminators
            .iter()
            .any(|f| f.value.contains("Bad"))
    );
    assert!(
        errors[0]
            .recommended_next_step
            .as_deref()
            .is_some_and(|step| step.contains("exact error variant"))
    );
    Ok(())
}

#[test]
fn modeled_question_mark_error_keeps_its_missing_variant() -> Result<(), Box<dyn std::error::Error>>
{
    let source = "#[derive(Debug, PartialEq)]
enum E { Old, Bad }
fn digit(c: char) -> Option<u8> { if c == '1' { Some(1) } else { None } }
fn parse(c: char) -> Result<u8, E> {
    let d = digit(c).ok_or(E::Old)?;
    Ok(d)
}
#[test]
fn good() { assert_eq!(parse('1'), Ok(1)); }
";
    let fixture = Fixture::new(
        source,
        "let d = digit(c).ok_or(E::Old)?;",
        "let d = digit(c).ok_or(E::Bad)?;",
    )?;
    let output = fixture.check()?;

    let errors = output
        .findings
        .iter()
        .filter(|f| f.probe.family == ripr::domain::ProbeFamily::ErrorPath)
        .collect::<Vec<_>>();
    assert_eq!(errors.len(), 1);
    assert!(matches!(
        errors[0].class,
        ExposureClass::WeaklyExposed | ExposureClass::ReachableUnrevealed
    ));
    assert!(
        errors[0]
            .activation
            .missing_discriminators
            .iter()
            .any(|f| f.value.contains("Bad"))
    );
    Ok(())
}

#[test]
fn opaque_error_conversion_retains_its_existing_limit() -> Result<(), Box<dyn std::error::Error>> {
    let source = "use std::error::Error;
fn leaf() -> Result<(), std::io::Error> { Ok(()) }
fn wrap() -> Result<(), Box<dyn Error>> {
    leaf().map_err(|e| Box::<dyn Error>::from(e))?;
    Ok(())
}
#[test]
fn existing() { assert_eq!(wrap().is_ok(), true); }
";
    let fixture = Fixture::new(
        source,
        "leaf().map_err(|e| Box::<dyn Error>::from(e))?;",
        "leaf().map_err(|e| Into::<Box<dyn Error>>::into(e))?;",
    )?;
    let output = fixture.check()?;

    assert!(!output.findings.is_empty());
    assert!(
        output
            .findings
            .iter()
            .all(|f| f.class == ExposureClass::PropagationUnknown)
    );
    Ok(())
}

#[test]
fn direct_odd_length_error_keeps_its_exact_discriminator() -> Result<(), Box<dyn std::error::Error>>
{
    let source = "#[derive(Debug, PartialEq)]
enum E { Odd }
fn decode(data: &[u8], out: &mut [u8]) -> Result<(), E> {
    if data.len() % 2 != 0 {
        return Err(E::Odd);
    }
    Ok(())
}
#[test]
fn odd() { assert_eq!(decode(b\"6\", &mut [0]), Err(E::Odd)); }
";
    let fixture = Fixture::new(
        source,
        "return Err(E::Odd);",
        "return Err::<(), E>(E::Odd);",
    )?;
    let output = fixture.check()?;
    let errors = output
        .findings
        .iter()
        .filter(|f| f.probe.family == ripr::domain::ProbeFamily::ErrorPath)
        .collect::<Vec<_>>();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn success_only_checks_keep_the_missing_error_arm_gap() -> Result<(), Box<dyn std::error::Error>> {
    let source = "use std::io::{self, Read};
fn read_all<R: Read>(mut rdr: R) -> io::Result<usize> {
    let mut buf = [0_u8; 32];
    let n = rdr.read(&mut buf)?;
    Ok(n)
}
#[test]
fn success() {
    let rdr: &[u8] = b\"homer lisa\";
    assert_eq!(read_all(rdr).ok(), Some(10));
}
";
    let fixture = Fixture::new(
        source,
        "let n = rdr.read(&mut buf)?;",
        "let n = rdr.read(&mut buf)? + 0;",
    )?;
    let output = fixture.check()?;
    let errors = output
        .findings
        .iter()
        .filter(|f| f.probe.family == ripr::domain::ProbeFamily::ErrorPath)
        .collect::<Vec<_>>();
    assert_eq!(errors.len(), 1);
    assert!(matches!(
        errors[0].class,
        ExposureClass::WeaklyExposed | ExposureClass::ReachableUnrevealed
    ));
    Ok(())
}

#[test]
fn proximity_error_oracle_does_not_replace_a_reaching_error_oracle()
-> Result<(), Box<dyn std::error::Error>> {
    let source = "#[derive(Debug, PartialEq)]
enum E { Bad }
fn leaf(c: char) -> Result<u8, E> { if c == '1' { Ok(1) } else { Err(E::Bad) } }
fn parse(c: char) -> Result<u8, E> {
    let d = leaf(c)?;
    Ok(d)
}
#[test]
fn success() { assert_eq!(parse('1'), Ok(1)); }
";
    for (error_test, expected) in [
        (
            "fn sibling() -> Result<u8, E> { Err(E::Bad) }\n#[test]\nfn exact_sibling() { assert_eq!(sibling(), Err(E::Bad)); }\n",
            ExposureClass::WeaklyExposed,
        ),
        (
            "#[test]\nfn exact_owner() { assert_eq!(parse('x'), Err(E::Bad)); }\n",
            ExposureClass::PropagationUnknown,
        ),
    ] {
        let fixture = Fixture::new(
            &format!("{source}{error_test}"),
            "let d = leaf(c)?;",
            "let d = leaf(c)? + 0;",
        )?;
        let output = fixture.check()?;
        let errors = output
            .findings
            .iter()
            .filter(|f| f.probe.family == ripr::domain::ProbeFamily::ErrorPath)
            .collect::<Vec<_>>();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].related_tests.iter().any(|test| {
            test.oracle_kind == ripr::domain::OracleKind::ExactErrorVariant
                && test.oracle_strength == ripr::domain::OracleStrength::Strong
        }));
        assert_eq!(errors[0].class, expected, "{error_test}");
    }
    Ok(())
}

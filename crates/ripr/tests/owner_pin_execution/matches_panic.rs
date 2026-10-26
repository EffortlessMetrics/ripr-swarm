//! Native harness truth stays distinct from bounded static support.
use super::{Scratch, compiles, run};
use std::path::Path;

#[test]
fn matches_expected_panic_native_outcomes_are_not_support_promotion() -> Result<(), String> {
    let scratch = Scratch::create()?;
    let source = r#"
#[derive(Debug, PartialEq)]
enum Fault { Bad, Other }
fn parse(_: &str) -> Result<(), Fault> {
    #[cfg(mutant)] { Err(Fault::Other) }
    #[cfg(not(mutant))] { Err(Fault::Bad) }
}
#[test]
#[ignore]
fn ignored() { assert!(matches!(parse("bad"), Err(Fault::Bad))); }
#[test]
#[should_panic]
fn inverse() { assert!(matches!(parse("bad"), Err(Fault::Other)), "oracle mismatch"); }
#[test]
#[should_panic(expected = "oracle mismatch")]
fn narrow_inverse() { assert!(matches!(parse("bad"), Err(Fault::Other)), "oracle mismatch"); }
#[test]
#[should_panic]
fn broad_fallback() { assert!(matches!(parse("bad"), Err(Fault::Bad)), "broad panic"); panic!("broad panic"); }
#[test]
#[should_panic(expected = "broad panic")]
fn narrow_fallback() { assert!(matches!(parse("bad"), Err(Fault::Bad)), "broad panic"); panic!("broad panic"); }
#[test]
#[should_panic(expected = "tail oracle")]
fn deliberate_tail_refusal() { assert!(matches!(parse("bad"), Err(Fault::Bad)), "mismatch"); panic!("tail oracle"); }
#[test]
#[should_panic(expected = "required message")]
fn wrong_expected_message() { assert!(matches!(parse("bad"), Err(Fault::Bad)), "different message"); }
"#;
    let input = scratch.0.join("native_matches.rs");
    std::fs::write(&input, source).map_err(|error| error.to_string())?;
    for (name, mutant, passed, failed) in [("original", false, 5, 1), ("mutant", true, 2, 4)] {
        let binary = scratch
            .0
            .join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        let mut arguments = vec![
            "--edition=2024".as_ref(),
            "--test".as_ref(),
            input.as_os_str(),
            "-o".as_ref(),
            binary.as_os_str(),
        ];
        if mutant {
            arguments.extend([
                std::ffi::OsStr::new("--cfg"),
                std::ffi::OsStr::new("mutant"),
            ]);
        }
        compiles(run(Path::new("rustc"), &arguments)?, name)?;
        let output = run(&binary, &["--test-threads=1".as_ref()])?;
        assert_eq!(output.status.code(), Some(101), "{name}");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("running 7 tests"), "{name}: {stdout:?}");
        assert!(
            stdout.contains(&format!(
                "{passed} passed; {failed} failed; 1 ignored; 0 measured; 0 filtered out"
            )),
            "{name}: {stdout:?}"
        );
        for (test, success) in [
            ("broad_fallback", true),
            ("narrow_fallback", true),
            ("inverse", !mutant),
            ("narrow_inverse", !mutant),
            ("deliberate_tail_refusal", !mutant),
            ("wrong_expected_message", false),
        ] {
            let result = if success { "ok" } else { "FAILED" };
            assert!(
                stdout.contains(&format!("test {test} - should panic ... {result}")),
                "{name}: {stdout:?}"
            );
        }
        assert!(
            stdout.contains("test ignored ... ignored"),
            "{name}: {stdout:?}"
        );
    }
    Ok(())
}

//! RIPR-SPEC-0197: static helper credit and independent runtime discrimination.
use super::source_runtime_control;

#[test]
fn polling_helper_runtime_controls_observe_the_polled_owner() -> Result<(), String> {
    for (case, source, detects_wrong) in [
        (
            "direct",
            include_str!("../../../../fixtures/rust_polling_helper_direct/input/src/lib.rs"),
            true,
        ),
        (
            "shadowed",
            include_str!("../../../../fixtures/rust_polling_helper_shadowed/input/src/lib.rs"),
            false,
        ),
        (
            "primitive_shadow",
            include_str!(
                "../../../../fixtures/rust_polling_helper_primitive_shadow/input/src/lib.rs"
            ),
            false,
        ),
    ] {
        for (label, replacement, fails) in [
            ("correct", "    p / 10 + p", false),
            ("wrong", "    p / 5 + p", detects_wrong),
        ] {
            source_runtime_control(
                &source.replacen("    p / 10 + p", replacement, 1),
                &format!("polling_helper_{case}_{label}"),
                1,
                fails,
            )?;
        }
    }
    Ok(())
}

/// A zero exit before libtest completes its declared subject is not an oracle.
#[test]
fn a_trait_intercepted_helper_has_no_completed_runtime_discriminator() -> Result<(), String> {
    use super::{Scratch, compiles, run};
    use std::ffi::OsStr;
    use std::path::Path;
    for (case, source, child, marker) in [
        (
            "method",
            include_str!("../../../../fixtures/rust_polling_helper_method_shadow/input/src/lib.rs"),
            None,
            "HIJACK_AS_MUT",
        ),
        (
            "associated",
            include_str!(
                "../../../../fixtures/rust_polling_helper_associated_macro/input/src/lib.rs"
            ),
            None,
            "ASSOCIATED_HIJACK_AS_MUT",
        ),
        (
            "import_root",
            include_str!(
                "../../../../fixtures/rust_polling_helper_counterfeit_import_root/input/src/lib.rs"
            ),
            Some(include_str!(
                "../../../../fixtures/rust_polling_helper_counterfeit_import_root/input/src/tests/std.rs"
            )),
            "CROSS_ROOT_HIJACK_AS_MUT",
        ),
    ] {
        for (label, expression) in [("correct", "    p / 10 + p"), ("wrong", "    p / 5 + p")] {
            let scratch = Scratch::create()?;
            let path = scratch.0.join("subject.rs");
            std::fs::write(&path, source.replacen("    p / 10 + p", expression, 1))
                .map_err(|e| e.to_string())?;
            if let Some(child) = child {
                std::fs::create_dir_all(scratch.0.join("tests")).map_err(|e| e.to_string())?;
                std::fs::write(scratch.0.join("tests/std.rs"), child).map_err(|e| e.to_string())?;
            }
            let executable = scratch
                .0
                .join(format!("test{}", std::env::consts::EXE_SUFFIX));
            compiles(
                run(
                    Path::new("rustc"),
                    &[
                        OsStr::new("--edition=2024"),
                        OsStr::new("--test"),
                        path.as_os_str(),
                        OsStr::new("-o"),
                        executable.as_os_str(),
                    ],
                )?,
                label,
            )?;
            let inventory = run(&executable, &[OsStr::new("--list")])?;
            assert!(inventory.status.success());
            assert!(
                String::from_utf8_lossy(&inventory.stdout).contains("1 test, 0 benchmarks"),
                "nonempty exact subject inventory"
            );
            let result = run(&executable, &[OsStr::new("--nocapture")])?;
            let stdout = String::from_utf8_lossy(&result.stdout);
            assert_eq!(
                result.status.code(),
                Some(0),
                "{case}/{label}: process exits before test completion"
            );
            assert!(
                stdout.contains("running 1 test"),
                "{case}/{label}: subject starts"
            );
            assert!(
                String::from_utf8_lossy(&result.stderr).contains(marker),
                "{case}/{label}: counterfeit method actually executes"
            );
            assert!(
                !stdout.contains("test result:"),
                "{case}/{label}: zero completed-test evidence"
            );
        }
    }
    Ok(())
}

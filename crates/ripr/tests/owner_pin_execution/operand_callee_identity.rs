//! Independent runtime controls for the operand-callee public-API fixtures.

use super::{Scratch, compiles, run};
use std::path::Path;

const SHADOW: &str = include_str!("../fixtures/operand_callee_identity/let_shadow.rs");
const METHOD: &str = include_str!("../fixtures/operand_callee_identity/method_confusion.rs");
const PRIMITIVE: &str = include_str!("../fixtures/operand_callee_identity/primitive_control.rs");
const INITIALIZER: &str = "total_cents: subtotal + shipping";

#[test]
fn compiled_operator_removal_controls_match_observed_effects() -> Result<(), String> {
    let mut cases = vec![
        ("local shadow", SHADOW.to_string(), true),
        ("method confusion", METHOD.to_string(), true),
        ("primitive control", PRIMITIVE.to_string(), false),
    ];
    for binding in [
        "use crate::flat_rate as shipping_cents;",
        "use crate::local_rates::shipping_cents;",
        "use crate::local_rates::*;",
        "declare!(fn shipping_cents(_: u64) -> Shipping { Shipping });",
    ] {
        cases.push((
            binding,
            format!(
                "mod local_rates {{ pub use crate::flat_rate as shipping_cents; }}\n\
             macro_rules! declare {{ ($item:item) => {{ $item }}; }}\n{}",
                SHADOW.replace("let shipping_cents = flat_rate;", binding)
            ),
            true,
        ));
    }
    for replacement in [
        "let shipping = shipping_cents(subtotal);\n    let shipping_cents = 7u64;",
        "let shipping = shipping_cents(subtotal)   ;",
        "let shipping_cents = shipping_cents(subtotal);\n    let shipping = shipping_cents;",
    ] {
        cases.push((
            replacement,
            PRIMITIVE.replace("let shipping = shipping_cents(subtotal);", replacement),
            false,
        ));
    }
    for (name, original, observes_effect) in cases {
        assert_eq!(original.matches(INITIALIZER).count(), 1);
        for remove_operator in [false, true] {
            let source = if remove_operator {
                original.replacen(INITIALIZER, "total_cents: subtotal", 1)
            } else {
                original.to_string()
            };
            // These roots are never analyzed by the public-API cases, so
            // neither runtime variant can add facts to their RustIndex.
            let scratch = Scratch::create()?;
            let source_path = scratch.0.join("runtime.rs");
            std::fs::write(&source_path, source).map_err(|error| error.to_string())?;
            let executable = scratch
                .0
                .join(format!("fixture{}", std::env::consts::EXE_SUFFIX));
            compiles(
                run(
                    Path::new("rustc"),
                    &[
                        "--edition=2024".as_ref(),
                        "--crate-name=operand_fixture".as_ref(),
                        "--test".as_ref(),
                        source_path.as_os_str(),
                        "-o".as_ref(),
                        executable.as_os_str(),
                    ],
                )?,
                &format!("{name}, removal={remove_operator}"),
            )?;
            let result = run(&executable, &["--test-threads=1".as_ref()])?;
            let stdout = String::from_utf8_lossy(&result.stdout);
            let stderr = String::from_utf8_lossy(&result.stderr);
            assert!(
                stdout.contains("running 1 test") && stdout.contains("0 ignored"),
                "{name}: nonempty runtime subject: {stdout}; {stderr}"
            );
            let effect_must_fail = remove_operator && observes_effect;
            assert_eq!(
                result.status.success(),
                !effect_must_fail,
                "{name}, removal={remove_operator}: {stdout}; {stderr}"
            );
            if effect_must_fail {
                let transcript = format!("{stdout}\n{stderr}");
                assert!(
                    transcript.contains("tests::observes_the_operator")
                        && transcript.contains("left: 0")
                        && transcript.contains("right: 1"),
                    "{name}: removal must fail on the operator-effect count: {transcript}"
                );
            }
        }
    }
    Ok(())
}

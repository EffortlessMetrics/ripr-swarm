//! #6482: an `assert_eq!` in a test-local check helper the test calls on
//! an eager path is admitted as if it were the test's own; every other
//! helper shape stays refused.

use super::*;
use crate::analysis::facts::build_index;
use crate::analysis::syntax::parser_oracles_for_function;
use std::error::Error;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

const TIP_LIB: &str = "pub fn with_tip(bill: u64, tip: u64) -> u64 {\n    tip + bill\n}\n\n";

/// The #6482 shape. The helper's parameters share no token with the
/// changed `tip + bill`, so only the owner pin can confirm it.
const HELPER: &str = "    fn check_tip(b: u64, t: u64, want: u64) {\n        assert_eq!(with_tip(b, t), want);\n    }\n";

const CALLS: &str = "check_tip(40, 6, 46);\n        check_tip(10, 0, 10);";

fn module(helper: &str, body: &str) -> String {
    format!(
        "{TIP_LIB}#[cfg(test)]\nmod tests {{\n    use super::*;\n\n{helper}\n    #[test]\n    fn tip_is_added() {{\n        {body}\n    }}\n}}\n"
    )
}

/// What the owner-pin authority decides for the helper's assertion:
/// `(shared equality admission, owner-return pin)`.
fn helper_assertion_admitted(source: &str) -> Result<(bool, bool), String> {
    let index = index(&[(LIB, source)]);
    let test = index
        .tests()
        .iter()
        .find(|test| test.name == "tip_is_added")
        .ok_or("premise: `tip_is_added` is indexed")?;
    let helper = index
        .functions()
        .iter()
        .find(|function| function.name == "check_tip")
        .ok_or("premise: `check_tip` is indexed")?;
    let assertion = parser_oracles_for_function(&helper.body, helper.start_line)
        .unwrap_or_default()
        .into_iter()
        .find(|oracle| oracle.text.starts_with("assert_eq!(with_tip("))
        .ok_or("premise: the helper's assert_eq! parses")?;
    let owner = owner(&index, "with_tip");
    let probe = return_probe(owner, "tip + bill");
    let pin = OwnerReturnPin::establish(&probe, owner, &index)
        .ok_or("premise: the free owner establishes a pin")?;
    let syntax = OwnerPinSyntax::default();
    let shared = syntax.admits_equality_assertion(&probe, test, &assertion, &index);
    let pinned = pin.admits(test, &assertion, &index, &|_, _| false, &syntax);
    Ok((shared, pinned))
}

#[test]
fn an_eagerly_called_local_check_helper_lends_its_assertion() -> Result<(), String> {
    assert_eq!(
        helper_assertion_admitted(&module(HELPER, CALLS))?,
        (true, true)
    );
    // `#[track_caller]` changes only the reported panic location.
    let tracked = format!("    #[track_caller]\n{HELPER}");
    assert_eq!(
        helper_assertion_admitted(&module(&tracked, CALLS))?,
        (true, true)
    );
    // One eager call is enough; a further deferred call adds nothing.
    let mixed = format!("{CALLS}\n        if false {{ check_tip(1, 1, 3); }}");
    assert_eq!(
        helper_assertion_admitted(&module(HELPER, &mixed))?,
        (true, true)
    );
    // A directly invoked closure is an eager path, as for the test's own.
    assert_eq!(
        helper_assertion_admitted(&module(HELPER, "(|| check_tip(40, 6, 46))();"))?,
        (true, true)
    );
    Ok(())
}

#[test]
fn a_helper_call_off_the_eager_path_lends_nothing() -> Result<(), String> {
    assert_eq!(
        helper_assertion_admitted(&module(HELPER, CALLS))?,
        (true, true),
        "control"
    );
    for body in [
        "for (b, t, w) in [(40, 6, 46)] { check_tip(b, t, w); }",
        "if true { check_tip(40, 6, 46); }",
        "let _later = || check_tip(40, 6, 46);",
        "let _ = Some(check_tip(40, 6, 46));",
        "return; check_tip(40, 6, 46);",
        "#[cfg(any())] check_tip(40, 6, 46);",
        "let _later = async { check_tip(40, 6, 46); };",
        "self::check_tip(40, 6, 46);",
        "dbg!(check_tip(40, 6, 46));",
        // A macro-shaped helper (or any untrusted macro) may hide an exit.
        "check!(40, 6, 46); check_tip(40, 6, 46);",
    ] {
        assert_eq!(
            helper_assertion_admitted(&module(HELPER, body))?,
            (false, false),
            "{body}"
        );
    }
    Ok(())
}

#[test]
fn a_call_that_may_not_name_the_helper_lends_nothing() -> Result<(), String> {
    assert_eq!(
        helper_assertion_admitted(&module(HELPER, CALLS))?,
        (true, true),
        "control"
    );
    for body in [
        // A local binding or nested item shadows the module's helper.
        "let check_tip = |_: u64, _: u64, _: u64| {}; check_tip(40, 6, 46);",
        "fn check_tip(_: u64, _: u64, _: u64) {} check_tip(40, 6, 46);",
        // An import in the test body may name another function.
        "use other::check_tip; check_tip(40, 6, 46);",
    ] {
        assert_eq!(
            helper_assertion_admitted(&module(HELPER, body))?,
            (false, false),
            "{body}"
        );
    }
    // A second definition elsewhere in the file makes the name ambiguous.
    let duplicate = format!(
        "{}\nmod other {{\n    fn check_tip(_: u64, _: u64, _: u64) {{}}\n}}\n",
        module(HELPER, CALLS)
    );
    assert_eq!(helper_assertion_admitted(&duplicate)?, (false, false));
    // A helper outside the test's own module is reached through an import.
    let outside = format!(
        "{TIP_LIB}{}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn tip_is_added() {{\n        {CALLS}\n    }}\n}}\n",
        HELPER.replace("\n    ", "\n").trim_start()
    );
    assert_eq!(helper_assertion_admitted(&outside)?, (false, false));
    Ok(())
}

#[test]
fn only_a_plain_helper_that_runs_to_its_end_lends_its_assertion() -> Result<(), String> {
    assert_eq!(
        helper_assertion_admitted(&module(HELPER, CALLS))?,
        (true, true),
        "control"
    );
    let assertion = "assert_eq!(with_tip(b, t), want);";
    for helper in [
        // Generic.
        format!(
            "    fn check_tip<T: Into<u64>>(b: T, t: u64, want: u64) {{ let b = b.into(); {assertion} }}\n"
        ),
        // An early exit before the assertion.
        format!(
            "    fn check_tip(b: u64, t: u64, want: u64) {{ if b == 40 {{ return; }} {assertion} }}\n"
        ),
        // A return type, so a `?` or value may stand in for the assertion.
        format!(
            "    fn check_tip(b: u64, t: u64, want: u64) -> Option<()> {{ {assertion} Some(()) }}\n"
        ),
        // The assertion off the helper's own eager path.
        format!(
            "    fn check_tip(b: u64, t: u64, want: u64) {{ for _ in 0..b {{ {assertion} }} }}\n"
        ),
        // A macro other than the trusted standard ones.
        format!("    fn check_tip(b: u64, t: u64, want: u64) {{ log!(); {assertion} }}\n"),
        // A cfg attribute.
        format!(
            "    #[cfg(any())]\n    fn check_tip(b: u64, t: u64, want: u64) {{ {assertion} }}\n"
        ),
        // async: its body runs only when polled.
        format!("    async fn check_tip(b: u64, t: u64, want: u64) {{ {assertion} }}\n"),
    ] {
        assert_eq!(
            helper_assertion_admitted(&module(&helper, CALLS))?,
            (false, false),
            "{helper}"
        );
    }
    Ok(())
}

#[test]
fn a_helper_that_rebinds_or_feeds_back_the_owner_is_not_a_pin() -> Result<(), String> {
    assert_eq!(
        helper_assertion_admitted(&module(HELPER, CALLS))?,
        (true, true),
        "control"
    );
    // Execution is established, but the owner's name is the helper's
    // parameter, or a call-site argument computes the expected value with
    // the owner itself.
    let parameter = "    fn check_tip(with_tip: fn(u64, u64) -> u64, want: u64) {\n        assert_eq!(with_tip(1, 2), want);\n    }\n";
    let source = module(parameter, "check_tip(|a, b| a * b, 2);");
    assert_eq!(helper_assertion_admitted(&source)?, (true, false));
    let fed_back = "check_tip(40, 6, with_tip(40, 6));";
    assert_eq!(
        helper_assertion_admitted(&module(HELPER, fed_back))?,
        (true, false)
    );
    Ok(())
}

/// The retained production path: the index's own helper crediting puts the
/// helper's assertion on the test, and the same coordinate is admitted.
#[test]
fn the_indexed_helper_assertion_is_the_admitted_one() -> Result<(), Box<dyn Error>> {
    let root = std::env::temp_dir().join(format!(
        "ripr-owner-pin-helper-{}-{}",
        std::process::id(),
        NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(root.join("src"))?;
    fs::write(root.join(LIB), module(HELPER, CALLS))?;
    let built = build_index(&root, &[PathBuf::from(LIB)]);
    fs::remove_dir_all(&root)?;
    let index = built?;
    let test = index
        .tests()
        .iter()
        .find(|test| test.name == "tip_is_added")
        .ok_or("premise: `tip_is_added` is indexed")?;
    let assertion = test
        .assertions
        .iter()
        .find(|assertion| assertion.text == "assert_eq!(with_tip(b, t), want);")
        .ok_or("premise: the helper's assertion is credited to the test")?;
    assert!(!(test.start_line..=test.end_line).contains(&assertion.line));
    let owner = owner(&index, "with_tip");
    let probe = return_probe(owner, "tip + bill");
    let pin = OwnerReturnPin::establish(&probe, owner, &index)
        .ok_or("premise: the free owner establishes a pin")?;
    let syntax = OwnerPinSyntax::default();
    assert!(syntax.admits_equality_assertion(&probe, test, assertion, &index));
    assert!(pin.admits(test, assertion, &index, &|_, _| false, &syntax));
    Ok(())
}

/// `source` as `src/lib.rs`, indexed through the production path, which
/// credits a check helper's assertion to the tests that call it.
fn indexed(source: &str) -> Result<RustIndex, String> {
    let root = std::env::temp_dir().join(format!(
        "ripr-owner-pin-helper-{}-{}",
        std::process::id(),
        NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(root.join("src")).map_err(|err| err.to_string())?;
    fs::write(root.join(LIB), source).map_err(|err| err.to_string())?;
    let built = build_index(&root, &[PathBuf::from(LIB)]);
    fs::remove_dir_all(&root).map_err(|err| err.to_string())?;
    built.map_err(|err| err.to_string())
}

/// The loan's facts the boundary pairing reads (#6482): the helper's
/// parameters only when each is a plain name its body never rebinds, and
/// only the eager calls that are alone on their line.
#[test]
fn the_loan_maps_only_plain_parameters_and_lone_eager_calls() -> Result<(), String> {
    let loan = |source: &str| -> Result<Option<HelperLoan>, String> {
        let index = indexed(source)?;
        let test = index
            .tests()
            .iter()
            .find(|test| test.name == "tip_is_added")
            .ok_or("premise: `tip_is_added` is indexed")?;
        let assertion = test
            .assertions
            .iter()
            .find(|assertion| assertion.text.starts_with("assert_eq!(with_tip("))
            .ok_or("premise: the helper's assertion is credited to the test")?;
        Ok(OwnerPinSyntax::default().helper_loan(test, assertion, &index))
    };
    let plain = loan(&module(HELPER, CALLS))?.ok_or("control: the loan exists")?;
    assert_eq!(plain.name, "check_tip");
    assert_eq!(plain.parameters, ["b", "t", "want"]);
    assert_eq!(plain.call_lines.len(), 2);
    // A second call on the same line: no single call owns that line.
    let shared = loan(&module(
        HELPER,
        "check_tip(40, 6, 46); let _ = with_tip(1, 2);\n        check_tip(10, 0, 10);",
    ))?
    .ok_or("the loan still exists")?;
    assert_eq!(shared.call_lines.len(), 1);
    for helper in [
        // A rebound parameter no longer holds the call's argument.
        "    fn check_tip(b: u64, t: u64, want: u64) {\n        let b = b + 1;\n        assert_eq!(with_tip(b, t), want);\n    }\n",
        // A `mut` parameter may be reassigned.
        "    fn check_tip(mut b: u64, t: u64, want: u64) {\n        b += 1;\n        assert_eq!(with_tip(b, t), want);\n    }\n",
    ] {
        let rebound = loan(&module(helper, CALLS))?.ok_or("the loan exists")?;
        assert!(rebound.parameters.is_empty(), "{helper}");
    }
    // The test's own assertion borrows nothing.
    let index = indexed(&module(HELPER, "assert_eq!(with_tip(1, 2), 3);"))?;
    let test = index
        .tests()
        .iter()
        .find(|test| test.name == "tip_is_added")
        .ok_or("premise: `tip_is_added` is indexed")?;
    let own = test
        .assertions
        .iter()
        .find(|assertion| assertion.text == "assert_eq!(with_tip(1, 2), 3);")
        .ok_or("premise: own assertion")?;
    assert!(
        OwnerPinSyntax::default()
            .helper_loan(test, own, &index)
            .is_none()
    );
    Ok(())
}

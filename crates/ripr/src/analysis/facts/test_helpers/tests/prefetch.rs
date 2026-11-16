//! #12947: the helper-credit prefetch warms the run memo on the rayon pool
//! and leaves every credit decision to the serial fold.

use super::*;

/// One file with every fold path: a plain `assert_eq!` helper, a helper that
/// invokes its closure parameter, a plain helper handed a call-making closure
/// (#9997), a forwarded closure to a same-file invoker and to an unresolved
/// callee (#9981), and a same-file assertion macro (#9122).
fn every_fold_path() -> String {
    format!(
        "{GATE}#[cfg(test)]\nmod tests {{\n    use super::*;\n\n\
    fn check(x: u32, want: bool) {{\n        assert_eq!(gate(x), want);\n    }}\n\n\
    fn observe(f: impl FnOnce() -> bool, want: bool) {{\n        let got = f();\n        assert_eq!(got, want);\n    }}\n\n\
    fn ignore(want: bool, f: impl FnOnce() -> bool) {{\n        assert_eq!(want, true);\n    }}\n\n\
    fn invoke(f: impl FnOnce() -> bool, want: bool) {{\n        let got = f();\n        assert_eq!(got, want);\n    }}\n\n\
    fn forward(f: impl FnOnce() -> bool, want: bool) {{\n        invoke(f, want);\n        assert!(want);\n    }}\n\n\
    fn escape(f: impl FnOnce() -> bool, want: bool) {{\n        elsewhere::run(f);\n        assert!(want);\n    }}\n\n\
    macro_rules! assert_gate {{\n        ($x:expr) => {{ assert!(gate($x)); }};\n    }}\n\n\
    #[test]\n    fn plain() {{\n        check(10, false);\n        check(11, true);\n    }}\n\n\
    #[test]\n    fn callback() {{\n        observe(|| gate(11), true);\n    }}\n\n\
    #[test]\n    fn withheld() {{\n        ignore(true, || gate(11));\n    }}\n\n\
    #[test]\n    fn forwarded() {{\n        forward(|| gate(11), true);\n    }}\n\n\
    #[test]\n    fn unresolved() {{\n        escape(|| gate(11), true);\n    }}\n\n\
    #[test]\n    fn via_macro() {{\n        assert_gate!(11);\n    }}\n}}\n"
    )
}

fn all_tests(index: &RustIndex) -> Vec<TestFact> {
    index.tests().iter().cloned().collect()
}

#[test]
fn prefetched_credit_equals_the_fold_alone() -> Result<(), Box<dyn Error>> {
    let source = every_fold_path();
    let _ = take_prefetch_runs();
    let alone = with_fold_alone(|| index_for(&source))?;
    assert_eq!(take_prefetch_runs(), 0, "the fold alone");
    let prefetched = index_for(&source)?;
    assert_eq!(take_prefetch_runs(), 1, "the build prefetched");

    // Premise: the fixture reaches the credited, withheld and limit paths.
    let plain = test_named(&alone, "plain")?;
    assert_eq!(assertion_texts(plain), vec!["assert_eq!(gate(x), want);"]);
    let callback = test_named(&alone, "callback")?;
    assert_eq!(assertion_texts(callback), vec!["assert_eq!(got, want);"]);
    let withheld = test_named(&alone, "withheld")?;
    assert!(
        withheld.assertions.is_empty(),
        "{:?}",
        assertion_texts(withheld)
    );
    let forwarded = test_named(&alone, "forwarded")?;
    assert!(
        assertion_texts(forwarded).contains(&"assert_eq!(got, want);"),
        "{:?}",
        assertion_texts(forwarded)
    );
    assert!(test_named(&alone, "unresolved")?.assertions.is_empty());
    assert!(
        !alone.forwarded_callback_limits.is_empty(),
        "premise: the unresolved forward is a named limit"
    );
    assert!(!test_named(&alone, "via_macro")?.assertions.is_empty());

    assert_eq!(all_tests(&alone), all_tests(&prefetched));
    assert_eq!(
        alone.forwarded_callback_limits,
        prefetched.forwarded_callback_limits
    );
    Ok(())
}

#[test]
fn prefetch_leaves_the_fold_no_parse_of_its_own() -> Result<(), Box<dyn Error>> {
    // Only the prefetched paths: plain and invoked-callback helpers.
    let source = format!(
        "{GATE}#[cfg(test)]\nmod tests {{\n    use super::*;\n\n\
    fn check(x: u32, want: bool) {{\n        assert_eq!(gate(x), want);\n    }}\n\n\
    fn observe(f: impl FnOnce() -> bool, want: bool) {{\n        let got = f();\n        assert_eq!(got, want);\n    }}\n\n\
    #[test]\n    fn plain() {{\n        check(10, false);\n        check(11, true);\n    }}\n\n\
    #[test]\n    fn callback() {{\n        observe(|| gate(11), true);\n    }}\n}}\n"
    );
    let mut index = index_for(&source)?;
    let credited = all_tests(&index);
    assert_eq!(
        assertion_texts(test_named(&index, "callback")?),
        vec!["assert_eq!(got, want);"]
    );

    // Alone, the fold parses each test and helper body itself: the plain
    // call's closure check and oracles, and the callback helper's invoked
    // parameters, the test's closure positions and its oracles.
    with_fold_alone(|| credit_same_file_assertion_helpers(&mut index));
    assert_eq!(last_fold_parses(), 5);

    // Prefetched, every one of those bodies was parsed before the fold
    // (on the rayon pool when the batch splits), and the fold only reads the
    // memo. The credit is unchanged (idempotent, #8547).
    credit_same_file_assertion_helpers(&mut index);
    assert_eq!(last_fold_parses(), 0);
    assert_eq!(all_tests(&index), credited);
    Ok(())
}

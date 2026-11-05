use super::*;

#[test]
fn cloned_call_making_callbacks_do_not_lend_plain_helper_assertions() -> Result<(), Box<dyn Error>>
{
    for body in [
        "        let c = || gate(11);\n        run_test(true, c.clone());\n",
        "        let c = || gate(11);\n        let d = c.clone();\n        run_test(true, d);\n",
        "        let c = || gate(11);\n        let d = &c;\n        run_test(true, (&d).clone());\n",
        "        let c = std::sync::Arc::new(|| gate(11));\n        run_test(true, c.clone());\n",
        "        let c = || gate(11);\n        let c = true;\n        run_test(true, c.clone());\n",
    ] {
        let index = index_for(&callback_source(UNUSED_CLOSURE_HELPER, body))?;
        let test = test_named(&index, "boundary")?;
        assert!(calls(test).contains(&"run_test"), "{body}: {:?}", calls(test));
        assert!(
            test.assertions.is_empty(),
            "the helper never invokes the callback: {body}: {:?}",
            assertion_texts(test)
        );
        let file_test = index
            .files()
            .get(Path::new("src/lib.rs"))
            .and_then(|facts| facts.tests.iter().find(|test| test.name == "boundary"))
            .ok_or("premise: per-file test fact exists")?;
        assert_eq!(file_test, test, "per-file and flat facts agree: {body}");
    }
    Ok(())
}

#[test]
fn ordinary_value_clones_keep_plain_helper_assertions() -> Result<(), Box<dyn Error>> {
    let helper = "    fn run_test(want: bool, got: bool) {\n        assert_eq!(got, want);\n    }\n";
    for body in [
        "        run_test(true, gate(11).clone());\n",
        "        let c = gate(11);\n        run_test(true, c.clone());\n",
        "        let c = gate(11);\n        let d = c.clone();\n        run_test(true, d);\n",
        "        { let c = || gate(11); }\n        let c = gate(11);\n        run_test(true, c.clone());\n",
        "        let c = gate(11);\n        run_test(true, c.clone());\n        let c = || gate(11);\n",
        "        let c = gate(11);\n        let d = c.clone();\n        let c = || gate(11);\n        run_test(true, d);\n",
    ] {
        let index = index_for(&callback_source(helper, body))?;
        let test = test_named(&index, "boundary")?;
        assert_eq!(assertion_texts(test), vec!["assert_eq!(got, want);"], "{body}");
    }
    Ok(())
}

#[test]
fn cloned_call_less_closure_keeps_plain_helper_assertions() -> Result<(), Box<dyn Error>> {
    let index = index_for(&callback_source(
        UNUSED_CLOSURE_HELPER,
        "        let c = || true;\n        let d = c.clone();\n        run_test(true, d);\n",
    ))?;
    let test = test_named(&index, "boundary")?;
    assert_eq!(assertion_texts(test), vec!["assert_eq!(want, true);"]);
    Ok(())
}

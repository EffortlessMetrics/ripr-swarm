//! Generated execution-context sweep for effect-family assertion admission (#8927).
//! Dead contexts must not credit their assertion. Direct and mixed live rows
//! retain credit; other live forms document the current conservative refusals.

use super::*;

#[test]
fn effect_assertion_dead_context_sweep_refuses_every_never_run_assertion() -> Result<(), String> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/observation_verified_semicolon_effect");
    let eq = "*notifier.sent.borrow(), vec![\"order-42\".to_string()]";
    let forms: Vec<(String, String)> = vec![
        ("assert_eq".into(), format!("assert_eq!({eq});")),
        ("std::assert_eq".into(), format!("std::assert_eq!({eq});")),
        ("assert_matches".into(), "assert!(matches!(notifier.sent.borrow().first().map(String::as_str), Some(\"order-42\")));".into()),
        ("assert_eqexpr".into(), "assert!(*notifier.sent.borrow() == vec![\"order-42\".to_string()]);".into()),
        ("std::assert".into(), "std::assert!(*notifier.sent.borrow() == vec![\"order-42\".to_string()]);".into()),
    ];
    let dead = |a: &str| -> Vec<(&'static str, String)> {
        vec![
            ("after_return", format!("return;\n{a}")),
            ("after_panic", format!("panic!(\"stop\");\n{a}")),
            ("after_block_return", format!("{{ return; }}\n{a}")),
            ("after_loop_return", format!("loop {{ return; }}\n{a}")),
            ("if_false", format!("if false {{\n{a}\n}}")),
            ("while_false", format!("while false {{\n{a}\n}}")),
            ("if_not_true", format!("if !true {{\n{a}\n}}")),
            ("else_of_true", format!("if true {{ }} else {{\n{a}\n}}")),
            ("closure_uncalled", format!("let _c = || {{\n{a}\n}};")),
            (
                "fn_uncalled",
                format!("fn never_called(notifier: &Notifier) {{\n{a}\n}}"),
            ),
            ("after_unreachable", format!("unreachable!();\n{a}")),
            ("after_todo", format!("todo!();\n{a}")),
            (
                "match_dead_arm",
                format!("match 1 {{ 2 => {{\n{a}\n}} _ => {{}} }}"),
            ),
            ("const_false_cfg", format!("if cfg!(any()) {{\n{a}\n}}")),
            (
                "for_after_break",
                format!("for _ in [1, 2] {{ break; {a} }}"),
            ),
            (
                "for_after_continue",
                format!("for _ in [1, 2] {{ continue; {a} }}"),
            ),
            ("for_empty_range", format!("for _ in 0..0 {{\n{a}\n}}")),
            (
                "labeled_block_after_break",
                format!("'a: {{ break 'a; {a} }}"),
            ),
            (
                "outer_loop_after_inner_break",
                format!("'o: loop {{ loop {{ break 'o; }} {a} }}"),
            ),
            ("loop_after_break", format!("loop {{ break; {a} }}")),
            (
                "uninvoked_macro_rules",
                format!("macro_rules! never {{ () => {{ {a} }}; }}"),
            ),
            (
                "cfg_dropped_let",
                format!("#[cfg(any())]\nlet _x = {{ {a} 1 }};"),
            ),
            (
                "after_process_exit",
                format!("::std::process::exit(1);\n{a}"),
            ),
            ("after_empty_loop", format!("loop {{}}\n{a}")),
        ]
    };
    let live = |a: &str| -> Vec<(&'static str, String)> {
        vec![
            ("direct", a.to_string()),
            // The label does not skip its body; refusal here is a conservative
            // miss, not evidence that the assertion never runs.
            ("labeled_block", format!("'a: {{\n{a}\n}}")),
            ("if_true", format!("if true {{\n{a}\n}}")),
            ("closure_called", format!("let c = || {{\n{a}\n}};\nc();")),
            (
                "cond_return_before",
                format!("if notifier.sent.borrow().is_empty() {{ return; }}\n{a}"),
            ),
            (
                "fn_called",
                format!("fn check(notifier: &Notifier) {{\n{a}\n}}\ncheck(&notifier);"),
            ),
        ]
    };
    let mut bad = Vec::new();
    let mut n = 0;
    for (name, a) in &forms {
        // (context, oracle, runs): `runs` holds for a live assertion.
        let mut cases: Vec<(String, String, bool)> = Vec::new();
        for (c, o) in dead(a) {
            cases.push((c.to_string(), o, false));
        }
        for (c, o) in live(a) {
            cases.push((c.to_string(), o, true));
        }
        // Mixed: a dead assertion beside a live one keeps the live credit.
        for (c, o) in [
            ("mixed_if_false", format!("if false {{\n{a}\n}}\n{a}")),
            (
                "mixed_uncalled_closure",
                format!("let _c = || {{\n{a}\n}};\n{a}"),
            ),
        ] {
            cases.push((c.to_string(), o, true));
        }
        for (ctx, oracle, is_live) in cases {
            let ctx = ctx.as_str();
            n += 1;
            let src = format!(
                "{}\n#[cfg(test)]\nmod tests {{\nuse super::*;\n#[test]\nfn process_order_sends_exact_order_id() {{\nlet notifier = Notifier::new();\nprocess_order(&notifier, \"order-42\");\n{oracle}\n}}\n}}\n",
                std::fs::read_to_string(fixture.join("input/src/lib.rs"))
                    .map_err(|e| e.to_string())?
            );
            let scratch = Scratch::create()?;
            std::fs::create_dir(scratch.0.join("src")).map_err(|e| e.to_string())?;
            std::fs::copy(
                fixture.join("input/Cargo.toml"),
                scratch.0.join("Cargo.toml"),
            )
            .map_err(|e| e.to_string())?;
            std::fs::write(scratch.0.join("src/lib.rs"), src).map_err(|e| e.to_string())?;
            let report = check_workspace(CheckInput {
                root: scratch.0.clone(),
                diff_file: Some(fixture.join("diff.patch")),
                mode: Mode::Fast,
                format: OutputFormat::Json,
                include_unchanged_tests: true,
                ..CheckInput::default()
            })?;
            let json: serde_json::Value =
                serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
                    .map_err(|e| e.to_string())?;
            assert_eq!(
                json["analysis_outcome"]["analysis_complete"], true,
                "{name}/{ctx}"
            );
            let findings = json["findings"].as_array().ok_or("missing findings")?;
            assert_eq!(findings.len(), 2, "{name}/{ctx}: exact subjects");
            let families = findings
                .iter()
                .map(|f| f["probe"]["family"].as_str().unwrap_or(""))
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(
                families,
                std::collections::BTreeSet::from(["call_deletion", "side_effect"]),
                "{name}/{ctx}"
            );
            let mut effect_findings = 0;
            for f in findings {
                assert_eq!(f["probe"]["line"], 18, "{name}/{ctx}");
                assert_eq!(
                    f["probe"]["expression"], "notifier.send(order_id)",
                    "{name}/{ctx}"
                );
                assert!(
                    f["related_tests"]
                        .as_array()
                        .is_some_and(|tests| !tests.is_empty()),
                    "{name}/{ctx}"
                );
                let fam = f["probe"]["family"].as_str().unwrap_or("");
                if fam != "side_effect" && fam != "call_deletion" {
                    continue;
                }
                effect_findings += 1;
                let cls = f["classification"].as_str().unwrap_or("").to_string();
                let credited = cls == "exposed" || cls == "weakly_exposed";
                if !is_live && credited {
                    bad.push(format!("DEAD-CREDITED {name}/{ctx}/{fam} {cls}"));
                }
                // Direct and mixed live assertions of the unqualified forms are
                // the discriminating controls. Other live contexts (`if true`,
                // a called closure or fn, a conditional return before) are
                // refused conservatively, and a qualified `std::` form with
                // `vec!` operands reads `static_unknown` even in direct
                // position (fail-closed, as for the return family).
                let must_credit = ctx == "direct" || ctx.starts_with("mixed");
                if is_live && must_credit && !credited && !name.starts_with("std::") {
                    bad.push(format!(
                        "direct control uncredited {name}/{ctx}/{fam} {cls}"
                    ));
                }
            }
            if effect_findings != 2 {
                bad.push(format!(
                    "vacuous case {name}/{ctx}: {effect_findings} effect findings"
                ));
            }
        }
    }
    assert!(n >= 150, "sweep must cover generated cases, ran {n}");
    assert!(bad.is_empty(), "{bad:#?}");
    Ok(())
}

//! Generated dead-context sweep for effect-family assertion admission (#8927).
//! Every context below provably never runs the assertion, so no effect finding
//! may be credited from it; the `direct` rows are the discriminating controls.

use super::*;

#[test]
fn effect_assertion_dead_context_sweep_refuses_every_never_run_assertion() -> Result<(), String> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/observation_verified_side_effect");
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
        ]
    };
    let live = |a: &str| -> Vec<(&'static str, String)> {
        vec![
            ("direct", a.to_string()),
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
        let mut cases: Vec<(&str, String, bool)> = Vec::new();
        for (c, o) in dead(a) {
            cases.push((c, o, false));
        }
        if let Some((c, o)) = live(a).into_iter().next() {
            cases.push((c, o, true));
        }
        for (ctx, oracle, is_live) in cases {
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
            for f in json["findings"].as_array().ok_or("f")? {
                let fam = f["probe"]["family"].as_str().unwrap_or("");
                if fam != "side_effect" && fam != "call_deletion" {
                    continue;
                }
                let cls = f["classification"].as_str().unwrap_or("").to_string();
                let credited = cls == "exposed" || cls == "weakly_exposed";
                if !is_live && credited {
                    bad.push(format!("DEAD-CREDITED {name}/{ctx}/{fam} {cls}"));
                }
                if is_live
                    && !credited
                    && matches!(
                        name.as_str(),
                        "assert_eq" | "assert_matches" | "assert_eqexpr"
                    )
                {
                    bad.push(format!(
                        "direct control uncredited {name}/{ctx}/{fam} {cls}"
                    ));
                }
            }
        }
    }
    assert!(n >= 60, "sweep must cover generated cases, ran {n}");
    assert!(bad.is_empty(), "{bad:#?}");
    Ok(())
}

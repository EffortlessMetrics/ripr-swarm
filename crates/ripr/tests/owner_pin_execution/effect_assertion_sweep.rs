//! Generated dead-context sweep for effect-family assertion admission (#8927).
//! Every context below provably never runs the assertion, so no effect finding
//! may be credited from it; the `direct` rows are the discriminating controls.

use super::*;

/// Aligned standalone stimulus: the changed statement is the semicolon call at
/// line 18 (`@@ -16,5 +16,5 @@`), followed by a trailing `true` at line 19. It
/// yields exactly a `call_deletion` and a `side_effect` finding at line 18, so
/// no family is fabricated by a shifted hunk.
pub(super) const ALIGNED_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\nindex 0000000..1111111 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -16,5 +16,5 @@ impl Notifier {\n \n pub fn process_order(notifier: &Notifier, order_id: &str) -> bool {\n-    notifier.send(\"static_msg\");\n+    notifier.send(order_id);\n     true\n }\n";
pub(super) const ALIGNED_LIB: &str = "use std::cell::RefCell;\n\npub struct Notifier {\n    pub sent: RefCell<Vec<String>>,\n}\n\nimpl Notifier {\n    pub fn new() -> Self {\n        Notifier { sent: RefCell::new(Vec::new()) }\n    }\n    pub fn send(&self, payload: &str) -> bool {\n        self.sent.borrow_mut().push(payload.to_string());\n        true\n    }\n}\n\npub fn process_order(notifier: &Notifier, order_id: &str) -> bool {\n    notifier.send(order_id);\n    true\n}\n";

#[test]
fn effect_assertion_dead_context_sweep_refuses_every_never_run_assertion() -> Result<(), String> {
    let eq = "*notifier.sent.borrow(), vec![\"order-42\".to_string()]";
    let forms: Vec<(String, String)> = vec![
        ("assert_eq".into(), format!("assert_eq!({eq});")),
        ("std::assert_eq".into(), format!("std::assert_eq!({eq});")),
        ("assert_matches".into(), "assert!(matches!(notifier.sent.borrow().first().map(String::as_str), Some(\"order-42\")));".into()),
        ("assert_eqexpr".into(), "assert!(*notifier.sent.borrow() == vec![\"order-42\".to_string()]);".into()),
        ("std::assert".into(), "std::assert!(*notifier.sent.borrow() == vec![\"order-42\".to_string()]);".into()),
        // No nested operand macro: the standard-rooted boolean forms a live
        // test really runs (#8927 review).
        ("std::assert_vecfrom".into(), "std::assert!(*notifier.sent.borrow() == Vec::from([\"order-42\".to_string()]));".into()),
        ("core::assert_vecfrom".into(), "::core::assert!(*notifier.sent.borrow() == Vec::from([\"order-42\".to_string()]));".into()),
        ("std::assert_eq_novec".into(), "std::assert_eq!(notifier.sent.borrow().first().map(String::as_str), Some(\"order-42\"));".into()),
    ];
    let dead = |a: &str| -> Vec<(&'static str, String)> {
        vec![
            ("after_return", format!("return;\n{a}")),
            // Identical spellings on one line, none of which can run.
            ("compact_twins_after_return", format!("return; {a} {a}")),
            (
                "compact_twins_if_false",
                format!("if false {{ {a} }} if false {{ {a} }}"),
            ),
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
            ("labeled_block", format!("'a: {{\n{a}\n}}")),
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
            // One-line `if <macro-cond> { A }`: the oracle text starts with the
            // prefix token, so it must take the context gate, not read as a
            // plain `ifmatches!`/`if!`/`ifcfg!` macro path.
            ("oneline_if_cfg_any", format!("if cfg!(any()) {{ {a} }}")),
            (
                "oneline_if_not_cfg_test",
                format!("if !cfg!(test) {{ {a} }}"),
            ),
            (
                "oneline_if_matches_false",
                format!("if matches!(1, 2) {{ {a} }}"),
            ),
            (
                "oneline_if_vec_empty",
                format!("if vec![1].is_empty() {{ {a} }}"),
            ),
            (
                "oneline_if_core_matches",
                format!("if ::core::matches!(1, 2) {{ {a} }}"),
            ),
            (
                "oneline_if_core_cfg",
                format!("if ::core::cfg!(any()) {{ {a} }}"),
            ),
            (
                "oneline_if_std_cfg",
                format!("if ::std::cfg!(any()) {{ {a} }}"),
            ),
            (
                "oneline_while_core_matches",
                format!("while ::core::matches!(1, 2) {{ {a} }}"),
            ),
            (
                "oneline_match_core_line",
                format!("match ::core::line!() {{ 0 => {a} _ => {{}} }}"),
            ),
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
            // Compact: the first assertion runs before the `return`.
            ("mixed_compact_return", format!("{a} return; {a}")),
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
                ALIGNED_LIB
            );
            let scratch = Scratch::create()?;
            std::fs::create_dir(scratch.0.join("src")).map_err(|e| e.to_string())?;
            std::fs::write(
                scratch.0.join("Cargo.toml"),
                std::fs::read_to_string(
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../../fixtures/observation_verified_side_effect/input/Cargo.toml"),
                )
                .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            std::fs::write(scratch.0.join("src/lib.rs"), src).map_err(|e| e.to_string())?;
            let diff_path = scratch.0.join("diff.patch");
            std::fs::write(&diff_path, ALIGNED_DIFF).map_err(|e| e.to_string())?;
            let report = check_workspace(CheckInput {
                root: scratch.0.clone(),
                diff_file: Some(diff_path),
                mode: Mode::Fast,
                format: OutputFormat::Json,
                include_unchanged_tests: true,
                ..CheckInput::default()
            })?;
            let json: serde_json::Value =
                serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
                    .map_err(|e| e.to_string())?;
            let mut effect_findings = 0;
            let mut seen: Vec<String> = Vec::new();
            for f in json["findings"].as_array().ok_or("f")? {
                let fam = f["probe"]["family"].as_str().unwrap_or("");
                if fam != "side_effect" && fam != "call_deletion" {
                    continue;
                }
                effect_findings += 1;
                if f["probe"]["line"] != 18 {
                    bad.push(format!("wrong subject line {name}/{ctx}/{fam}"));
                }
                seen.push(fam.to_string());
                let cls = f["classification"].as_str().unwrap_or("").to_string();
                let credited = cls == "exposed" || cls == "weakly_exposed";
                if !is_live && credited {
                    bad.push(format!("DEAD-CREDITED {name}/{ctx}/{fam} {cls}"));
                }
                // Direct and mixed live assertions are the discriminating
                // controls, including the standard-rooted forms without a
                // nested operand macro. Other live contexts (`if true`, a
                // called closure or fn, a conditional return before) are
                // refused conservatively, and a qualified form whose operands
                // hold a nested `vec!` reads `static_unknown` (fail-closed).
                let must_credit = ctx == "direct" || ctx.starts_with("mixed");
                let nested_macro = matches!(name.as_str(), "std::assert_eq" | "std::assert");
                if is_live && must_credit && !credited && !nested_macro {
                    bad.push(format!(
                        "direct control uncredited {name}/{ctx}/{fam} {cls}"
                    ));
                }
            }
            seen.sort();
            if seen != ["call_deletion", "side_effect"] {
                bad.push(format!("wrong effect inventory {name}/{ctx}: {seen:?}"));
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

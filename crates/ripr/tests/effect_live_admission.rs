//! Reachable effect observers survive the bounded execution gate (#8927).
//! Subjects use an aligned, semicolon-terminated changed call, never fallback.
use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Subject(PathBuf);
impl Drop for Subject {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn analyze(
    oracle: &str,
    attributes: &str,
    bindings: &str,
    dependencies: &str,
) -> Result<serde_json::Value, String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let subject = Subject(std::env::temp_dir().join(format!(
        "ripr-effect-live-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    std::fs::create_dir_all(subject.0.join("src")).map_err(|e| e.to_string())?;
    std::fs::write(subject.0.join("Cargo.toml"), format!("[package]\nname = \"effect_live\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[dependencies]\n{dependencies}")).map_err(|e| e.to_string())?;
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/observation_verified_side_effect/input/src/lib.rs");
    let production = std::fs::read_to_string(fixture).map_err(|e| e.to_string())?;
    assert_eq!(
        production.matches("    notifier.send(order_id)\n").count(),
        1
    );
    let production = production.replace(
        "    notifier.send(order_id)\n",
        "    notifier.send(order_id);\n    true\n",
    );
    assert_eq!(
        production.lines().nth(17),
        Some("    notifier.send(order_id);")
    );
    let source = format!(
        "{production}\n#[cfg(test)]\nmod tests {{\n{bindings}\nuse super::*;\n#[test]\n{attributes}\nfn observes_payload() {{\nlet notifier = Notifier::new();\nprocess_order(&notifier, \"order-42\");\n{oracle}\n}}\n}}\n"
    );
    std::fs::write(subject.0.join("src/lib.rs"), source).map_err(|e| e.to_string())?;
    std::fs::write(subject.0.join("diff.patch"), "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -16,5 +16,5 @@\n \n pub fn process_order(notifier: &Notifier, order_id: &str) -> bool {\n-    notifier.send(\"static_msg\");\n+    notifier.send(order_id);\n     true\n }\n").map_err(|e| e.to_string())?;
    let report = check_workspace(CheckInput {
        root: subject.0.clone(),
        diff_file: Some(subject.0.join("diff.patch")),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    serde_json::from_str(&render_check(&report, &OutputFormat::Json)?).map_err(|e| e.to_string())
}

#[test]
fn live_effect_observers_keep_credit_without_borrowing_dead_siblings() -> Result<(), String> {
    let forms = [
        (
            "eq",
            "assert_eq!(*notifier.sent.borrow(), vec![\"order-42\".to_string()]);",
            "exposed",
        ),
        (
            "matches",
            "assert!(matches!(notifier.sent.borrow().first().map(String::as_str), Some(\"order-42\")));",
            "exposed",
        ),
        (
            "std",
            "std::assert!(*notifier.sent.borrow() == Vec::from([\"order-42\".to_string()]));",
            "weakly_exposed",
        ),
        (
            "core",
            "core::assert!(*notifier.sent.borrow() == Vec::from([\"order-42\".to_string()]));",
            "weakly_exposed",
        ),
        (
            "absolute_std",
            "::std::assert!(*notifier.sent.borrow() == Vec::from([\"order-42\".to_string()]));",
            "weakly_exposed",
        ),
        (
            "spaced_std",
            "std :: assert!(*notifier.sent.borrow() == Vec::from([\"order-42\".to_string()]));",
            "weakly_exposed",
        ),
        (
            "absolute_core",
            "::core::assert!(*notifier.sent.borrow() == Vec::from([\"order-42\".to_string()]));",
            "weakly_exposed",
        ),
    ];
    let mut errors = Vec::new();
    for (form, assertion, grade) in forms {
        for (context, oracle, attributes, bindings, expected) in [
            ("direct", assertion.to_string(), "", "", Some(grade)),
            (
                "expanded_mixed",
                format!("{assertion}\nreturn;\n{assertion}"),
                "",
                "",
                Some(grade),
            ),
            (
                "compact_mixed",
                format!("{assertion} return; {assertion}"),
                "",
                "",
                Some(grade),
            ),
            (
                "after_return",
                format!("return; {assertion} {assertion}"),
                "",
                "",
                None,
            ),
            (
                "false_first",
                format!("if false {{ {assertion} }} return; {assertion}"),
                "",
                "",
                None,
            ),
            (
                "closure_first",
                format!("let _check = || {{ {assertion} }}; return; {assertion}"),
                "",
                "",
                None,
            ),
            (
                "rebound_between",
                format!("{assertion} let notifier = Notifier::new(); return; {assertion}"),
                "",
                "",
                None,
            ),
            (
                "conditional_between",
                format!(
                    "{assertion} if notifier.sent.borrow().is_empty() {{ return; }} {assertion}"
                ),
                "",
                "",
                None,
            ),
            (
                "shadowed_bare_macro",
                format!("{assertion} return; {assertion}"),
                "",
                "macro_rules! assert_eq { ($($t:tt)*) => {} }\nmacro_rules! assert { ($($t:tt)*) => {} }",
                if matches!(form, "eq" | "matches") {
                    None
                } else {
                    Some(grade)
                },
            ),
            (
                "should_panic",
                format!("{assertion} return; {assertion}"),
                "#[should_panic]",
                "",
                None,
            ),
            (
                "shadowed_root",
                format!("{assertion} return; {assertion}"),
                "",
                "mod std { pub use core::debug_assert as assert; }\nmod core { pub use ::core::debug_assert as assert; }",
                None,
            ),
        ] {
            // Rebinding std/core cannot invalidate an unrelated bare assertion.
            if context == "shadowed_root" && !matches!(form, "std" | "core") {
                continue;
            }
            let json = analyze(&oracle, attributes, bindings, "")?;
            assert_eq!(
                json["analysis_outcome"]["analysis_complete"], true,
                "{form}/{context}"
            );
            let findings = json["findings"].as_array().ok_or("missing findings")?;
            assert_eq!(findings.len(), 2, "{form}/{context}: exact subjects");
            for family in ["side_effect", "call_deletion"] {
                let selected: Vec<_> = findings
                    .iter()
                    .filter(|f| f["probe"]["family"] == family)
                    .collect();
                assert_eq!(selected.len(), 1, "{form}/{context}/{family}");
                let finding = selected[0];
                assert_eq!(finding["probe"]["line"], 18, "{form}/{context}/{family}");
                assert_eq!(
                    finding["probe"]["expression"], "notifier.send(order_id)",
                    "{form}/{context}/{family}"
                );
                assert!(
                    !finding["related_tests"]
                        .as_array()
                        .ok_or("missing related tests")?
                        .is_empty()
                );
                let actual = finding["classification"]
                    .as_str()
                    .ok_or("missing classification")?;
                if let Some(expected) = expected {
                    if actual != expected {
                        errors.push(format!(
                            "{form}/{context}/{family}: {actual}, expected {expected}"
                        ));
                    }
                } else if matches!(actual, "exposed" | "weakly_exposed") {
                    errors.push(format!("{form}/{context}/{family}: unsafe credit {actual}"));
                }
            }
        }
    }
    assert!(errors.is_empty(), "{}", errors.join("\n"));
    Ok(())
}

#[test]
fn qualified_assert_root_proof_also_governs_later_bare_observers() -> Result<(), String> {
    for root in ["std", "core", "::std", "::core"] {
        let oracle = format!(
            "{root}::assert!(true);\nassert_eq!(*notifier.sent.borrow(), vec![\"order-42\".to_string()]);"
        );
        for (case, bindings, dependencies, credit) in [
            ("clean", "", "", true),
            (
                "cargo_alias",
                "",
                "std = { package = \"fake-std\", version = \"1\" }\ncore = { package = \"fake-core\", version = \"1\" }\n",
                false,
            ),
            ("no_prelude", "#![no_implicit_prelude]", "", false),
        ] {
            let json = analyze(&oracle, "", bindings, dependencies)?;
            assert_eq!(
                json["analysis_outcome"]["analysis_complete"], true,
                "{root}/{case}"
            );
            let findings = json["findings"].as_array().ok_or("missing findings")?;
            assert_eq!(findings.len(), 2, "{root}/{case}");
            for family in ["call_deletion", "side_effect"] {
                let selected: Vec<_> = findings
                    .iter()
                    .filter(|f| f["probe"]["family"] == family)
                    .collect();
                assert_eq!(selected.len(), 1, "{root}/{case}/{family}");
                assert_eq!(selected[0]["probe"]["line"], 18);
                let class = selected[0]["classification"]
                    .as_str()
                    .ok_or("missing class")?;
                assert_eq!(
                    matches!(class, "exposed" | "weakly_exposed"),
                    credit,
                    "{root}/{case}/{family}: {class}"
                );
            }
        }
    }
    Ok(())
}

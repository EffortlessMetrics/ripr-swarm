//! Public relation controls for a free owner reached through a method hop
//! (#9603, RIPR-SPEC-0158). Receiver-name coincidence cannot hide helper reach.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir();
        std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        let path = root.join(format!(
            "ripr-method-hop-relations-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).map_err(|error| error.to_string())?;
        let scratch = Self(path);
        std::fs::create_dir(scratch.0.join("src")).map_err(|error| error.to_string())?;
        Ok(scratch)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn finding_for(observation: &str, method_hop: bool) -> Result<Value, String> {
    let (chain, anchor) = if method_hop {
        (
            "struct Size(u32);\nimpl Size {\n    fn shown(&self) -> bool {\n        to_string(self.0)\n    }\n}\n",
            "Size(10)",
        )
    } else {
        ("", "10")
    };
    let helper = if method_hop {
        "fn check(size: Size) { assert!(size.shown()); }"
    } else {
        "fn check(qty: u32) { assert!(to_string(qty)); }"
    };
    let source = format!(
        "fn to_string(qty: u32) -> bool {{\n    10 <= qty\n}}\n{chain}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n    {helper}\n    #[test]\n    fn anchor() {{ check({anchor}); }}\n    #[test]\n    fn observes() {{\n{observation}\n    }}\n}}\n"
    );
    let scratch = Scratch::create()?;
    std::fs::write(
        scratch.0.join("Cargo.toml"),
        "[package]\nname = \"method_hop_relations\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("src/lib.rs"), source).map_err(|error| error.to_string())?;
    let diff = scratch.0.join("change.diff");
    std::fs::write(
        &diff,
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n fn to_string(qty: u32) -> bool {\n-    qty >= 10\n+    10 <= qty\n }\n",
    )
    .map_err(|error| error.to_string())?;
    let report = check_workspace(CheckInput {
        root: scratch.0.clone(),
        diff_file: Some(diff),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let rendered = render_check(&report, &OutputFormat::Json)?;
    let json: Value = serde_json::from_str(&rendered).map_err(|error| error.to_string())?;
    let predicates: Vec<_> = json["findings"]
        .as_array()
        .ok_or("missing findings")?
        .iter()
        .filter(|finding| finding["probe"]["family"] == "predicate")
        .collect();
    assert_eq!(predicates.len(), 1, "one changed predicate: {json}");
    let finding = predicates[0];
    assert_eq!(finding["probe"]["expression"], "10 <= qty", "{finding}");
    assert_eq!(finding["probe"]["line"], 2, "{finding}");
    assert_eq!(
        relation_of(finding, "anchor"),
        Some(if method_hop {
            "helper_owner_call"
        } else {
            "direct_owner_call"
        }),
        "setup: the anchor must resolve through the expected route: {finding}"
    );
    Ok(finding.clone())
}

fn relation_of<'a>(finding: &'a Value, test: &str) -> Option<&'a str> {
    finding["related_tests"]
        .as_array()?
        .iter()
        .find(|related| related["name"] == test)?["relation_reason"]
        .as_str()
}

#[test]
fn receiver_only_call_does_not_relate_to_the_free_owner_by_name() -> Result<(), String> {
    let finding = finding_for("let v = 10u32; assert_eq!(v.to_string(), \"10\");", true)?;
    assert_eq!(
        relation_of(&finding, "observes"),
        Some("same_test_file"),
        "{finding}"
    );
    Ok(())
}

#[test]
fn receiver_name_collision_does_not_hide_the_helper_relation() -> Result<(), String> {
    let finding = finding_for(
        "let v = 10u32; let _ = v.to_string();\ncheck(Size(10));",
        true,
    )?;
    assert_eq!(
        relation_of(&finding, "observes"),
        Some("helper_owner_call"),
        "{finding}"
    );
    Ok(())
}

#[test]
fn body_only_free_calls_keep_the_direct_owner_relation() -> Result<(), String> {
    // A captured free call would add a second reached caller and decline the
    // method-hop chain. Newline call syntax exercises the full-body fallback.
    for call in ["to_string", "crate::to_string"] {
        let finding = finding_for(&format!("let got = {call}\n(10); assert!(got);"), true)?;
        assert_eq!(
            relation_of(&finding, "observes"),
            Some("direct_owner_call"),
            "{call}: {finding}"
        );
    }
    Ok(())
}

#[test]
fn a_receiver_call_does_not_hide_a_genuine_body_only_free_call() -> Result<(), String> {
    for call in ["to_string", "crate::to_string"] {
        let finding = finding_for(
            &format!(
                "let v = 10u32; let _ = v.to_string();\nlet got = {call}\n(10); assert!(got);"
            ),
            true,
        )?;
        assert_eq!(
            relation_of(&finding, "observes"),
            Some("direct_owner_call"),
            "{call}: {finding}"
        );
    }
    Ok(())
}

#[test]
fn ordinary_free_helper_keeps_its_existing_receiver_name_fallback() -> Result<(), String> {
    let finding = finding_for("let v = 10u32; assert_eq!(v.to_string(), \"10\");", false)?;
    assert_eq!(
        relation_of(&finding, "observes"),
        Some("weak_token_substring"),
        "{finding}"
    );
    Ok(())
}

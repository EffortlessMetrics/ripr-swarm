//! Real producer/render controls for the human observation wording (#9244,
//! RIPR-SPEC-0122): a quoted source expression that contains the machine token
//! as an identifier must print exactly as written in the human Evidence output.
//!
//! The TypeScript boundary-gap fixture shape is reused with its `amount`
//! parameter renamed, so the missing-proof text the TypeScript producer writes
//! carries the identifier through the real analysis and render path. A build
//! without `lang-typescript` cannot observe this adapter path.

#![cfg(feature = "lang-typescript")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

fn source(param: &str) -> String {
    format!(
        "export function applyDiscount({param}: number, threshold: number): number {{
    if ({param} >= threshold) {{
        return {param} - 10;
    }}
    return {param};
}}
"
    )
}

fn diff() -> &'static str {
    "diff --git a/src/discount.ts b/src/discount.ts
index 0000000..1111111 100644
--- a/src/discount.ts
+++ b/src/discount.ts
@@ -1,5 +1,5 @@
 export function applyDiscount(PARAM: number, threshold: number): number {
-    if (PARAM > threshold) {
+    if (PARAM >= threshold) {
         return PARAM - 10;
     }
     return PARAM;
"
}

const TESTS: &str = "test(\"below threshold has no discount\", () => {
    const result = applyDiscount(50, 100);
    if (result !== 50) {
        throw new Error(\"expected 50\");
    }
});

test(\"far above threshold discounts\", () => {
    const result = applyDiscount(10000, 100);
    if (result !== 9990) {
        throw new Error(\"expected 9990\");
    }
});
";

fn render_human_full(param: &str) -> Result<String, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let root: PathBuf = std::env::temp_dir().join(format!(
        "ripr-observation-wording-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let write = |path: &str, text: &str| -> Result<(), String> {
        let file = root.join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).map_err(|error| format!("mkdir {path}: {error}"))?;
        }
        std::fs::write(&file, text).map_err(|error| format!("write {path}: {error}"))
    };
    write(
        "ripr.toml",
        "[languages]\nenabled = [\"rust\", \"typescript\"]\n",
    )?;
    write("src/discount.ts", &source(param))?;
    write("tests/discount.test.ts", TESTS)?;
    write("diff.patch", &diff().replace("PARAM", param))?;
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(["check", "--root"])
        .arg(&root)
        .arg("--diff")
        .arg(root.join("diff.patch"))
        .args(["--mode", "fast", "--format", "human-full"])
        .output()
        .map_err(|error| format!("run ripr: {error}"));
    let result = output.map(|out| String::from_utf8_lossy(&out.stdout).into_owned());
    let _ = std::fs::remove_dir_all(&root);
    result
}

#[test]
fn renamed_parameter_equal_to_the_token_prints_as_written() -> Result<(), String> {
    let text = render_human_full("observation_unverified")?;
    assert!(
        text.contains("`observation_unverified == threshold`"),
        "missing quoted expression:\n{text}"
    );
    assert!(
        !text.contains("the observation is unconfirmed =="),
        "quoted source expression was rewritten:\n{text}"
    );
    Ok(())
}

#[test]
fn renamed_parameter_containing_the_token_prints_as_written() -> Result<(), String> {
    let text = render_human_full("check_observation_unverified_count")?;
    assert!(
        text.contains("`check_observation_unverified_count == threshold`"),
        "missing quoted expression:\n{text}"
    );
    assert!(
        !text.contains("check_the observation is unconfirmed"),
        "quoted source expression was rewritten:\n{text}"
    );
    Ok(())
}

#[test]
fn ordinary_parameter_keeps_the_prose_free_of_the_raw_marker() -> Result<(), String> {
    let text = render_human_full("amount")?;
    assert!(text.contains("`amount == threshold`"), "{text}");
    assert!(!text.contains("(observation_unverified)"), "{text}");
    Ok(())
}

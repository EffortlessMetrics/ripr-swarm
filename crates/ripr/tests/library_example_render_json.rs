//! The `render_json` library example runs and shows what it claims (#14098).
//!
//! The example is compiled into this test crate by path, so the code a reader
//! runs with `cargo run --example render_json` is the code checked here. Each
//! example has its own test file because all three load the shared
//! `examples/support/sample_workspace.rs` module, and one crate may load a
//! file as a module only once. A malformed bundled diff shows up here
//! as an incomplete analysis or a finding on the wrong line.

#[path = "../examples/render_json.rs"]
mod render_json;

#[test]
fn render_json_reports_a_complete_analysis_of_the_bundled_diff() -> Result<(), String> {
    let rendered = render_json::report()?;
    let document: serde_json::Value =
        serde_json::from_str(&rendered).map_err(|error| format!("invalid JSON: {error}"))?;

    let outcome = &document["analysis_outcome"];
    assert_eq!(outcome["analysis_complete"], true, "{outcome}");
    assert_eq!(
        outcome["outcome"]["kind"], "complete_with_findings",
        "{outcome}"
    );
    assert_eq!(
        outcome["outcome"]["limitations"],
        serde_json::json!([]),
        "{outcome}"
    );

    let lines: Vec<_> = document["findings"]
        .as_array()
        .ok_or("findings is not an array")?
        .iter()
        .map(|finding| {
            (
                finding["probe"]["file"].as_str().unwrap_or_default(),
                finding["probe"]["line"].as_u64().unwrap_or_default(),
            )
        })
        .collect();
    assert_eq!(
        lines,
        [
            ("./src/lib.rs", 8),
            ("./src/lib.rs", 22),
            ("./src/lib.rs", 22),
            ("./src/lib.rs", 22)
        ]
    );

    render_json::main()
}

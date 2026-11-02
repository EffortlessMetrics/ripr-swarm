//! Matched public-API controls for the #9478 owner path-value leaf (`.map(owner)`).

use super::Scratch;
use ripr::{CheckInput, ExposureClass, Mode, OutputFormat, ProbeFamily, check_workspace};

const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub fn inc(x: u32) -> u32 {
-    x + 1
+    1 + x
 }
";

const CASES: &[(&str, &str, bool)] = &[
    (
        "bare_map",
        "assert_eq!(vec![1, 2].into_iter().map(inc).collect::<Vec<_>>(), vec![2, 3]);",
        true,
    ),
    (
        "qualified",
        "assert_eq!(vec![1, 2].into_iter().map(crate::inc).collect::<Vec<_>>(), vec![2, 3]);",
        true,
    ),
    (
        "unresolved",
        "assert_eq!(vec![1, 2].into_iter().map(missing).collect::<Vec<_>>(), vec![2, 3]);",
        false,
    ),
    (
        "shadowed_binding",
        "let inc = other; assert_eq!(vec![1, 2].into_iter().map(inc).collect::<Vec<_>>(), vec![2, 3]);",
        false,
    ),
    (
        "foreign_same_name",
        "assert_eq!(vec![1, 2].into_iter().map(decoy::inc).collect::<Vec<_>>(), vec![10, 11]);",
        false,
    ),
    (
        "blind",
        "assert_eq!(vec![1, 2].into_iter().map(inc).collect::<Vec<_>>(), vec![9, 9]);",
        false,
    ),
];

fn source(body: &str) -> String {
    format!(
        "pub fn inc(x: u32) -> u32 {{
    1 + x
}}
pub mod decoy {{
    pub fn inc(x: u32) -> u32 {{ x + 9 }}
}}
#[cfg(test)]
mod tests {{
    use super::*;
    fn other(x: u32) -> u32 {{ x + 9 }}
    #[test]
    fn owner_as_path_value() {{
        {body}
    }}
}}
"
    )
}

#[test]
fn path_value_pins_only_the_observed_owner() -> Result<(), String> {
    for &(case, body, credited) in CASES {
        let scratch = Scratch::create()?;
        std::fs::create_dir(scratch.0.join("src")).map_err(|error| error.to_string())?;
        std::fs::write(
            scratch.0.join("Cargo.toml"),
            "[package]\nname = \"path-value-owner\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(scratch.0.join("src/lib.rs"), source(body))
            .map_err(|error| error.to_string())?;
        std::fs::write(scratch.0.join("change.diff"), DIFF).map_err(|error| error.to_string())?;
        let report = check_workspace(CheckInput {
            root: scratch.0.clone(),
            diff_file: Some(scratch.0.join("change.diff")),
            mode: Mode::Fast,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let changed: Vec<_> = report
            .findings
            .iter()
            .filter(|finding| {
                finding.probe.family == ProbeFamily::ReturnValue && finding.probe.location.line == 2
            })
            .collect();
        assert_eq!(changed.len(), 1, "{case}: one changed return value");
        assert_eq!(
            changed[0].class == ExposureClass::Exposed,
            credited,
            "{case}: {:?}",
            changed[0]
        );
        if case == "blind" {
            assert_eq!(changed[0].class, ExposureClass::WeaklyExposed);
        }
    }
    Ok(())
}

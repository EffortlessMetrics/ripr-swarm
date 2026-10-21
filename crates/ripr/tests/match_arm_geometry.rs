//! Source-backed geometry and production calibration controls for #5905.
//! These are bounded location-join inputs, not cargo-mutants run results.

use ripr::output::{
    CargoMutantsLocationJoin, CargoMutantsOutcomeRecord, CargoMutantsRuntimeSpan,
    CargoMutantsSeamLocation, join_cargo_mutants_location,
};
use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const SOURCE: &str = "pub fn route(value: Option<u8>) -> Result<u8, ()> {\n    Ok(match value {\n        Some(v) => v + 1,\n        _ => 0,\n    })\n}\n";

struct Workspace(PathBuf);

impl Workspace {
    fn new(source: &str) -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-arm-geometry-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = 'arm-geometry'\nversion = '0.0.0'\nedition = '2024'\n[workspace]\n",
        )
        .map_err(|e| e.to_string())?;
        std::fs::write(root.join("src/lib.rs"), source).map_err(|e| e.to_string())?;
        Ok(Self(root))
    }

    fn inventory(&self, format: OutputFormat) -> Result<Value, String> {
        let output = check_workspace(CheckInput {
            root: self.0.clone(),
            mode: Mode::Ready,
            format,
            ..CheckInput::default()
        })?;
        serde_json::from_str(&render_check(&output, &format)?).map_err(|e| e.to_string())
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn seams(document: &Value) -> Result<&Vec<Value>, String> {
    document["seams"].as_array().ok_or("missing seams".into())
}

fn arm<'a>(document: &'a Value, expression: &str) -> Result<&'a Value, String> {
    let matches: Vec<_> = seams(document)?
        .iter()
        .filter(|s| s["kind"] == "match_arm" && s["expression"] == expression)
        .collect();
    if matches.len() != 1 {
        return Err(format!("expected one {expression:?} arm, got {matches:?}"));
    }
    Ok(matches[0])
}

fn location(seam: &Value) -> Result<CargoMutantsSeamLocation, String> {
    let coordinate = |name: &str| seam[name].as_u64().map(|n| n as usize);
    Ok(CargoMutantsSeamLocation {
        file: seam["file"].as_str().ok_or("missing file")?.into(),
        line: coordinate("line").ok_or("missing line")?,
        column: coordinate("column"),
        end_line: coordinate("end_line"),
        end_column: coordinate("end_column"),
    })
}

#[test]
fn full_arm_source_range_joins_its_arm_in_both_inventories() -> Result<(), String> {
    let workspace = Workspace::new(SOURCE)?;
    for format in [OutputFormat::RepoSeamsJson, OutputFormat::RepoExposureJson] {
        // Repeat to challenge the warm classified/file-fact cache path.
        for _ in 0..2 {
            let document = workspace.inventory(format)?;
            assert_eq!(
                document["schema_version"],
                if format == OutputFormat::RepoSeamsJson {
                    "0.3"
                } else {
                    "0.5"
                }
            );
            let some = arm(&document, "Some(v) =>")?;
            let fallback = arm(&document, "_ =>")?;
            let head = arm(&document, "match value")?;
            assert!(head.get("column").is_none(), "match keyword is not an arm");
            assert_eq!(
                location(some)?,
                CargoMutantsSeamLocation {
                    file: "src/lib.rs".into(),
                    line: 3,
                    column: Some(9),
                    end_line: Some(3),
                    end_column: Some(26),
                }
            );
            assert_eq!(
                location(fallback)?,
                CargoMutantsSeamLocation {
                    file: "src/lib.rs".into(),
                    line: 4,
                    column: Some(9),
                    end_line: Some(4),
                    end_column: Some(16),
                }
            );
            let records = seams(&document)?;
            let locations = records
                .iter()
                .map(location)
                .collect::<Result<Vec<_>, _>>()?;
            // Exact ranges of the two comma-terminated arms in SOURCE. No
            // mutation genre or observed runtime outcome is manufactured.
            for (expected, start, end) in [(some, (3, 9), (3, 26)), (fallback, (4, 9), (4, 16))] {
                let input = CargoMutantsOutcomeRecord {
                    mutant_id: None,
                    seam_id: None,
                    file: Some("src/lib.rs".into()),
                    line: Some(start.0),
                    span: Some(CargoMutantsRuntimeSpan { start, end }),
                    span_conflict: false,
                    mutation_operator: "unknown".into(),
                    runtime_outcome: "unknown".into(),
                    duration: None,
                    test_command: None,
                };
                let CargoMutantsLocationJoin::Matched("span_containment", index) =
                    join_cargo_mutants_location(&input, &locations)
                else {
                    return Err(format!("arm range did not join by containment: {document}"));
                };
                assert_eq!(records[index]["seam_id"], expected["seam_id"]);
            }
        }
    }
    Ok(())
}

#[test]
fn multiline_pattern_uses_its_start_line_and_unicode_columns() -> Result<(), String> {
    let source = "pub fn route(value: &str) -> u8 {\n    match value {\n        \"é\"\n        | \"β\" => {\n            1\n        },\n        _ => 0,\n    }\n}\n";
    for source in [source.to_owned(), source.replace('\n', "\r\n")] {
        let workspace = Workspace::new(&source)?;
        for format in [OutputFormat::RepoSeamsJson, OutputFormat::RepoExposureJson] {
            let document = workspace.inventory(format)?;
            let selected = arm(&document, "\"é\" | \"β\" =>")?;
            assert_eq!(
                location(selected)?,
                CargoMutantsSeamLocation {
                    file: "src/lib.rs".into(),
                    line: 3,
                    column: Some(9),
                    end_line: Some(6),
                    end_column: Some(11),
                }
            );
            assert_eq!(location(arm(&document, "_ =>")?)?.line, 7);
        }
    }
    let workspace = Workspace::new(
        "pub fn route(value: &str) -> u8 {\n    match value {\n        \"é\" => 1,\n        _ => 0,\n    }\n}\n",
    )?;
    let document = workspace.inventory(OutputFormat::RepoSeamsJson)?;
    assert_eq!(location(arm(&document, "\"é\" =>")?)?.end_column, Some(18));
    Ok(())
}

#[test]
fn attributed_and_comma_less_arms_keep_complete_body_spans() -> Result<(), String> {
    let workspace = Workspace::new(
        "pub fn route(value: Option<u8>) -> u8 {\n    match value {\n        #[allow(unused_braces)]\n        Some(v) => { v }\n        _ => 0,\n    }\n}\n",
    )?;
    for format in [OutputFormat::RepoSeamsJson, OutputFormat::RepoExposureJson] {
        let document = workspace.inventory(format)?;
        let selected = arm(&document, "#[allow(unused_braces)] Some(v) =>")?;
        // Geometry starts at the pattern, not its attribute or arrow token;
        // a block arm needs no comma before the following wildcard arm.
        assert_eq!(
            location(selected)?,
            CargoMutantsSeamLocation {
                file: "src/lib.rs".into(),
                line: 4,
                column: Some(9),
                end_line: Some(4),
                end_column: Some(25),
            }
        );
        assert_eq!(location(arm(&document, "_ =>")?)?.line, 5);
    }
    Ok(())
}

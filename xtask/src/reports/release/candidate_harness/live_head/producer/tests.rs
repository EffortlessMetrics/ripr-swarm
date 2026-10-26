use super::*;
use crate::reports::release::candidate_harness::live_head::{LiveHeadSnapshot, tests::Fixture};
use serde_json::json;

fn produce(fixture: &Fixture, output: &str) -> Result<(), String> {
    let args = [
        "release-manifest-proposal",
        "--controller-root",
        fixture.root.to_str().ok_or("non-UTF-8 fixture")?,
        "--version",
        "0.11.0",
        "--input",
        "manifest.json",
        "--out",
        output,
    ];
    crate::dispatch::execute(crate::XtaskCommand::parse(
        args.into_iter().map(String::from),
    ))
}

#[test]
fn proposal_dispatch_serializes_the_complete_typed_manifest_without_accepting_it()
-> Result<(), String> {
    let mut fixture = Fixture::new()?;
    fixture.document["non_claims"] = json!([
        "synthetic fixture, not release acceptance",
        "```text\nnot a heading"
    ]);
    fixture.write()?;
    produce(&fixture, "proposal")?;
    let bytes = fs::read(fixture.root.join("proposal/manifest.json")).map_err(|e| e.to_string())?;
    let emitted: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    assert_eq!(
        emitted, fixture.document,
        "all typed fields, including custody and owner packets, survive production"
    );
    let markdown =
        fs::read_to_string(fixture.root.join("proposal/manifest.md")).map_err(|e| e.to_string())?;
    let serialized = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
    assert!(markdown.contains(serialized));
    assert!(
        markdown.contains("````json\n"),
        "input Markdown cannot close the handoff fence"
    );
    assert!(markdown.contains("Independent #1609 acceptance is not established."));
    assert!(markdown.contains(&super::super::digest(&bytes)));
    assert!(
        LiveHeadSnapshot::admit(
            &fixture.root,
            "0.11.0",
            Path::new("proposal/manifest.json"),
            ""
        )
        .is_err(),
        "emission grants no independent acceptance"
    );
    // Synthetic acceptance is only interoperability proof, never a native
    // #1609 decision or final-cut acceptance.
    LiveHeadSnapshot::admit(
        &fixture.root,
        "0.11.0",
        Path::new("proposal/manifest.json"),
        &super::super::digest(&bytes),
    )?;
    let before = bytes;
    assert!(produce(&fixture, "proposal").is_err());
    assert_eq!(
        fs::read(fixture.root.join("proposal/manifest.json")).map_err(|e| e.to_string())?,
        before
    );
    Ok(())
}

#[test]
fn proposal_dispatch_refuses_incomplete_stale_and_unknown_inputs_before_emission()
-> Result<(), String> {
    for (pointer, value) in [
        ("/status", json!("active_selection_template")),
        ("/candidate", serde_json::Value::Null),
        ("/qualification/state", json!("passed")),
        ("/source_parent", json!("f".repeat(40))),
        (
            "/prerequisites/audit/acceptance/status",
            json!("not_established"),
        ),
        (
            "/prerequisites/denominator/acceptance/candidate_sha",
            json!("f".repeat(40)),
        ),
        ("/pin/remote_ref_readback/path", json!("selection.json")),
    ] {
        let mut fixture = Fixture::new()?;
        *fixture
            .document
            .pointer_mut(pointer)
            .ok_or("missing fixture pointer")? = value;
        fixture.write()?;
        assert!(
            produce(&fixture, "proposal").is_err(),
            "must refuse {pointer}"
        );
        assert!(!fixture.root.join("proposal").exists());
    }
    for action in ["unknown", "missing", "tamper", "missing_packet", "pin"] {
        let mut fixture = Fixture::new()?;
        if action == "unknown" {
            fixture.document["future_acceptance"] = json!(true);
        }
        if action == "missing" {
            fixture
                .document
                .as_object_mut()
                .ok_or("not object")?
                .remove("qualification");
        }
        fixture.write()?;
        match action {
            "tamper" => fs::write(fixture.root.join("audit.json"), b"changed accepted packet")
                .map_err(|e| e.to_string())?,
            "missing_packet" => {
                fs::remove_file(fixture.root.join("selection.json")).map_err(|e| e.to_string())?
            }
            "pin" => {
                let bytes = format!("{}\n", "f".repeat(40));
                fs::write(fixture.root.join("pin-remote.sha"), &bytes)
                    .map_err(|e| e.to_string())?;
                fixture.document["pin"]["remote_ref_readback"]["sha256"] =
                    json!(super::super::digest(bytes.as_bytes()));
                fixture.write()?;
            }
            _ => {}
        }
        assert!(
            produce(&fixture, "proposal").is_err(),
            "must refuse {action}"
        );
        assert!(!fixture.root.join("proposal").exists());
    }
    Ok(())
}

#[test]
fn proposal_dispatch_refuses_unsafe_and_existing_destinations() -> Result<(), String> {
    let fixture = Fixture::new()?;
    fixture.write()?;
    for output in [
        "../escape",
        "/absolute",
        "nested/../escape",
        "manifest.json",
        ".",
        "missing/child",
    ] {
        assert!(produce(&fixture, output).is_err(), "must refuse {output}");
    }
    #[cfg(unix)]
    {
        let outside = Fixture::new()?;
        std::os::unix::fs::symlink(&outside.root, fixture.root.join("escape"))
            .map_err(|e| e.to_string())?;
        assert!(produce(&fixture, "escape/proposal").is_err());
        assert!(!outside.root.join("proposal").exists());
    }
    Ok(())
}

#[test]
fn proposal_dispatch_refuses_serialization_growth_past_the_aggregate_input_budget()
-> Result<(), String> {
    let mut fixture = Fixture::new()?;
    let cap = input::MAX_INPUT_BYTES as usize;
    let block = vec![b'x'; cap];
    for (owner, file) in [
        ("selected_claims", "selection.json"),
        ("denominator", "denominator.json"),
        ("audit", "audit.json"),
    ] {
        fs::write(fixture.root.join(file), &block).map_err(|e| e.to_string())?;
        let hash = super::super::digest(&block);
        fixture.document["prerequisites"][owner]["packet"]["sha256"] = json!(&hash);
        fixture.document["prerequisites"][owner]["acceptance"]["reviewed_packet_sha256"] =
            json!(hash);
    }
    drop(block);
    fixture.write()?;
    let original_len = fs::metadata(fixture.root.join("manifest.json"))
        .map_err(|e| e.to_string())?
        .len();
    let mut other_bytes = 0;
    for file in [
        "selection.json",
        "denominator.json",
        "audit.json",
        "pin-remote.sha",
        "ruleset.json",
        "blind-contract.json",
    ] {
        other_bytes += fs::metadata(fixture.root.join(file))
            .map_err(|e| format!("{file}: {e}"))?
            .len();
    }
    let remaining = input::MAX_RETAINED_BYTES - original_len - other_bytes;
    assert!(remaining > 0 && remaining <= input::MAX_INPUT_BYTES);
    let harness = vec![b'h'; remaining as usize];
    fs::write(fixture.root.join("harness.json"), &harness).map_err(|e| e.to_string())?;
    fixture.document["qualification"]["proof_inputs"][0]["sha256"] =
        json!(super::super::digest(&harness));
    drop(harness);
    let accepted = fixture.write()?;
    assert_eq!(
        fs::metadata(fixture.root.join("manifest.json"))
            .map_err(|e| e.to_string())?
            .len(),
        original_len,
        "fixed-width digest update must preserve boundary"
    );
    // The exact input really fits the existing consumer. Production adds a
    // final LF, so emission must refuse rather than produce inadmissible bytes.
    let admitted = LiveHeadSnapshot::admit(
        &fixture.root,
        "0.11.0",
        Path::new("manifest.json"),
        &accepted,
    )?;
    drop(admitted);
    match produce(&fixture, "proposal") {
        Err(error) if error.contains("64 MiB aggregate") => {}
        Err(error) => return Err(format!("wrong boundary refusal: {error}")),
        Ok(()) => {
            return Err("producer emitted a manifest beyond admission's aggregate cap".to_string());
        }
    }
    assert!(!fixture.root.join("proposal").exists());
    Ok(())
}

#[test]
fn proposal_partial_write_retains_the_incomplete_directory_and_existing_bytes() -> Result<(), String>
{
    let fixture = Fixture::new()?;
    let output = fixture.root.join("partial");
    fs::create_dir(&output).map_err(|e| e.to_string())?;
    fs::write(output.join("manifest.md"), b"existing evidence").map_err(|e| e.to_string())?;
    match write_files(&output, b"proposed json", b"new markdown") {
        Err(error) if error.contains("retain incomplete") => {}
        _ => return Err("partial output must report an incomplete directory".to_string()),
    }
    assert_eq!(
        fs::read(output.join("manifest.json")).map_err(|e| e.to_string())?,
        b"proposed json"
    );
    assert_eq!(
        fs::read(output.join("manifest.md")).map_err(|e| e.to_string())?,
        b"existing evidence"
    );
    Ok(())
}

#[test]
fn proposal_dispatch_refuses_a_compact_manifest_that_pretty_prints_past_the_per_file_budget()
-> Result<(), String> {
    // A compact input under 16 MiB whose pretty serialization exceeds it,
    // while the aggregate stays far under 64 MiB, so only the per-file branch
    // can refuse it (#10251). Each one-character non-claim is `"x",` compact
    // and at least five bytes longer pretty (newline plus indent). The long
    // array is spliced in as text so the test holds no per-element `Value`s.
    let mut fixture = Fixture::new()?;
    let count = (input::MAX_INPUT_BYTES as usize * 9 / 16) / 4;
    fixture.document["non_claims"] = json!(["PLACEHOLDER"]);
    let template = serde_json::to_string(&fixture.document).map_err(|e| e.to_string())?;
    let mut items = "\"x\",".repeat(count);
    items.pop();
    let compact = template.replacen("\"PLACEHOLDER\"", &items, 1);
    drop(items);
    let pretty_floor = (compact.len() + 5 * count) as u64;
    assert!((compact.len() as u64) < input::MAX_INPUT_BYTES);
    assert!(pretty_floor > input::MAX_INPUT_BYTES);
    assert!(2 * pretty_floor < input::MAX_RETAINED_BYTES);
    fs::write(fixture.root.join("manifest.json"), &compact).map_err(|e| e.to_string())?;
    drop(compact);
    match produce(&fixture, "proposal") {
        Err(error) if error.contains("16 MiB per file") => {}
        Err(error) => return Err(format!("wrong refusal: {error}")),
        Ok(()) => return Err("pretty growth past 16 MiB must be refused".to_string()),
    }
    assert!(
        !fixture.root.join("proposal").exists(),
        "a refused proposal must create no output directory"
    );
    Ok(())
}

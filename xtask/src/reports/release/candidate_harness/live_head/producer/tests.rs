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
    assert!(markdown.contains("independent") || markdown.contains("Independent"));
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

//! `ripr agent stub`: the one-step route from a gap to a runnable test.
//!
//! CLI adapter only. Selection, the guarded write, and the run command live
//! in `crate::app::test_stub`; the stub itself is produced by
//! `crate::analysis::test_stub`. A refusal or an unknown selector is a
//! decision (exit 3) with empty stdout, like `agent card`.

use crate::agent::loop_commands::{bound_root, shell_arg};
use crate::analysis::seams::SeamGripClass;
use crate::app::agent_brief::AgentBriefPolicy;
use crate::app::test_stub::{
    TestStubError, TestStubSelector, line_of_offset, package_manifest_for, resolve_test_stub,
    run_command, write_test_stub,
};
use crate::cli::CommandError;
use crate::cli::agent::AgentStubOptions;
use crate::cli::commands_context::{ensure_command_root, write_stderr_refusal_json};
use crate::config::{RiprConfig, load_for_root};
use crate::output::gap_vocabulary::exposure_counterpart;

/// Schema version of the `rust_test_stub` JSON document.
const RUST_TEST_STUB_SCHEMA_VERSION: &str = "0.2";

pub(super) fn run_agent_stub(options: AgentStubOptions) -> Result<(), CommandError> {
    ensure_command_root(&options.root, "agent stub")?;
    let config = load_for_root(&options.root).map_err(CommandError::Failure)?;
    let resolution = match resolve_test_stub(&options.root, &config, &options.selector) {
        Ok(resolution) => resolution,
        Err(error) => {
            let (refusal_kind, message) = match error {
                TestStubError::NotFound(message) => ("seam_not_found", message),
                TestStubError::Ambiguous(message) => ("selector_ambiguous", message),
                TestStubError::Operational(message) => return Err(CommandError::Failure(message)),
            };
            let root = shell_arg(&bound_root(&options.root.to_string_lossy()));
            if options.json {
                // EXIT_CODES.md promises the versioned `rust_test_stub`
                // `state: "refused"` envelope under `--json` for every
                // agent-stub refusal, explicitly including "the selector
                // names no reported gap" (#7390). The document keeps the
                // post-resolution refusal's field set, with `null` where
                // seam resolution would have supplied a value; stdout stays
                // empty and the prose still reaches stderr via
                // `CommandError` after the envelope.
                match render_selector_refusal_document(&options.selector, refusal_kind, &message) {
                    // A JSON document, not report text: keep it parseable (#6309).
                    Ok(rendered) => write_stderr_refusal_json(rendered),
                    Err(render_error) => eprintln!("ripr: {render_error}"),
                }
            }
            return Err(CommandError::Decision(format!(
                "agent stub: {message}. Run `ripr check --root {root}` to list finding locations or `ripr pilot --root {root}` to list seam IDs."
            )));
        }
    };
    let disclosure = stub_grip_disclosure(&resolution.seam_id, resolution.grip_class, &config);
    let stub = match &resolution.outcome {
        Ok(stub) => stub,
        Err(refusal) => {
            if options.json {
                let document = serde_json::json!({
                    "schema_version": RUST_TEST_STUB_SCHEMA_VERSION,
                    "kind": "rust_test_stub",
                    "seam_id": resolution.seam_id,
                    "owner": resolution.owner,
                    "state": "refused",
                    "refusal": {"kind": refusal.as_str(), "reason": refusal.reason()},
                    "grip_class": disclosure.grip_class,
                    "classification": disclosure.classification,
                    "warnings": disclosure.warnings,
                });
                // A JSON document, not report text: keep it parseable (#6309).
                write_stderr_refusal_json(render(&document)?);
            }
            return Err(CommandError::Decision(format!(
                "agent stub: no test stub for seam {} ({}): {}",
                resolution.seam_id,
                refusal.as_str(),
                refusal.reason()
            )));
        }
    };
    let written = if options.write {
        Some(write_test_stub(&options.root, &resolution, stub).map_err(CommandError::Failure)?)
    } else {
        None
    };
    let line = stub
        .placement
        .offset()
        .map(|offset| line_of_offset(&resolution.source, offset));
    let file = stub
        .placement
        .file()
        .display()
        .to_string()
        .replace('\\', "/");
    let run = package_manifest_for(&options.root, stub).map(|manifest| {
        let manifest = options.root.join(manifest);
        let manifest = manifest
            .strip_prefix(".")
            .unwrap_or(&manifest)
            .to_string_lossy()
            .replace('\\', "/");
        run_command(&shell_arg(&manifest), stub)
    });

    if options.json {
        let document = serde_json::json!({
            "schema_version": RUST_TEST_STUB_SCHEMA_VERSION,
            "kind": "rust_test_stub",
            "seam_id": resolution.seam_id,
            "owner": resolution.owner,
            "state": "ready",
            "placement": {
                "kind": stub.placement.kind_str(),
                "file": file,
                "offset": stub.placement.offset(),
                "line": line,
            },
            "test_name": stub.test_name,
            "text": stub.text,
            "fill_ins": stub.fill_ins,
            "derived_inputs": stub.derived_inputs,
            "written": written.is_some(),
            "run_command": run,
            "grip_class": disclosure.grip_class,
            "classification": disclosure.classification,
            "warnings": disclosure.warnings,
        });
        println!("{}", render(&document)?);
        return Ok(());
    }

    let place = match line {
        Some(line) => format!(
            "{file}:{line} ({})",
            stub.placement.kind_str().replace('_', " ")
        ),
        None => format!("{file} (new file)"),
    };
    for warning in &disclosure.warnings {
        println!("{warning}");
    }
    if written.is_some() {
        println!("Wrote test stub `{}` to {place}.", stub.test_name);
    } else {
        println!("Test stub `{}` for {place}:", stub.test_name);
        println!();
        print!("{}", stub.text.trim_start_matches('\n'));
        println!();
    }
    if !stub.derived_inputs.is_empty() {
        println!(
            "Inputs from the changed comparison: {}",
            stub.derived_inputs.join(", ")
        );
    }
    for fill_in in &stub.fill_ins {
        println!("Fill in: {fill_in}");
    }
    if written.is_none() {
        println!("Write it: {}", write_command(&options));
    }
    match &run {
        Some(run) => println!("Run it: {run}"),
        None => {
            println!("Run it: no Cargo package owns {file}; add it to one, then run its tests.")
        }
    }
    Ok(())
}

fn write_command(options: &AgentStubOptions) -> String {
    let root = shell_arg(&bound_root(&options.root.to_string_lossy()));
    let selector = match &options.selector {
        TestStubSelector::SeamId(id) => format!("--seam-id {}", shell_arg(id)),
        TestStubSelector::At { file, line, kind } => {
            let at = format!("--at {}", shell_arg(&format!("{file}:{line}")));
            match kind {
                Some(kind) => format!("{at} --kind {}", shell_arg(kind)),
                None => at,
            }
        }
    };
    format!("ripr agent stub --root {root} {selector} --write")
}

fn render(document: &serde_json::Value) -> Result<String, CommandError> {
    serde_json::to_string_pretty(document).map_err(|error| {
        CommandError::Failure(format!("serialize agent stub document failed: {error}"))
    })
}

/// The versioned `rust_test_stub` `state: "refused"` document for a
/// selector that named no reported gap (#7390) or was ambiguous (#7762):
/// the same field set the
/// post-resolution refusal arm emits, with `null` where seam resolution
/// would have supplied a value. `seam_id` repeats the requested id for a
/// `--seam-id` selector and is `null` for a location selector, which never
/// resolved to a seam id.
fn render_selector_refusal_document(
    selector: &TestStubSelector,
    refusal_kind: &str,
    message: &str,
) -> Result<String, CommandError> {
    let seam_id = match selector {
        TestStubSelector::SeamId(id) => serde_json::json!(id),
        TestStubSelector::At { .. } => serde_json::Value::Null,
    };
    let document = serde_json::json!({
        "schema_version": RUST_TEST_STUB_SCHEMA_VERSION,
        "kind": "rust_test_stub",
        "seam_id": seam_id,
        "owner": serde_json::Value::Null,
        "state": "refused",
        "refusal": {"kind": refusal_kind, "reason": message},
        "grip_class": serde_json::Value::Null,
        "classification": serde_json::Value::Null,
        "warnings": [],
    });
    render(&document)
}

/// Inventory grip plus the check-side exposure counterpart, and the
/// packet/repair omission prose when that class is omitted from agent
/// results. Additive on `rust_test_stub` schema `0.1` (#7290).
#[derive(Debug, PartialEq, Eq)]
struct StubGripDisclosure {
    grip_class: Option<&'static str>,
    classification: Option<&'static str>,
    warnings: Vec<String>,
}

fn stub_grip_disclosure(
    seam_id: &str,
    grip_class: Option<SeamGripClass>,
    config: &RiprConfig,
) -> StubGripDisclosure {
    let warnings = grip_class
        .and_then(|class| AgentBriefPolicy::from_config(config).omission_reason_for_class(class))
        .map(|reason| format!("seam {seam_id} {reason}"))
        .into_iter()
        .collect();
    let grip_class = grip_class.map(|class| class.as_str());
    StubGripDisclosure {
        classification: grip_class.and_then(exposure_counterpart),
        grip_class,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #7390: the not-found refusal — the most common agent-stub refusal, a
    /// stale or mistyped seam id — must render the same versioned
    /// `rust_test_stub` `state: "refused"` envelope the post-resolution
    /// refusals emit, so an `--json` consumer can branch on the typed
    /// `refusal.kind` instead of meeting free prose.
    #[test]
    fn not_found_refusal_renders_the_typed_envelope() -> Result<(), String> {
        let rendered = render_selector_refusal_document(
            &TestStubSelector::SeamId("no-such-seam".to_string()),
            "seam_not_found",
            "seam_id no-such-seam was not found",
        )
        .map_err(|error| format!("envelope render refused: {error:?}"))?;
        let document: serde_json::Value = serde_json::from_str(&rendered)
            .map_err(|error| format!("envelope was not parseable JSON: {error}"))?;
        assert_eq!(document["schema_version"], "0.2");
        assert_eq!(document["kind"], "rust_test_stub");
        assert_eq!(document["seam_id"], "no-such-seam");
        assert_eq!(document["state"], "refused");
        assert_eq!(document["refusal"]["kind"], "seam_not_found");
        assert_eq!(
            document["refusal"]["reason"],
            "seam_id no-such-seam was not found"
        );
        assert_eq!(document["warnings"], serde_json::json!([]));
        Ok(())
    }

    /// A location selector never resolved to a seam id, so the envelope
    /// keeps the field set but leaves `seam_id` null rather than inventing
    /// an identity.
    #[test]
    fn not_found_refusal_for_location_selector_keeps_seam_id_null() -> Result<(), String> {
        let rendered = render_selector_refusal_document(
            &TestStubSelector::At {
                file: "src/lib.rs".to_string(),
                line: 12,
                kind: None,
            },
            "seam_not_found",
            "no finding matched src/lib.rs:12",
        )
        .map_err(|error| format!("envelope render refused: {error:?}"))?;
        let document: serde_json::Value = serde_json::from_str(&rendered)
            .map_err(|error| format!("envelope was not parseable JSON: {error}"))?;
        assert_eq!(document["state"], "refused");
        assert_eq!(document["refusal"]["kind"], "seam_not_found");
        if !document["seam_id"].is_null() {
            return Err(format!(
                "a location selector must not invent a seam id: {document}"
            ));
        }
        Ok(())
    }

    #[test]
    fn strongly_gripped_stub_reuses_the_packet_omission_reason() {
        let disclosure = stub_grip_disclosure(
            "clamp-seam",
            Some(SeamGripClass::StronglyGripped),
            &RiprConfig::default(),
        );
        assert_eq!(disclosure.grip_class, Some("strongly_gripped"));
        assert_eq!(disclosure.classification, Some("exposed"));
        assert_eq!(
            disclosure.warnings,
            vec![
                "seam clamp-seam is configured off for strongly_gripped seams and is not included in agent results"
                    .to_string()
            ]
        );
    }

    #[test]
    fn open_gap_stub_has_no_omission_warning() {
        let disclosure = stub_grip_disclosure(
            "gap-seam",
            Some(SeamGripClass::Ungripped),
            &RiprConfig::default(),
        );
        assert_eq!(disclosure.grip_class, Some("ungripped"));
        assert_eq!(disclosure.classification, Some("no_static_path"));
        assert!(disclosure.warnings.is_empty(), "{:?}", disclosure.warnings);
    }

    #[test]
    fn unknown_grip_stays_null_without_inventing_a_class() {
        let disclosure = stub_grip_disclosure("unknown-seam", None, &RiprConfig::default());
        assert_eq!(disclosure.grip_class, None);
        assert_eq!(disclosure.classification, None);
        assert!(disclosure.warnings.is_empty());
    }
}

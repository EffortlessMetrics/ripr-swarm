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
use crate::cli::commands_context::ensure_command_root;
use crate::config::{RiprConfig, load_for_root};
use crate::output::gap_vocabulary::exposure_counterpart;

/// Schema version of the `rust_test_stub` JSON document.
const RUST_TEST_STUB_SCHEMA_VERSION: &str = "0.1";

pub(super) fn run_agent_stub(options: AgentStubOptions) -> Result<(), CommandError> {
    ensure_command_root(&options.root, "agent stub")?;
    let config = load_for_root(&options.root).map_err(CommandError::Failure)?;
    let resolution = match resolve_test_stub(&options.root, &config, &options.selector) {
        Ok(resolution) => resolution,
        Err(TestStubError::NotFound(message)) => {
            let root = shell_arg(&bound_root(&options.root.to_string_lossy()));
            return Err(CommandError::Decision(format!(
                "agent stub: {message}. Run `ripr check --root {root}` to list finding locations or `ripr pilot --root {root}` to list seam IDs."
            )));
        }
        Err(TestStubError::Operational(message)) => return Err(CommandError::Failure(message)),
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
                ::std::eprintln!(
                    "{}",
                    crate::terminal_text::json_terminal_safe(render(&document)?)
                );
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

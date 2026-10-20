//! `ripr agent stub`: the one-step route from a gap to a runnable test.
//!
//! CLI adapter only. Selection, the guarded write, and the run command live
//! in `crate::app::test_stub`; the stub itself is produced by
//! `crate::analysis::test_stub`. A refusal or an unknown selector is a
//! decision (exit 3) with empty stdout, like `agent card`.

use crate::agent::loop_commands::{bound_root, shell_arg};
use crate::analysis::seams::SeamGripClass;
use crate::analysis::test_stub::TestStubPlacement;
use crate::app::agent_brief::AgentBriefPolicy;
use crate::app::test_stub::{
    TargetLookup, TestStubError, TestStubSelector, integration_target_lookup, line_of_offset,
    package_manifest_for, resolve_test_stub, run_command, write_test_stub,
};
use crate::cli::CommandError;
use crate::cli::agent::AgentStubOptions;
use crate::cli::commands_context::{ensure_command_root, write_stderr_refusal_json};
use crate::config::{RiprConfig, load_for_root};
use crate::output::gap_vocabulary::exposure_counterpart;

/// Schema version of the `rust_test_stub` JSON document.
const RUST_TEST_STUB_SCHEMA_VERSION: &str = "0.2";

pub(super) fn run_agent_stub(mut options: AgentStubOptions) -> Result<(), CommandError> {
    // #8767: same implicit root `ripr check` uses; `--root .` opts out.
    let prefix =
        super::check::resolve_implicit_command_root(&mut options.root, options.root_explicit)?;
    if let crate::app::test_stub::TestStubSelector::At { file, .. } = &mut options.selector {
        *file = super::check::bind_member_selector_file(prefix.as_deref(), &options.root, file)?;
    }
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
    let mut disclosure = stub_grip_disclosure(&resolution.seam_id, resolution.grip_class, &config);
    if let Some(note) = &resolution.at_note {
        disclosure.warnings.insert(0, note.clone());
    }
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
            return Err(CommandError::Decision(refusal_message(
                &resolution.seam_id,
                refusal.as_str(),
                refusal.reason(),
                resolution.at_note.as_deref(),
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
    let owning_manifest = package_manifest_for(&options.root, stub);
    let lookup = owning_manifest
        .as_ref()
        .map(|manifest| integration_target_lookup(&options.root, manifest, stub));
    let run = owning_manifest.as_ref().and_then(|manifest| {
        let target = match &lookup {
            Some(TargetLookup::Name(name)) => Some(name.clone()),
            _ => None,
        };
        let manifest = options.root.join(manifest);
        let manifest = manifest
            .strip_prefix(".")
            .unwrap_or(&manifest)
            .to_string_lossy()
            .replace('\\', "/");
        run_command(&shell_arg(&manifest), stub, target.as_deref())
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
        None if matches!(
            stub.placement,
            TestStubPlacement::ExistingIntegrationFile { .. }
        ) =>
        {
            format!("{file} (end of existing integration test file)")
        }
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
    println!(
        "{}",
        run_line(
            run.as_deref(),
            owning_manifest.is_some(),
            lookup.as_ref(),
            &file
        )
    );
    Ok(())
}

/// The human "Run it" line: the command, or why none is offered. Only a
/// positive determination says the file is not a registered cargo target;
/// an unresolved lookup says the target is not established.
fn run_line(
    run: Option<&str>,
    has_owning_manifest: bool,
    lookup: Option<&TargetLookup>,
    file: &str,
) -> String {
    match run {
        Some(run) => format!("Run it: {run}"),
        None if has_owning_manifest && lookup == Some(&TargetLookup::NotRegistered) => format!(
            "Run it: {file} is not a registered cargo test target (check [[test]] entries and autotests in Cargo.toml); no command is offered."
        ),
        None if has_owning_manifest => format!(
            "Run it: the cargo test target for {file} is not established (the manifest, edition, harness or required-features could not be resolved to a runnable target); no command is offered."
        ),
        None => format!("Run it: no Cargo package owns {file}; add it to one, then run its tests."),
    }
}

#[cfg(test)]
mod run_line_tests {
    use super::run_line;
    use crate::app::test_stub::TargetLookup;

    #[test]
    fn unestablished_target_says_the_file_is_not_a_registered_cargo_target() {
        let none = TargetLookup::NotRegistered;
        let line = run_line(None, true, Some(&none), "tests/discount.rs");
        assert!(line.contains("tests/discount.rs is not a registered cargo test target"));
        assert!(!line.contains("cargo test --"));
        let unresolved = run_line(None, true, Some(&TargetLookup::Unresolved), "tests/d.rs");
        assert!(unresolved.contains("is not established"));
        assert!(!unresolved.contains("not a registered"));
        assert_eq!(
            run_line(Some("cargo test --test x t"), true, None, "tests/x.rs"),
            "Run it: cargo test --test x t"
        );
        assert!(run_line(None, false, None, "tests/x.rs").contains("no Cargo package owns"));
    }
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

/// The human refusal text. A refused nearest gap still names the seam, so the
/// `--at` substitution note must travel with it or the line the user named
/// is swapped for another silently (#6298).
fn refusal_message(seam_id: &str, kind: &str, reason: &str, at_note: Option<&str>) -> String {
    let mut message = format!("agent stub: no test stub for seam {seam_id} ({kind}): {reason}");
    if let Some(note) = at_note {
        message.push_str(&format!(". {note}"));
    }
    message
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
    fn human_refusal_carries_the_at_substitution_note() {
        let plain = refusal_message("seam:1", "observer_required", "needs an observer", None);
        assert_eq!(
            plain,
            "agent stub: no test stub for seam seam:1 (observer_required): needs an observer"
        );
        let noted = refusal_message(
            "seam:1",
            "observer_required",
            "needs an observer",
            Some("note: src/lib.rs:8 has no stubbable gap"),
        );
        assert_eq!(
            noted,
            "agent stub: no test stub for seam seam:1 (observer_required): needs an observer. note: src/lib.rs:8 has no stubbable gap"
        );
    }

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

    /// #6712: `.cargo/config.toml` redirects `TMPDIR` at `<workspace>/target`,
    /// which is absent in a fresh worktree whose build output lives elsewhere.
    fn scratch_crate(label: &str, source: &str) -> Result<std::path::PathBuf, String> {
        let parent = std::env::temp_dir();
        std::fs::create_dir_all(&parent).map_err(|error| error.to_string())?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let root = parent.join(format!(
            "ripr-agent-stub-at-{label}-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
        std::fs::write(
            root.join("Cargo.toml"),
            format!("[package]\nname = \"{label}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n"),
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(root.join("src/lib.rs"), source).map_err(|error| error.to_string())?;
        Ok(root)
    }

    fn stub_at(root: &std::path::Path, at: &str, kind: &str) -> Result<(), CommandError> {
        let selector = TestStubSelector::parse_at(at)
            .and_then(|selector| selector.with_kind(kind))
            .map_err(CommandError::Failure)?;
        run_agent_stub(AgentStubOptions {
            root: root.to_path_buf(),
            root_explicit: true,
            selector,
            write: false,
            json: false,
        })
    }

    /// Same fixture as `at_kind_stub_discloses_a_strongly_gripped_inventory_class`
    /// in `app/test_stub.rs`: the comparison on line 2 is already pinned. Line 3
    /// is the function's closing brace, so `--at` substitutes. The #8364 test
    /// covers `return_value` through `resolve_test_stub`; this one is the CLI
    /// adapter and a different family (`predicate`).
    const GRIPPED_PREDICATE: &str = "pub fn clamp_to(x: i32) -> i32 {\n    if x >= 100 { 100 } else { x }\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn at_boundary_clamps() {\n        assert_eq!(clamp_to(100), 100);\n        assert_eq!(clamp_to(99), 99);\n        assert_eq!(clamp_to(500), 100);\n    }\n}\n";

    const ASYNC_PREDICATE: &str =
        "pub async fn clamp_to(x: i32) -> i32 {\n    if x >= 100 { 100 } else { x }\n}\n";

    /// #8444 item 3: an off-line `--at` whose nearest gap is stubbable still
    /// returns a ready stub through the CLI adapter, not a selector miss.
    /// Rendered warning *order* is pinned by `crates/ripr/tests/agent_stub_at_disclosure.rs`
    /// because this command writes the note with `println!`.
    #[test]
    fn off_line_predicate_at_on_a_gripped_gap_stays_ready() -> Result<(), String> {
        let root = scratch_crate("gripped-predicate", GRIPPED_PREDICATE)?;
        let result = stub_at(&root, "src/lib.rs:3", "predicate");
        let _ = std::fs::remove_dir_all(&root);
        result.map_err(|error| {
            format!("off-line --at must still stub the nearest predicate: {error}")
        })
    }

    /// #8444 item 3: an off-line `--at` whose nearest gap then refuses is a
    /// post-resolution refusal for that seam, not `seam_not_found`. The
    /// substitution note is not in this `Decision` string (human stdout stays
    /// empty on refusal); the JSON envelope pin lives in
    /// `crates/ripr/tests/agent_stub_at_disclosure.rs`.
    #[test]
    fn off_line_at_that_ends_in_a_refusal_names_the_nearest_seam() -> Result<(), String> {
        let root = scratch_crate("async-predicate", ASYNC_PREDICATE)?;
        let result = stub_at(&root, "src/lib.rs:3", "predicate");
        let _ = std::fs::remove_dir_all(&root);
        let Err(CommandError::Decision(message)) = result else {
            return Err(format!(
                "substituted async owner must be a post-resolution refusal, got {result:?}"
            ));
        };
        if !message.contains("no test stub for seam") || !message.contains("owner_async") {
            return Err(format!(
                "refusal must name the nearest seam's producer reason, got {message}"
            ));
        }
        if message.contains("no predicate seam ripr can stub is in the function") {
            return Err(format!(
                "off-line --at must not fall through to a selector miss: {message}"
            ));
        }
        Ok(())
    }
}

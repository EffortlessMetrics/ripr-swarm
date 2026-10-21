//! Production producer for the compact repair-card handoff (RIPR-SPEC-0194,
//! #4667): `ripr agent card` assembles the same typed authorities the packet
//! and editor surfaces consume — seam inventory, repair-packet eligibility,
//! the check finding set and its witness, the recommended-test/edit-cage
//! derivation, the repair-attempt inventory, and the typed command catalog —
//! into one bounded [`RepairCardV1`]. Nothing is re-derived and no
//! classification vocabulary is invented here.
//!
//! The complete canonical packet stays behind the card's explicit
//! `canonical_packet` detail route (`ripr agent packet --seam-id ... --json`);
//! the packet render is unchanged and remains the compatibility path.
//!
//! #5007 (RIPR-SPEC-0202): producer failures are split at their origin into
//! [`AgentCardError::Refusal`] — a deliberate named refusal the CLI types
//! into the versioned `agent_card_refusal` envelope (exit code 3) — and
//! [`AgentCardError::Operational`] — a could-not-complete that stays exit 2.
//! The human prose is unchanged in both cases; only the machine state is new.

use std::path::Path;

use crate::agent::artifact::git_output;
use crate::agent::command_specs::{AgentArtifactRoute, agent_inspection_command_spec};
use crate::agent::loop_commands::{bound_root, shell_arg};
use crate::analysis::ClassifiedSeam;
use crate::analysis::RepoSeam;
use crate::analysis::repair_route::{
    RepairRouteReadiness, RepairTargetSelection, repair_packet_eligibility,
};
use crate::config::RiprConfig;
use crate::domain::{
    AgentCardRefusalKind, CardCurrentnessGoal, CommandSpec, DiagnosticWitness, EditCageGoal,
    FindingCanonicalGap, FixInstructionSummary, FocusedExecutionGoal, MutationConfirmationGoal,
    RepairCardBudget, RepairCardDetailFamily, RepairCardDetailState, RepairCardDoneWhen,
    RepairCardSnapshot, RepairCardSnapshotCurrentness, RepairCardSubject, RepairCardV1,
    StaticMovementGoal, repair_card_route_exposable,
};
use crate::output::agent_seam_packets::{
    EDIT_CAGE_PRODUCTION_STATEMENT, EDIT_CAGE_TERMINALITY_WARNING, PacketCommandContext,
    TASK_WRITE_TARGETED_TEST, inline_test_module_edit_statement, recommended_inline_test_module,
    recommended_test_for, render_agent_seam_packet_json_with_context, task_for,
};
use crate::output::path::display_path;
use crate::repair_card_budget::RepairCardDetailSource;

use super::repair_attempt::{
    RepairAttemptInventoryEntry, RepairAttemptManifest, inventory_repair_attempts,
};
use super::repair_card::{RepairCardInput, build_repair_card};
use super::{CheckInput, check_workspace_with_config};

/// Producer failure of the `ripr agent card` handoff, split into the two
/// machine states an orchestrator branches on (#5007, RIPR-SPEC-0202):
/// a deliberate named refusal (exit code 3; the CLI renders the versioned
/// typed envelope naming the kind and the remedy) and an operational
/// could-not-complete (exit code 2; human prose only, retrying differently
/// is appropriate). The human message is the non-authority rendering in both
/// cases: the kind is the contract, the prose stays free to change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AgentCardError {
    /// A deliberate named refusal: the command ran to a blocking answer.
    Refusal {
        /// The typed refusal kind; the envelope's `error.kind` wire spelling.
        kind: AgentCardRefusalKind,
        /// The exact human prose this refusal rendered before typing, kept
        /// verbatim so stderr stays unchanged for humans.
        message: String,
    },
    /// The command could not complete (operational, usage, or internal).
    Operational(String),
}

impl AgentCardError {
    /// A deliberate named refusal with the typed kind the CLI envelope pins.
    pub(crate) fn refusal(kind: AgentCardRefusalKind, message: String) -> Self {
        Self::Refusal { kind, message }
    }

    fn witness_unavailable(message: String) -> Self {
        Self::refusal(AgentCardRefusalKind::WitnessUnavailable, message)
    }

    fn identity_unnameable(message: String) -> Self {
        Self::refusal(AgentCardRefusalKind::IdentityUnnameable, message)
    }

    fn budget_overflow(message: String) -> Self {
        Self::refusal(AgentCardRefusalKind::BudgetOverflow, message)
    }

    pub(crate) fn operational(message: String) -> Self {
        Self::Operational(message)
    }

    /// The human-readable message, reported on stderr unchanged.
    fn message(&self) -> &str {
        match self {
            Self::Refusal { message, .. } | Self::Operational(message) => message,
        }
    }
}

impl std::fmt::Display for AgentCardError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.message())
    }
}

impl From<AgentCardError> for String {
    fn from(error: AgentCardError) -> Self {
        error.to_string()
    }
}

/// Producer-gathered facts for one seam's repair card. The pure assembly
/// [`assemble_repair_card`] takes these directly so the projection contract is
/// unit-testable without git or analysis IO; [`repair_card_for_entry`] gathers
/// them from the live authorities.
pub(crate) struct SeamCardFacts<'a> {
    pub(crate) entry: &'a ClassifiedSeam,
    /// The witness of the check finding that names this seam's canonical gap,
    /// when one exists in the current analysis.
    pub(crate) witness: Option<&'a DiagnosticWitness>,
    /// The finding ID the witness was projected from (the `ripr explain`
    /// route names it).
    pub(crate) finding_id: Option<&'a str>,
    /// The canonical gap id of the bound finding, exactly as `ripr check`,
    /// `ripr_list_gaps` and `ripr_get_gap` print it. The card reports this id
    /// and never re-mints one from the seam: a finding can key its id from
    /// text the seam does not carry (a whole changed line), so only the
    /// bound finding's own id is byte-equal across surfaces (#8638). `None`
    /// when no finding binds the seam.
    pub(crate) finding_gap_id: Option<&'a str>,
    /// The latest recorded repair attempt for this seam, when one exists.
    pub(crate) attempt: Option<&'a RepairAttemptManifest>,
    /// The rendered complete canonical packet envelope this card routes to.
    pub(crate) packet_json: &'a str,
    pub(crate) repository_head: &'a str,
    pub(crate) workspace_identity: &'a str,
    /// Producer-observed working-tree currentness for the files this card's
    /// evidence binds: `Current` on a clean scope, `AcceptedDirtyDraft` when
    /// an uncommitted change touches them. Both currentness axes project this
    /// fact verbatim; the assembly never mints an unchecked `current` claim.
    pub(crate) currentness: RepairCardSnapshotCurrentness,
    /// The typed inspection route (`ripr agent packet ... --json`) offered as
    /// the card's one next action when the route gate is open.
    pub(crate) next_command: Option<CommandSpec>,
}

/// Gather the live authorities for one classified seam and assemble its card.
pub(crate) fn repair_card_for_entry(
    entry: &ClassifiedSeam,
    root: &Path,
    config: &RiprConfig,
    candidate_seams: &[ClassifiedSeam],
) -> Result<RepairCardV1, AgentCardError> {
    let eligibility = repair_packet_eligibility(entry);
    let witness_pack = witness_for_seam(
        root,
        config,
        entry,
        eligibility.readiness.canonical_gap_id.as_deref(),
        candidate_seams,
    )
    .map_err(AgentCardError::witness_unavailable)?;
    let (witness_pack, finding_gap_id) = witness_pack;
    let (finding_id, witness) = match witness_pack {
        Some((id, witness)) => (Some(id), Some(witness)),
        None => (None, None),
    };
    let repository_head = git_output(root, &["rev-parse", "HEAD"])
        .map_err(|error| {
            AgentCardError::operational(format!(
                "agent card could not resolve the repository head: {error}"
            ))
        })?
        .trim()
        .to_string();
    let seam_id = entry.seam.id().as_str().to_string();
    // #5007: an unnameable portable identity is a deliberate named refusal
    // (the remedy is the full packet route), not an operational failure. The
    // CLI prints this refusal to a user who may paste it from any directory,
    // so its packet route binds the selected root (#3999).
    let workspace_identity = workspace_identity_naming_route(
        entry,
        &eligibility.readiness,
        &bound_packet_command(root, &seam_id),
    )
    .map_err(AgentCardError::identity_unnameable)?;
    let attempt = latest_attempt_for_seam(root, &seam_id).map_err(AgentCardError::operational)?;
    let currentness =
        evidence_tree_currentness(root, entry).map_err(AgentCardError::operational)?;
    let packet_json = card_packet_json(entry);
    // The typed args stay portable (`--root .`); only the display a CLI user
    // pastes binds the selected root (#3999). Display never enters identity.
    let next_command = agent_inspection_command_spec(
        AgentArtifactRoute::Packet,
        &bound_root(&root.to_string_lossy()),
        &seam_id,
    );

    assemble_repair_card(&SeamCardFacts {
        entry,
        witness: witness.as_ref(),
        finding_id: finding_id.as_deref(),
        finding_gap_id: finding_gap_id.as_deref(),
        attempt: attempt.as_ref(),
        packet_json: &packet_json,
        repository_head: &repository_head,
        workspace_identity: &workspace_identity,
        currentness,
        next_command: Some(next_command),
    })
}

/// Render the canonical-packet envelope whose bytes feed the card's
/// `canonical_packet` content digest. The content-bound render always uses
/// the portable root (`.`), never the bound checkout spelling: the packet's
/// `next`-block command strings embed that root, and `detail_digest` /
/// `complete_evidence_digest` are load-bearing identity — equivalent roots
/// must hash identically (RIPR-SPEC-0192 authority 4; the #3166
/// equivalent-roots acceptance). The human-facing packet command keeps
/// binding the concrete root (#4000's display contract is untouched:
/// display is presentation and never enters identity).
pub(crate) fn card_packet_json(entry: &ClassifiedSeam) -> String {
    render_agent_seam_packet_json_with_context(
        entry,
        PacketCommandContext::Standalone { root: "." },
    )
}

/// One bounded dirty-state probe over the files this card's evidence binds:
/// the seam file (the forbidden edit surface), every related-test file (the
/// related-test-candidates evidence family binds them all), and, when the
/// packet task is the targeted-test task, the recommended test file (the
/// allowed edit surface). Any uncommitted change in that scope — tracked
/// modification, untracked draft, or ignored edit, all of which the analysis
/// reads from working-tree bytes while `snapshot.repository_head` names the
/// committed head — projects `AcceptedDirtyDraft`. A clean scope projects
/// `Current`. A probe failure fails closed: the gather errors, no card is
/// minted, and no unchecked `current` claim escapes.
pub(crate) fn evidence_tree_currentness(
    root: &Path,
    entry: &ClassifiedSeam,
) -> Result<RepairCardSnapshotCurrentness, String> {
    Ok(evidence_tree_currentness_batch(root, &[entry])?.remove(0))
}

/// Upper bound on pathspecs per `git status` child: a binding set with
/// thousands of evidence paths chunks into several children instead of
/// growing one argv without bound.
const CURRENTNESS_PROBE_PATHS_PER_CHILD: usize = 180;

/// Command-line budget per `git status` child, in UTF-16 units of pathspec
/// text: the count bound alone does not bound argv length, and Windows
/// command lines end near 32k units once the executable, flags and quoting
/// join the line. A single path longer than the whole budget still goes
/// through alone (git fails it on its own merits; it cannot be split).
const CURRENTNESS_PROBE_PATH_BUDGET_UTF16_UNITS: usize = 24_000;

/// #7292 test seam: children spawned by the batched currentness probe.
#[cfg(test)]
pub(crate) static CURRENTNESS_PROBE_CHILDREN: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// One working-tree probe for MANY seams: every binding's evidence scope
/// shares one (chunked) `git status --porcelain --ignored` child instead of
/// one child per seam. The MCP refresh binds card producers through this
/// authority after its analysis, and ce154a67c made those bindings actually
/// match — which turned the previously dead per-seam probe live on exactly
/// the refresh path: one mid-corpus refresh paid twenty sequential `git
/// status` children for twenty bound seams (#7292). The per-entry outcome is
/// the single-entry probe's own: the entry's scope is dirty exactly when the
/// status reports any of that entry's paths, so the batch keeps the
/// single-entry contract while paying a fraction of the children.
pub(crate) fn evidence_tree_currentness_batch(
    root: &Path,
    entries: &[&ClassifiedSeam],
) -> Result<Vec<RepairCardSnapshotCurrentness>, String> {
    if entries.is_empty() {
        return Ok(Vec::new());
    }
    let probe_paths: Vec<Vec<std::path::PathBuf>> = entries
        .iter()
        .map(|entry| evidence_probe_paths(entry))
        .collect();
    // The union of every entry's evidence paths, deduplicated: one status
    // child answers for all of them, and each entry reads only its own
    // paths back out of the record set. The pathspec keeps each path's own
    // spelling — a literal backslash in a filename is real data, not a
    // separator — and normalization happens only for record comparison.
    let mut union: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for paths in &probe_paths {
        for path in paths {
            if seen.insert(path.to_string_lossy().into_owned()) {
                union.push(path.to_string_lossy().into_owned());
            }
        }
    }
    let mut dirty: std::collections::HashSet<String> = std::collections::HashSet::new();
    for chunk in currentness_probe_chunks(&union) {
        probe_paths_dirty(root, &chunk, &mut dirty)?;
    }
    let scope_is_dirty = |paths: &[std::path::PathBuf]| {
        paths.iter().any(|path| {
            let spelling = currentness_comparison_key(&path.to_string_lossy());
            dirty.contains(&spelling)
        })
    };
    Ok(probe_paths
        .into_iter()
        .map(|paths| {
            if scope_is_dirty(&paths) {
                RepairCardSnapshotCurrentness::AcceptedDirtyDraft
            } else {
                RepairCardSnapshotCurrentness::Current
            }
        })
        .collect())
}

/// Split the union pathspec list into per-child argument lists bounded by
/// both count and command-line length: Windows child command lines end near
/// 32k UTF-16 units, so a fixed path count is not enough when evidence
/// paths are long.
fn currentness_probe_chunks(union: &[String]) -> Vec<Vec<String>> {
    let mut chunks = Vec::new();
    let mut chunk: Vec<String> = Vec::new();
    let mut units = 0usize;
    for path in union {
        let path_units = path.encode_utf16().count() + 1;
        let over_count = chunk.len() >= CURRENTNESS_PROBE_PATHS_PER_CHILD;
        let over_length = units + path_units > CURRENTNESS_PROBE_PATH_BUDGET_UTF16_UNITS;
        if !chunk.is_empty() && (over_count || over_length) {
            chunks.push(std::mem::take(&mut chunk));
            units = 0;
        }
        units += path_units;
        chunk.push(path.clone());
    }
    if !chunk.is_empty() {
        chunks.push(chunk);
    }
    chunks
}

/// Run one `git status --porcelain -z --ignored` over `paths` and record
/// every reported path as dirty. The `-z` form never quotes paths, and a
/// renamed or copied entry carries the original path as one more NUL field;
/// both spellings are evidence scope.
fn probe_paths_dirty(
    root: &Path,
    paths: &[String],
    dirty: &mut std::collections::HashSet<String>,
) -> Result<(), String> {
    let mut args: Vec<String> = vec![
        // Evidence filenames are literal paths, never patterns: a seam file
        // named `foo*bar.rs` must not glob-match unrelated files.
        "--literal-pathspecs".to_string(),
        "status".to_string(),
        "--porcelain".to_string(),
        "-z".to_string(),
        "--ignored".to_string(),
        "--".to_string(),
    ];
    args.extend(paths.iter().cloned());
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    #[cfg(test)]
    CURRENTNESS_PROBE_CHILDREN.fetch_add(1, std::sync::atomic::Ordering::Release);
    let output = git_output(root, &arg_refs)
        .map_err(|error| format!("agent card could not probe the working-tree state: {error}"))?;
    // A truncated -z stream would silently lose the tail records and read
    // dirty scopes clean, so a nonempty body that does not end on the record
    // terminator fails closed instead.
    if !output.is_empty() && !output.ends_with('\0') {
        return Err(
            "agent card could not probe the working-tree state: git status output was truncated"
                .to_string(),
        );
    }
    let mut records = output
        .split('\0')
        .filter(|record| !record.is_empty())
        .peekable();
    while let Some(record) = records.next() {
        // Porcelain v1: two status columns, one space, then the path. The
        // first column is a space for unstaged-only changes, so the record
        // splits on its columns, never on the first space.
        let bytes = record.as_bytes();
        if bytes.len() < 4 || bytes[2] != b' ' {
            continue;
        }
        let status = &record[..2];
        let path = &record[3..];
        dirty.insert(currentness_comparison_key(path));
        if status.contains('R') || status.contains('C') {
            // The rename's other path arrives as the next NUL field, unquoted
            // and unconditional in the -z format; a missing field means a
            // truncated stream, which the terminator guard above already
            // refused.
            if let Some(original) = records.next() {
                dirty.insert(currentness_comparison_key(original));
            }
        }
    }
    Ok(())
}

/// Compare a git record path and an evidence spelling in one namespace. On
/// Windows the seam spelling and git's record spelling may disagree about
/// separators, so both sides normalize; on Unix a backslash is a filename
/// byte and two spellings that differ are different files.
fn currentness_comparison_key(path: &str) -> String {
    if cfg!(windows) {
        path.replace('\\', "/")
    } else {
        path.to_string()
    }
}

/// The probe pathspecs in their filesystem spelling. `display_path` is a
/// presentation renderer (it percent-encodes `%` and rewrites separators);
/// feeding a display spelling to git would probe a path that does not exist
/// and silently read a dirty scope as clean.
fn evidence_probe_paths(entry: &ClassifiedSeam) -> Vec<std::path::PathBuf> {
    let mut paths: Vec<std::path::PathBuf> = vec![entry.seam.file().to_path_buf()];
    for related in &entry.evidence.related_tests {
        if !paths.contains(&related.file) {
            paths.push(related.file.clone());
        }
    }
    if task_for(entry) == TASK_WRITE_TARGETED_TEST {
        let recommended = recommended_test_for(entry);
        if recommended.file != "not_applicable" {
            let recommended_path = std::path::PathBuf::from(&recommended.file);
            if !paths.contains(&recommended_path) {
                paths.push(recommended_path);
            }
        }
    }
    paths
}

/// Assemble one [`RepairCardV1`] from producer-owned facts. Pure projection:
/// every field names the authority it was copied from, the route gate stays
/// fail-closed, and the complete packet rides behind its detail reference.
/// Builder, route-gate, and budget refusals surface as the
/// [`AgentCardRefusalKind::BudgetOverflow`] refusal kind; detail-source
/// serialization failures stay operational (#5007).
pub(crate) fn assemble_repair_card(
    facts: &SeamCardFacts<'_>,
) -> Result<RepairCardV1, AgentCardError> {
    let entry = facts.entry;
    let eligibility = repair_packet_eligibility(entry);
    let readiness = &eligibility.readiness;
    let instruction = facts
        .witness
        .map(FixInstructionSummary::from_witness)
        .unwrap_or_else(FixInstructionSummary::unavailable);

    // Edit cage: the exact derivation the seam packet states (#4330), so the
    // card cannot promise a different edit surface than the attempt enforces.
    let actionable = task_for(entry) == TASK_WRITE_TARGETED_TEST;
    let recommended = recommended_test_for(entry);
    // Admission is a ceiling on a statically ready route. A seam already
    // blocked by missing/static evidence has no edit to admit; retain that
    // more relevant diagnosis instead of replacing it with a cage error.
    let edit_cage_refusal = if readiness.is_repair_ready() {
        super::repair_attempt::edit_cage_policy_from_packet(
            facts.packet_json,
            entry.seam.id().as_str(),
        )
        .err()
    } else {
        None
    };
    let production_file = display_path(entry.seam.file());
    let allowed_files: Vec<String> = if actionable && recommended.file != "not_applicable" {
        vec![recommended.file.clone()]
    } else {
        Vec::new()
    };
    let forbidden_files: Vec<String> = if allowed_files
        .first()
        .is_some_and(|allowed| allowed == &production_file)
    {
        Vec::new()
    } else {
        vec![production_file]
    };

    let done_when = RepairCardDoneWhen {
        static_movement: StaticMovementGoal::ClosedBySelectedRoute,
        focused_test_execution: if actionable {
            FocusedExecutionGoal::VerifiedPass
        } else {
            FocusedExecutionGoal::ExplicitlyNotRun
        },
        edit_cage: EditCageGoal::Compliant,
        mutation_confirmation: MutationConfirmationGoal::NotRequested,
        // Both currentness axes name the one producer-observed tree fact; a
        // dirty evidence scope stays an accepted dirty draft, never a silent
        // `current` (#5008).
        currentness: match facts.currentness {
            RepairCardSnapshotCurrentness::Current => CardCurrentnessGoal::Current,
            RepairCardSnapshotCurrentness::AcceptedDirtyDraft => {
                CardCurrentnessGoal::AcceptedDirtyDraft
            }
        },
    };
    let mut stop_conditions = vec![EDIT_CAGE_PRODUCTION_STATEMENT.to_string()];
    if !allowed_files.is_empty() {
        stop_conditions.push(format!(
            "{EDIT_CAGE_TERMINALITY_WARNING} (allowed: {})",
            allowed_files.join(", ")
        ));
    }
    // A production file is allowed only as its inline test module (#5210);
    // the card states that confinement in the packet's own words, so a
    // compact handoff never reads as file-level permission.
    if actionable && let Some(module) = recommended_inline_test_module(entry, &recommended.file) {
        stop_conditions.push(inline_test_module_edit_statement(&recommended.file, module));
    }

    let changed_behavior = facts
        .witness
        .map(|witness| witness.changed_expression.trim())
        .filter(|expression| !expression.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| entry.seam.expression().to_string());
    let exact_blocker = facts
        .witness
        .and_then(|witness| witness.missing_discriminators.first())
        .map(|fact| format!("{}: {}", fact.value, fact.reason));
    let has_observer_setup = facts
        .witness
        .is_some_and(|witness| witness.expected_sink.is_some());
    let assertion_goal_detail = facts
        .witness
        .and_then(|witness| witness.suggested_assertion.clone());
    let limitations: Vec<String> = facts
        .witness
        .map(|witness| {
            witness
                .limitations
                .iter()
                .map(|limitation| limitation.detail.trim())
                .filter(|detail| !detail.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    // The card never presents a runnable route the shared instruction
    // vocabulary and repair-route readiness gate would not expose.
    let route_exposed = eligibility.eligible()
        && edit_cage_refusal.is_none()
        && repair_card_route_exposable(instruction.state, readiness.is_repair_ready());
    let next_command = if route_exposed {
        facts.next_command.as_ref()
    } else {
        None
    };

    let seam_id = entry.seam.id().as_str().to_string();
    let packet_route = format!("ripr agent packet --seam-id {} --json", shell_arg(&seam_id));
    let detail_sources = detail_sources_for(facts, &packet_route, &done_when)
        .map_err(AgentCardError::operational)?;

    // #5007: a builder, route-gate, or budget refusal is a deliberate named
    // refusal (the remedy is the canonical packet route), never an internal
    // error; the builder's prose travels verbatim as the human rendering.
    build_repair_card(&RepairCardInput {
        snapshot: RepairCardSnapshot {
            workspace_identity: facts.workspace_identity.to_string(),
            repository_head: facts.repository_head.to_string(),
            currentness: facts.currentness,
        },
        subject: RepairCardSubject {
            seam_id,
            // #8638: the card reports the bound finding's own canonical gap
            // id, the readable id `ripr check`, `ripr_list_gaps` and
            // `ripr_get_gap` use. It is never re-minted from the seam (a
            // finding can key its id from a whole changed line the seam does
            // not carry), and fails closed to null when no finding binds the
            // seam. The seam-side content hash stays the internal join key
            // and rides additively under its own name.
            canonical_gap_id: facts.finding_gap_id.map(str::to_string),
            seam_content_gap_id: readiness.canonical_gap_id.clone(),
            finding_id: facts.finding_id.map(str::to_string),
        },
        instruction: &instruction,
        readiness,
        changed_behavior,
        exact_blocker,
        has_observer_setup,
        assertion_goal_detail,
        candidate_value: None,
        packet_eligible: eligibility.eligible(),
        edit_cage_refusal,
        next_command,
        packet_route,
        allowed_files,
        forbidden_files,
        done_when,
        stop_conditions,
        selected_basis: None,
        rejected_alternatives: Vec::new(),
        attempt: facts.attempt,
        limitations,
        detail_sources,
        budget: RepairCardBudget::default(),
    })
    .map_err(AgentCardError::budget_overflow)
}

/// Whether the rendered packet envelope actually surfaces this seam. The
/// envelope's `packets` array honors the shared queue policy; an entry whose
/// `seam_id` names this seam is the only current-surface proof.
fn packet_surfaces_seam(packet_content: &serde_json::Value, seam_id: &str) -> bool {
    packet_content
        .get("packets")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|packets| {
            packets.iter().any(|packet| {
                packet.get("seam_id").and_then(serde_json::Value::as_str) == Some(seam_id)
            })
        })
}

/// Every load-bearing evidence family rides behind a typed reference. The
/// canonical packet and the static-movement axis name the packet route; the
/// witness-derived families name the `ripr explain` route for the same
/// finding; families with no producer on this surface record an exact
/// unavailable reason instead of inventing content.
fn detail_sources_for(
    facts: &SeamCardFacts<'_>,
    packet_route: &str,
    done_when: &RepairCardDoneWhen,
) -> Result<Vec<RepairCardDetailSource>, String> {
    let packet_content: serde_json::Value =
        serde_json::from_str(facts.packet_json).map_err(|error| {
            format!("agent card could not bind the canonical packet content: {error}")
        })?;
    // The packet envelope honors the shared queue policy: a seam the queue
    // does not surface renders with no matching entry in `packets`. Crediting
    // the card with a current packet route for a seam the packet itself omits
    // would overstate the evidence, so the family records an exact unavailable
    // reason instead (fail-closed, like every other omitted family).
    let mut sources = Vec::new();
    if packet_surfaces_seam(&packet_content, facts.entry.seam.id().as_str()) {
        sources.push(RepairCardDetailSource::current(
            RepairCardDetailFamily::CanonicalPacket,
            packet_route,
            packet_content,
        ));
    } else {
        sources.push(RepairCardDetailSource::unavailable(
            RepairCardDetailFamily::CanonicalPacket,
            "the canonical packet does not surface this seam under the current packet queue policy",
        ));
    }
    match (facts.witness, facts.finding_id) {
        (Some(witness), Some(finding_id)) => {
            // A finding id embeds the raw file path, so a path with a space,
            // apostrophe or `;` must stay one argument when pasted.
            let explain_route = format!("ripr explain {}", shell_arg(finding_id));
            sources.push(RepairCardDetailSource::current(
                RepairCardDetailFamily::FixInstruction,
                &explain_route,
                serde_json::to_value(witness)
                    .map_err(|error| format!("agent card witness content failed: {error}"))?,
            ));
            sources.push(RepairCardDetailSource::current(
                RepairCardDetailFamily::WitnessStageEvidence,
                "ripr check --json",
                serde_json::to_value(&facts.entry.evidence).map_err(|error| {
                    format!("agent card stage-evidence content failed: {error}")
                })?,
            ));
            sources.push(RepairCardDetailSource::current(
                RepairCardDetailFamily::LimitationDetail,
                &explain_route,
                serde_json::to_value(&witness.limitations)
                    .map_err(|error| format!("agent card limitation content failed: {error}"))?,
            ));
        }
        _ => {
            let reason = "no witness-producing finding names this seam in the current analysis";
            sources.push(RepairCardDetailSource::unavailable(
                RepairCardDetailFamily::FixInstruction,
                reason,
            ));
            sources.push(RepairCardDetailSource::unavailable(
                RepairCardDetailFamily::WitnessStageEvidence,
                reason,
            ));
            sources.push(RepairCardDetailSource::unavailable(
                RepairCardDetailFamily::LimitationDetail,
                reason,
            ));
        }
    }
    sources.push(RepairCardDetailSource::current(
        RepairCardDetailFamily::RelatedTestCandidates,
        packet_route,
        serde_json::to_value(&facts.entry.evidence.related_tests)
            .map_err(|error| format!("agent card related-test content failed: {error}"))?,
    ));
    sources.push(RepairCardDetailSource::current(
        RepairCardDetailFamily::StaticMovement,
        packet_route,
        serde_json::to_value(done_when)
            .map_err(|error| format!("agent card done-when content failed: {error}"))?,
    ));
    match facts.attempt {
        Some(manifest) => {
            // The attempt manifest is bound to the repository head it was
            // recorded at; an attempt recorded against another head is stale
            // evidence for this snapshot, not current status.
            let mut source = RepairCardDetailSource::current(
                RepairCardDetailFamily::RepairAttemptStatus,
                "ripr agent status --json",
                serde_json::to_value(manifest)
                    .map_err(|error| format!("agent card attempt content failed: {error}"))?,
            );
            if manifest.repository_head != facts.repository_head {
                source.state = RepairCardDetailState::Stale;
            }
            sources.push(source);
        }
        None => sources.push(RepairCardDetailSource::unavailable(
            RepairCardDetailFamily::RepairAttemptStatus,
            "no repair attempt is recorded for this seam",
        )),
    }
    sources.push(RepairCardDetailSource::unavailable(
        RepairCardDetailFamily::FocusedProofReceipt,
        "focused-proof receipts are written by the repair journey; none is recorded for this seam",
    ));
    sources.push(RepairCardDetailSource::unavailable(
        RepairCardDetailFamily::MutationCalibration,
        "mutation calibration is optional and never required to close a card; none is produced for this seam",
    ));
    Ok(sources)
}

/// Run the check pipeline and project the witness for the finding that names
/// this seam's canonical gap, when one exists. The witness is the single
/// instruction authority (`FixInstructionSummary::from_witness`); no second
/// derivation happens here.
fn witness_for_seam(
    root: &Path,
    config: &RiprConfig,
    entry: &ClassifiedSeam,
    canonical_gap_id: Option<&str>,
    candidate_seams: &[ClassifiedSeam],
) -> Result<(Option<(String, DiagnosticWitness)>, Option<String>), String> {
    // The expensive workspace check only feeds the canonical-gap match; with
    // no gap id the match cannot bind, so skip the analysis entirely.
    if canonical_gap_id.is_none() {
        return Ok((None, None));
    }
    let output = check_workspace_with_config(
        CheckInput {
            root: root.to_path_buf(),
            git_timeout: Some(super::default_cli_git_timeout()),
            ..Default::default()
        },
        config,
    )
    .map_err(|error| format!("agent card could not run the witness analysis: {error}"))?;
    Ok((
        witness_from_findings(&output.findings, entry, canonical_gap_id, candidate_seams),
        bound_finding_gap_id(&output.findings, entry, canonical_gap_id, candidate_seams),
    ))
}

/// Select the finding that names this seam's canonical gap from an
/// already-completed finding set, without projecting a witness. The
/// owner-match rule is the single seam↔finding binding authority: consumers
/// that already hold a completed analysis (the LSP snapshot, the MCP
/// snapshot) bind the finding here instead of re-running the check pipeline,
/// and no consumer may re-derive the match.
///
/// Two branches, in order. The content-hash branch binds a finding whose gap
/// id is this seam's own canonical gap id with this seam's owner (exact
/// identity; first match wins and ends the search). The producer branch
/// binds a finding whose producer-structured gap (file, owner, probe family)
/// names this seam — the identity producers actually emit on findings, which
/// never carry the content-hash id (#7162) — and only when this seam is the
/// uniquely most-specific claimant among `candidate_seams` (#7179): sibling
/// findings that share the shape stay ambiguous and bind nothing, enclosing
/// outer seams yield to more-specific inner spans, and competing minimal
/// claimants (same-point twins, identical spans, spanless twins, or crossing
/// spans) stay ambiguous rather than crediting the wrong seam.
///
/// Binding is not projection: a bound finding does not always project a
/// witness (a fully exposed finding is deliberately never relabeled as a
/// gap), while the binding itself still holds — the MCP card binds its seam
/// to its item exactly where the CLI binds its optional witness.
pub(crate) fn finding_for_seam<'a>(
    findings: &'a [crate::domain::Finding],
    entry: &ClassifiedSeam,
    canonical_gap_id: Option<&str>,
    candidate_seams: &[ClassifiedSeam],
) -> Option<&'a crate::domain::Finding> {
    // The content-hash id is the headline-eligibility ticket, not the finding
    // locator: only seams that carry one bind a finding, so the expensive
    // analysis skip in `witness_for_seam` and the non-headline no-witness
    // posture both hold on every branch.
    let gap_id = canonical_gap_id?;
    if let Some(hit) = findings.iter().find(|finding| {
        finding
            .canonical_gap
            .as_ref()
            .is_some_and(|gap| gap_names_seam(gap, gap_id, entry.seam.owner()))
    }) {
        return Some(hit);
    }
    let mut matches = findings.iter().filter(|finding| {
        finding
            .canonical_gap
            .as_ref()
            .is_some_and(|gap| producer_candidate_qualifies(entry, finding, gap, candidate_seams))
    });
    let hit = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    Some(hit)
}

/// The canonical gap id of the finding the shared [`finding_for_seam`] join
/// binds to this seam, as `ripr check` / `ripr_list_gaps` print it. Binding is
/// not projection: this is present for a bound finding that projects no
/// witness too.
pub(crate) fn bound_finding_gap_id(
    findings: &[crate::domain::Finding],
    entry: &ClassifiedSeam,
    canonical_gap_id: Option<&str>,
    candidate_seams: &[ClassifiedSeam],
) -> Option<String> {
    finding_for_seam(findings, entry, canonical_gap_id, candidate_seams)
        .and_then(|finding| finding.canonical_gap.as_ref().map(|gap| gap.id.clone()))
}

/// Project the witness for the finding that names this seam's canonical gap
/// from an already-completed finding set: the shared [`finding_for_seam`]
/// join, then the witness projection. Projection can still refuse a bound
/// finding (a fully exposed finding projects no witness); the card then
/// carries no witness — never another finding's.
pub(crate) fn witness_from_findings(
    findings: &[crate::domain::Finding],
    entry: &ClassifiedSeam,
    canonical_gap_id: Option<&str>,
    candidate_seams: &[ClassifiedSeam],
) -> Option<(String, DiagnosticWitness)> {
    let hit = finding_for_seam(findings, entry, canonical_gap_id, candidate_seams)?;
    DiagnosticWitness::from_finding(hit).map(|witness| (hit.id.clone(), witness))
}

/// A canonical gap id is content-derived and excludes the source location, so
/// sibling seams can share one id; the id match alone would credit this seam
/// with another seam's witness. The owner is the discriminating identity and
/// must match this seam's owner.
fn gap_names_seam(gap: &FindingCanonicalGap, gap_id: &str, owner: &str) -> bool {
    gap.id == gap_id && gap.owner == owner
}

/// Whether a finding's producer gap names this seam and this seam is the
/// uniquely most-specific claimant among all candidate seams.
///
/// A finding matches a seam when its file, owner, and probe family match
/// and its probe line falls on the seam (#7177). When multiple candidate
/// seams claim the same finding (same file/owner/kind containing the probe
/// line), most-specific span selection applies (#7179):
/// - If `entry` strictly encloses any sibling claimant, `entry` is an outer
///   seam and yields to the more-specific inner seam.
/// - If any sibling claimant does not strictly enclose `entry`, that sibling
///   is a competing minimal claimant (identical span, same-line spanless twin,
///   or crossing span): the collision is ambiguous and refuses (fail-closed).
/// - Otherwise, every other claimant strictly encloses `entry`, making
///   `entry` the unique most-specific seam.
fn producer_candidate_qualifies(
    entry: &ClassifiedSeam,
    finding: &crate::domain::Finding,
    gap: &FindingCanonicalGap,
    candidate_seams: &[ClassifiedSeam],
) -> bool {
    if !producer_gap_names_seam(gap, &entry.seam, finding.probe.location.line) {
        return false;
    }
    for sibling in candidate_seams {
        if sibling.seam.id() == entry.seam.id() {
            continue;
        }
        if producer_gap_names_seam(gap, &sibling.seam, finding.probe.location.line) {
            if entry.seam.strictly_encloses(&sibling.seam) {
                return false;
            }
            if !sibling.seam.strictly_encloses(&entry.seam) {
                return false;
            }
        }
    }
    true
}

/// The producer-structured half of the seam↔finding join: a finding's
/// producer gap names this seam when its stable file, owner, and probe
/// family all name the seam's own and its probe line falls on the seam.
/// Producers mint `gap:<lang>:…` finding
/// identities from the same index facts seams carry (file, symbol owner,
/// probe shape) but never the seam's content-hash id, so the content-hash
/// branch alone can never bind a produced finding — this branch is the join
/// that can (#7162). The owner comparison accepts the live rust-index symbol
/// id spelling (`{file}::{qualified}`, which the Rust producer's
/// `qualified_owner` strips to the gap owner at mint time) as well as the
/// bare owner hand-built seams carry; a gap id shared with a sibling owner
/// still never credits this seam. The line check splits twin seams of one
/// function that share file, owner, and kind: without it one finding binds
/// every twin, crediting unchanged seams with the changed seam's witness
/// (#7177).
fn producer_gap_names_seam(gap: &FindingCanonicalGap, seam: &RepoSeam, probe_line: usize) -> bool {
    let seam_file = crate::analysis::stable_path_text(seam.file());
    if gap.file != seam_file {
        return false;
    }
    let owner = seam.owner();
    let owner_matches = gap.owner == owner
        || owner
            .strip_prefix(seam_file.as_str())
            .and_then(|tail| tail.strip_prefix("::"))
            == Some(gap.owner.as_str());
    if !owner_matches {
        return false;
    }
    if crate::analysis::seam_kind_for_probe_family(&gap.probe_kind) != Some(seam.kind()) {
        return false;
    }
    probe_line_names_seam(probe_line, seam)
}

/// Whether a 1-based probe line falls on the seam: inside its parser span
/// when one is recorded, else exactly its display line. Line 0 (unplaced)
/// never names a seam. Same-line same-kind twins without spans and nested
/// same-kind spans are resolved across candidate seams by
/// [`producer_candidate_qualifies`] (#7177, #7179).
fn probe_line_names_seam(probe_line: usize, seam: &RepoSeam) -> bool {
    if probe_line == 0 {
        return false;
    }
    match seam.span() {
        Some(span) => (span.start_line..=span.end_line).contains(&probe_line),
        None => seam.display_line() == probe_line,
    }
}

/// The portable workspace identity is producer-owned through the admitted
/// test-target evidence (the `TestTargetEvidence` precedent). When nothing on
/// this seam names it, the card refuses to mint one: identity is never
/// fabricated from a checkout path. Exposed crate-internally so the LSP
/// editor projection (#4668) consumes the exact same derivation.
pub(crate) fn workspace_identity_for(
    entry: &ClassifiedSeam,
    readiness: &RepairRouteReadiness,
) -> Result<String, String> {
    workspace_identity_naming_route(
        entry,
        readiness,
        &format!(
            "ripr agent packet --seam-id {} --json",
            shell_arg(entry.seam.id().as_str())
        ),
    )
}

/// The `ripr agent packet` command a CLI user can paste from any working
/// directory: the root binds against the invocation's working directory
/// (#3999), the way `first-pr` and the `agent card` refusal remedies bind it.
/// Presentation only; the card's typed next action and its detail routes
/// keep their portable spelling and never enter identity with a checkout path.
pub(crate) fn bound_packet_command(root: &Path, seam_id: &str) -> String {
    agent_inspection_command_spec(
        AgentArtifactRoute::Packet,
        &bound_root(&root.to_string_lossy()),
        seam_id,
    )
    .display
}

/// [`workspace_identity_for`] with the caller's packet route in the refusal:
/// the editor and MCP projections name the portable route, the CLI names the
/// bound one.
fn workspace_identity_naming_route(
    entry: &ClassifiedSeam,
    readiness: &RepairRouteReadiness,
    packet_route: &str,
) -> Result<String, String> {
    if let RepairTargetSelection::Existing(target) = &readiness.target_selection {
        return Ok(target.workspace_identity().to_string());
    }
    for test in &entry.evidence.related_tests {
        if let Some(target) = &test.test_target {
            return Ok(target.workspace_identity().to_string());
        }
    }
    Err(format!(
        "agent card cannot name the portable workspace identity for seam `{}`: no admitted test-target evidence names it; run `{packet_route}` for the full evidence packet",
        entry.seam.id().as_str()
    ))
}

/// The most recently created recorded attempt for this seam, if any. Attempt
/// ids are content hashes (not time-ordered), so recency is the manifest's
/// own `created_unix_ms`. Exposed crate-internally so the LSP editor
/// projection (#4668) binds attempt state through the same inventory.
pub(crate) fn latest_attempt_for_seam(
    root: &Path,
    seam_id: &str,
) -> Result<Option<RepairAttemptManifest>, String> {
    let attempts = inventory_repair_attempts(root)?;
    Ok(attempts
        .into_iter()
        .filter_map(|entry| match entry {
            RepairAttemptInventoryEntry::Valid(manifest) if manifest.seam_id == seam_id => {
                Some(*manifest)
            }
            _ => None,
        })
        .max_by_key(|manifest| manifest.created_unix_ms))
}

#[cfg(test)]
mod tests {
    use super::super::repair_attempt::{
        REPAIR_ATTEMPT_SCHEMA_VERSION, RepairAttemptId, RepairAttemptState,
    };
    use super::*;
    use crate::analysis::seams::{
        ExpectedSink, RepoSeam, RequiredDiscriminator, SeamGripClass, SeamKind,
    };
    use crate::analysis::test_grip_evidence::TestGripEvidence;
    use crate::domain::{Confidence, FixInstructionState, StageEvidence, StageState};

    fn stage(state: StageState) -> StageEvidence {
        StageEvidence::new(state, Confidence::Medium, "test stage")
    }

    fn boundary_seam() -> RepoSeam {
        RepoSeam::new(
            "src/pricing.rs",
            "pricing::discounted_total",
            SeamKind::PredicateBoundary,
            42,
            88,
            "amount >= discount_threshold",
            RequiredDiscriminator::BoundaryValue {
                description: "amount >= discount_threshold".to_string(),
            },
            ExpectedSink::ReturnValue,
        )
    }

    fn weakly_gripped_entry() -> ClassifiedSeam {
        weakly_gripped_entry_for_seam(boundary_seam())
    }

    #[test]
    fn parsed_match_arm_geometry_keeps_nested_ownership_and_head_ambiguity() -> Result<(), String> {
        use crate::analysis::{RaRustSyntaxAdapter, RustIndex, RustSyntaxAdapter};
        let path = Path::new("src/lib.rs");
        let source = "pub fn route(x: Option<Option<u8>>) -> u8 {\n    match x {\n        Some(inner) => match inner {\n            Some(v) =>\n                v,\n            _ => 0,\n        },\n        _ => 1,\n    }\n}\n";
        let facts = RaRustSyntaxAdapter.summarize_file(path, source)?;
        let mut index = RustIndex::default();
        index.extend_functions(facts.functions.clone());
        index.insert_file_only(path.to_path_buf(), facts);
        let candidates: Vec<_> =
            crate::analysis::inventory_seams_from_index(&[path.to_path_buf()], &index)
                .into_iter()
                .filter(|s| s.kind() == SeamKind::MatchArm)
                .map(weakly_gripped_entry_for_seam)
                .collect();
        assert_eq!(candidates.len(), 6, "two match heads and four actual arms");
        let gap = FindingCanonicalGap {
            id: "geometry-control".into(),
            language: "rust".into(),
            file: "src/lib.rs".into(),
            owner: "route".into(),
            behavior_kind: "match_arm".into(),
            probe_kind: "match_arm".into(),
            normalized_discriminator: String::new(),
        };
        let mut finding =
            producer_gap_finding("geometry", &gap.id, &gap.file, &gap.owner, "match_arm");
        finding.probe.family = crate::domain::ProbeFamily::MatchArm;
        for (line, anchor) in [(5, 4), (6, 6), (8, 8)] {
            finding.probe.location.line = line;
            let qualified: Vec<_> = candidates
                .iter()
                .filter(|entry| producer_candidate_qualifies(entry, &finding, &gap, &candidates))
                .map(|entry| entry.seam.display_line())
                .collect();
            assert_eq!(
                qualified,
                [anchor],
                "body line {line} must name only its innermost arm"
            );
        }
        // The nested `match` head is line-only on the outer arm's own line;
        // it remains an ambiguous same-kind claimant rather than fabricated precision.
        finding.probe.location.line = 3;
        assert!(!candidates.iter().any(|entry| producer_candidate_qualifies(
            entry,
            &finding,
            &gap,
            &candidates
        )));
        Ok(())
    }

    /// #7177: a weakly-gripped predicate boundary at `display_line`, the
    /// twin-seam collision control: same file/owner/kind, different line.
    fn twin_boundary_entry(display_line: usize) -> ClassifiedSeam {
        weakly_gripped_entry_for_seam(RepoSeam::new(
            "src/pricing.rs",
            "pricing::discounted_total",
            SeamKind::PredicateBoundary,
            display_line * 10,
            display_line,
            "amount >= discount_threshold",
            RequiredDiscriminator::BoundaryValue {
                description: "amount >= discount_threshold".to_string(),
            },
            ExpectedSink::ReturnValue,
        ))
    }

    /// #7179: a weakly-gripped predicate boundary carrying an explicit span.
    fn spanned_twin_boundary_entry(
        byte_offset: usize,
        display_line: usize,
        span: crate::analysis::seams::SeamSpan,
    ) -> ClassifiedSeam {
        let mut entry = weakly_gripped_entry_for_seam(RepoSeam::new(
            "src/pricing.rs",
            "pricing::discounted_total",
            SeamKind::PredicateBoundary,
            byte_offset,
            display_line,
            "amount >= discount_threshold",
            RequiredDiscriminator::BoundaryValue {
                description: "amount >= discount_threshold".to_string(),
            },
            ExpectedSink::ReturnValue,
        ));
        entry.seam = entry.seam.with_span(span);
        entry
    }

    fn weakly_gripped_entry_for_seam(seam: RepoSeam) -> ClassifiedSeam {
        let seam_id = seam.id().clone();
        ClassifiedSeam {
            seam,
            evidence: TestGripEvidence {
                seam_id,
                related_tests: Vec::new(),
                reach: stage(StageState::Yes),
                activate: stage(StageState::Yes),
                propagate: stage(StageState::Yes),
                observe: stage(StageState::Yes),
                discriminate: stage(StageState::No),
                observed_values: Vec::new(),
                missing_discriminators: Vec::new(),
                statically_contradicted_related_tests: 0,
                new_test_target: None,
            },
            class: SeamGripClass::WeaklyGripped,
        }
    }

    /// The rendered packet envelope for this seam: `packets` names the seam,
    /// so the canonical packet family projects as current.
    fn packet_for(entry: &ClassifiedSeam) -> String {
        format!(
            r#"{{"schema_version":"0.5","packets":[{{"seam_id":"{}"}}]}}"#,
            entry.seam.id().as_str()
        )
    }

    fn facts_for<'a>(entry: &'a ClassifiedSeam, packet_json: &'a str) -> SeamCardFacts<'a> {
        SeamCardFacts {
            entry,
            witness: None,
            finding_id: None,
            finding_gap_id: None,
            attempt: None,
            packet_json,
            repository_head: "abc123",
            workspace_identity: "workspace:demo",
            currentness: RepairCardSnapshotCurrentness::Current,
            next_command: None,
        }
    }

    /// A repair-ready seam whose weak related test lives in the seam's own
    /// file, whose one governed inline module the InlineUnit producer
    /// recorded (#5210): the card's allowed file is the production file.
    fn own_file_inline_entry() -> ClassifiedSeam {
        use crate::analysis::test_grip_evidence::{
            RelatedTestGrip, RelationConfidence, RelationReason, TestTargetEvidence,
        };
        use crate::domain::{
            FlowSinkFact, FlowSinkKind, MissingDiscriminatorFact, OracleKind, OracleStrength,
            ValueContext, ValueFact,
        };
        let mut entry = weakly_gripped_entry();
        entry.evidence.discriminate = stage(StageState::Weak);
        entry.evidence.observed_values = vec![ValueFact {
            file: None,
            line: 120,
            text: "discounted_total(50, 100)".to_string(),
            value: "50".to_string(),
            context: ValueContext::FunctionArgument,
        }];
        entry.evidence.missing_discriminators = vec![MissingDiscriminatorFact {
            value: "discount_threshold (equality boundary)".to_string(),
            reason: "observed values do not include the equality-boundary case".to_string(),
            flow_sink: Some(FlowSinkFact {
                kind: FlowSinkKind::ReturnValue,
                text: "return discounted_total".to_string(),
                line: 88,
                owner: None,
            }),
        }];
        entry.evidence.related_tests = vec![std::sync::Arc::new(RelatedTestGrip {
            test_name: "below_threshold_has_no_discount".to_string(),
            file: std::path::PathBuf::from("src/pricing.rs"),
            line: 120,
            test_target: Some(TestTargetEvidence::fixture(
                "below_threshold_has_no_discount",
                Path::new("src/pricing.rs"),
                120,
            )),
            oracle_kind: OracleKind::BroadError,
            oracle_strength: OracleStrength::Weak,
            evidence_summary: "broad assertion".to_string(),
            relation_reason: RelationReason::DirectOwnerCall,
            relation_confidence: RelationConfidence::High,
        })];
        entry.evidence.new_test_target =
            Some(crate::analysis::new_test_target::NewTestTargetAdmission {
                owner_inline_region: Some(
                    crate::analysis::new_test_target::InlineTestRegionAuthority {
                        file: std::path::PathBuf::from("src/pricing.rs"),
                        module_name: "tests".to_string(),
                        parent_modules: Vec::new(),
                        body_start: 0,
                        close_brace_start: 0,
                        source_digest: String::new(),
                    },
                ),
                ..Default::default()
            });
        entry
    }

    /// #8638: the card's `subject.canonical_gap_id` is the bound finding's own
    /// gap id (never re-minted from the seam), fails closed to null when no
    /// finding binds, and the content hash rides additively as
    /// `seam_content_gap_id`.
    #[test]
    fn card_subject_reports_the_bound_finding_gap_id_and_keeps_the_hash_additive()
    -> Result<(), String> {
        let entry = own_file_inline_entry();
        let packet = packet_for(&entry);
        let hash = crate::analysis::canonical_gap::canonical_gap_identity(&entry)
            .ok_or_else(|| "fixture seam must carry a canonical gap identity".to_string())?
            .id;
        let finding_gap =
            "gap:rust:src/pricing.rs:pricing::score:predicate_boundary:predicate:let_amount>=limit";
        let mut facts = facts_for(&entry, &packet);
        facts.finding_gap_id = Some(finding_gap);
        let card = assemble_repair_card(&facts)?;
        if card.subject.canonical_gap_id.as_deref() != Some(finding_gap) {
            return Err(format!(
                "card did not report the bound finding's id: {:?}",
                card.subject.canonical_gap_id
            ));
        }
        if card.subject.seam_content_gap_id.as_deref() != Some(hash.as_str()) {
            return Err("the content hash was not kept additively".to_string());
        }
        let unbound = assemble_repair_card(&facts_for(&entry, &packet))?;
        if unbound.subject.canonical_gap_id.is_some() {
            return Err("an unbound seam must fail closed to a null gap id".to_string());
        }
        Ok(())
    }

    /// Devin 4179587247: a compact card that allows a production file must
    /// carry the inline-module confinement in its stop conditions, in the
    /// packet's own words, so the handoff is safe without the packet.
    #[test]
    fn inline_repair_card_stop_conditions_name_the_module_confinement() -> Result<(), String> {
        let entry = own_file_inline_entry();
        let packet = packet_for(&entry);
        let card = assemble_repair_card(&facts_for(&entry, &packet))?;
        if card.allowed_files != vec!["src/pricing.rs".to_string()] {
            return Err(format!(
                "fixture must allow the production file, got {:?}",
                card.allowed_files
            ));
        }
        let confinement = crate::output::agent_seam_packets::inline_test_module_edit_statement(
            "src/pricing.rs",
            "tests",
        );
        if !card.stop_conditions.contains(&confinement) {
            return Err(format!(
                "stop conditions omit the module confinement: {:?}",
                card.stop_conditions
            ));
        }
        // A test-surface card states no module confinement.
        let mut under_tests = own_file_inline_entry();
        for test in &mut under_tests.evidence.related_tests {
            let test = std::sync::Arc::make_mut(test);
            test.file = std::path::PathBuf::from("tests/pricing.rs");
            test.test_target = Some(
                crate::analysis::test_grip_evidence::TestTargetEvidence::fixture(
                    "below_threshold_has_no_discount",
                    Path::new("tests/pricing.rs"),
                    120,
                ),
            );
        }
        let card = assemble_repair_card(&facts_for(&under_tests, &packet))?;
        if card.allowed_files != vec!["tests/pricing.rs".to_string()]
            || card
                .stop_conditions
                .iter()
                .any(|condition| condition.contains("#[cfg(test)] mod"))
        {
            return Err(format!(
                "a test-surface card must not be module-confined: {:?} {:?}",
                card.allowed_files, card.stop_conditions
            ));
        }
        Ok(())
    }

    #[test]
    fn repair_card_assembly_projects_every_family_with_packet_route() -> Result<(), String> {
        let entry = weakly_gripped_entry();
        let packet = packet_for(&entry);
        let card = assemble_repair_card(&facts_for(&entry, &packet))?;
        if card.schema_version != crate::domain::REPAIR_CARD_SCHEMA_VERSION {
            return Err("card does not name the repair card schema".to_string());
        }
        if card.subject.seam_id != entry.seam.id().as_str() {
            return Err("card subject does not name the seam".to_string());
        }
        if card.detail_references.len() != 9 {
            return Err(format!(
                "expected nine detail references, got {}",
                card.detail_references.len()
            ));
        }
        let packet_ref = card
            .detail_references
            .iter()
            .find(|reference| reference.family == RepairCardDetailFamily::CanonicalPacket)
            .ok_or_else(|| "canonical packet reference missing".to_string())?;
        if packet_ref.state != crate::domain::RepairCardDetailState::Current
            || !packet_ref
                .route
                .as_deref()
                .is_some_and(|route| route.contains("ripr agent packet --seam-id"))
        {
            return Err("canonical packet must ride behind the packet route".to_string());
        }
        for family in [
            RepairCardDetailFamily::FixInstruction,
            RepairCardDetailFamily::WitnessStageEvidence,
            RepairCardDetailFamily::LimitationDetail,
        ] {
            let reference = card
                .detail_references
                .iter()
                .find(|reference| reference.family == family)
                .ok_or_else(|| format!("reference missing for {family:?}"))?;
            if reference.state != crate::domain::RepairCardDetailState::Unavailable
                || reference
                    .unavailable_reason
                    .as_deref()
                    .is_none_or(str::is_empty)
            {
                return Err(format!(
                    "{family:?} must record an exact unavailable reason"
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn repair_card_assembly_preserves_static_limitations_without_cage_admission()
    -> Result<(), String> {
        let entry = weakly_gripped_entry();
        let readiness = repair_packet_eligibility(&entry).readiness;
        assert!(!readiness.is_repair_ready());
        assert!(!readiness.missing_evidence.is_empty());
        let packet = packet_for(&entry);
        assert_eq!(
            super::super::repair_attempt::edit_cage_policy_from_packet(
                &packet,
                entry.seam.id().as_str()
            )
            .err()
            .as_deref(),
            Some("repair packet is missing allowed_edit_surface")
        );
        let card = assemble_repair_card(&facts_for(&entry, &packet))?;
        assert!(!card.readiness.repair_ready);
        assert_eq!(card.readiness.missing_evidence, readiness.missing_evidence);
        assert!(card.exact_blocker.is_none());
        assert!(card.next_action.is_none());
        Ok(())
    }

    #[test]
    fn repair_card_assembly_omits_next_action_when_route_gate_is_closed() -> Result<(), String> {
        let entry = weakly_gripped_entry();
        let command = agent_inspection_command_spec(
            AgentArtifactRoute::Packet,
            "src",
            entry.seam.id().as_str(),
        );
        let packet = packet_for(&entry);
        let mut facts = facts_for(&entry, &packet);
        facts.next_command = Some(command);
        let card = assemble_repair_card(&facts)?;
        if card.next_action.is_some() {
            return Err(
                "a card without witness instruction must not present a next action".to_string(),
            );
        }
        if card.instruction.state != FixInstructionState::Unavailable {
            return Err("missing witness must project an unavailable instruction".to_string());
        }
        Ok(())
    }

    #[test]
    fn repair_card_assembly_stays_within_the_default_bounds() -> Result<(), String> {
        let entry = weakly_gripped_entry();
        let packet = packet_for(&entry);
        let card = assemble_repair_card(&facts_for(&entry, &packet))?;
        if card.detail_references.len() > crate::domain::DEFAULT_REPAIR_CARD_MAX_DETAIL_ITEMS {
            return Err("card exceeded the default item bound".to_string());
        }
        if card.detail_summary.selected_bytes
            > crate::domain::DEFAULT_REPAIR_CARD_MAX_SERIALIZED_BYTES
        {
            return Err("card exceeded the default byte bound".to_string());
        }
        Ok(())
    }

    #[test]
    fn gap_names_seam_requires_owner_match() -> Result<(), String> {
        let gap = FindingCanonicalGap {
            id: "gap-1".to_string(),
            language: "rust".to_string(),
            file: "src/pricing.rs".to_string(),
            owner: "pricing::discounted_total".to_string(),
            behavior_kind: "predicate_boundary".to_string(),
            probe_kind: "predicate_boundary".to_string(),
            normalized_discriminator: "amount >= discount_threshold".to_string(),
        };
        if !gap_names_seam(&gap, "gap-1", "pricing::discounted_total") {
            return Err("the seam's own owner must name the seam".to_string());
        }
        if gap_names_seam(&gap, "gap-1", "pricing::other_total") {
            return Err("a gap id shared with another owner must not name the seam".to_string());
        }
        Ok(())
    }

    /// #7162: the producer-structured join names a seam by stable file, owner,
    /// and probe family — never by id-string equality across the two schemes.
    /// A live rust-index owner (`{file}::{qualified}`) and a bare hand-built
    /// owner both match; a sibling owner, a foreign file, a foreign probe
    /// family, and a partial file-segment prefix never do. #7177: the probe
    /// line must fall on the seam — inside its span when recorded, else
    /// exactly its display line — so twin seams split by line.
    #[test]
    fn producer_gap_names_seam_by_file_owner_and_kind() -> Result<(), String> {
        fn gap_for(file: &str, owner: &str, probe_kind: &str) -> FindingCanonicalGap {
            FindingCanonicalGap {
                id: format!(
                    "gap:rust:{file}:{owner}:predicate_boundary:{probe_kind}:amount==discount_threshold"
                ),
                language: "rust".to_string(),
                file: file.to_string(),
                owner: owner.to_string(),
                behavior_kind: "predicate_boundary".to_string(),
                probe_kind: probe_kind.to_string(),
                normalized_discriminator: "amount==discount_threshold".to_string(),
            }
        }
        fn seam_for(file: &str, owner: &str) -> RepoSeam {
            RepoSeam::new(
                file,
                owner,
                SeamKind::PredicateBoundary,
                42,
                88,
                "amount > discount_threshold",
                RequiredDiscriminator::BoundaryValue {
                    description: "amount > discount_threshold".to_string(),
                },
                ExpectedSink::ReturnValue,
            )
        }
        let gap = gap_for("src/lib.rs", "discounted_total", "predicate");
        // The live shape: the seam carries the rust-index symbol id while the
        // gap carries the qualified tail the producer stripped at mint time.
        if !producer_gap_names_seam(
            &gap,
            &seam_for("src/lib.rs", "src/lib.rs::discounted_total"),
            88,
        ) {
            return Err("a live-shaped producer gap must name its seam".to_string());
        }
        // The hand-built shape: a bare owner on the same file still matches.
        if !producer_gap_names_seam(&gap, &seam_for("src/lib.rs", "discounted_total"), 88) {
            return Err("a bare owner on the same file must name its seam".to_string());
        }
        // Every discriminating axis refuses on its own.
        for (name, gap, seam) in [
            (
                "sibling owner",
                gap_for("src/lib.rs", "other_total", "predicate"),
                seam_for("src/lib.rs", "src/lib.rs::discounted_total"),
            ),
            (
                "foreign file",
                gap_for("src/other.rs", "discounted_total", "predicate"),
                seam_for("src/lib.rs", "src/lib.rs::discounted_total"),
            ),
            (
                "foreign probe family",
                gap_for("src/lib.rs", "discounted_total", "return_value"),
                seam_for("src/lib.rs", "src/lib.rs::discounted_total"),
            ),
            (
                "unmapped probe family",
                gap_for("src/lib.rs", "discounted_total", "static_unknown"),
                seam_for("src/lib.rs", "src/lib.rs::discounted_total"),
            ),
            // An owner that starts with the file text but is not
            // segment-qualified by it must not strip as a prefix.
            (
                "partial file segment",
                gap_for("src/lib.rs", "discounted_total", "predicate"),
                seam_for("src/lib.rs", "src/lib.rs2::discounted_total"),
            ),
        ] {
            if producer_gap_names_seam(&gap, &seam, 88) {
                return Err(format!("a {name} must not name the seam"));
            }
        }
        // The line axis refuses on its own (#7177): an off-line probe and
        // an unplaced probe never name the seam.
        for (name, probe_line) in [("off-line probe", 7), ("unplaced probe", 0)] {
            if producer_gap_names_seam(
                &gap,
                &seam_for("src/lib.rs", "src/lib.rs::discounted_total"),
                probe_line,
            ) {
                return Err(format!("a {name} must not name the seam"));
            }
        }
        // A recorded span contains by range: both edges bind, both neighbors
        // refuse — even though neither equals the display line.
        let spanned = seam_for("src/lib.rs", "src/lib.rs::discounted_total").with_span(
            crate::analysis::seams::SeamSpan {
                start_line: 10,
                start_column: 1,
                end_line: 14,
                end_column: 2,
            },
        );
        for probe_line in [10, 14] {
            if !producer_gap_names_seam(&gap, &spanned, probe_line) {
                return Err(format!("a span edge line {probe_line} must name the seam"));
            }
        }
        for probe_line in [9, 15] {
            if producer_gap_names_seam(&gap, &spanned, probe_line) {
                return Err(format!(
                    "a span neighbor line {probe_line} must not name the seam"
                ));
            }
        }
        Ok(())
    }

    /// #5268: the Rust producer now populates canonical gaps on diff
    /// findings, but a producer gap id (`gap:rust:...`) is a different
    /// identity scheme from the repo seam's content-hashed canonical gap id,
    /// so a populated finding must still not be credited as this seam's
    /// witness — the id match alone would bind a diff finding to a seam it
    /// did not come from.
    #[test]
    fn rust_producer_gap_does_not_bind_a_seam_witness_by_id_coincidence() -> Result<(), String> {
        use crate::domain::{
            ActivationEvidence, DeltaKind, ExposureClass, OracleKind, OracleStrength, Probe,
            ProbeFamily, ProbeId, RelatedTest, RevealEvidence, RiprEvidence, SourceLocation,
            SymbolId,
        };
        let entry = weakly_gripped_entry();
        let seam_gap_id = crate::analysis::canonical_gap::canonical_gap_identity(&entry)
            .ok_or_else(|| {
                "the weakly gripped seam must carry a canonical gap identity".to_string()
            })?
            .id;
        // Weakly exposed with an oracle row, so a (wrong) id match would
        // actually produce a witness and fail this control.
        let finding = crate::domain::Finding {
            id: "probe:src_lib.rs:predicate:4036edfa".to_string(),
            canonical_gap: Some(FindingCanonicalGap {
                id: "gap:rust:src/lib.rs:pricing::discounted_total:predicate_boundary:predicate:amount==discount_threshold"
                    .to_string(),
                language: "rust".to_string(),
                file: "src/lib.rs".to_string(),
                owner: "pricing::discounted_total".to_string(),
                behavior_kind: "predicate_boundary".to_string(),
                probe_kind: "predicate".to_string(),
                normalized_discriminator: "amount==discount_threshold".to_string(),
            }),
            probe: Probe {
                id: ProbeId("probe:src_lib.rs:predicate:4036edfa".to_string()),
                location: SourceLocation::new("src/lib.rs", 2, 5),
                owner: Some(SymbolId("src/lib.rs::pricing::discounted_total".to_string())),
                family: ProbeFamily::Predicate,
                delta: DeltaKind::Control,
                before: None,
                after: None,
                expression: "if amount >= discount_threshold {".to_string(),
                expected_sinks: Vec::new(),
                required_oracles: Vec::new(),
            },
            class: ExposureClass::WeaklyExposed,
            ripr: RiprEvidence {
                reach: stage(StageState::Yes),
                infect: stage(StageState::Weak),
                propagate: stage(StageState::Yes),
                reveal: RevealEvidence {
                    observe: stage(StageState::Yes),
                    discriminate: stage(StageState::Weak),
                },
            },
            confidence: 0.5,
            evidence: Vec::new(),
            missing: vec!["Missing discriminator value: amount == discount_threshold".to_string()],
            flow_sinks: Vec::new(),
            activation: ActivationEvidence::default(),
            stop_reasons: Vec::new(),
            related_tests_matched_total: Some(1),
            related_tests: vec![RelatedTest {
                name: "below_threshold_has_no_discount".to_string(),
                file: std::path::PathBuf::from("tests/pricing.rs"),
                line: 2,
                oracle: Some("assert_eq!(discounted_total(50, 100), 50)".to_string()),
                oracle_kind: OracleKind::ExactValue,
                oracle_strength: OracleStrength::Strong,
                relation_reason: None,
                relation_confidence: None,
                miss: None,
            }],
            recommended_next_step: None,
            language: Some(crate::domain::LanguageId::Rust),
            language_status: None,
            owner_kind: None,
            static_limit_kind: None,
            changed_sink: None,
            observed_sink: None,
            oracle_alignment: None,
            alignment_reason: None,
            source_currentness: crate::domain::SourceCurrentness::CandidateCurrent,
        };
        if witness_from_findings(
            std::slice::from_ref(&finding),
            &entry,
            Some(&seam_gap_id),
            &[],
        )
        .is_some()
        {
            return Err(format!(
                "a producer-gap finding must not bind as the seam witness for gap id {seam_gap_id}"
            ));
        }
        Ok(())
    }

    /// One weakly-exposed producer-gap finding with an oracle row, so a bound
    /// witness is observable and a wrong bind would fail its test. The gap id
    /// is caller-chosen: producer-shaped ids exercise the structured branch,
    /// the seam's own content-hash id the legacy branch.
    fn producer_gap_finding(
        id: &str,
        gap_id: &str,
        file: &str,
        owner: &str,
        probe_kind: &str,
    ) -> crate::domain::Finding {
        use crate::domain::{
            ActivationEvidence, DeltaKind, ExposureClass, Finding, OracleKind, OracleStrength,
            Probe, ProbeFamily, ProbeId, RelatedTest, RevealEvidence, RiprEvidence, SourceLocation,
        };
        Finding {
            id: id.to_string(),
            canonical_gap: Some(FindingCanonicalGap {
                id: gap_id.to_string(),
                language: "rust".to_string(),
                file: file.to_string(),
                owner: owner.to_string(),
                behavior_kind: "predicate_boundary".to_string(),
                probe_kind: probe_kind.to_string(),
                normalized_discriminator: "amount==discount_threshold".to_string(),
            }),
            probe: Probe {
                id: ProbeId(id.to_string()),
                location: SourceLocation::new(file, 2, 5),
                owner: None,
                family: ProbeFamily::Predicate,
                delta: DeltaKind::Control,
                before: None,
                after: None,
                expression: "amount > discount_threshold".to_string(),
                expected_sinks: Vec::new(),
                required_oracles: Vec::new(),
            },
            class: ExposureClass::WeaklyExposed,
            ripr: RiprEvidence {
                reach: stage(StageState::Yes),
                infect: stage(StageState::Weak),
                propagate: stage(StageState::Yes),
                reveal: RevealEvidence {
                    observe: stage(StageState::Yes),
                    discriminate: stage(StageState::Weak),
                },
            },
            confidence: 0.5,
            evidence: Vec::new(),
            missing: Vec::new(),
            flow_sinks: Vec::new(),
            activation: ActivationEvidence::default(),
            stop_reasons: Vec::new(),
            related_tests_matched_total: Some(1),
            related_tests: vec![RelatedTest {
                name: "exact_boundary_gets_the_discount".to_string(),
                file: std::path::PathBuf::from("tests/pricing.rs"),
                line: 12,
                oracle: Some("assert_eq!(discounted_total(100, 100), 90)".to_string()),
                oracle_kind: OracleKind::ExactValue,
                oracle_strength: OracleStrength::Strong,
                relation_reason: None,
                relation_confidence: None,
                miss: None,
            }],
            recommended_next_step: None,
            language: Some(crate::domain::LanguageId::Rust),
            language_status: None,
            owner_kind: None,
            static_limit_kind: None,
            changed_sink: None,
            observed_sink: None,
            oracle_alignment: None,
            alignment_reason: None,
            source_currentness: crate::domain::SourceCurrentness::CandidateCurrent,
        }
    }

    /// #7162: a produced finding whose gap structurally names the seam binds
    /// as its witness — the producer id scheme is the join key the
    /// content-hash branch can never match. Two findings sharing the shape
    /// stay ambiguous and bind nothing, and a content-hash match still ends
    /// the search first.
    #[test]
    fn producer_gap_finding_binds_the_seam_witness_when_unique() -> Result<(), String> {
        let entry = weakly_gripped_entry();
        let seam_gap_id = crate::analysis::canonical_gap::canonical_gap_identity(&entry)
            .ok_or_else(|| {
                "the weakly gripped seam must carry a canonical gap identity".to_string()
            })?
            .id;
        let producer_id = "gap:rust:src/pricing.rs:pricing::discounted_total:predicate_boundary:predicate:amount==discount_threshold";
        if producer_id == seam_gap_id {
            return Err("the producer id must differ from the content-hash id".to_string());
        }
        let mut finding = producer_gap_finding(
            "finding:producer:1",
            producer_id,
            "src/pricing.rs",
            "pricing::discounted_total",
            "predicate",
        );
        // #7177: the probe line must fall on the seam (display line 88).
        finding.probe.location.line = entry.seam.display_line();
        let (bound_id, _) = witness_from_findings(
            std::slice::from_ref(&finding),
            &entry,
            Some(&seam_gap_id),
            &[],
        )
        .ok_or_else(|| "a uniquely matching producer finding must bind".to_string())?;
        if bound_id != "finding:producer:1" {
            return Err(format!("bound the wrong finding: {bound_id}"));
        }
        // A sibling finding sharing the shape makes the match ambiguous: no
        // witness rather than another seam's instruction.
        let mut twin = producer_gap_finding(
            "finding:producer:2",
            producer_id,
            "src/pricing.rs",
            "pricing::discounted_total",
            "predicate",
        );
        twin.probe.location.line = entry.seam.display_line();
        if witness_from_findings(&[finding, twin], &entry, Some(&seam_gap_id), &[]).is_some() {
            return Err("ambiguous producer matches must bind no witness".to_string());
        }
        Ok(())
    }

    /// #7162 + #5268: a producer gap id shared with a sibling owner never
    /// credits this seam — even when the file and probe family both match.
    #[test]
    fn producer_gap_with_sibling_owner_binds_no_witness() -> Result<(), String> {
        let entry = weakly_gripped_entry();
        let seam_gap_id = crate::analysis::canonical_gap::canonical_gap_identity(&entry)
            .ok_or_else(|| {
                "the weakly gripped seam must carry a canonical gap identity".to_string()
            })?
            .id;
        let finding = producer_gap_finding(
            "finding:sibling:1",
            "gap:rust:src/pricing.rs:pricing::other_total:predicate_boundary:predicate:amount==discount_threshold",
            "src/pricing.rs",
            "pricing::other_total",
            "predicate",
        );
        if witness_from_findings(
            std::slice::from_ref(&finding),
            &entry,
            Some(&seam_gap_id),
            &[],
        )
        .is_some()
        {
            return Err("a sibling owner's producer gap must not bind".to_string());
        }
        Ok(())
    }

    /// #7177: twin-seam collision — two headline predicate boundaries in one
    /// function (same file/owner/kind, different lines) with one producer
    /// finding at the second boundary's line: only that seam binds, never
    /// the twin.
    #[test]
    fn producer_finding_binds_only_the_twin_seam_at_its_line() -> Result<(), String> {
        let first = twin_boundary_entry(7);
        let second = twin_boundary_entry(12);
        let candidates = [first.clone(), second.clone()];
        let first_gap_id = crate::analysis::canonical_gap::canonical_gap_identity(&first)
            .ok_or_else(|| "the first twin must carry a canonical gap identity".to_string())?
            .id;
        let second_gap_id = crate::analysis::canonical_gap::canonical_gap_identity(&second)
            .ok_or_else(|| "the second twin must carry a canonical gap identity".to_string())?
            .id;
        let producer_id = "gap:rust:src/pricing.rs:pricing::discounted_total:predicate_boundary:predicate:amount==discount_threshold";
        if producer_id == first_gap_id || producer_id == second_gap_id {
            return Err("the producer id must differ from both content-hash ids".to_string());
        }
        let mut finding = producer_gap_finding(
            "finding:twin:1",
            producer_id,
            "src/pricing.rs",
            "pricing::discounted_total",
            "predicate",
        );
        finding.probe.location.line = 12;
        if witness_from_findings(
            std::slice::from_ref(&finding),
            &second,
            Some(&second_gap_id),
            &candidates,
        )
        .is_none()
        {
            return Err("the seam at the finding line must bind".to_string());
        }
        if witness_from_findings(
            std::slice::from_ref(&finding),
            &first,
            Some(&first_gap_id),
            &candidates,
        )
        .is_some()
        {
            return Err("the twin seam off the finding line must not bind".to_string());
        }
        Ok(())
    }

    /// #7179: nested-span collision — two headline predicate boundaries in one
    /// function (same file/owner/kind) where an outer span strictly encloses an
    /// inner span (e.g. `(a > 10) == flag` lines 2..4 vs `a > 10` line 2..2).
    /// A finding at the inner comparison's line binds the inner seam only and
    /// never the outer seam. A finding on an outer-only line binds the outer
    /// seam. Competing minimal claimants (same-point twins with identical spans
    /// or spanless twins at the same line) refuse to bind (ambiguous tie).
    #[test]
    fn producer_finding_binds_only_the_inner_seam_when_nested() -> Result<(), String> {
        let outer_span = crate::analysis::seams::SeamSpan {
            start_line: 2,
            start_column: 1,
            end_line: 4,
            end_column: 10,
        };
        let inner_span = crate::analysis::seams::SeamSpan {
            start_line: 2,
            start_column: 5,
            end_line: 2,
            end_column: 11,
        };
        let outer = spanned_twin_boundary_entry(20, 2, outer_span);
        let inner = spanned_twin_boundary_entry(24, 2, inner_span);
        let candidates = [outer.clone(), inner.clone()];

        let outer_gap_id = crate::analysis::canonical_gap::canonical_gap_identity(&outer)
            .ok_or_else(|| "outer seam must carry a canonical gap identity".to_string())?
            .id;
        let inner_gap_id = crate::analysis::canonical_gap::canonical_gap_identity(&inner)
            .ok_or_else(|| "inner seam must carry a canonical gap identity".to_string())?
            .id;
        let producer_id = "gap:rust:src/pricing.rs:pricing::discounted_total:predicate_boundary:predicate:amount==discount_threshold";

        // Finding at line 2 falls in both spans; inner is strictly more specific,
        // so inner binds and outer refuses.
        let mut inner_finding = producer_gap_finding(
            "finding:nested:inner",
            producer_id,
            "src/pricing.rs",
            "pricing::discounted_total",
            "predicate",
        );
        inner_finding.probe.location.line = 2;

        let inner_bound = witness_from_findings(
            std::slice::from_ref(&inner_finding),
            &inner,
            Some(&inner_gap_id),
            &candidates,
        );
        if inner_bound.is_none() {
            return Err("the inner seam must bind the finding on its span".to_string());
        }

        let outer_bound = witness_from_findings(
            std::slice::from_ref(&inner_finding),
            &outer,
            Some(&outer_gap_id),
            &candidates,
        );
        if outer_bound.is_some() {
            return Err("the outer seam must not bind the inner finding's witness".to_string());
        }

        // Finding at line 4 falls only in the outer span; outer binds, inner refuses.
        let mut outer_finding = producer_gap_finding(
            "finding:nested:outer",
            producer_id,
            "src/pricing.rs",
            "pricing::discounted_total",
            "predicate",
        );
        outer_finding.probe.location.line = 4;

        if witness_from_findings(
            std::slice::from_ref(&outer_finding),
            &outer,
            Some(&outer_gap_id),
            &candidates,
        )
        .is_none()
        {
            return Err("the outer seam must bind the finding on its outer line".to_string());
        }
        if witness_from_findings(
            std::slice::from_ref(&outer_finding),
            &inner,
            Some(&inner_gap_id),
            &candidates,
        )
        .is_some()
        {
            return Err("the inner seam must not bind a finding outside its span".to_string());
        }

        // Competing minimal claimants: same-point twins with identical spans refuse.
        let twin1 = spanned_twin_boundary_entry(
            120,
            12,
            crate::analysis::seams::SeamSpan {
                start_line: 12,
                start_column: 1,
                end_line: 12,
                end_column: 20,
            },
        );
        let twin2 = spanned_twin_boundary_entry(
            125,
            12,
            crate::analysis::seams::SeamSpan {
                start_line: 12,
                start_column: 1,
                end_line: 12,
                end_column: 20,
            },
        );
        let twin1_gap_id = crate::analysis::canonical_gap::canonical_gap_identity(&twin1)
            .ok_or_else(|| "twin1 must carry a canonical gap identity".to_string())?
            .id;
        let mut twin_finding = producer_gap_finding(
            "finding:twin:12",
            producer_id,
            "src/pricing.rs",
            "pricing::discounted_total",
            "predicate",
        );
        twin_finding.probe.location.line = 12;
        let identical_candidates = [twin1.clone(), twin2.clone()];

        if witness_from_findings(
            std::slice::from_ref(&twin_finding),
            &twin1,
            Some(&twin1_gap_id),
            &identical_candidates,
        )
        .is_some()
        {
            return Err("identical-span twins must refuse as ambiguous".to_string());
        }

        // Competing minimal claimants: spanless twins at the same line refuse.
        let spanless1 = weakly_gripped_entry_for_seam(RepoSeam::new(
            "src/pricing.rs",
            "pricing::discounted_total",
            SeamKind::PredicateBoundary,
            120,
            12,
            "amount >= discount_threshold",
            RequiredDiscriminator::BoundaryValue {
                description: "amount >= discount_threshold".to_string(),
            },
            ExpectedSink::ReturnValue,
        ));
        let spanless2 = weakly_gripped_entry_for_seam(RepoSeam::new(
            "src/pricing.rs",
            "pricing::discounted_total",
            SeamKind::PredicateBoundary,
            125,
            12,
            "amount >= discount_threshold",
            RequiredDiscriminator::BoundaryValue {
                description: "amount >= discount_threshold".to_string(),
            },
            ExpectedSink::ReturnValue,
        ));
        let spanless_candidates = [spanless1.clone(), spanless2.clone()];
        let spanless1_gap_id = crate::analysis::canonical_gap::canonical_gap_identity(&spanless1)
            .ok_or_else(|| "spanless1 must carry a canonical gap identity".to_string())?
            .id;
        if witness_from_findings(
            std::slice::from_ref(&twin_finding),
            &spanless1,
            Some(&spanless1_gap_id),
            &spanless_candidates,
        )
        .is_some()
        {
            return Err("spanless twins at the same line must refuse as ambiguous".to_string());
        }

        Ok(())
    }

    /// #7162: branch order — a content-hash match ends the search even when a
    /// producer-structured match also exists.
    #[test]
    fn content_hash_match_wins_over_producer_match() -> Result<(), String> {
        let entry = weakly_gripped_entry();
        let seam_gap_id = crate::analysis::canonical_gap::canonical_gap_identity(&entry)
            .ok_or_else(|| {
                "the weakly gripped seam must carry a canonical gap identity".to_string()
            })?
            .id;
        let legacy = producer_gap_finding(
            "finding:legacy:1",
            &seam_gap_id,
            "src/pricing.rs",
            "pricing::discounted_total",
            "predicate",
        );
        let produced = producer_gap_finding(
            "finding:producer:1",
            "gap:rust:src/pricing.rs:pricing::discounted_total:predicate_boundary:predicate:amount==discount_threshold",
            "src/pricing.rs",
            "pricing::discounted_total",
            "predicate",
        );
        let (bound_id, _) =
            witness_from_findings(&[produced, legacy], &entry, Some(&seam_gap_id), &[])
                .ok_or_else(|| "a content-hash match must bind".to_string())?;
        if bound_id != "finding:legacy:1" {
            return Err(format!("the legacy match must win, bound {bound_id}"));
        }
        Ok(())
    }

    /// Finding ids embed the raw file path. A path with a space, apostrophe
    /// and `;` must reach the pasted `ripr explain` route as one argument,
    /// never splitting or running its tail as a second command.
    #[test]
    fn fix_instruction_route_quotes_a_path_bearing_finding_id() -> Result<(), String> {
        let entry = weakly_gripped_entry();
        let packet = packet_for(&entry);
        let witness = super::super::repair_card_usability::measurement_witness();
        let finding_id = "probe:src/it's;x rm.rs:2:predicate";
        let mut facts = facts_for(&entry, &packet);
        facts.witness = Some(&witness);
        facts.finding_id = Some(finding_id);
        let card = assemble_repair_card(&facts)?;
        let route = card
            .detail_references
            .iter()
            .find(|reference| reference.family == RepairCardDetailFamily::FixInstruction)
            .and_then(|reference| reference.route.as_deref())
            .ok_or_else(|| "fix instruction route missing".to_string())?;
        // Host-aware quoting (#7729): the route quotes the finding id in the
        // running host's shell form.
        let expected = format!("ripr explain {}", shell_arg(finding_id));
        if route != expected {
            return Err(format!("expected `{expected}`, got `{route}`"));
        }
        Ok(())
    }

    #[test]
    fn packet_queue_omission_marks_canonical_packet_unavailable() -> Result<(), String> {
        let entry = weakly_gripped_entry();
        let empty_packet = r#"{"schema_version":"0.5","packets":[]}"#;
        let card = assemble_repair_card(&facts_for(&entry, empty_packet))?;
        let reference = card
            .detail_references
            .iter()
            .find(|reference| reference.family == RepairCardDetailFamily::CanonicalPacket)
            .ok_or_else(|| "canonical packet reference missing".to_string())?;
        if reference.state != RepairCardDetailState::Unavailable {
            return Err(
                "a seam the packet queue omits must not project a current canonical packet"
                    .to_string(),
            );
        }
        if reference
            .unavailable_reason
            .as_deref()
            .is_none_or(str::is_empty)
        {
            return Err("the omitted packet must record an exact unavailable reason".to_string());
        }
        Ok(())
    }

    fn attempt_at(repository_head: &str, seam_id: &str) -> Result<RepairAttemptManifest, String> {
        Ok(RepairAttemptManifest {
            schema_version: REPAIR_ATTEMPT_SCHEMA_VERSION.to_string(),
            kind: "repair_attempt".to_string(),
            repair_attempt_id: RepairAttemptId::parse("repair-attempt-0123456789abcdef01234567")
                .map_err(|error| format!("test attempt id: {error}"))?,
            state: RepairAttemptState::AwaitingEdit,
            root: ".".to_string(),
            repository_head: repository_head.to_string(),
            producer_version: "test".to_string(),
            seam_id: seam_id.to_string(),
            created_unix_ms: 0,
            artifacts: Vec::new(),
            next_command: "ripr agent repair --root . --seam-id seam-a --phase after".to_string(),
            limitations: Vec::new(),
            non_claims: Vec::new(),
            after: None,
            last_after_refusal: None,
            after_refusal_count: 0,
            after_refusal_history: Vec::new(),
            terminal_artifacts: Vec::new(),
            store: None,
        })
    }

    #[test]
    fn attempt_recorded_at_another_head_is_stale_not_current() -> Result<(), String> {
        let entry = weakly_gripped_entry();
        let attempt = attempt_at("deadbeef", entry.seam.id().as_str())?;
        let packet = packet_for(&entry);
        let mut facts = facts_for(&entry, &packet);
        facts.attempt = Some(&attempt);
        let card = assemble_repair_card(&facts)?;
        let reference = card
            .detail_references
            .iter()
            .find(|reference| reference.family == RepairCardDetailFamily::RepairAttemptStatus)
            .ok_or_else(|| "repair attempt reference missing".to_string())?;
        if reference.state != RepairCardDetailState::Stale {
            return Err(
                "an attempt recorded at another head must project stale, not current".to_string(),
            );
        }
        if reference.route.as_deref().is_none() {
            return Err("a stale attempt keeps its typed status route".to_string());
        }
        Ok(())
    }

    #[test]
    fn dirty_evidence_tree_projects_accepted_dirty_draft_on_both_axes() -> Result<(), String> {
        let entry = weakly_gripped_entry();
        let packet = packet_for(&entry);
        let mut facts = facts_for(&entry, &packet);
        facts.currentness = RepairCardSnapshotCurrentness::AcceptedDirtyDraft;
        let card = assemble_repair_card(&facts)?;
        if card.snapshot.currentness != RepairCardSnapshotCurrentness::AcceptedDirtyDraft {
            return Err(
                "a dirty evidence scope must project accepted_dirty_draft on the snapshot axis"
                    .to_string(),
            );
        }
        if card.done_when.currentness != CardCurrentnessGoal::AcceptedDirtyDraft {
            return Err(
                "a dirty evidence scope must project accepted_dirty_draft on the done_when axis"
                    .to_string(),
            );
        }
        // The dirty-draft state is load-bearing identity, not presentation.
        let clean = assemble_repair_card(&facts_for(&entry, &packet))?;
        if clean.repair_card_id == card.repair_card_id {
            return Err("the currentness projection must remint the semantic identity".to_string());
        }
        Ok(())
    }

    #[test]
    fn clean_evidence_tree_projects_current_on_both_axes() -> Result<(), String> {
        let entry = weakly_gripped_entry();
        let packet = packet_for(&entry);
        let card = assemble_repair_card(&facts_for(&entry, &packet))?;
        if card.snapshot.currentness != RepairCardSnapshotCurrentness::Current {
            return Err(
                "a clean evidence scope must project current on the snapshot axis".to_string(),
            );
        }
        if card.done_when.currentness != CardCurrentnessGoal::Current {
            return Err(
                "a clean evidence scope must project current on the done_when axis".to_string(),
            );
        }
        Ok(())
    }

    /// #5005: the packet bytes the card content-hashes must carry no absolute
    /// checkout spelling. Binding the checkout the way the pre-fix producer
    /// did (`bound_root`) would embed the absolutized working directory in
    /// every `next`-block command string and leak it into
    /// `detail_digest`.
    #[test]
    fn card_packet_json_binds_no_checkout_spelling() -> Result<(), String> {
        let entry = weakly_gripped_entry();
        let first = card_packet_json(&entry);
        let second = card_packet_json(&entry);
        if first != second {
            return Err("the content-bound packet render is not deterministic".to_string());
        }
        let bound_cwd = crate::agent::loop_commands::bound_root(".");
        if bound_cwd == "." {
            return Err("the test working directory must resolve to an absolute root".to_string());
        }
        if first.contains(&bound_cwd) {
            return Err(
                "the content-bound packet render leaks the absolute checkout spelling".to_string(),
            );
        }
        Ok(())
    }

    /// #5005: equivalent evidence assembled through the producer's own
    /// content-bound render mints one card identity — the packet content
    /// carries no root spelling to differ on.
    #[test]
    fn equivalent_evidence_mints_one_card_identity_through_the_portable_render()
    -> Result<(), String> {
        let entry = weakly_gripped_entry();
        let first = card_packet_json(&entry);
        let second = card_packet_json(&entry);
        let first_card = assemble_repair_card(&facts_for(&entry, &first))?;
        let second_card = assemble_repair_card(&facts_for(&entry, &second))?;
        if first_card.repair_card_id != second_card.repair_card_id {
            return Err("equivalent evidence minted different card identities".to_string());
        }
        if first_card.complete_evidence_digest != second_card.complete_evidence_digest {
            return Err(
                "equivalent evidence minted different complete evidence identities".to_string(),
            );
        }
        Ok(())
    }

    /// #5008 review: the probe pathspecs keep the filesystem spelling.
    /// `display_path` percent-encodes `%` for presentation; feeding that
    /// spelling to git would probe a path that does not exist and silently
    /// read a dirty scope as clean.
    #[test]
    fn evidence_probe_paths_keep_the_filesystem_spelling() -> Result<(), String> {
        let seam = RepoSeam::new(
            "src/price%off.rs",
            "pricing::discounted_total",
            SeamKind::PredicateBoundary,
            42,
            88,
            "amount >= discount_threshold",
            RequiredDiscriminator::BoundaryValue {
                description: "amount >= discount_threshold".to_string(),
            },
            ExpectedSink::ReturnValue,
        );
        let seam_id = seam.id().clone();
        let entry = ClassifiedSeam {
            seam,
            evidence: TestGripEvidence {
                seam_id,
                related_tests: Vec::new(),
                reach: stage(StageState::Yes),
                activate: stage(StageState::Yes),
                propagate: stage(StageState::Yes),
                observe: stage(StageState::Yes),
                discriminate: stage(StageState::No),
                observed_values: Vec::new(),
                missing_discriminators: Vec::new(),
                statically_contradicted_related_tests: 0,
                new_test_target: None,
            },
            class: SeamGripClass::WeaklyGripped,
        };
        let paths = evidence_probe_paths(&entry);
        if paths != vec![std::path::PathBuf::from("src/price%off.rs")] {
            return Err(format!(
                "the probe must use the filesystem spelling, got {paths:?}"
            ));
        }
        Ok(())
    }

    /// #5008: a probe failure fails closed — no card, hence no `current`
    /// claim. A `.git` file with a dangling `gitdir:` pointer is fatal in
    /// every git version (unlike an invalid `.git` directory, which some
    /// versions skip during discovery), so the probe errors exactly as it
    /// would outside a repo, independent of where the test temp directory
    /// lives.
    #[test]
    fn evidence_tree_currentness_probe_failure_fails_closed() -> Result<(), String> {
        let probe_root = std::env::temp_dir().join(format!(
            "ripr-card-currentness-probe-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| format!("clock error: {error}"))?
                .as_nanos()
        ));
        std::fs::create_dir_all(&probe_root)
            .map_err(|error| format!("probe fixture directory failed: {error}"))?;
        std::fs::write(probe_root.join(".git"), "gitdir: /nonexistent/ripr-probe\n")
            .map_err(|error| format!("probe fixture gitfile failed: {error}"))?;
        let entry = weakly_gripped_entry();
        let outcome = evidence_tree_currentness(&probe_root, &entry);
        std::fs::remove_dir_all(&probe_root)
            .map_err(|error| format!("probe fixture cleanup failed: {error}"))?;
        let Err(_message) = outcome else {
            return Err(
                "a dirty-state probe failure must fail closed, not default to current".to_string(),
            );
        };
        Ok(())
    }

    /// Run one fixture git command with the repository-routing environment
    /// removed: a hook or caller carrying GIT_DIR/GIT_WORK_TREE would
    /// redirect the fixture setup at that repository instead of the temp
    /// fixture.
    fn currentness_test_git(root: &std::path::Path, args: &[&str]) -> Result<(), String> {
        let output = std::process::Command::new("git")
            .current_dir(root)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_COMMON_DIR")
            .env_remove("GIT_OBJECT_DIRECTORY")
            .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
            .args(args)
            .output()
            .map_err(|error| format!("git {args:?} failed to run: {error}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    }

    fn currentness_git_fixture(label: &str) -> Result<std::path::PathBuf, String> {
        let root = std::env::temp_dir().join(format!(
            "ripr-card-currentness-batch-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| format!("clock error: {error}"))?
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("fixture src failed: {error}"))?;
        currentness_test_git(&root, &["init", "--initial-branch", "main"])
            .or_else(|_| currentness_test_git(&root, &["init"]))?;
        currentness_test_git(&root, &["config", "user.email", "test@example.com"])?;
        currentness_test_git(&root, &["config", "user.name", "Test"])?;
        currentness_test_git(&root, &["config", "commit.gpgsign", "false"])?;
        Ok(root)
    }

    fn entry_for_file(file: &str) -> ClassifiedSeam {
        weakly_gripped_entry_for_seam(RepoSeam::new(
            file,
            "pricing::discounted_total",
            SeamKind::PredicateBoundary,
            42,
            8,
            "amount >= discount_threshold",
            RequiredDiscriminator::BoundaryValue {
                description: "amount >= discount_threshold".to_string(),
            },
            ExpectedSink::ReturnValue,
        ))
    }

    /// #7292 discriminating test: the batched currentness probe serves many
    /// bound seams with ONE `git status` child. Before the batch, the MCP
    /// refresh ran one child per bound seam (twenty sequential children for
    /// one mid-corpus refresh). With the probe back on a per-entry loop
    /// (pre-fix shape), the child count rises with the seam count and this
    /// assertion fails.
    #[test]
    fn batched_currentness_spawns_one_child_for_many_seams() -> Result<(), String> {
        use std::sync::atomic::Ordering;
        let root = currentness_git_fixture("one-child")?;
        let result = (|| -> Result<(), String> {
            let mut entries = Vec::new();
            for index in 0..25 {
                let file = format!("src/seam_{index:02}.rs");
                std::fs::write(root.join(&file), format!("pub fn f_{index}() {{}}\n"))
                    .map_err(|error| format!("write {file}: {error}"))?;
                entries.push(entry_for_file(&file));
            }
            currentness_test_git(&root, &["add", "."])?;
            currentness_test_git(&root, &["commit", "-q", "-m", "base"])?;
            // One tracked edit: seam 00's scope is dirty, the rest clean.
            std::fs::write(root.join("src/seam_00.rs"), "pub fn dirty() {}\n")
                .map_err(|error| format!("dirty write: {error}"))?;

            let refs: Vec<&ClassifiedSeam> = entries.iter().collect();
            // Differential child count: other lib tests probe concurrently
            // and share this process-global counter, so the assertion reads
            // the counter across a one-seam run and the 25-seam run. The
            // batched probe is flat in the seam count (delta ~0, tolerating
            // a racing probe); the pre-fix per-entry loop grows with the
            // seam count (delta 24).
            CURRENTNESS_PROBE_CHILDREN.store(0, Ordering::Release);
            let _single_outcomes = evidence_tree_currentness_batch(&root, &refs[..1])?;
            let after_single = CURRENTNESS_PROBE_CHILDREN.load(Ordering::Acquire);
            let outcomes = evidence_tree_currentness_batch(&root, &refs)?;
            let after_all = CURRENTNESS_PROBE_CHILDREN.load(Ordering::Acquire);
            let delta = after_all.saturating_sub(after_single);
            // The bound sits far from both shapes: the batched probe is flat
            // (delta ~0 plus concurrent-probe noise), the per-entry loop
            // grows by 24. Concurrent lib tests share this process-global
            // counter, so the tolerance absorbs a racing probe burst while
            // staying well under the per-entry delta.
            if delta > 12 {
                return Err(format!(
                    "the currentness probe grew from {after_single} to {after_all} git children \
                     between 1 and 25 seams: it is per-entry, not batched"
                ));
            }
            if outcomes[0] != RepairCardSnapshotCurrentness::AcceptedDirtyDraft {
                return Err("the dirty seam must read AcceptedDirtyDraft".to_string());
            }
            if outcomes[1..]
                .iter()
                .any(|outcome| *outcome != RepairCardSnapshotCurrentness::Current)
            {
                return Err("the clean seams must read Current".to_string());
            }
            Ok(())
        })();
        let _ = std::fs::remove_dir_all(&root);
        result
    }

    /// The batch keeps the single-entry contract per seam: over a fixture
    /// mixing a tracked edit, an untracked draft and an ignored file, every
    /// batched outcome equals the per-entry probe's own.
    #[test]
    fn batched_currentness_matches_the_single_probe_per_entry() -> Result<(), String> {
        let root = currentness_git_fixture("parity")?;
        let result = (|| -> Result<(), String> {
            std::fs::write(root.join("src/tracked.rs"), "pub fn a() {}\n")
                .map_err(|error| format!("write: {error}"))?;
            std::fs::write(root.join("src/ignored.rs"), "pub fn b() {}\n")
                .map_err(|error| format!("write: {error}"))?;
            currentness_test_git(&root, &["add", "src/tracked.rs"])?;
            currentness_test_git(&root, &["commit", "-q", "-m", "base"])?;
            std::fs::write(root.join(".gitignore"), "src/ignored.rs\n")
                .map_err(|error| format!("gitignore: {error}"))?;
            std::fs::write(root.join("src/untracked.rs"), "pub fn c() {}\n")
                .map_err(|error| format!("write: {error}"))?;
            // Fixture roles: `src/ignored.rs` stays untracked and ignored
            // (the probe uses --ignored, so its scope reads dirty), and
            // `src/untracked.rs` stays untracked and reported.
            let entries = [
                entry_for_file("src/tracked.rs"),
                entry_for_file("src/untracked.rs"),
                entry_for_file("src/ignored.rs"),
                entry_for_file("src/missing.rs"),
            ];
            // The tracked file gets a working-tree edit (dirty scope).
            std::fs::write(root.join("src/tracked.rs"), "pub fn a2() {}\n")
                .map_err(|error| format!("edit: {error}"))?;

            let refs: Vec<&ClassifiedSeam> = entries.iter().collect();
            let batched = evidence_tree_currentness_batch(&root, &refs)?;
            for (entry, batched) in entries.iter().zip(&batched) {
                let single = evidence_tree_currentness(&root, entry)?;
                if *batched != single {
                    return Err(format!(
                        "batched outcome {batched:?} diverges from the single probe {single:?} for \
                         {}",
                        entry.seam.file().display()
                    ));
                }
            }
            // Sanity on the fixture itself: the edited tracked file reads
            // dirty, the absent file reads current.
            if batched[0] != RepairCardSnapshotCurrentness::AcceptedDirtyDraft {
                return Err("the edited tracked file must be dirty".to_string());
            }
            if batched[3] != RepairCardSnapshotCurrentness::Current {
                return Err("the absent file must be current".to_string());
            }
            Ok(())
        })();
        let _ = std::fs::remove_dir_all(&root);
        result
    }

    /// #5007: the producer split keeps the typed kind and the verbatim human
    /// prose together on every path, and the `String` conversion (what LSP
    /// omission and the usability measurement consume) preserves the prose.
    #[test]
    fn agent_card_error_split_carries_kind_and_prose() -> Result<(), String> {
        let refusal = AgentCardError::witness_unavailable("witness prose".to_string());
        match &refusal {
            AgentCardError::Refusal { kind, message } => {
                if *kind != AgentCardRefusalKind::WitnessUnavailable {
                    return Err("the witness failure must carry the witness kind".to_string());
                }
                if message != "witness prose" {
                    return Err("the refusal must keep the verbatim human prose".to_string());
                }
            }
            AgentCardError::Operational(_) => {
                return Err("the witness failure is a refusal, not operational".to_string());
            }
        }
        if refusal.message() != "witness prose" {
            return Err("message() must render the verbatim human prose".to_string());
        }
        let rendered: String = refusal.into();
        if rendered != "witness prose" {
            return Err("the String rendering must stay the human prose".to_string());
        }
        match AgentCardError::identity_unnameable("identity prose".to_string()) {
            AgentCardError::Refusal {
                kind: AgentCardRefusalKind::IdentityUnnameable,
                ..
            } => {}
            _ => return Err("identity failure must carry the identity kind".to_string()),
        }
        match AgentCardError::budget_overflow("budget prose".to_string()) {
            AgentCardError::Refusal {
                kind: AgentCardRefusalKind::BudgetOverflow,
                ..
            } => {}
            _ => return Err("builder failure must carry the budget kind".to_string()),
        }
        match AgentCardError::operational("ops prose".to_string()) {
            AgentCardError::Operational(message) if message == "ops prose" => {}
            _ => return Err("operational failures must stay operational".to_string()),
        }
        Ok(())
    }
}

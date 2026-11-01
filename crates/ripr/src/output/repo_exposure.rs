//! Render the classified seam inventory as a repo exposure report
//! (JSON + Markdown).
//!
//! Schema is documented in `docs/OUTPUT_SCHEMA.md` under
//! `repo-exposure.json`. The schema version pin is
//! `REPO_EXPOSURE_SCHEMA_VERSION`; bumping it requires updating the
//! doc and any downstream consumers in lockstep.

use crate::agent::artifact::{
    CONTENT_SHA256_PLACEHOLDER, RepoExposureArtifactContext, Sha256Writer,
    repo_exposure_artifact_metadata,
};
use crate::analysis::ClassifiedSeam;
use crate::analysis::SeamLimitInfo;
use crate::analysis::SeamLimitSource;
use crate::analysis::TypeScriptRepoReadiness;
use crate::analysis::canonical_gap::{CanonicalGapIdentity, canonical_gap_identities};
use crate::analysis::resource_cost::trace_latency_phase;
use crate::analysis::seams::SeamGripClass;
use crate::output::evidence_record::{evidence_record_for, evidence_record_json_value};
use crate::output::json::escape as json_escape;
use crate::output::json::escape_into as json_escape_into;
use crate::output::markdown::{code_span, inline_prose_literal, table_cell_text};
use crate::output::path::display_path;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Guidance disclosure emitted when a predominantly TypeScript/JavaScript
/// workspace is scanned in repo-exposure mode and contributes zero seams.
///
/// TypeScript is diff-first by design; the repo-exposure scan is
/// Rust-seam-oriented and will always return zero TS seams regardless of
/// how much TypeScript the workspace contains. Without this named disclosure,
/// the user receives a silent empty result with no signal to try diff mode.
///
/// Per RIPR-SPEC-0089: this is a named limitation only — it never fabricates
/// TS seams or exposes TS findings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TsFullRepoGuidance {
    /// Number of TypeScript/JavaScript files detected in the workspace.
    pub(crate) ts_file_count: usize,
    /// Root-level readiness facts derived from the same TypeScript preview
    /// package/test discovery producers that diff-mode findings use.
    pub(crate) readiness: TypeScriptRepoReadiness,
}

impl TsFullRepoGuidance {
    /// The stable machine-readable category string for this disclosure.
    pub(crate) const CATEGORY: &'static str = "typescript_diff_first";

    /// The human-readable repair route for this disclosure.
    pub(crate) const REPAIR_ROUTE: &'static str = "TypeScript is analyzed diff-first; run \
         'ripr check' or '--diff <file>' to evaluate \
         changed TypeScript behavior. Full-repo TypeScript exposure is not \
         yet modeled (named limitation).";
}

/// Guidance disclosure emitted when a Python workspace is scanned in
/// repo-exposure mode and contributes zero seams.
///
/// Repo exposure counts Rust and Perl seams only. Python findings are
/// produced by diff-scoped `ripr check` (and by the preview repo producer),
/// but this artifact does not render them. Without a named disclosure, a
/// Python-only workspace looks like a clean empty seam inventory.
///
/// This never fabricates Python seams and does not change `run_status`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PythonRepoExposureGuidance {
    /// Number of Python files detected in the workspace.
    pub(crate) python_file_count: usize,
}

impl PythonRepoExposureGuidance {
    /// The stable machine-readable category string for this disclosure.
    pub(crate) const CATEGORY: &'static str = "python_diff_first";

    /// The human-readable repair route for this disclosure.
    ///
    /// Does not claim that full-repo Python analysis is unmodeled: the
    /// preview repo producer exists, but this seam inventory does not render
    /// its findings.
    pub(crate) const REPAIR_ROUTE: &'static str = "Python changed behavior is reviewed \
         diff-first on this seam report; run 'ripr check' \
         or '--diff <file>' to evaluate it. This report counts Rust and Perl \
         seams only and does not render Python findings, so a zero-seam \
         result is not a clean Python result (named limitation).";
}

/// Disclosure when repo exposure skipped generated Rust that `ripr check`
/// already leaves out of scope, or read Rust files a parse limit declined
/// (#9308). One carrier keeps every renderer entry point from growing a
/// parameter; `paths` is empty when only declined files are disclosed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GeneratedRustSkip {
    pub(crate) paths: Vec<PathBuf>,
    naming_only_paths: Vec<PathBuf>,
    pub(crate) declined: Vec<crate::analysis::DeclinedRustFile>,
}

impl GeneratedRustSkip {
    pub(crate) const CATEGORY: &'static str = "generated_rust_source_skipped";

    /// `run_status` of a repo-exposure run whose Rust files were declined by a
    /// parse limit and not otherwise cut by a seam limit (#9308).
    pub(crate) const DECLINED_RUN_STATUS: &'static str = "limited_declined_files";
    pub(crate) const DECLINED_CATEGORY: &'static str = "rust_files_declined";

    #[cfg(test)]
    pub(crate) fn from_paths(paths: Vec<PathBuf>, naming_only_paths: Vec<PathBuf>) -> Option<Self> {
        Self::from_parts(paths, naming_only_paths, Vec::new())
    }

    pub(crate) fn from_parts(
        paths: Vec<PathBuf>,
        naming_only_paths: Vec<PathBuf>,
        declined: Vec<crate::analysis::DeclinedRustFile>,
    ) -> Option<Self> {
        if paths.is_empty() && declined.is_empty() {
            None
        } else {
            Some(Self {
                paths,
                naming_only_paths,
                declined,
            })
        }
    }

    fn declined_repair_route(&self) -> &'static str {
        "Split the listed file(s) below the Rust parse budget, or save them as UTF-8, then re-run; \
         until then their tests and seams are under-credited and this run is not a complete result."
    }

    fn declined_detail(&self) -> String {
        let listed = self
            .declined
            .iter()
            .take(3)
            .map(|file| {
                let token = file
                    .reason
                    .strip_prefix("static limit ")
                    .and_then(|reason| reason.split(':').next())
                    .unwrap_or("parse_declined");
                format!(
                    "{} ({token})",
                    file.path.to_string_lossy().replace('\\', "/")
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let more = self.declined.len().saturating_sub(3);
        let more = if more > 0 {
            format!(" and {more} more")
        } else {
            String::new()
        };
        format!(
            "{} Rust file(s) were declined by a parse limit and read lexically, which emits no \
             probe shapes: {listed}{more}",
            self.declined.len()
        )
    }

    fn listed_paths(&self) -> String {
        bounded_generated_path_listing(&self.paths)
    }

    pub(crate) fn repair_route(&self) -> String {
        crate::analysis::generated_rust_recovery(&self.paths, &self.naming_only_paths)
    }

    fn detail(&self) -> String {
        format!(
            "{} generated Rust file(s) were intentionally skipped by the generated-file \
             conventions, configured patterns, generator headers or vendor markers: {}",
            self.paths.len(),
            self.listed_paths()
        )
    }
}

fn bounded_generated_path_listing(paths: &[PathBuf]) -> String {
    let normalized: Vec<String> = paths
        .iter()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .collect();
    let shown = normalized
        .iter()
        .take(3)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    let more = normalized.len().saturating_sub(3);
    let listed = if more > 0 {
        format!("{shown} and {more} more")
    } else {
        shown
    };
    if listed.chars().count() > 160 {
        format!("{}…", listed.chars().take(159).collect::<String>())
    } else {
        listed
    }
}

pub(crate) const REPO_EXPOSURE_SCHEMA_VERSION: &str = "0.5";
pub(crate) const REPO_EXPOSURE_SUMMARY_SCHEMA_VERSION: &str = "0.1";

/// Cap on related-tests rendered per seam in the JSON output. The
/// existing `test_grip_evidence::find_related_tests` heuristic is
/// permissive (any test whose file or name shares the seam owner
/// counts), so a single high-traffic file (e.g. `classifier.rs`) can
/// fan out to hundreds of tests per seam. The cap keeps the artifact
/// review-sized; tightening the heuristic itself is a future PR.
const MAX_RELATED_TESTS_PER_SEAM_JSON: usize = 8;
const MAX_TOP_FILES_SUMMARY_JSON: usize = 25;

const REPO_EXPOSURE_INVENTORY_LIMIT_CATEGORY: &str = "repo_seam_limit_applied";
const REPO_EXPOSURE_PILOT_BUDGET_CATEGORY: &str = "pilot_seam_budget_applied";
const REPO_EXPOSURE_INVENTORY_LIMIT_CONTROL: &str = "RIPR_REPO_EXPOSURE_SEAM_LIMIT";
const REPO_EXPOSURE_PILOT_BUDGET_CONTROL: &str = "RIPR_PILOT_SEAM_BUDGET";
const REPO_EXPOSURE_SEAM_LIMIT_RUN_STATUS: &str = "seam_limit_applied";

/// The producer that actually limited this artifact's seam population.
/// Default/configured describes the value's source, not the cap's identity.
#[derive(Clone, Copy)]
pub(crate) enum RepoExposureLimit<'a> {
    Inventory(&'a SeamLimitInfo),
    PilotBudget(&'a SeamLimitInfo),
    Both(&'a SeamLimitInfo, &'a SeamLimitInfo),
}

impl<'a> RepoExposureLimit<'a> {
    pub(crate) fn info(self) -> &'a SeamLimitInfo {
        match self {
            Self::Inventory(info) | Self::PilotBudget(info) | Self::Both(info, _) => info,
        }
    }

    pub(crate) fn parts(self) -> [Option<Self>; 2] {
        match self {
            Self::Both(budget, inventory) => [
                Some(Self::PilotBudget(budget)),
                Some(Self::Inventory(inventory)),
            ],
            one => [Some(one), None],
        }
    }

    pub(crate) fn category(self) -> &'static str {
        match self {
            Self::Inventory(_) => REPO_EXPOSURE_INVENTORY_LIMIT_CATEGORY,
            Self::PilotBudget(_) | Self::Both(_, _) => REPO_EXPOSURE_PILOT_BUDGET_CATEGORY,
        }
    }

    pub(crate) fn control(self) -> &'static str {
        match self {
            Self::Inventory(_) => REPO_EXPOSURE_INVENTORY_LIMIT_CONTROL,
            Self::PilotBudget(_) | Self::Both(_, _) => REPO_EXPOSURE_PILOT_BUDGET_CONTROL,
        }
    }

    pub(crate) fn repair_route(self) -> &'static str {
        match (self, &self.info().source) {
            (Self::Inventory(_), SeamLimitSource::Default) => {
                "Set RIPR_REPO_EXPOSURE_SEAM_LIMIT=0 to analyze all seams, or scope the run to a change with `ripr check --base <REV>` (or `ripr check --diff <PATH>`)."
            }
            (Self::Inventory(_), SeamLimitSource::Configured) => {
                "Remove or raise RIPR_REPO_EXPOSURE_SEAM_LIMIT to analyze more seams, or scope the run to a change with `ripr check --base <REV>` (or `ripr check --diff <PATH>`)."
            }
            (Self::PilotBudget(_) | Self::Both(_, _), SeamLimitSource::Default) => {
                "Set RIPR_PILOT_SEAM_BUDGET=0 to disable the pilot artifact budget."
            }
            (Self::PilotBudget(_) | Self::Both(_, _), SeamLimitSource::Configured) => {
                "Set RIPR_PILOT_SEAM_BUDGET=0 to disable the pilot artifact budget, or raise it to render more seams in the pilot artifacts."
            }
        }
    }

    fn markdown_repair_route(self) -> &'static str {
        match (self, &self.info().source) {
            (Self::Inventory(_), SeamLimitSource::Default) => {
                "set RIPR_REPO_EXPOSURE_SEAM_LIMIT=0 to analyze all seams, or scope the run to a change with `ripr check --base <REV>` (or `ripr check --diff <PATH>`)"
            }
            (Self::Inventory(_), SeamLimitSource::Configured) => {
                "remove or raise RIPR_REPO_EXPOSURE_SEAM_LIMIT to analyze more seams, or scope the run to a change with `ripr check --base <REV>` (or `ripr check --diff <PATH>`)"
            }
            (Self::PilotBudget(_) | Self::Both(_, _), SeamLimitSource::Default) => {
                "set RIPR_PILOT_SEAM_BUDGET=0 to disable the pilot artifact budget"
            }
            (Self::PilotBudget(_) | Self::Both(_, _), SeamLimitSource::Configured) => {
                "set RIPR_PILOT_SEAM_BUDGET=0 to disable the pilot artifact budget, or raise it to render more seams in the pilot artifacts"
            }
        }
    }
}

/// Environment variable overriding the pilot `repo-exposure.json` byte budget
/// (#12108). `0` removes the cap; a positive integer sets the budget in
/// bytes. When unset, `DEFAULT_PILOT_REPO_EXPOSURE_BYTES` applies.
pub(crate) const PILOT_REPO_EXPOSURE_BYTES_ENV: &str = "RIPR_PILOT_REPO_EXPOSURE_BYTES";

/// Default byte budget for the `seams[]` entries of pilot's
/// `repo-exposure.json`. `RIPR_PILOT_SEAM_BUDGET` bounds the seam count only:
/// at the default 2000 seams the file measured 13.1 MB on this repository
/// (about 6.6 KB per seam, mostly each seam's `evidence_record`). The budget
/// keeps the file agent-readable; the full set stays one
/// `RIPR_PILOT_REPO_EXPOSURE_BYTES=0` rerun away.
pub(crate) const DEFAULT_PILOT_REPO_EXPOSURE_BYTES: usize = 8_000_000;

/// `run_status` of a pilot `repo-exposure.json` whose `seams[]` the byte
/// budget cut and no seam limit also marked. Uses the `limited_*` family so a
/// reader never takes the file for a complete one.
pub(crate) const REPO_EXPOSURE_BYTE_BOUND_RUN_STATUS: &str = "limited_repo_exposure_bytes";
const REPO_EXPOSURE_BYTE_BOUND_CATEGORY: &str = "pilot_repo_exposure_byte_bound_applied";
const REPO_EXPOSURE_BYTE_BOUND_ROUTE: &str = "Set RIPR_PILOT_REPO_EXPOSURE_BYTES=0 and rerun `ripr pilot` to write every seam, or read `agent-seam-packets.json` and `pilot-summary.json`, which this budget does not cut.";

/// Resolve the pilot repo-exposure byte budget. `None` is the unbounded
/// opt-out. Fails closed on an unparseable value, naming the variable.
pub(crate) fn pilot_repo_exposure_byte_budget() -> Result<Option<(usize, SeamLimitSource)>, String>
{
    pilot_repo_exposure_byte_budget_from_env(std::env::var(PILOT_REPO_EXPOSURE_BYTES_ENV))
}

fn pilot_repo_exposure_byte_budget_from_env(
    value: Result<String, std::env::VarError>,
) -> Result<Option<(usize, SeamLimitSource)>, String> {
    let env_name = PILOT_REPO_EXPOSURE_BYTES_ENV;
    match value {
        Ok(raw) => {
            let trimmed = raw.trim();
            let invalid = || {
                format!(
                    "{env_name} `{raw}` is not a byte budget: set a positive integer, or 0 to remove the cap"
                )
            };
            match trimmed.parse::<usize>() {
                Ok(0) if trimmed == "0" => Ok(None),
                Ok(0) | Err(_) => Err(invalid()),
                Ok(limit) => Ok(Some((limit, SeamLimitSource::Configured))),
            }
        }
        Err(std::env::VarError::NotPresent) => Ok(Some((
            DEFAULT_PILOT_REPO_EXPOSURE_BYTES,
            SeamLimitSource::Default,
        ))),
        Err(std::env::VarError::NotUnicode(value)) => Err(format!(
            "{env_name} is not valid UTF-8 (got {value:?}): set a positive byte budget, or 0 to remove the cap"
        )),
    }
}

/// A byte-budget cut of the `seams[]` entries of one `repo-exposure.json`.
/// It records which entries (by index into the classified list) the file
/// carries, and what the budget dropped, so the document discloses it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RepoExposureByteBound {
    kept: Vec<usize>,
    pub(crate) total: usize,
    pub(crate) budget: usize,
    /// Bytes of the `seams[]` entries that were kept.
    pub(crate) bytes: usize,
    pub(crate) source: SeamLimitSource,
    /// Seams the selection skipped because they did not fit the remaining
    /// budget while a later seam of the same priority group did (#12151): the
    /// holes in the kept set. Seams dropped only because the budget ran out
    /// are not holes and are not counted; `total - rendered` is the number
    /// of seams absent from the file either way.
    pub(crate) skipped_over_budget: usize,
}

impl RepoExposureByteBound {
    pub(crate) fn rendered(&self) -> usize {
        self.kept.len()
    }
}

/// The one predicate for "this repo-exposure document's `seams[]` was cut by
/// the pilot byte budget" (#12108). It reads the limitation category as well
/// as `run_status`, because a seam cap that also fired keeps
/// `run_status = seam_limit_applied` and the byte cut is then only a
/// `limitations[]` entry. Every consumer that compares seams uses this.
pub(crate) fn is_byte_bounded_repo_exposure(document: &Value) -> bool {
    byte_bounded_from_parts(
        document.get("run_status").and_then(Value::as_str),
        document.get("limitations").and_then(Value::as_array),
    )
}

/// The predicate over an already-parsed `run_status` and `limitations[]`, so a
/// typed consumer does not parse the document a second time.
pub(crate) fn byte_bounded_from_parts(
    run_status: Option<&str>,
    limitations: Option<&Vec<Value>>,
) -> bool {
    run_status == Some(REPO_EXPOSURE_BYTE_BOUND_RUN_STATUS)
        || limitations.is_some_and(|entries| {
            entries.iter().any(|entry| {
                entry.get("category").and_then(Value::as_str)
                    == Some(REPO_EXPOSURE_BYTE_BOUND_CATEGORY)
            })
        })
}

/// The controls whose seam cut limited this document's `seams[]` (#12150), in
/// `limitations[]` order, or empty when no seam cut applied. A seam cap
/// (`RIPR_REPO_EXPOSURE_SEAM_LIMIT` or the pilot `RIPR_PILOT_SEAM_BUDGET`)
/// keeps a prefix of the inventory, so a seam absent from the file is
/// unobserved, not removed; this is the same class as the byte bound. It reads
/// the categories `RepoExposureLimit` writes, and falls back to `run_status`
/// alone (both controls named) for a file whose limitation entry is missing.
/// Declined-file and generated-skip limitations are not seam cuts.
pub(crate) fn seam_limit_cut_controls_from_parts(
    run_status: Option<&str>,
    limitations: Option<&Vec<Value>>,
) -> Vec<&'static str> {
    let mut controls = Vec::new();
    for entry in limitations.into_iter().flatten() {
        let control = match entry.get("category").and_then(Value::as_str) {
            Some(REPO_EXPOSURE_INVENTORY_LIMIT_CATEGORY) => REPO_EXPOSURE_INVENTORY_LIMIT_CONTROL,
            Some(REPO_EXPOSURE_PILOT_BUDGET_CATEGORY) => REPO_EXPOSURE_PILOT_BUDGET_CONTROL,
            _ => continue,
        };
        if !controls.contains(&control) {
            controls.push(control);
        }
    }
    if controls.is_empty() && run_status == Some(REPO_EXPOSURE_SEAM_LIMIT_RUN_STATUS) {
        controls.push(REPO_EXPOSURE_PILOT_BUDGET_CONTROL);
        controls.push(REPO_EXPOSURE_INVENTORY_LIMIT_CONTROL);
    }
    controls
}

/// [`seam_limit_cut_controls_from_parts`] over a parsed document.
pub(crate) fn seam_limit_cut_controls(document: &Value) -> Vec<&'static str> {
    seam_limit_cut_controls_from_parts(
        document.get("run_status").and_then(Value::as_str),
        document.get("limitations").and_then(Value::as_array),
    )
}

/// The refusal clause every comparing consumer appends for a seam-cut file:
/// it names the control(s) and the repair route.
pub(crate) fn seam_limit_cut_repair(controls: &[&str]) -> String {
    format!(
        "set {}=0 and regenerate the file (`ripr pilot` for the pilot budget, `ripr check --format repo-exposure-json` for the inventory limit), then compare",
        controls.join("=0 and ")
    )
}

/// The full control list for a document cut both ways (#12226): the pilot byte
/// budget first, then every seam-limit control `seam_cut` names. A file the
/// byte bound and a seam limit both cut (the default above 2,000 seams) needs
/// every control raised, so the first refusal lists them all and the user gets
/// one repair route instead of a second refusal. `seam_cut` is
/// [`seam_limit_cut_controls`] of the same document; an empty `seam_cut` is a
/// byte-only cut, which keeps its own single-control message, so callers branch
/// on that before using this.
pub(crate) fn byte_and_seam_cut_controls(seam_cut: &[&'static str]) -> Vec<&'static str> {
    let mut controls = vec![PILOT_REPO_EXPOSURE_BYTES_ENV];
    controls.extend_from_slice(seam_cut);
    controls
}

/// Bytes the writer adds between two `seams[]` entries (`,\n`).
const SEAM_SEPARATOR_BYTES: usize = 2;

/// Choose the `seams[]` entries that fit `budget` bytes, in three priority
/// groups: seams `keep` names (the current change) in inventory order, then
/// the other seams in `ranked` order (the pilot ranking, #12152), then every
/// remaining seam in inventory order. A seam that does not fit the remaining
/// budget is skipped and counted (#12151), so one oversized seam does not
/// starve the smaller seams behind it in the same group. Once a group has
/// skipped a seam the selection ends: a lower-priority seam never fills budget
/// a higher-priority one could not use. The kept seams are still written in
/// inventory order. `None` when there is no budget or every entry fits: the
/// document is then unchanged.
pub(crate) fn bound_repo_exposure_seams(
    classified: &[ClassifiedSeam],
    budget: Option<(usize, SeamLimitSource)>,
    keep: impl Fn(&ClassifiedSeam) -> bool,
    ranked: &[&ClassifiedSeam],
) -> Option<RepoExposureByteBound> {
    let (budget, source) = budget?;
    let canonical_gaps = canonical_gap_identities(classified);
    let sizes: Vec<usize> = classified
        .iter()
        .map(|entry| render_seam_json(entry, &canonical_gaps).len())
        .collect();
    // The writer joins entries with `,\n`, so a selection of n entries costs
    // their bytes plus 2 * (n - 1): the budget bounds the written `seams[]`
    // content, not just the entry payloads.
    if sizes.iter().sum::<usize>().saturating_add(
        classified
            .len()
            .saturating_sub(1)
            .saturating_mul(SEAM_SEPARATOR_BYTES),
    ) <= budget
    {
        return None;
    }
    // Position of each seam in the pilot ranking. Keyed by address: `ranked`
    // borrows from `classified`, and the pointer is only compared, never read.
    let rank_position: std::collections::HashMap<*const ClassifiedSeam, usize> = ranked
        .iter()
        .enumerate()
        .map(|(position, entry)| (std::ptr::from_ref::<ClassifiedSeam>(entry), position))
        .collect();
    let rank_of = |idx: usize| {
        rank_position
            .get(&std::ptr::from_ref(&classified[idx]))
            .copied()
    };
    let current_change: Vec<usize> = (0..classified.len())
        .filter(|idx| keep(&classified[*idx]))
        .collect();
    let mut ranked_rest: Vec<usize> = (0..classified.len())
        .filter(|idx| !keep(&classified[*idx]) && rank_of(*idx).is_some())
        .collect();
    ranked_rest.sort_by_key(|idx| rank_of(*idx));
    let unranked: Vec<usize> = (0..classified.len())
        .filter(|idx| !keep(&classified[*idx]) && rank_of(*idx).is_none())
        .collect();
    let mut used = 0usize;
    let mut chosen_count = 0usize;
    let mut skipped_over_budget = 0usize;
    let mut chosen = vec![false; classified.len()];
    for group in [&current_change, &ranked_rest, &unranked] {
        let mut group_skipped = false;
        let mut pending_holes = 0usize;
        for idx in group.iter().copied() {
            let cost = sizes[idx].saturating_add(if chosen_count == 0 {
                0
            } else {
                SEAM_SEPARATOR_BYTES
            });
            if used.saturating_add(cost) > budget {
                pending_holes += 1;
                group_skipped = true;
                continue;
            }
            skipped_over_budget += pending_holes;
            pending_holes = 0;
            used += cost;
            chosen_count += 1;
            chosen[idx] = true;
        }
        if group_skipped {
            break;
        }
    }
    Some(RepoExposureByteBound {
        kept: chosen
            .iter()
            .enumerate()
            .filter_map(|(idx, chosen)| chosen.then_some(idx))
            .collect(),
        total: classified.len(),
        budget,
        bytes: used,
        source,
        skipped_over_budget,
    })
}

/// Render the repo exposure JSON.
#[cfg(test)]
pub(crate) fn render_repo_exposure_json(
    classified: &[ClassifiedSeam],
    limit_info: Option<&SeamLimitInfo>,
    ts_guidance: Option<&TsFullRepoGuidance>,
    python_guidance: Option<&PythonRepoExposureGuidance>,
) -> String {
    render_repo_exposure_json_with_generated_skip(
        classified,
        limit_info,
        ts_guidance,
        python_guidance,
        None,
    )
}

#[cfg(test)]
pub(crate) fn render_repo_exposure_json_with_generated_skip(
    classified: &[ClassifiedSeam],
    limit_info: Option<&SeamLimitInfo>,
    ts_guidance: Option<&TsFullRepoGuidance>,
    python_guidance: Option<&PythonRepoExposureGuidance>,
    generated_skip: Option<&GeneratedRustSkip>,
) -> String {
    render_repo_exposure_json_with_limit(
        classified,
        limit_info.map(RepoExposureLimit::Inventory),
        ts_guidance,
        python_guidance,
        generated_skip,
    )
}

#[cfg(test)]
pub(crate) fn render_repo_exposure_json_with_limit(
    classified: &[ClassifiedSeam],
    limit_info: Option<RepoExposureLimit<'_>>,
    ts_guidance: Option<&TsFullRepoGuidance>,
    python_guidance: Option<&PythonRepoExposureGuidance>,
    generated_skip: Option<&GeneratedRustSkip>,
) -> String {
    render_repo_exposure_json_bounded(
        classified,
        limit_info,
        ts_guidance,
        python_guidance,
        generated_skip,
        None,
    )
}

/// Like [`render_repo_exposure_json_with_limit`], with the `seams[]` entries
/// cut to `byte_bound` and the cut disclosed (#12108). The bounded document
/// carries no artifact identity, so no consumer can take it for a comparable
/// snapshot.
pub(crate) fn render_repo_exposure_json_bounded(
    classified: &[ClassifiedSeam],
    limit_info: Option<RepoExposureLimit<'_>>,
    ts_guidance: Option<&TsFullRepoGuidance>,
    python_guidance: Option<&PythonRepoExposureGuidance>,
    generated_skip: Option<&GeneratedRustSkip>,
    byte_bound: Option<&RepoExposureByteBound>,
) -> String {
    let mut bytes = Vec::new();
    if write_repo_exposure_json(
        classified,
        limit_info,
        ts_guidance,
        python_guidance,
        generated_skip,
        byte_bound,
        &mut bytes,
    )
    .is_err()
    {
        return String::new();
    }
    match String::from_utf8(bytes) {
        Ok(json) => json,
        Err(err) => String::from_utf8_lossy(&err.into_bytes()).into_owned(),
    }
}

/// Stream the repo exposure JSON without materializing the whole artifact.
///
/// Repo exposure can be multi-gigabyte on dogfood-sized workspaces because
/// each seam carries its evidence record. CLI callers use this writer path so
/// the JSON schema stays unchanged while memory pressure scales with one seam
/// record rather than the full artifact.
fn write_repo_exposure_json<W: io::Write>(
    classified: &[ClassifiedSeam],
    limit_info: Option<RepoExposureLimit<'_>>,
    ts_guidance: Option<&TsFullRepoGuidance>,
    python_guidance: Option<&PythonRepoExposureGuidance>,
    generated_skip: Option<&GeneratedRustSkip>,
    byte_bound: Option<&RepoExposureByteBound>,
    out: &mut W,
) -> io::Result<()> {
    let mut seams = SeamJsons::new(classified, 0);
    if let Some(bound) = byte_bound {
        seams.restrict_to(&bound.kept);
    }
    write_repo_exposure_json_document(
        limit_info,
        RepoExposureJsonDisclosures {
            ts_guidance,
            python_guidance,
            generated_skip,
            analysis_inputs: None,
            byte_bound,
        },
        None,
        None,
        &seams,
        out,
    )
}

/// Stream a producer-owned repo-exposure artifact with repository, revision,
/// worktree, and content identity.  The first pass hashes the exact document
/// with the fixed content placeholder; the second pass emits the same bytes
/// with the resulting digest.  This preserves the bounded-memory writer path.
pub(crate) fn write_repo_exposure_json_with_context<W: io::Write>(
    classified: &[ClassifiedSeam],
    limit_info: Option<&SeamLimitInfo>,
    ts_guidance: Option<&TsFullRepoGuidance>,
    python_guidance: Option<&PythonRepoExposureGuidance>,
    generated_skip: Option<&GeneratedRustSkip>,
    context: &RepoExposureArtifactContext,
    out: &mut W,
) -> Result<(), String> {
    let placeholder = repo_exposure_artifact_metadata(context, CONTENT_SHA256_PLACEHOLDER)?;
    let seams = SeamJsons::new(classified, PRERENDERED_SEAM_JSON_BUDGET_BYTES);
    let source_subject_started = Instant::now();
    let mut source_subject = repo_exposure_source_subject(&seams, &context.root);
    if let Some(key) = &context.corpus_key {
        let subject = source_subject
            .get_or_insert_with(|| serde_json::json!({"digest_algorithm": "sha256", "files": []}));
        subject["corpus"] = crate::output::gap_source_subject::captured_rust_corpus_subject(key);
        if let Some(files) = context.analyzed_production_files.as_deref()
            && let Some(files) = crate::output::gap_source_subject::production_files_subject(files)
        {
            subject["corpus"]["analyzed_production_files"] = files;
        }
    }
    trace_latency_phase(
        "serialize_source_subject",
        "ok",
        source_subject_started.elapsed(),
    );
    let mut hasher = Sha256Writer::new();
    let analysis_inputs = context
        .corpus_key
        .as_ref()
        .map(crate::output::gap_source_subject::analysis_inputs_subject);
    let disclosures = RepoExposureJsonDisclosures {
        ts_guidance,
        python_guidance,
        generated_skip,
        analysis_inputs: analysis_inputs.as_ref(),
        byte_bound: None,
    };
    // The hash pass streams the document through `Sha256Writer`, which
    // hashes each chunk without buffering it, so no destination I/O
    // happens here. The write pass below renders the document a second
    // time into `out`, so `serialize_write_pass` minus
    // `serialize_hash_pass` mixes destination I/O with the hash cost and
    // second-render cache effects: it bounds I/O, it does not isolate it
    // (#6898).
    let hash_started = Instant::now();
    write_repo_exposure_json_document(
        limit_info.map(RepoExposureLimit::Inventory),
        disclosures,
        Some(&placeholder),
        source_subject.as_ref(),
        &seams,
        &mut hasher,
    )
    .map_err(|err| format!("hash repo exposure JSON failed: {err}"))?;
    trace_latency_phase("serialize_hash_pass", "ok", hash_started.elapsed());
    let content_sha256 = hasher.finish();
    let mut metadata = placeholder;
    metadata["content_sha256"] = serde_json::Value::String(content_sha256);
    let write_started = Instant::now();
    write_repo_exposure_json_document(
        limit_info.map(RepoExposureLimit::Inventory),
        disclosures,
        Some(&metadata),
        source_subject.as_ref(),
        &seams,
        out,
    )
    .map_err(|err| format!("write repo exposure JSON failed: {err}"))?;
    trace_latency_phase("serialize_write_pass", "ok", write_started.elapsed());
    Ok(())
}

/// Render a producer-owned repo-exposure artifact for non-streaming library
/// callers.  The CLI uses the streaming sibling above.
pub(crate) fn render_repo_exposure_json_with_context(
    classified: &[ClassifiedSeam],
    limit_info: Option<&SeamLimitInfo>,
    ts_guidance: Option<&TsFullRepoGuidance>,
    python_guidance: Option<&PythonRepoExposureGuidance>,
    generated_skip: Option<&GeneratedRustSkip>,
    context: &RepoExposureArtifactContext,
) -> Result<String, String> {
    let mut bytes = Vec::new();
    write_repo_exposure_json_with_context(
        classified,
        limit_info,
        ts_guidance,
        python_guidance,
        generated_skip,
        context,
        &mut bytes,
    )?;
    String::from_utf8(bytes).map_err(|err| format!("repo exposure JSON was not UTF-8: {err}"))
}

/// Upper bound on per-seam JSON held in memory so the source-subject, hash
/// and write passes of [`write_repo_exposure_json_with_context`] render each
/// seam once (#5348). Rendering every seam three times (a third of a warm
/// `ripr pilot` per pass on a 900-seam crate) was the price of the
/// bounded-memory streaming writer; entries that fit this budget are kept and
/// reused, and only the entries past it are rendered again per pass.
const PRERENDERED_SEAM_JSON_BUDGET_BYTES: usize = 64 * 1024 * 1024;

/// The `seams[]` entries for one repo-exposure write: the canonical gap
/// identities computed once, and the longest prefix of entries whose rendered
/// total fits the budget, kept for every pass. Entries past the prefix are
/// rendered on demand, so an artifact over the budget pays only for its
/// remainder, never for a discarded prefix.
struct SeamJsons<'a> {
    classified: &'a [ClassifiedSeam],
    canonical_gaps: BTreeMap<crate::analysis::seams::SeamId, CanonicalGapIdentity>,
    /// Indices into `classified` of the entries this document carries: all of
    /// them, or the subset a byte bound kept (#12108). Prerendering runs only
    /// for the full set.
    order: Vec<usize>,
    prefix: Vec<String>,
}

impl<'a> SeamJsons<'a> {
    fn new(classified: &'a [ClassifiedSeam], budget_bytes: usize) -> Self {
        let gaps_started = Instant::now();
        let canonical_gaps = canonical_gap_identities(classified);
        trace_latency_phase("serialize_gap_identities", "ok", gaps_started.elapsed());
        let mut prefix = Vec::new();
        let mut total = 0usize;
        // A zero budget (the plain streaming writer) keeps nothing, so it
        // renders no entry just to discard it.
        let prerender_started = Instant::now();
        for entry in classified.iter().take_while(|_| budget_bytes > 0) {
            let seam_json = render_seam_json(entry, &canonical_gaps);
            total = total.saturating_add(seam_json.len());
            if total > budget_bytes {
                break;
            }
            prefix.push(seam_json);
        }
        trace_latency_phase(
            "serialize_seam_prerender",
            "ok",
            prerender_started.elapsed(),
        );
        Self {
            classified,
            canonical_gaps,
            order: (0..classified.len()).collect(),
            prefix,
        }
    }

    /// Narrow the document to `kept` (indices into `classified`). The prefix is
    /// indexed by position in `order`, so a subset must drop it: a stale prefix
    /// would hand position `n` of the subset the full set's entry `n` (#12153).
    fn restrict_to(&mut self, kept: &[usize]) {
        self.order = kept.to_vec();
        self.prefix.clear();
    }

    /// Hand `idx`'s entry to `use_json`, from the prefix when it holds one.
    fn with_entry<R>(&self, pos: usize, use_json: impl FnOnce(&str) -> R) -> R {
        match self.prefix.get(pos) {
            Some(seam_json) => use_json(seam_json),
            None => use_json(&render_seam_json(
                &self.classified[self.order[pos]],
                &self.canonical_gaps,
            )),
        }
    }
}

fn render_seam_json(
    entry: &ClassifiedSeam,
    canonical_gaps: &BTreeMap<crate::analysis::seams::SeamId, CanonicalGapIdentity>,
) -> String {
    // A rendered seam averages ~12KB on the reference workload; 8KB costs
    // at most one regrowth while never over-reserving into the kept prefix
    // (#6898).
    let mut seam_json = String::with_capacity(8 * 1024);
    push_classified_json(&mut seam_json, entry, canonical_gaps.get(entry.seam.id()));
    seam_json
}

/// #4544: the content digests of every workspace file the seam entries name,
/// read in this analysis run. A gap ledger or actionable-gaps report derived
/// from this artifact copies these digests; it never hashes the workspace
/// itself. `None` when no seam names a file or a file cannot be read, so the
/// derived reports disclose `unverifiable_subject` instead of a partial stamp.
fn repo_exposure_source_subject(
    seams: &SeamJsons<'_>,
    root: &std::path::Path,
) -> Option<serde_json::Value> {
    let mut files = std::collections::BTreeSet::new();
    let mut collect = |seam_json: &str| -> Option<()> {
        let seam = serde_json::from_str::<serde_json::Value>(seam_json).ok()?;
        crate::output::gap_source_subject::named_files_in_value(root, &seam, &mut files);
        Some(())
    };
    for pos in 0..seams.order.len() {
        seams.with_entry(pos, &mut collect)?;
    }
    if files.is_empty() {
        return None;
    }
    let stamp = crate::output::gap_source_subject::stamp_source_subject(root, &files).ok()?;
    serde_json::to_value(stamp).ok()
}

/// Additive disclosures that stay in `limitations[]` without changing
/// `run_status`. Grouped so the document writer does not grow a new
/// argument for each skip or guidance category.
#[derive(Clone, Copy)]
struct RepoExposureJsonDisclosures<'a> {
    ts_guidance: Option<&'a TsFullRepoGuidance>,
    python_guidance: Option<&'a PythonRepoExposureGuidance>,
    generated_skip: Option<&'a GeneratedRustSkip>,
    /// Non-Rust analysis input identity (#9599). Top-level, not inside
    /// `source_subject`, whose corpus identity stays checkout-path independent.
    analysis_inputs: Option<&'a serde_json::Value>,
    /// The byte-budget cut of `seams[]` (#12108); `None` when nothing was cut.
    byte_bound: Option<&'a RepoExposureByteBound>,
}

fn write_repo_exposure_json_document<W: io::Write>(
    limit_info: Option<RepoExposureLimit<'_>>,
    disclosures: RepoExposureJsonDisclosures<'_>,
    artifact: Option<&serde_json::Value>,
    source_subject: Option<&serde_json::Value>,
    seams: &SeamJsons<'_>,
    out: &mut W,
) -> io::Result<()> {
    let RepoExposureJsonDisclosures {
        ts_guidance,
        python_guidance,
        generated_skip,
        analysis_inputs,
        byte_bound,
    } = disclosures;
    // The seams' own slice: the entry index below can never run past it.
    let metrics = ExposureMetrics::from(seams.order.iter().map(|idx| &seams.classified[*idx]));

    writeln!(out, "{{")?;
    writeln!(
        out,
        "  \"schema_version\": \"{}\",",
        REPO_EXPOSURE_SCHEMA_VERSION
    )?;
    if let Some(artifact) = artifact {
        writeln!(out, "  \"artifact\": {},", artifact)?;
    }
    if let Some(source_subject) = source_subject {
        writeln!(out, "  \"source_subject\": {},", source_subject)?;
    }
    if let Some(analysis_inputs) = analysis_inputs {
        writeln!(out, "  \"analysis_inputs\": {},", analysis_inputs)?;
    }
    writeln!(out, "  \"scope\": \"repo\",")?;

    // run_status and limitations[]:
    //   - "complete" with no limitations array when nothing limits the run.
    //   - "seam_limit_applied" with a limitations entry when seam capping fires.
    //   - "typescript_diff_first" guidance is an additive entry in limitations[]
    //     that fires when a TS-predominant workspace returns zero seams; it does
    //     not change run_status (the Rust scan itself completed normally).
    //   - "python_diff_first" is the same kind of additive entry for a
    //     Python workspace with zero seams. It does not render Python findings.
    //   - "generated_rust_source_skipped" names generated Rust files that
    //     `ripr check` also skips. It does not change run_status.
    //   - "rust_files_declined" names Rust files a parse limit declined
    //     (#9308). Their seams are under-credited, so `run_status` becomes
    //     "limited_declined_files" unless a seam cut already set it.
    let declined = generated_skip.filter(|skip| !skip.declined.is_empty());
    let has_limitations = limit_info.is_some()
        || ts_guidance.is_some()
        || python_guidance.is_some()
        || generated_skip.is_some()
        || byte_bound.is_some();
    match (limit_info, declined) {
        (None, None) if byte_bound.is_some() => {
            writeln!(
                out,
                "  \"run_status\": \"{REPO_EXPOSURE_BYTE_BOUND_RUN_STATUS}\","
            )?;
        }
        (None, None) => {
            writeln!(out, "  \"run_status\": \"complete\",")?;
        }
        (None, Some(_)) => {
            writeln!(
                out,
                "  \"run_status\": \"{}\",",
                GeneratedRustSkip::DECLINED_RUN_STATUS
            )?;
        }
        (Some(_), _) => {
            writeln!(out, "  \"run_status\": \"seam_limit_applied\",")?;
        }
    }

    if has_limitations {
        writeln!(out, "  \"limitations\": [")?;
        let mut first = true;
        for limit in limit_info
            .into_iter()
            .flat_map(RepoExposureLimit::parts)
            .flatten()
        {
            if !first {
                writeln!(out, "    ,")?;
            }
            let info = limit.info();
            let repair_route = limit.repair_route();
            writeln!(out, "    {{")?;
            writeln!(out, "      \"category\": \"{}\",", limit.category())?;
            writeln!(out, "      \"seams_analyzed\": {},", info.analyzed)?;
            writeln!(out, "      \"seams_total\": {},", info.total)?;
            writeln!(out, "      \"limit_source\": \"{}\",", info.source.as_str())?;
            writeln!(out, "      \"control\": \"{}\",", limit.control())?;
            writeln!(
                out,
                "      \"repair_route\": \"{}\"",
                json_escape(repair_route)
            )?;
            writeln!(out, "    }}")?;
            first = false;
        }
        if let Some(bound) = byte_bound {
            if !first {
                writeln!(out, "    ,")?;
            }
            first = false;
            writeln!(out, "    {{")?;
            writeln!(
                out,
                "      \"category\": \"{REPO_EXPOSURE_BYTE_BOUND_CATEGORY}\","
            )?;
            writeln!(out, "      \"seams_rendered\": {},", bound.rendered())?;
            writeln!(out, "      \"seams_total\": {},", bound.total)?;
            writeln!(
                out,
                "      \"seams_skipped_over_budget\": {},",
                bound.skipped_over_budget
            )?;
            writeln!(
                out,
                "      \"selection_order\": \"current_change_then_pilot_rank_then_inventory\","
            )?;
            writeln!(out, "      \"seam_bytes_rendered\": {},", bound.bytes)?;
            writeln!(out, "      \"byte_budget\": {},", bound.budget)?;
            writeln!(
                out,
                "      \"limit_source\": \"{}\",",
                bound.source.as_str()
            )?;
            writeln!(
                out,
                "      \"control\": \"{PILOT_REPO_EXPOSURE_BYTES_ENV}\","
            )?;
            writeln!(
                out,
                "      \"repair_route\": \"{}\"",
                json_escape(REPO_EXPOSURE_BYTE_BOUND_ROUTE)
            )?;
            writeln!(out, "    }}")?;
        }
        if let Some(guidance) = ts_guidance {
            if !first {
                writeln!(out, "    ,")?;
            }
            writeln!(out, "    {{")?;
            writeln!(
                out,
                "      \"category\": \"{}\",",
                TsFullRepoGuidance::CATEGORY
            )?;
            writeln!(out, "      \"ts_file_count\": {},", guidance.ts_file_count)?;
            writeln!(out, "      \"typescript_readiness\": {{")?;
            writeln!(
                out,
                "        \"source\": \"repo_exposure_typescript_readiness.v1\","
            )?;
            writeln!(
                out,
                "        \"authority_boundary\": \"preview_advisory_only\","
            )?;
            writeln!(out, "        \"analysis_model\": \"diff_first\",")?;
            writeln!(
                out,
                "        \"source_file_count\": {},",
                guidance.readiness.source_file_count
            )?;
            writeln!(
                out,
                "        \"test_file_count\": {},",
                guidance.readiness.test_file_count
            )?;
            writeln!(
                out,
                "        \"package_root_count\": {},",
                guidance.readiness.package_root_count
            )?;
            writeln!(
                out,
                "        \"package_confidence\": \"{}\",",
                json_escape(&guidance.readiness.package_confidence)
            )?;
            writeln!(
                out,
                "        \"runner_status\": \"{}\",",
                json_escape(&guidance.readiness.runner_status)
            )?;
            writeln!(
                out,
                "        \"verify_command_count\": {},",
                guidance.readiness.verify_command_count
            )?;
            match guidance.readiness.top_blocker.as_ref() {
                Some(blocker) => writeln!(
                    out,
                    "        \"top_blocker\": \"{}\",",
                    json_escape(blocker)
                )?,
                None => writeln!(out, "        \"top_blocker\": null,")?,
            }
            writeln!(
                out,
                "        \"non_claims\": [\"no full-repo TypeScript seam model\", \"no runtime TypeScript execution\", \"no gate or badge authority\"]"
            )?;
            writeln!(out, "      }},")?;
            writeln!(
                out,
                "      \"repair_route\": \"{}\"",
                json_escape(TsFullRepoGuidance::REPAIR_ROUTE)
            )?;
            writeln!(out, "    }}")?;
            first = false;
        }
        if let Some(guidance) = python_guidance {
            if !first {
                writeln!(out, "    ,")?;
            }
            writeln!(out, "    {{")?;
            writeln!(
                out,
                "      \"category\": \"{}\",",
                PythonRepoExposureGuidance::CATEGORY
            )?;
            writeln!(
                out,
                "      \"python_file_count\": {},",
                guidance.python_file_count
            )?;
            writeln!(
                out,
                "      \"authority_boundary\": \"preview_advisory_only\","
            )?;
            writeln!(out, "      \"analysis_model\": \"diff_first\",")?;
            writeln!(
                out,
                "      \"non_claims\": [\"no Python seams in this inventory\", \"no Python test execution\", \"no gate or badge authority\"],"
            )?;
            writeln!(
                out,
                "      \"repair_route\": \"{}\"",
                json_escape(PythonRepoExposureGuidance::REPAIR_ROUTE)
            )?;
            writeln!(out, "    }}")?;
            first = false;
        }
        if let Some(skip) = generated_skip.filter(|skip| !skip.paths.is_empty()) {
            if !first {
                writeln!(out, "    ,")?;
            }
            first = false;
            writeln!(out, "    {{")?;
            writeln!(
                out,
                "      \"category\": \"{}\",",
                GeneratedRustSkip::CATEGORY
            )?;
            writeln!(out, "      \"skipped_file_count\": {},", skip.paths.len())?;
            writeln!(out, "      \"skipped_files\": [")?;
            for (idx, path) in skip.paths.iter().take(3).enumerate() {
                let trailing = if idx + 1 == skip.paths.len().min(3) {
                    ""
                } else {
                    ","
                };
                writeln!(
                    out,
                    "        \"{}\"{trailing}",
                    json_escape(&path.to_string_lossy().replace('\\', "/"))
                )?;
            }
            writeln!(out, "      ],")?;
            if skip.paths.len() > 3 {
                writeln!(
                    out,
                    "      \"skipped_files_omitted\": {},",
                    skip.paths.len() - 3
                )?;
            }
            writeln!(
                out,
                "      \"repair_route\": \"{}\",",
                json_escape(&skip.repair_route())
            )?;
            writeln!(out, "      \"detail\": \"{}\"", json_escape(&skip.detail()))?;
            writeln!(out, "    }}")?;
        }
        if let Some(skip) = declined {
            if !first {
                writeln!(out, "    ,")?;
            }
            writeln!(out, "    {{")?;
            writeln!(
                out,
                "      \"category\": \"{}\",",
                GeneratedRustSkip::DECLINED_CATEGORY
            )?;
            writeln!(
                out,
                "      \"declined_file_count\": {},",
                skip.declined.len()
            )?;
            writeln!(out, "      \"declined_files\": [")?;
            for (idx, file) in skip.declined.iter().take(3).enumerate() {
                let trailing = if idx + 1 == skip.declined.len().min(3) {
                    ""
                } else {
                    ","
                };
                writeln!(
                    out,
                    "        {{\"path\": \"{}\", \"reason\": \"{}\"}}{trailing}",
                    json_escape(&file.path.to_string_lossy().replace('\\', "/")),
                    json_escape(&file.reason)
                )?;
            }
            writeln!(out, "      ],")?;
            if skip.declined.len() > 3 {
                writeln!(
                    out,
                    "      \"declined_files_omitted\": {},",
                    skip.declined.len() - 3
                )?;
            }
            writeln!(
                out,
                "      \"repair_route\": \"{}\",",
                json_escape(skip.declined_repair_route())
            )?;
            writeln!(
                out,
                "      \"detail\": \"{}\"",
                json_escape(&skip.declined_detail())
            )?;
            writeln!(out, "    }}")?;
        }
        writeln!(out, "  ],")?;
    }

    writeln!(out, "  \"metrics\": {{")?;
    writeln!(out, "    \"seams_total\": {},", metrics.seams_total)?;
    if let Some(bound) = byte_bound {
        // #12153: a bounded document's metrics count the kept seams only.
        // Name that population and carry the true total beside the kept
        // count, so a reader that skips `limitations[]` is not misled.
        writeln!(out, "    \"population\": \"byte_bound_kept_seams\",")?;
        writeln!(out, "    \"population_seams_total\": {},", bound.total)?;
    }
    writeln!(
        out,
        "    \"headline_eligible\": {},",
        metrics.headline_eligible
    )?;
    let total_classes = SeamGripClass::ALL.len();
    for (idx, class) in SeamGripClass::ALL.iter().enumerate() {
        let count = metrics.count_for(*class);
        let trailing = if idx + 1 == total_classes { "" } else { "," };
        writeln!(out, "    \"{}\": {}{}", class.as_str(), count, trailing)?;
    }
    writeln!(out, "  }},")?;

    write!(out, "  \"seams\": [")?;
    let entries = seams.order.len();
    for pos in 0..entries {
        if pos == 0 {
            writeln!(out)?;
        }
        seams.with_entry(pos, |seam_json| out.write_all(seam_json.as_bytes()))?;
        if pos + 1 != entries {
            writeln!(out, ",")?;
        } else {
            writeln!(out)?;
        }
    }
    if entries != 0 {
        write!(out, "  ")?;
    }
    writeln!(out, "]")?;
    writeln!(out, "}}")?;
    out.flush()
}

/// Render a repo-exposure refusal document (#7332) for a run that never
/// analyzed. Keeps the format's own conventions: `scope: "repo"`, a
/// `run_status`/`limitations[]` pair (here `refused` with the typed refusal
/// identity as the limitation category), zeroed `metrics`, and an empty
/// `seams` array — parseable as `repo-exposure-json`, never zero bytes.
pub(crate) fn render_refusal_json(notice: &crate::core_error::RefusalNotice) -> String {
    let mut metrics = String::from("  \"metrics\": {\n");
    metrics.push_str("    \"seams_total\": 0,\n");
    metrics.push_str("    \"headline_eligible\": 0");
    for class in SeamGripClass::ALL {
        metrics.push_str(&format!(",\n    \"{}\": 0", class.as_str()));
    }
    metrics.push_str("\n  },\n");
    format!(
        "{{\n  \"schema_version\": \"{}\",\n  \"scope\": \"repo\",\n  \"run_status\": \"refused\",\n  \"limitations\": [\n    {{\n      \"category\": \"{}\",\n      \"repair_route\": \"{}\",\n      \"message\": \"{}\"\n    }}\n  ],\n{}  \"seams\": []\n}}\n",
        REPO_EXPOSURE_SCHEMA_VERSION,
        notice.identity,
        notice.repair_route,
        json_escape(&notice.message),
        metrics,
    )
}

/// Render a bounded repo exposure summary JSON.
///
/// Unlike `repo-exposure-json`, this shape deliberately omits per-seam
/// evidence arrays and nested evidence records. It is for badge, queue, and
/// large-repo planning consumers that need aggregate counts and bounded top
/// files without multi-GB payloads.
pub(crate) fn render_repo_exposure_summary_json(
    classified: &[ClassifiedSeam],
    root: &Path,
    base: Option<&str>,
    mode: &str,
) -> String {
    let value = repo_exposure_summary_json_value(classified, root, base, mode);
    match serde_json::to_string_pretty(&value) {
        Ok(mut json) => {
            json.push('\n');
            json
        }
        Err(_) => "{}\n".to_string(),
    }
}

fn repo_exposure_summary_json_value(
    classified: &[ClassifiedSeam],
    root: &Path,
    base: Option<&str>,
    mode: &str,
) -> Value {
    let metrics = ExposureMetrics::from(classified.iter());
    let canonical_gaps = canonical_gap_identities(classified);
    let mut canonical_gap_ids = BTreeSet::<String>::new();
    let mut actionable_gap_ids = BTreeSet::<String>::new();
    let mut raw_actionable_seam_records = 0usize;

    let mut actionability_counts = BTreeMap::<String, usize>::new();
    let mut actionable_seam_kind_counts = BTreeMap::<String, usize>::new();
    let mut gap_state_counts = BTreeMap::<String, usize>::new();
    let mut file_summaries = BTreeMap::<String, FileExposureSummary>::new();

    for entry in classified {
        let file = display_path(entry.seam.file());
        let file_summary = file_summaries.entry(file).or_default();
        file_summary.raw_seams += 1;
        increment(&mut file_summary.grip_class_counts, entry.class.as_str());
        if entry.class.is_headline_eligible() {
            file_summary.headline_eligible_seams += 1;
        }
        if entry.class == SeamGripClass::Suppressed {
            file_summary.suppressed_exposure_gaps += 1;
        }

        let canonical_gap = canonical_gaps.get(entry.seam.id());
        if let Some(gap) = canonical_gap {
            canonical_gap_ids.insert(gap.id.clone());
            file_summary.canonical_gap_ids.insert(gap.id.clone());
        }

        let record = evidence_record_for(entry, canonical_gap);
        increment(
            &mut gap_state_counts,
            record.canonical_item.gap_state.as_str(),
        );

        if let Some(gap_id) = record.canonical_item.canonical_gap_id.as_ref()
            && is_summary_actionable_canonical_item(&record.canonical_item)
        {
            raw_actionable_seam_records += 1;
            if actionable_gap_ids.insert(gap_id.clone()) {
                increment(
                    &mut actionability_counts,
                    record.canonical_item.actionability.as_str(),
                );
                increment(&mut actionable_seam_kind_counts, entry.seam.kind().as_str());
            }
            if file_summary.actionable_gap_ids.insert(gap_id.clone()) {
                increment(
                    &mut file_summary.actionability_counts,
                    record.canonical_item.actionability.as_str(),
                );
            }
        }
    }

    let grip_class_counts = grip_class_counts_json(&metrics);
    let top_files_total = file_summaries.len();
    let mut top_files = file_summaries.into_iter().collect::<Vec<_>>();
    top_files.sort_by(|(file_a, a), (file_b, b)| {
        b.actionable_gap_ids
            .len()
            .cmp(&a.actionable_gap_ids.len())
            .then(b.headline_eligible_seams.cmp(&a.headline_eligible_seams))
            .then(b.raw_seams.cmp(&a.raw_seams))
            .then(file_a.cmp(file_b))
    });
    let top_files_json = top_files
        .iter()
        .take(MAX_TOP_FILES_SUMMARY_JSON)
        .map(|(file, summary)| {
            json!({
                "file": file,
                "raw_seams": summary.raw_seams,
                "headline_eligible_seams": summary.headline_eligible_seams,
                "canonical_gap_records": summary.canonical_gap_ids.len(),
                "unsuppressed_exposure_gaps": summary.actionable_gap_ids.len(),
                "suppressed_exposure_gaps": summary.suppressed_exposure_gaps,
                "reason_breakdown": {
                    "actionability": summary.actionability_counts,
                    "grip_class": summary.grip_class_counts,
                }
            })
        })
        .collect::<Vec<_>>();

    json!({
        "schema_version": REPO_EXPOSURE_SUMMARY_SCHEMA_VERSION,
        "format": "repo-exposure-summary-json",
        "tool": "ripr",
        "ripr_version": env!("CARGO_PKG_VERSION"),
        "scope": "repo",
        "basis": "canonical_actionable_gap",
        "metadata": {
            "root": display_path(root),
            "base": base,
            "head": "HEAD",
            "mode": mode,
        },
        "metrics": {
            "raw_seams": metrics.seams_total,
            "headline_eligible_seams": metrics.headline_eligible,
            "canonical_gap_records": canonical_gap_ids.len(),
            "raw_actionable_seam_records": raw_actionable_seam_records,
            "unsuppressed_exposure_gaps": actionable_gap_ids.len(),
            "suppressed_exposure_gaps": metrics.count_for(SeamGripClass::Suppressed),
            "grip_class": grip_class_counts.clone(),
        },
        "reason_breakdown": {
            "actionability": actionability_counts,
            "gap_state": gap_state_counts,
            "seam_kind": actionable_seam_kind_counts,
            "grip_class": grip_class_counts,
        },
        "limits": {
            "top_files_limit": MAX_TOP_FILES_SUMMARY_JSON,
            "top_files_total": top_files_total,
            "top_files_truncated": top_files_total > MAX_TOP_FILES_SUMMARY_JSON,
        },
        "top_files": top_files_json,
    })
}

fn is_summary_actionable_canonical_item(
    item: &crate::output::evidence_record::EvidenceRecordCanonicalItem,
) -> bool {
    item.gap_state == "actionable" && item.repair_route.is_some() && item.verify_command.is_some()
}

#[derive(Default)]
struct FileExposureSummary {
    raw_seams: usize,
    headline_eligible_seams: usize,
    suppressed_exposure_gaps: usize,
    canonical_gap_ids: BTreeSet<String>,
    actionable_gap_ids: BTreeSet<String>,
    actionability_counts: BTreeMap<String, usize>,
    grip_class_counts: BTreeMap<String, usize>,
}

fn grip_class_counts_json(metrics: &ExposureMetrics) -> BTreeMap<String, usize> {
    SeamGripClass::ALL
        .into_iter()
        .map(|class| (class.as_str().to_string(), metrics.count_for(class)))
        .collect()
}

fn increment(counts: &mut BTreeMap<String, usize>, key: &str) {
    *counts.entry(key.to_string()).or_insert(0) += 1;
}

fn push_classified_json(
    out: &mut String,
    entry: &ClassifiedSeam,
    canonical_gap: Option<&CanonicalGapIdentity>,
) {
    let seam = &entry.seam;
    let evidence = &entry.evidence;
    // #6898: this hot loop renders every seam up to four times per
    // snapshot (prerender, source subject, hash pass, write pass), so it
    // writes fields straight into `out` instead of `format!` temporaries.
    // The emitted bytes are unchanged; only the intermediate allocations
    // are gone.
    out.push_str("    {\n");
    let _ = write!(out, "      \"seam_id\": \"");
    json_escape_into(out, seam.id().as_str());
    out.push_str("\",\n");
    let _ = writeln!(out, "      \"kind\": \"{}\",", seam.kind().as_str());
    let _ = write!(out, "      \"file\": \"");
    json_escape_into(out, &display_path(seam.file()));
    out.push_str("\",\n");
    let _ = writeln!(
        out,
        "      \"line\": {},",
        seam.span()
            .map_or(seam.display_line(), |span| span.start_line)
    );
    if let Some(span) = seam.span() {
        let _ = writeln!(out, "      \"column\": {},", span.start_column);
        let _ = writeln!(out, "      \"end_line\": {},", span.end_line);
        let _ = writeln!(out, "      \"end_column\": {},", span.end_column);
    }
    let _ = write!(out, "      \"owner\": \"");
    json_escape_into(out, seam.owner());
    out.push_str("\",\n");
    let _ = write!(out, "      \"expression\": \"");
    json_escape_into(out, seam.expression());
    out.push_str("\",\n");
    let _ = writeln!(out, "      \"grip_class\": \"{}\",", entry.class.as_str());
    let _ = writeln!(
        out,
        "      \"headline_eligible\": {},",
        entry.class.is_headline_eligible()
    );

    out.push_str("      \"evidence\": {\n");
    let _ = writeln!(
        out,
        "        \"reach\": \"{}\",",
        evidence.reach.state.as_str()
    );
    let _ = writeln!(
        out,
        "        \"activate\": \"{}\",",
        evidence.activate.state.as_str()
    );
    let _ = writeln!(
        out,
        "        \"propagate\": \"{}\",",
        evidence.propagate.state.as_str()
    );
    let _ = writeln!(
        out,
        "        \"observe\": \"{}\",",
        evidence.observe.state.as_str()
    );
    let _ = writeln!(
        out,
        "        \"discriminate\": \"{}\"",
        evidence.discriminate.state.as_str()
    );
    out.push_str("      },\n");

    let related_total = evidence.related_tests.len();
    let related_rendered = related_total.min(MAX_RELATED_TESTS_PER_SEAM_JSON);
    let _ = writeln!(out, "      \"related_tests_total\": {related_total},");
    out.push_str("      \"related_tests\": [");
    if related_rendered > 0 {
        out.push('\n');
        for (idx, grip) in evidence
            .related_tests
            .iter()
            .take(related_rendered)
            .enumerate()
        {
            out.push_str("        {");
            let _ = write!(out, "\"name\": \"");
            json_escape_into(out, grip.test_name.as_str());
            out.push_str("\", ");
            let _ = write!(out, "\"file\": \"");
            json_escape_into(out, &display_path(&grip.file));
            out.push_str("\", ");
            let _ = write!(out, "\"line\": {}, ", grip.line);
            let _ = write!(out, "\"oracle_kind\": \"{}\", ", grip.oracle_kind.as_str());
            let _ = write!(
                out,
                "\"oracle_strength\": \"{}\", ",
                grip.oracle_strength.as_str()
            );
            let _ = write!(out, "\"evidence_summary\": \"");
            json_escape_into(out, grip.evidence_summary.as_str());
            out.push_str("\", ");
            let _ = write!(
                out,
                "\"relation_reason\": \"{}\", ",
                grip.relation_reason.as_str()
            );
            let _ = write!(
                out,
                "\"relation_confidence\": \"{}\"",
                grip.relation_confidence.as_str()
            );
            out.push('}');
            if idx + 1 != related_rendered {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str("      ");
    }
    out.push_str("],\n");

    out.push_str("      \"observed_values\": [");
    for (idx, value) in evidence.observed_values.iter().enumerate() {
        out.push('"');
        json_escape_into(out, value.value.as_str());
        out.push('"');
        if idx + 1 != evidence.observed_values.len() {
            out.push_str(", ");
        }
    }
    out.push_str("],\n");

    out.push_str("      \"missing_discriminators\": [");
    if !evidence.missing_discriminators.is_empty() {
        out.push('\n');
        for (idx, missing) in evidence.missing_discriminators.iter().enumerate() {
            let _ = write!(out, "        {{\"value\": \"");
            json_escape_into(out, missing.value.as_str());
            let _ = write!(out, "\", \"reason\": \"");
            json_escape_into(out, missing.reason.as_str());
            out.push_str("\"}");
            if idx + 1 != evidence.missing_discriminators.len() {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str("      ");
    }
    out.push_str("],\n");
    let record = evidence_record_for(entry, canonical_gap);
    out.push_str("      \"evidence_record\": ");
    out.push_str(&evidence_record_json_value(&record).to_string());
    out.push('\n');
    out.push_str("    }");
}

/// Next step printed when the repo inventory found nothing to analyze and no
/// language guidance applies.
const REPO_EXPOSURE_EMPTY_NEXT_STEP: &str = "\nNothing was found to analyze under this root. \
Next: point `--root` at the directory holding `Cargo.toml` (or at TypeScript or Python \
sources with that language enabled in `ripr.toml`), then run `ripr doctor --root <DIR>` \
to see what the root can analyze.\n";

/// Render the repo exposure Markdown report. The output uses the
/// static seam evidence vocabulary only — no runtime-mutation outcome
/// words per RIPR-SPEC-0005 § Static-Language Boundaries.
#[cfg(test)]
pub(crate) fn render_repo_exposure_md(
    classified: &[ClassifiedSeam],
    limit_info: Option<&SeamLimitInfo>,
    ts_guidance: Option<&TsFullRepoGuidance>,
    python_guidance: Option<&PythonRepoExposureGuidance>,
) -> String {
    render_repo_exposure_md_with_generated_skip(
        classified,
        limit_info,
        ts_guidance,
        python_guidance,
        None,
    )
}

pub(crate) fn render_repo_exposure_md_with_generated_skip(
    classified: &[ClassifiedSeam],
    limit_info: Option<&SeamLimitInfo>,
    ts_guidance: Option<&TsFullRepoGuidance>,
    python_guidance: Option<&PythonRepoExposureGuidance>,
    generated_skip: Option<&GeneratedRustSkip>,
) -> String {
    render_repo_exposure_md_with_limit(
        classified,
        limit_info.map(RepoExposureLimit::Inventory),
        ts_guidance,
        python_guidance,
        generated_skip,
    )
}

pub(crate) fn render_repo_exposure_md_with_limit(
    classified: &[ClassifiedSeam],
    limit_info: Option<RepoExposureLimit<'_>>,
    ts_guidance: Option<&TsFullRepoGuidance>,
    python_guidance: Option<&PythonRepoExposureGuidance>,
    generated_skip: Option<&GeneratedRustSkip>,
) -> String {
    let metrics = ExposureMetrics::from(classified.iter());
    let mut out = String::new();
    out.push_str("# ripr repo exposure report\n\n");
    out.push_str(&format!(
        "Schema version: {}\n",
        REPO_EXPOSURE_SCHEMA_VERSION
    ));
    out.push_str("Scope: repo\n\n");

    out.push_str("## Summary\n\n");
    out.push_str("| Class | Count |\n| --- | --- |\n");
    out.push_str(&format!("| seams_total | {} |\n", metrics.seams_total));
    out.push_str(&format!(
        "| headline_eligible | {} |\n",
        metrics.headline_eligible
    ));
    for class in SeamGripClass::ALL {
        out.push_str(&format!(
            "| {} | {} |\n",
            class.as_str(),
            metrics.count_for(class)
        ));
    }

    let has_limitations = limit_info.is_some()
        || ts_guidance.is_some()
        || python_guidance.is_some()
        || generated_skip.is_some();
    if has_limitations {
        out.push_str("\n## Limitations\n\n");
        // Seam-limit disclosure: emit only when a real cap fired.
        for limit in limit_info
            .into_iter()
            .flat_map(RepoExposureLimit::parts)
            .flatten()
        {
            let info = limit.info();
            let control = limit.markdown_repair_route();
            out.push_str(&format!(
                "> Partial scan: analyzed {} of {} seams (seam_limit_applied; {}).\n\n",
                info.analyzed, info.total, control,
            ));
        }
        // Emit TypeScript diff-first guidance before the empty-seams bail-out
        // so the disclosure is visible even when the inventory is empty.
        if let Some(guidance) = ts_guidance {
            out.push_str(&format!(
                "**{}** (ts_file_count: {})\n\n",
                TsFullRepoGuidance::CATEGORY,
                guidance.ts_file_count,
            ));
            out.push_str(TsFullRepoGuidance::REPAIR_ROUTE);
            out.push('\n');
            push_typescript_readiness_md(&mut out, &guidance.readiness);
        }
        if let Some(guidance) = python_guidance {
            out.push_str(&format!(
                "**{}** (python_file_count: {})\n\n",
                PythonRepoExposureGuidance::CATEGORY,
                guidance.python_file_count,
            ));
            out.push_str(PythonRepoExposureGuidance::REPAIR_ROUTE);
            out.push_str(
                "\n\nThis seam inventory does not render Python findings. \
                 Authority stays preview and advisory: no Python test execution, \
                 and no gate or badge authority.\n",
            );
        }
        if let Some(skip) = generated_skip.filter(|skip| !skip.paths.is_empty()) {
            out.push_str(&format!(
                "**{}** (skipped_file_count: {})\n\n",
                GeneratedRustSkip::CATEGORY,
                skip.paths.len(),
            ));
            out.push_str(&skip.repair_route());
            out.push('\n');
        }
        if let Some(skip) = generated_skip.filter(|skip| !skip.declined.is_empty()) {
            out.push_str(&format!(
                "**{}** (declined_file_count: {})\n\n{}.\n\n{}\n",
                GeneratedRustSkip::DECLINED_CATEGORY,
                skip.declined.len(),
                skip.declined_detail(),
                skip.declined_repair_route(),
            ));
        }
    }

    if classified.is_empty() {
        out.push_str(
            "\nNo classified seams. The repo seam inventory is empty or no \
             production seams were detected.\n",
        );
        // An empty report must still name a next step (an empty directory
        // used to end here with nothing to do). Language guidance above
        // already names its own next step.
        if ts_guidance.is_none() && python_guidance.is_none() && generated_skip.is_none() {
            out.push_str(REPO_EXPOSURE_EMPTY_NEXT_STEP);
        }
        return out;
    }

    out.push_str("\n## Top gaps\n\n");
    let mut top_gaps: Vec<&ClassifiedSeam> = classified
        .iter()
        .filter(|entry| entry.class.is_headline_eligible())
        .collect();
    top_gaps.sort_by(|a, b| {
        a.seam
            .file()
            .cmp(b.seam.file())
            .then(a.seam.display_line().cmp(&b.seam.display_line()))
            .then(a.seam.id().as_str().cmp(b.seam.id().as_str()))
    });
    if top_gaps.is_empty() {
        out.push_str(
            "No headline-eligible seams. Static seam evidence reports no \
             detected grip gaps at the moment; runtime confirmation via \
             `cargo-mutants` remains a separate calibration step.\n",
        );
        return out;
    }
    let preview = top_gaps.iter().take(50);
    for entry in preview {
        push_top_gap_md(&mut out, entry);
    }
    if top_gaps.len() > 50 {
        out.push_str(&format!(
            "\n_... {} additional headline-eligible seams omitted; see \
            `repo-exposure.json` for the full list._\n",
            top_gaps.len() - 50
        ));
    }

    out.push_str(
        "\n_This report shows static test-grip evidence for repo seams. \
        Runtime confirmation (e.g. `cargo-mutants`) is a separate \
        calibration step. Static-language constraints from RIPR-SPEC-0005 \
        still apply._\n",
    );
    out
}

fn push_top_gap_md(out: &mut String, entry: &ClassifiedSeam) {
    let seam = &entry.seam;
    let evidence = &entry.evidence;
    // Path in a code span, like the owner line: odd names can contain
    // `[y](z)` and `*` that would otherwise render as a link or emphasis.
    // Do not fold this into #4473's prose neutralization; that owner still
    // leaves link/emphasis syntax live in headings (#4605).
    out.push_str(&format!(
        "### {}:{} {}\n\n",
        code_span(&display_path(seam.file())),
        seam.display_line(),
        seam.kind().as_str()
    ));
    out.push_str(&format!("- seam: {}\n", code_span(seam.expression())));
    out.push_str(&format!("- owner: {}\n", code_span(seam.owner())));
    out.push_str(&format!("- grip: {}\n", entry.class.as_str()));
    out.push_str("- evidence:\n");
    out.push_str(&format!("  - reach: {}\n", evidence.reach.state.as_str()));
    out.push_str(&format!(
        "  - activate: {}\n",
        evidence.activate.state.as_str()
    ));
    out.push_str(&format!(
        "  - propagate: {}\n",
        evidence.propagate.state.as_str()
    ));
    out.push_str(&format!(
        "  - observe: {}\n",
        evidence.observe.state.as_str()
    ));
    out.push_str(&format!(
        "  - discriminate: {}\n",
        evidence.discriminate.state.as_str()
    ));

    if !evidence.related_tests.is_empty() {
        out.push_str("- related tests:\n");
        for grip in evidence.related_tests.iter().take(5) {
            out.push_str(&format!(
                "  - {} ({}, {}) · {} / {}\n",
                code_span(grip.test_name.as_str()),
                grip.oracle_kind.as_str(),
                grip.oracle_strength.as_str(),
                grip.relation_reason.as_str(),
                grip.relation_confidence.as_str()
            ));
        }
    }
    if !evidence.observed_values.is_empty() {
        out.push_str("- observed values:\n");
        for value in evidence.observed_values.iter().take(5) {
            out.push_str(&format!("  - {}\n", code_span(value.value.as_str())));
        }
    }
    if !evidence.missing_discriminators.is_empty() {
        out.push_str("- missing discriminators:\n");
        for missing in &evidence.missing_discriminators {
            out.push_str(&format!(
                "  - {} — {}\n",
                code_span(missing.value.as_str()),
                inline_prose_literal(missing.reason.as_str())
            ));
        }
    }
    out.push('\n');
}

fn push_typescript_readiness_md(out: &mut String, readiness: &TypeScriptRepoReadiness) {
    out.push_str("\nTypeScript readiness (preview, advisory)\n\n");
    out.push_str("| Signal | Value |\n| --- | --- |\n");
    out.push_str(&format!(
        "| source files | {} |\n",
        readiness.source_file_count
    ));
    out.push_str(&format!("| test files | {} |\n", readiness.test_file_count));
    out.push_str(&format!(
        "| package roots | {} |\n",
        readiness.package_root_count
    ));
    out.push_str(&format!(
        "| package confidence | {} |\n",
        table_cell_text(&readiness.package_confidence)
    ));
    out.push_str(&format!(
        "| runner status | {} |\n",
        table_cell_text(&readiness.runner_status)
    ));
    out.push_str(&format!(
        "| verify commands | {} |\n",
        readiness.verify_command_count
    ));
    out.push_str(&format!(
        "| top blocker | {} |\n",
        readiness
            .top_blocker
            .as_deref()
            .map(table_cell_text)
            .unwrap_or_else(|| "none".to_string())
    ));
    out.push_str(
        "\nThis card is root-level readiness for diff-first TypeScript preview. \
         It does not emit full-repo TypeScript seams, run TypeScript tests, or \
         create gate or badge authority.\n",
    );
}

/// Per-class metric bucket for the repo exposure report.
struct ExposureMetrics {
    seams_total: usize,
    headline_eligible: usize,
    counts: [(SeamGripClass, usize); 11],
}

impl ExposureMetrics {
    fn from<'a>(classified: impl Iterator<Item = &'a ClassifiedSeam> + Clone) -> Self {
        let mut counts: [(SeamGripClass, usize); 11] = [
            (SeamGripClass::StronglyGripped, 0),
            (SeamGripClass::WeaklyGripped, 0),
            (SeamGripClass::Ungripped, 0),
            (SeamGripClass::ReachableUnrevealed, 0),
            (SeamGripClass::ActivationUnknown, 0),
            (SeamGripClass::PropagationUnknown, 0),
            (SeamGripClass::ObservationUnknown, 0),
            (SeamGripClass::DiscriminationUnknown, 0),
            (SeamGripClass::Opaque, 0),
            (SeamGripClass::Intentional, 0),
            (SeamGripClass::Suppressed, 0),
        ];
        let mut headline_eligible = 0;
        let seams_total = classified.clone().count();
        for entry in classified {
            for bucket in counts.iter_mut() {
                if bucket.0 == entry.class {
                    bucket.1 += 1;
                    break;
                }
            }
            if entry.class.is_headline_eligible() {
                headline_eligible += 1;
            }
        }
        ExposureMetrics {
            seams_total,
            headline_eligible,
            counts,
        }
    }

    fn count_for(&self, class: SeamGripClass) -> usize {
        self.counts
            .iter()
            .find(|(c, _)| *c == class)
            .map(|(_, n)| *n)
            .unwrap_or(0)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::analysis::seams::{ExpectedSink, RepoSeam, RequiredDiscriminator, SeamKind};
    use crate::analysis::test_grip_evidence::{RelatedTestGrip, TestGripEvidence};
    use crate::domain::{
        Confidence, MissingDiscriminatorFact, OracleKind, OracleStrength, StageEvidence,
        StageState, ValueFact,
    };
    use crate::output::markdown::{code_span, inline_prose_literal};

    #[test]
    fn refusal_document_keeps_the_format_contract_with_zeroed_metrics() -> Result<(), String> {
        // #7332: a refused repo-exposure run parses as repo-exposure-json —
        // the format's own run_status/limitations pair, zeroed metrics for
        // every grip class, and an empty seams array, never zero bytes.
        let notice =
            crate::core_error::CoreError::seam_id_unknown("agent packet seam_id s was not found")
                .refusal_notice();
        let rendered = render_refusal_json(&notice);
        let value: serde_json::Value =
            serde_json::from_str(&rendered).map_err(|err| format!("parse refusal JSON: {err}"))?;
        for (pointer, expected) in [
            ("/schema_version", REPO_EXPOSURE_SCHEMA_VERSION),
            ("/scope", "repo"),
            ("/run_status", "refused"),
            ("/limitations/0/category", "seam_id_unknown"),
            ("/limitations/0/repair_route", "analysis/seam-inventory"),
        ] {
            let actual = value.pointer(pointer).and_then(serde_json::Value::as_str);
            if actual != Some(expected) {
                return Err(format!("{pointer}: expected {expected}, got {actual:?}"));
            }
        }
        if value["limitations"][0]["message"] != serde_json::Value::String(notice.message.clone()) {
            return Err(format!("limitation must echo the diagnostic: {value}"));
        }
        if value["seams"].as_array().map(Vec::len) != Some(0) {
            return Err(format!("seams must be empty: {value}"));
        }
        let metrics = value
            .pointer("/metrics")
            .ok_or("metrics object must be present")?;
        for class in SeamGripClass::ALL {
            if metrics[class.as_str()] != 0 {
                return Err(format!("metric {} must be zeroed: {value}", class.as_str()));
            }
        }
        if metrics["seams_total"] != 0 || metrics["headline_eligible"] != 0 {
            return Err(format!("totals must be zeroed: {value}"));
        }
        Ok(())
    }

    fn stage(state: StageState) -> StageEvidence {
        StageEvidence::new(state, Confidence::Medium, "test stage")
    }

    fn weakly_gripped_classified() -> ClassifiedSeam {
        classified_at(
            "src/pricing.rs",
            "pricing::discounted_total",
            42,
            SeamGripClass::WeaklyGripped,
        )
    }

    /// #5348: the writer renders each seam once and reuses it across the
    /// subject, hash and write passes for the prefix that fits the budget,
    /// and renders only the entries past it per pass. Every budget must emit
    /// the same bytes, and the budget must be a real bound on the kept
    /// prefix. Negative experiments: dropping the `total > budget_bytes`
    /// check keeps all three entries under the smaller budgets; serving
    /// `prefix[0]` for every index breaks byte equality; discarding the
    /// prefix on overflow (the pre-review shape) makes the just-over-budget
    /// prefix empty instead of two entries.
    #[test]
    fn prerendered_seam_json_matches_streamed_document_and_respects_budget() {
        let classified = vec![
            weakly_gripped_classified(),
            classified_at("src/b.rs", "b::gate", 7, SeamGripClass::Ungripped),
            classified_at("src/c.rs", "c::gate", 9, SeamGripClass::StronglyGripped),
        ];
        let disclosures = RepoExposureJsonDisclosures {
            ts_guidance: None,
            python_guidance: None,
            generated_skip: None,
            analysis_inputs: None,
            byte_bound: None,
        };
        let document = |budget: usize| -> Result<Vec<u8>, String> {
            let mut out = Vec::new();
            write_repo_exposure_json_document(
                None,
                disclosures,
                None,
                None,
                &SeamJsons::new(&classified, budget),
                &mut out,
            )
            .map_err(|err| err.to_string())?;
            Ok(out)
        };
        let all = SeamJsons::new(&classified, usize::MAX).prefix;
        assert_eq!(all.len(), classified.len());
        assert!(
            all.windows(2).all(|pair| pair[0] != pair[1]),
            "fixture seams must render distinct entries"
        );
        let total: usize = all.iter().map(String::len).sum();
        let last = all.last().map_or(0, String::len);

        // The kept prefix is the longest run of entries within the budget.
        assert_eq!(SeamJsons::new(&classified, total).prefix, all);
        assert_eq!(
            SeamJsons::new(&classified, total - 1).prefix,
            all[..all.len() - 1].to_vec()
        );
        assert_eq!(
            SeamJsons::new(&classified, total - last).prefix.len(),
            all.len() - 1
        );
        assert!(SeamJsons::new(&classified, 0).prefix.is_empty());

        // Full, partial and empty prefixes all write the same document.
        let streamed = document(0);
        assert_eq!(document(usize::MAX), streamed);
        assert_eq!(document(total - 1), streamed);
        assert_eq!(document(all[0].len()), streamed);
    }

    fn ts_guidance(ts_file_count: usize) -> TsFullRepoGuidance {
        TsFullRepoGuidance {
            ts_file_count,
            readiness: TypeScriptRepoReadiness {
                source_file_count: ts_file_count.saturating_sub(1),
                test_file_count: 1,
                package_root_count: 1,
                package_confidence: "high".to_string(),
                runner_status: "resolved".to_string(),
                verify_command_count: 1,
                top_blocker: None,
            },
        }
    }

    fn classified_at(file: &str, owner: &str, line: usize, class: SeamGripClass) -> ClassifiedSeam {
        let seam = RepoSeam::new(
            file,
            owner,
            SeamKind::PredicateBoundary,
            line * 10,
            line,
            "amount >= discount_threshold",
            RequiredDiscriminator::BoundaryValue {
                description: "amount >= discount_threshold".to_string(),
            },
            ExpectedSink::ReturnValue,
        );
        let evidence = TestGripEvidence {
            seam_id: seam.id().clone(),
            related_tests: vec![std::sync::Arc::new(RelatedTestGrip {
                test_name: "below_threshold_has_no_discount".to_string(),
                file: std::path::PathBuf::from("tests/pricing_tests.rs"),
                line: 5,
                test_target: Some(
                    crate::analysis::test_grip_evidence::TestTargetEvidence::fixture(
                        "below_threshold_has_no_discount",
                        std::path::Path::new("tests/pricing_tests.rs"),
                        5,
                    ),
                ),
                oracle_kind: OracleKind::ExactValue,
                oracle_strength: OracleStrength::Strong,
                evidence_summary: "exact value assertion".to_string(),
                relation_reason:
                    crate::analysis::test_grip_evidence::RelationReason::DirectOwnerCall,
                relation_confidence: crate::analysis::test_grip_evidence::RelationConfidence::High,
            })],
            reach: stage(StageState::Yes),
            activate: stage(StageState::Yes),
            propagate: stage(StageState::Yes),
            observe: stage(StageState::Yes),
            discriminate: stage(StageState::Yes),
            observed_values: vec![
                ValueFact {
                    line: 5,
                    text: "discounted_total(50, 100)".to_string(),
                    value: "50".to_string(),
                    context: crate::domain::ValueContext::FunctionArgument,
                }
                .into(),
            ],
            missing_discriminators: vec![MissingDiscriminatorFact {
                value: "discount_threshold (equality boundary)".to_string(),
                reason: "observed values do not include the equality-boundary case".to_string(),
                flow_sink: None,
            }],
            new_test_target: None,
            statically_contradicted_related_tests: 0,
        };
        ClassifiedSeam {
            seam,
            evidence,
            class,
        }
    }

    #[test]
    fn json_carries_schema_version_scope_and_metrics() {
        let json = render_repo_exposure_json(&[weakly_gripped_classified()], None, None, None);
        for needle in [
            "\"schema_version\": \"0.5\"",
            "\"scope\": \"repo\"",
            "\"run_status\": \"complete\"",
            "\"seams_total\": 1",
            "\"headline_eligible\": 1",
            "\"weakly_gripped\": 1",
            "\"strongly_gripped\": 0",
        ] {
            assert!(json.contains(needle), "missing {needle:?} in json:\n{json}");
        }
        assert!(
            !json.contains("\"limitations\""),
            "limitations must be absent on complete run:\n{json}"
        );
    }

    #[test]
    fn json_carries_span_coordinates_when_seam_has_span() {
        use crate::analysis::seams::SeamSpan;
        let mut entry = weakly_gripped_classified();
        entry.seam = entry.seam.with_span(SeamSpan {
            start_line: 42,
            start_column: 9,
            end_line: 42,
            end_column: 38,
        });
        let json = render_repo_exposure_json(&[entry], None, None, None);
        for needle in ["\"column\": 9", "\"end_line\": 42", "\"end_column\": 38"] {
            assert!(json.contains(needle), "missing {needle:?} in json:\n{json}");
        }
    }

    #[test]
    fn json_omits_span_coordinates_when_seam_lacks_span() {
        // The seam header must go straight from "line" to "owner" when no
        // span geometry exists. (Nested projections such as evidence_record
        // carry their own end_line keys; this asserts the seam object shape.)
        let json = render_repo_exposure_json(&[weakly_gripped_classified()], None, None, None);
        assert!(
            json.contains("\"line\": 42,\n      \"owner\""),
            "seam header grew span fields without geometry in json:\n{json}"
        );
    }

    #[test]
    fn json_carries_run_status_complete_when_no_limit_applied() {
        let json = render_repo_exposure_json(&[weakly_gripped_classified()], None, None, None);
        assert!(
            json.contains("\"run_status\": \"complete\""),
            "run_status complete missing in:\n{json}"
        );
        assert!(
            !json.contains("\"limitations\""),
            "limitations must be absent when no limit applied:\n{json}"
        );
    }

    #[test]
    fn json_carries_run_status_and_limitations_when_limit_applied() {
        use crate::analysis::{SeamLimitInfo, SeamLimitSource};
        let info = SeamLimitInfo {
            analyzed: 1,
            total: 10,
            source: SeamLimitSource::Configured,
        };
        let json =
            render_repo_exposure_json(&[weakly_gripped_classified()], Some(&info), None, None);
        assert!(
            json.contains("\"run_status\": \"seam_limit_applied\""),
            "run_status seam_limit_applied missing in:\n{json}"
        );
        assert!(
            json.contains("\"limitations\""),
            "limitations block missing in:\n{json}"
        );
        assert!(
            json.contains("\"category\": \"repo_seam_limit_applied\""),
            "category missing in:\n{json}"
        );
        assert!(
            json.contains("\"seams_analyzed\": 1"),
            "seams_analyzed missing in:\n{json}"
        );
        assert!(
            json.contains("\"seams_total\": 10"),
            "seams_total missing in:\n{json}"
        );
        assert!(
            json.contains("\"control\": \"RIPR_REPO_EXPOSURE_SEAM_LIMIT\""),
            "control missing in:\n{json}"
        );
    }

    #[test]
    fn pilot_limit_disclosure_names_the_applied_cap() -> Result<(), String> {
        let classified = [weakly_gripped_classified()];
        for (source, source_name, route, markdown_line) in [
            (
                SeamLimitSource::Default,
                "default",
                "Set RIPR_PILOT_SEAM_BUDGET=0 to disable the pilot artifact budget.",
                "> Partial scan: analyzed 1 of 3 seams (seam_limit_applied; set RIPR_PILOT_SEAM_BUDGET=0 to disable the pilot artifact budget).",
            ),
            (
                SeamLimitSource::Configured,
                "configured",
                "Set RIPR_PILOT_SEAM_BUDGET=0 to disable the pilot artifact budget, or raise it to render more seams in the pilot artifacts.",
                "> Partial scan: analyzed 1 of 3 seams (seam_limit_applied; set RIPR_PILOT_SEAM_BUDGET=0 to disable the pilot artifact budget, or raise it to render more seams in the pilot artifacts).",
            ),
        ] {
            let info = SeamLimitInfo {
                analyzed: 1,
                total: 3,
                source,
            };
            let limit = Some(RepoExposureLimit::PilotBudget(&info));
            let json = render_repo_exposure_json_with_limit(&classified, limit, None, None, None);
            let artifact: Value = serde_json::from_str(&json)
                .map_err(|err| format!("parse pilot disclosure failed: {err}\n{json}"))?;
            assert_eq!(artifact["run_status"], "seam_limit_applied");
            assert_eq!(artifact["seams"].as_array().map(Vec::len), Some(1));
            assert_eq!(
                artifact["limitations"],
                json!([{
                    "category": "pilot_seam_budget_applied",
                    "seams_analyzed": 1,
                    "seams_total": 3,
                    "limit_source": source_name,
                    "control": "RIPR_PILOT_SEAM_BUDGET",
                    "repair_route": route,
                }])
            );
            let md = render_repo_exposure_md_with_limit(&classified, limit, None, None, None);
            assert_eq!(
                md.lines().find(|line| line.starts_with("> Partial scan:")),
                Some(markdown_line)
            );
        }
        let json = render_repo_exposure_json_with_limit(&classified, None, None, None, None);
        let artifact: Value = serde_json::from_str(&json)
            .map_err(|err| format!("parse uncapped disclosure failed: {err}\n{json}"))?;
        assert_eq!(artifact["run_status"], "complete");
        assert_eq!(artifact["seams"].as_array().map(Vec::len), Some(1));
        assert!(artifact.get("limitations").is_none());
        let md = render_repo_exposure_md_with_limit(&classified, None, None, None, None);
        assert!(!md.contains("> Partial scan:"));
        Ok(())
    }

    #[test]
    fn inventory_limit_routes_remain_unchanged() -> Result<(), String> {
        let classified = [weakly_gripped_classified()];
        for (source, route, markdown_line) in [
            (
                SeamLimitSource::Default,
                "Set RIPR_REPO_EXPOSURE_SEAM_LIMIT=0 to analyze all seams, or scope the run to a change with `ripr check --base <REV>` (or `ripr check --diff <PATH>`).",
                "> Partial scan: analyzed 1 of 3 seams (seam_limit_applied; set RIPR_REPO_EXPOSURE_SEAM_LIMIT=0 to analyze all seams, or scope the run to a change with `ripr check --base <REV>` (or `ripr check --diff <PATH>`)).",
            ),
            (
                SeamLimitSource::Configured,
                "Remove or raise RIPR_REPO_EXPOSURE_SEAM_LIMIT to analyze more seams, or scope the run to a change with `ripr check --base <REV>` (or `ripr check --diff <PATH>`).",
                "> Partial scan: analyzed 1 of 3 seams (seam_limit_applied; remove or raise RIPR_REPO_EXPOSURE_SEAM_LIMIT to analyze more seams, or scope the run to a change with `ripr check --base <REV>` (or `ripr check --diff <PATH>`)).",
            ),
        ] {
            let info = SeamLimitInfo {
                analyzed: 1,
                total: 3,
                source,
            };
            let json = render_repo_exposure_json(&classified, Some(&info), None, None);
            let artifact: Value = serde_json::from_str(&json)
                .map_err(|err| format!("parse inventory disclosure failed: {err}\n{json}"))?;
            assert_eq!(
                artifact["limitations"][0]["category"],
                "repo_seam_limit_applied"
            );
            assert_eq!(
                artifact["limitations"][0]["control"],
                "RIPR_REPO_EXPOSURE_SEAM_LIMIT"
            );
            assert_eq!(artifact["limitations"][0]["repair_route"], route);
            let md = render_repo_exposure_md(&classified, Some(&info), None, None);
            assert_eq!(
                md.lines().find(|line| line.starts_with("> Partial scan:")),
                Some(markdown_line)
            );
        }
        Ok(())
    }

    #[test]
    fn json_marks_declined_rust_files_as_a_limited_run() -> Result<(), String> {
        // #9308: a parse-limit decline under-credits seams, so the run is not
        // "complete"; a seam cap still keeps its own status.
        let declined = vec![crate::analysis::DeclinedRustFile {
            path: PathBuf::from("src/deep.rs"),
            reason: "static limit rust_nesting_budget: estimated nesting depth 10001 exceeds the 256-level parse budget".to_string(),
        }];
        let skip = GeneratedRustSkip::from_parts(Vec::new(), Vec::new(), declined)
            .ok_or_else(|| "declined files must produce a disclosure".to_string())?;
        let json = render_repo_exposure_json_with_generated_skip(
            &[weakly_gripped_classified()],
            None,
            None,
            None,
            Some(&skip),
        );
        let artifact: Value = serde_json::from_str(&json)
            .map_err(|err| format!("parse declined disclosure failed: {err}\n{json}"))?;
        assert_eq!(artifact["run_status"], "limited_declined_files", "{json}");
        let limitations = artifact["limitations"]
            .as_array()
            .ok_or_else(|| format!("limitations missing:\n{json}"))?;
        assert_eq!(limitations.len(), 1, "no generated entry without paths");
        assert_eq!(limitations[0]["category"], "rust_files_declined");
        assert_eq!(limitations[0]["declined_file_count"], 1);
        assert_eq!(limitations[0]["declined_files"][0]["path"], "src/deep.rs");
        let md = render_repo_exposure_md_with_generated_skip(
            &[weakly_gripped_classified()],
            None,
            None,
            None,
            Some(&skip),
        );
        assert!(md.contains("rust_files_declined"), "{md}");
        assert!(!md.contains("generated_rust_source_skipped"), "{md}");

        let info = SeamLimitInfo {
            analyzed: 1,
            total: 3,
            source: SeamLimitSource::Default,
        };
        let capped = render_repo_exposure_json_with_generated_skip(
            &[weakly_gripped_classified()],
            Some(&info),
            None,
            None,
            Some(&skip),
        );
        let artifact: Value = serde_json::from_str(&capped)
            .map_err(|err| format!("parse capped disclosure failed: {err}\n{capped}"))?;
        assert_eq!(artifact["run_status"], "seam_limit_applied");
        assert_eq!(artifact["limitations"].as_array().map(Vec::len), Some(2));
        Ok(())
    }

    #[test]
    fn json_discloses_generated_rust_skip_without_changing_run_status() -> Result<(), String> {
        let paths = vec![
            PathBuf::from("src/bindings.rs"),
            PathBuf::from("src/schema.rs"),
        ];
        let skip = GeneratedRustSkip::from_paths(paths.clone(), paths)
            .ok_or_else(|| "generated skip must be Some for nonempty paths".to_string())?;
        let json = render_repo_exposure_json_with_generated_skip(
            &[weakly_gripped_classified()],
            None,
            None,
            None,
            Some(&skip),
        );
        assert!(
            json.contains("\"run_status\": \"complete\""),
            "generated skip must not look like a truncated scan:\n{json}"
        );
        assert!(
            json.contains("\"category\": \"generated_rust_source_skipped\""),
            "category missing in:\n{json}"
        );
        assert!(
            json.contains("src/bindings.rs") && json.contains("src/schema.rs"),
            "skipped paths missing in:\n{json}"
        );
        assert!(
            !json.contains("src/lib.rs"),
            "hand-written files must not be listed as skipped:\n{json}"
        );
        let md = render_repo_exposure_md_with_generated_skip(
            &[weakly_gripped_classified()],
            None,
            None,
            None,
            Some(&skip),
        );
        assert!(
            md.contains("generated_rust_source_skipped"),
            "markdown skip category missing in:\n{md}"
        );
        assert!(
            md.contains("src/bindings.rs"),
            "markdown skip path missing in:\n{md}"
        );
        for rendered in [&json, &md] {
            assert!(rendered.contains("handwritten_files"));
            assert!(rendered.contains("vendor markers remain excluded"));
        }
        let protected =
            GeneratedRustSkip::from_paths(vec![PathBuf::from("src/schema.rs")], Vec::new())
                .ok_or("protected skip required")?;
        assert!(
            protected
                .repair_route()
                .contains("handwritten_files cannot override")
        );
        assert!(!protected.repair_route().contains("declare exact"));
        Ok(())
    }

    #[test]
    fn json_emits_ts_guidance_limitations_when_ts_workspace_and_empty_seams() {
        let guidance = ts_guidance(3);
        let json = render_repo_exposure_json(&[], None, Some(&guidance), None);
        assert!(
            json.contains("\"limitations\""),
            "limitations block missing in:\n{json}"
        );
        assert!(
            json.contains("\"category\": \"typescript_diff_first\""),
            "category typescript_diff_first missing in:\n{json}"
        );
        assert!(
            json.contains("\"ts_file_count\": 3"),
            "ts_file_count missing in:\n{json}"
        );
        assert!(
            json.contains("\"typescript_readiness\""),
            "typescript_readiness missing in:\n{json}"
        );
        assert!(
            json.contains("\"source_file_count\": 2"),
            "source_file_count missing in:\n{json}"
        );
        assert!(
            json.contains("\"test_file_count\": 1"),
            "test_file_count missing in:\n{json}"
        );
        assert!(
            json.contains("\"package_confidence\": \"high\""),
            "package_confidence missing in:\n{json}"
        );
        assert!(
            json.contains("\"runner_status\": \"resolved\""),
            "runner_status missing in:\n{json}"
        );
        assert!(
            json.contains("\"top_blocker\": null"),
            "top_blocker null missing in:\n{json}"
        );
        assert!(
            json.contains("run 'ripr check' or") && !json.contains("--base origin/main"),
            "repair_route missing in:\n{json}"
        );
        // run_status must still be complete (TS guidance does not change Rust scan status)
        assert!(
            json.contains("\"run_status\": \"complete\""),
            "run_status complete missing in:\n{json}"
        );
        // seams array must be empty — no fabricated TS seams
        assert!(json.contains("\"seams\": []"), "seams not empty:\n{json}");
    }

    #[test]
    fn json_emits_ts_guidance_alongside_seam_limit_when_both_apply() {
        use crate::analysis::{SeamLimitInfo, SeamLimitSource};
        let info = SeamLimitInfo {
            analyzed: 0,
            total: 5,
            source: SeamLimitSource::Default,
        };
        let guidance = ts_guidance(2);
        let json = render_repo_exposure_json(&[], Some(&info), Some(&guidance), None);
        assert!(
            json.contains("\"category\": \"repo_seam_limit_applied\""),
            "seam_limit category missing in:\n{json}"
        );
        assert!(
            json.contains("\"category\": \"typescript_diff_first\""),
            "ts_guidance category missing in:\n{json}"
        );
    }

    #[test]
    fn json_emits_python_guidance_when_python_workspace_and_empty_seams() -> Result<(), String> {
        let guidance = PythonRepoExposureGuidance {
            python_file_count: 2,
        };
        let json = render_repo_exposure_json(&[], None, None, Some(&guidance));
        assert!(json.contains("\"run_status\": \"complete\""), "{json}");
        assert!(
            json.contains("\"category\": \"python_diff_first\""),
            "{json}"
        );
        assert!(json.contains("\"python_file_count\": 2"), "{json}");
        assert!(
            json.contains("\"authority_boundary\": \"preview_advisory_only\""),
            "{json}"
        );
        assert!(
            json.contains("\"analysis_model\": \"diff_first\""),
            "{json}"
        );
        assert!(
            json.contains("does not render Python findings"),
            "repair_route missing in:\n{json}"
        );
        assert!(json.contains("\"seams\": []"), "seams not empty:\n{json}");
        assert!(
            !json.contains("\"category\": \"typescript_diff_first\""),
            "python-only guidance must not invent a TypeScript entry:\n{json}"
        );
        serde_json::from_str::<serde_json::Value>(&json)
            .map_err(|err| format!("python guidance JSON must parse: {err}\n{json}"))?;
        Ok(())
    }

    #[test]
    fn json_emits_python_guidance_after_typescript_guidance() -> Result<(), String> {
        let ts = ts_guidance(1);
        let python = PythonRepoExposureGuidance {
            python_file_count: 4,
        };
        let json = render_repo_exposure_json(&[], None, Some(&ts), Some(&python));
        let value: serde_json::Value = serde_json::from_str(&json)
            .map_err(|err| format!("parse combined guidance JSON failed: {err}\n{json}"))?;
        let limitations = value["limitations"]
            .as_array()
            .ok_or_else(|| format!("limitations missing: {json}"))?;
        let categories: Vec<&str> = limitations
            .iter()
            .filter_map(|item| item["category"].as_str())
            .collect();
        assert_eq!(
            categories,
            vec!["typescript_diff_first", "python_diff_first"]
        );
        Ok(())
    }

    #[test]
    fn json_carries_full_classified_record() {
        let json = render_repo_exposure_json(&[weakly_gripped_classified()], None, None, None);
        for needle in [
            "\"seam_id\":",
            "\"kind\": \"predicate_boundary\"",
            "\"grip_class\": \"weakly_gripped\"",
            "\"headline_eligible\": true",
            "\"reach\": \"yes\"",
            "\"discriminate\": \"yes\"",
            "\"evidence_record\":",
            "\"schema_version\":\"0.1\"",
            "\"canonical_gap_id\":\"gap:",
            "\"canonical_gap_group_size\":1",
            "\"canonical_gap_reason\":\"same owner, seam kind, flow sink, missing discriminator, and assertion shape\"",
            "\"raw_findings\":[",
            "\"canonical_item\":",
            "\"gap_state\":\"actionable\"",
            "\"actionability\":\"extend_related_test\"",
            "\"evidence_path\":",
            "\"actionable_related_test_extension\"",
            "\"agreement\":\"no_runtime_data\"",
            "below_threshold_has_no_discount",
            "exact_value",
            "discount_threshold (equality boundary)",
        ] {
            assert!(json.contains(needle), "missing {needle:?} in json:\n{json}");
        }
    }

    #[test]
    fn json_emits_empty_seams_array_when_inventory_is_empty() {
        let json = render_repo_exposure_json(&[], None, None, None);
        assert!(json.contains("\"seams\": []"));
        assert!(json.contains("\"seams_total\": 0"));
    }

    #[test]
    fn summary_json_contains_counts_and_omits_per_seam_payloads() -> Result<(), String> {
        let summary = render_repo_exposure_summary_json(
            &[weakly_gripped_classified()],
            std::path::Path::new("."),
            Some("origin/main"),
            "draft",
        );
        let value: serde_json::Value = serde_json::from_str(&summary)
            .map_err(|err| format!("parse summary JSON failed: {err}\n{summary}"))?;

        assert_eq!(value["schema_version"], "0.1");
        assert_eq!(value["format"], "repo-exposure-summary-json");
        assert_eq!(value["scope"], "repo");
        assert_eq!(value["basis"], "canonical_actionable_gap");
        assert_eq!(value["metadata"]["base"], "origin/main");
        assert_eq!(value["metadata"]["head"], "HEAD");
        assert_eq!(value["metrics"]["raw_seams"], 1);
        assert_eq!(value["metrics"]["headline_eligible_seams"], 1);
        assert_eq!(value["metrics"]["canonical_gap_records"], 1);
        assert_eq!(value["metrics"]["unsuppressed_exposure_gaps"], 1);
        assert_eq!(
            value["reason_breakdown"]["actionability"]["extend_related_test"],
            1
        );
        assert_eq!(value["reason_breakdown"]["grip_class"]["weakly_gripped"], 1);
        let top_files = value["top_files"]
            .as_array()
            .ok_or_else(|| format!("top_files is not an array: {value}"))?;
        assert_eq!(top_files.len(), 1);
        assert_eq!(top_files[0]["file"], "src/pricing.rs");
        assert_eq!(top_files[0]["unsuppressed_exposure_gaps"], 1);

        assert!(value.get("seams").is_none());
        for forbidden in [
            "\"evidence_record\"",
            "\"related_tests\"",
            "\"observed_values\"",
            "\"missing_discriminators\"",
        ] {
            assert!(
                !summary.contains(forbidden),
                "summary should omit {forbidden}: {summary}"
            );
        }
        Ok(())
    }

    #[test]
    fn summary_json_bounds_top_files() -> Result<(), String> {
        let mut classified = Vec::new();
        for idx in 0..30 {
            classified.push(classified_at(
                &format!("src/file_{idx}.rs"),
                &format!("pricing::owner_{idx}"),
                idx + 1,
                SeamGripClass::WeaklyGripped,
            ));
        }

        let summary = render_repo_exposure_summary_json(
            &classified,
            std::path::Path::new("."),
            None,
            "ready",
        );
        let value: serde_json::Value = serde_json::from_str(&summary)
            .map_err(|err| format!("parse summary JSON failed: {err}\n{summary}"))?;
        let top_files = value["top_files"]
            .as_array()
            .ok_or_else(|| format!("top_files is not an array: {value}"))?;

        assert_eq!(top_files.len(), 25);
        assert_eq!(value["limits"]["top_files_limit"], 25);
        assert_eq!(value["limits"]["top_files_total"], 30);
        assert_eq!(value["limits"]["top_files_truncated"], true);
        Ok(())
    }

    #[test]
    fn markdown_renders_summary_table_and_top_gaps() {
        let md = render_repo_exposure_md(&[weakly_gripped_classified()], None, None, None);
        assert!(md.contains("# ripr repo exposure report"));
        assert!(md.contains("## Summary"));
        assert!(md.contains("| seams_total | 1 |"));
        assert!(md.contains("| weakly_gripped | 1 |"));
        assert!(md.contains("## Top gaps"));
        assert!(md.contains("predicate_boundary"));
        assert!(md.contains("amount >= discount_threshold"));
        assert!(md.contains("discount_threshold (equality boundary)"));
    }

    fn top_gap_heading(md: &str) -> &str {
        md.lines()
            .find(|line| line.starts_with("### "))
            .unwrap_or("")
    }

    #[test]
    fn markdown_top_gap_heading_puts_the_path_in_a_code_span() {
        let md = render_repo_exposure_md(&[weakly_gripped_classified()], None, None, None);
        let heading = top_gap_heading(&md);
        assert_eq!(
            heading,
            format!(
                "### {}:{} {}",
                code_span("src/pricing.rs"),
                42,
                "predicate_boundary",
            ),
            "ordinary path must be a code span in the heading, like owner:\n{md}"
        );
        assert!(
            md.contains(&format!(
                "- owner: {}\n",
                code_span("pricing::discounted_total")
            )),
            "owner line remains a code span:\n{md}"
        );
        assert_ne!(
            heading,
            format!("### {}", code_span("src/pricing.rs:42 predicate_boundary")),
            "line and kind stay outside the path span:\n{md}"
        );
        assert_ne!(
            heading, "### src/pricing.rs:42 predicate_boundary",
            "bare path in the heading lets Markdown parse the file name:\n{md}"
        );
    }

    #[test]
    fn markdown_top_gap_heading_code_span_holds_link_emphasis_and_backticks() {
        // Unix-legal name reachable through #[path]: inner backtick, emphasis,
        // and a Markdown link. #4473's inline_prose leaves [y](z) and * live
        // in a heading; wrapping the path in code_span (same owner as owner:)
        // is the heading-only fix (#4605).
        let path = "src/a|b`x]*[y](z).rs";
        let md = render_repo_exposure_md(
            &[classified_at(
                path,
                "odd::owner",
                2,
                SeamGripClass::WeaklyGripped,
            )],
            None,
            None,
            None,
        );
        let heading = top_gap_heading(&md);
        let expected = format!("### {}:{} {}", code_span(path), 2, "predicate_boundary");
        assert_eq!(
            heading, expected,
            "odd path must use the owner-line code-span fence:\n{md}"
        );
        assert_ne!(
            heading,
            format!("### {path}:2 predicate_boundary"),
            "bare [y](z) and * in the heading render as a link and emphasis:\n{md}"
        );
        assert_ne!(
            heading,
            format!("### {}:2 predicate_boundary", inline_prose_literal(path)),
            "literal-markup neutralization is not this heading's contract:\n{md}"
        );
        assert!(
            md.contains(&format!("- owner: {}\n", code_span("odd::owner"))),
            "owner line stays a code span for the odd owner too:\n{md}"
        );
    }

    #[test]
    fn markdown_explains_when_inventory_is_empty() {
        let md = render_repo_exposure_md(&[], None, None, None);
        assert!(md.contains("repo seam inventory is empty"));
    }

    #[test]
    fn markdown_explains_when_no_headline_gaps() {
        // A classification record with no headline-eligible class leaves
        // the Top gaps section empty.
        let mut entry = weakly_gripped_classified();
        entry.class = SeamGripClass::StronglyGripped;
        let md = render_repo_exposure_md(&[entry], None, None, None);
        assert!(md.contains("No headline-eligible seams"));
    }

    #[test]
    fn markdown_uses_static_exposure_vocabulary() {
        // Pin seam evidence framing strings; the repo-wide
        // check-static-language gate enforces forbidden-token absence.
        let md = render_repo_exposure_md(&[weakly_gripped_classified()], None, None, None);
        assert!(md.contains("ripr repo exposure report"));
        assert!(md.contains("Runtime confirmation"));
        assert!(md.contains("cargo-mutants"));
    }

    #[test]
    fn markdown_emits_ts_guidance_section_when_ts_workspace_and_empty_seams() {
        let guidance = ts_guidance(5);
        let md = render_repo_exposure_md(&[], None, Some(&guidance), None);
        assert!(
            md.contains("typescript_diff_first"),
            "guidance category missing in:\n{md}"
        );
        assert!(
            md.contains("run 'ripr check' or") && !md.contains("--base origin/main"),
            "repair route missing in:\n{md}"
        );
        assert!(
            md.contains("ts_file_count: 5"),
            "ts_file_count missing in:\n{md}"
        );
        assert!(
            md.contains("TypeScript readiness (preview, advisory)"),
            "readiness card missing in:\n{md}"
        );
        assert!(
            md.contains("| source files | 4 |"),
            "source file count missing in:\n{md}"
        );
        assert!(
            md.contains("| package confidence | high |"),
            "package confidence missing in:\n{md}"
        );
        assert!(
            md.contains("| runner status | resolved |"),
            "runner status missing in:\n{md}"
        );
        // Must also contain the empty seams message
        assert!(
            md.contains("repo seam inventory is empty"),
            "empty seam message missing in:\n{md}"
        );
        // Must not use forbidden static-language words
        // ripr-allow: static-language: test guard checks that the prohibited term does not appear in rendered output
        assert!(!md.contains("untested"), "forbidden word 'untested': {md}"); // ripr-allow: static-language: guard string in test, not in output
        // ripr-allow: static-language: test guard checks that the prohibited term does not appear in rendered output
        assert!(!md.contains("proven"), "forbidden word 'proven': {md}"); // ripr-allow: static-language: guard string in test, not in output
    }

    #[test]
    fn markdown_emits_python_guidance_when_python_workspace_and_empty_seams() {
        let guidance = PythonRepoExposureGuidance {
            python_file_count: 3,
        };
        let md = render_repo_exposure_md(&[], None, None, Some(&guidance));
        assert!(md.contains("python_diff_first"), "{md}");
        assert!(md.contains("python_file_count: 3"), "{md}");
        assert!(md.contains("does not render Python findings"), "{md}");
        assert!(md.contains("repo seam inventory is empty"), "{md}");
        assert!(!md.contains("typescript_diff_first"), "{md}");
        // ripr-allow: static-language: test guard checks that the prohibited term does not appear in rendered output
        assert!(!md.contains("untested"), "{md}"); // ripr-allow: static-language: guard string in test, not in output
        // ripr-allow: static-language: test guard checks that the prohibited term does not appear in rendered output
        assert!(!md.contains("proven"), "{md}"); // ripr-allow: static-language: guard string in test, not in output
    }

    #[test]
    fn markdown_does_not_emit_ts_guidance_when_rust_seams_present() {
        // When seams exist (Rust workspace), guidance must not fire.
        let md = render_repo_exposure_md(&[weakly_gripped_classified()], None, None, None);
        assert!(
            !md.contains("typescript_diff_first"),
            "guidance must not appear for Rust workspace: {md}"
        );
    }

    #[test]
    fn markdown_discloses_seam_limit_when_cap_fires() {
        use crate::analysis::{SeamLimitInfo, SeamLimitSource};
        let info = SeamLimitInfo {
            analyzed: 5,
            total: 1000,
            source: SeamLimitSource::Default,
        };
        let md = render_repo_exposure_md(&[weakly_gripped_classified()], Some(&info), None, None);
        assert!(
            md.contains("Partial scan"),
            "partial scan disclosure missing in:\n{md}"
        );
        assert!(
            md.contains("analyzed 5 of 1000 seams"),
            "seam counts missing in:\n{md}"
        );
        assert!(
            md.contains("seam_limit_applied"),
            "seam_limit_applied token missing in:\n{md}"
        );
        assert!(
            md.contains("RIPR_REPO_EXPOSURE_SEAM_LIMIT"),
            "control env var missing in:\n{md}"
        );
        assert!(
            md.contains("## Limitations"),
            "Limitations section header missing in:\n{md}"
        );
    }

    #[test]
    fn markdown_does_not_disclose_seam_limit_when_no_cap_fires() {
        // Fail-closed: no limit_info → no partial-scan line.
        let md = render_repo_exposure_md(&[weakly_gripped_classified()], None, None, None);
        assert!(
            !md.contains("Partial scan"),
            "partial scan must not appear without a real cap: {md}"
        );
        assert!(
            !md.contains("seam_limit_applied"),
            "seam_limit_applied must not appear without a real cap: {md}"
        );
    }

    #[test]
    fn markdown_discloses_seam_limit_configured_source() {
        use crate::analysis::{SeamLimitInfo, SeamLimitSource};
        let info = SeamLimitInfo {
            analyzed: 3,
            total: 500,
            source: SeamLimitSource::Configured,
        };
        let md = render_repo_exposure_md(&[], Some(&info), None, None);
        assert!(
            md.contains("Partial scan"),
            "partial scan disclosure missing in:\n{md}"
        );
        assert!(
            md.contains("analyzed 3 of 500 seams"),
            "seam counts missing in:\n{md}"
        );
    }

    #[test]
    fn empty_markdown_report_names_a_next_step() {
        let md = render_repo_exposure_md(&[], None, None, None);
        assert!(md.contains("No classified seams."), "{md}");
        assert!(
            md.ends_with(REPO_EXPOSURE_EMPTY_NEXT_STEP),
            "empty report must end with a next step: {md}"
        );
        assert!(md.contains("ripr doctor --root <DIR>"), "{md}");
        // A non-empty report does not carry the empty-report next step.
        let populated = render_repo_exposure_md(&[weakly_gripped_classified()], None, None, None);
        assert!(!populated.contains("Nothing was found to analyze"));
    }

    #[test]
    fn seam_limit_hint_names_a_runnable_check_command() {
        use crate::analysis::{SeamLimitInfo, SeamLimitSource};
        for source in [SeamLimitSource::Default, SeamLimitSource::Configured] {
            let info = SeamLimitInfo {
                analyzed: 3,
                total: 500,
                source,
            };
            for rendered in [
                render_repo_exposure_md(&[], Some(&info), None, None),
                render_repo_exposure_json(&[], Some(&info), None, None),
            ] {
                assert!(
                    rendered.contains("`ripr check --base <REV>` (or `ripr check --diff <PATH>`)"),
                    "hint must be runnable: {rendered}"
                );
                assert!(
                    !rendered.contains("`ripr check --diff`"),
                    "--diff without PATH is not runnable: {rendered}"
                );
            }
        }
    }

    #[test]
    fn given_repo_exposure_related_tests_when_rendered_then_relation_reason_and_confidence_are_present()
     {
        // Both JSON and Markdown emit the relation_reason +
        // relation_confidence fields per related test. Pinned by
        // schema bump 0.1 → 0.2.
        let json = render_repo_exposure_json(&[weakly_gripped_classified()], None, None, None);
        assert!(
            json.contains("\"relation_reason\": \"direct_owner_call\""),
            "JSON missing relation_reason: {json}"
        );
        assert!(
            json.contains("\"relation_confidence\": \"high\""),
            "JSON missing relation_confidence: {json}"
        );

        let md = render_repo_exposure_md(&[weakly_gripped_classified()], None, None, None);
        assert!(
            md.contains("direct_owner_call"),
            "Markdown missing direct_owner_call tag: {md}"
        );
        assert!(md.contains("high"), "Markdown missing confidence tag: {md}");
    }

    #[test]
    fn repo_exposure_json_normalizes_seam_and_related_test_paths() -> Result<(), String> {
        let mut classified = classified_at(
            r"crates\faultline-app\src\lib.rs",
            "faultline_app::calculate",
            7,
            SeamGripClass::WeaklyGripped,
        );
        std::sync::Arc::make_mut(&mut classified.evidence.related_tests[0]).file =
            std::path::PathBuf::from(r"crates\faultline-app\tests\integration.rs");
        std::sync::Arc::make_mut(&mut classified.evidence.related_tests[0]).test_target = Some(
            crate::analysis::test_grip_evidence::TestTargetEvidence::fixture(
                "below_threshold_has_no_discount",
                std::path::Path::new(r"crates\faultline-app\tests\integration.rs"),
                5,
            ),
        );

        let json = render_repo_exposure_json(&[classified], None, None, None);
        let value: serde_json::Value = serde_json::from_str(&json)
            .map_err(|err| format!("parse repo exposure JSON failed: {err}\n{json}"))?;
        let seam = value["seams"]
            .as_array()
            .and_then(|seams| seams.first())
            .ok_or_else(|| format!("repo exposure JSON missing first seam: {value}"))?;
        let seam_file = seam["file"]
            .as_str()
            .ok_or_else(|| format!("seam file is not a string: {seam}"))?;
        let related_file = seam["related_tests"][0]["file"]
            .as_str()
            .ok_or_else(|| format!("related test file is not a string: {seam}"))?;

        if seam_file != "crates/faultline-app/src/lib.rs" {
            return Err(format!("seam file must use forward slashes: {seam_file}"));
        }
        if related_file != "crates/faultline-app/tests/integration.rs" {
            return Err(format!(
                "related test file must use forward slashes: {related_file}"
            ));
        }
        Ok(())
    }

    #[test]
    fn given_repo_exposure_related_tests_when_helper_owner_call_then_additive_reason_is_emitted() {
        let mut classified = weakly_gripped_classified();
        std::sync::Arc::make_mut(&mut classified.evidence.related_tests[0]).relation_reason =
            crate::analysis::test_grip_evidence::RelationReason::HelperOwnerCall;

        let json = render_repo_exposure_json(&[classified.clone()], None, None, None);
        assert!(
            json.contains("\"relation_reason\": \"helper_owner_call\""),
            "JSON missing helper_owner_call relation_reason: {json}"
        );

        let md = render_repo_exposure_md(&[classified], None, None, None);
        assert!(
            md.contains("helper_owner_call"),
            "Markdown missing helper_owner_call tag: {md}"
        );
    }

    fn byte_bound_corpus() -> Vec<ClassifiedSeam> {
        (0..6)
            .map(|idx| {
                classified_at(
                    &format!("src/f{idx}.rs"),
                    &format!("f{idx}::gate"),
                    idx + 1,
                    SeamGripClass::Ungripped,
                )
            })
            .collect()
    }

    fn render_bounded(
        classified: &[ClassifiedSeam],
        limit: Option<RepoExposureLimit<'_>>,
        bound: Option<&RepoExposureByteBound>,
    ) -> Result<serde_json::Value, String> {
        let json = render_repo_exposure_json_bounded(classified, limit, None, None, None, bound);
        serde_json::from_str(&json).map_err(|err| format!("bounded JSON did not parse: {err}"))
    }

    fn seam_files(document: &serde_json::Value) -> Vec<String> {
        document["seams"]
            .as_array()
            .map(|seams| {
                seams
                    .iter()
                    .filter_map(|seam| seam["file"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn byte_bound_cuts_seams_and_discloses_the_cut_in_the_document() -> Result<(), String> {
        let classified = byte_bound_corpus();
        let canonical_gaps = canonical_gap_identities(&classified);
        let one = render_seam_json(&classified[0], &canonical_gaps).len();
        // Room for exactly three entries.
        let budget = one * 3 + one / 2;
        let bound = bound_repo_exposure_seams(
            &classified,
            Some((budget, SeamLimitSource::Configured)),
            |_| false,
            &[],
        )
        .ok_or("six entries over a three-entry budget must be cut")?;
        assert_eq!(bound.rendered(), 3);
        assert_eq!(bound.total, 6);
        assert!(bound.bytes <= budget, "{} > {budget}", bound.bytes);

        let document = render_bounded(&classified, None, Some(&bound))?;
        assert_eq!(document["run_status"], REPO_EXPOSURE_BYTE_BOUND_RUN_STATUS);
        assert_eq!(document["seams"].as_array().map(Vec::len), Some(3));
        // The file's own metrics describe the file's own seams.
        assert_eq!(document["metrics"]["seams_total"], 3);
        assert!(
            document.get("artifact").is_none(),
            "a bounded document must carry no comparable identity: {document}"
        );
        let entry = &document["limitations"][0];
        assert_eq!(entry["category"], "pilot_repo_exposure_byte_bound_applied");
        assert_eq!(entry["seams_rendered"], 3);
        assert_eq!(entry["seams_total"], 6);
        assert_eq!(entry["seam_bytes_rendered"], bound.bytes);
        assert_eq!(entry["byte_budget"], budget);
        assert_eq!(entry["limit_source"], "configured");
        assert_eq!(entry["control"], "RIPR_PILOT_REPO_EXPOSURE_BYTES");
        assert!(
            entry["repair_route"]
                .as_str()
                .is_some_and(|route| route.contains("RIPR_PILOT_REPO_EXPOSURE_BYTES=0")),
            "{entry}"
        );
        // Without `keep`, the cut is the inventory-order prefix.
        assert_eq!(
            seam_files(&document),
            ["src/f0.rs", "src/f1.rs", "src/f2.rs"]
        );
        Ok(())
    }

    #[test]
    fn byte_bound_that_fits_changes_nothing() -> Result<(), String> {
        let classified = byte_bound_corpus();
        let unbounded = render_repo_exposure_json_with_limit(&classified, None, None, None, None);
        // Exactly the entries' total: nothing is cut, so no bound exists and
        // the document is byte-identical to the unbounded one.
        let canonical_gaps = canonical_gap_identities(&classified);
        let total: usize = classified
            .iter()
            .map(|entry| render_seam_json(entry, &canonical_gaps).len())
            .sum::<usize>()
            + (classified.len() - 1) * SEAM_SEPARATOR_BYTES;
        for budget in [Some((total, SeamLimitSource::Default)), None] {
            let bound = bound_repo_exposure_seams(&classified, budget, |_| false, &[]);
            assert_eq!(bound, None);
            assert_eq!(
                render_repo_exposure_json_bounded(
                    &classified,
                    None,
                    None,
                    None,
                    None,
                    bound.as_ref()
                ),
                unbounded
            );
        }
        let document: serde_json::Value =
            serde_json::from_str(&unbounded).map_err(|err| err.to_string())?;
        assert_eq!(document["run_status"], "complete");
        assert!(document.get("limitations").is_none(), "{document}");
        Ok(())
    }

    #[test]
    fn byte_bound_keeps_the_current_change_seams_first() -> Result<(), String> {
        let classified = byte_bound_corpus();
        let canonical_gaps = canonical_gap_identities(&classified);
        let one = render_seam_json(&classified[0], &canonical_gaps).len();
        let bound = bound_repo_exposure_seams(
            &classified,
            Some((one * 2 + one / 2, SeamLimitSource::Default)),
            |entry| entry.seam.file().ends_with("f5.rs"),
            &[],
        )
        .ok_or("must be cut")?;
        let document = render_bounded(&classified, None, Some(&bound))?;
        assert_eq!(seam_files(&document), ["src/f0.rs", "src/f5.rs"]);
        assert_eq!(document["limitations"][0]["limit_source"], "default");
        Ok(())
    }

    #[test]
    fn byte_bound_overflow_in_the_keep_group_ends_selection_for_both_groups() -> Result<(), String>
    {
        // f0, f1 are kept-group seams; f2.. are the rest. The budget fits one
        // kept seam but not both. The first overflow stops everything: the
        // smaller-or-equal rest seams must not fill leftover budget.
        let classified = byte_bound_corpus();
        let canonical_gaps = canonical_gap_identities(&classified);
        let one = render_seam_json(&classified[0], &canonical_gaps).len();
        let bound = bound_repo_exposure_seams(
            &classified,
            Some((one * 3 / 2, SeamLimitSource::Configured)),
            |entry| {
                let file = entry.seam.file();
                file.ends_with("f0.rs") || file.ends_with("f1.rs")
            },
            &[],
        )
        .ok_or("must be cut")?;
        let document = render_bounded(&classified, None, Some(&bound))?;
        assert_eq!(seam_files(&document), ["src/f0.rs"]);
        Ok(())
    }

    /// The corpus with seam `big` replaced by one whose rendered entry is
    /// larger than any test budget, so it can never fit.
    fn corpus_with_oversized(big: usize) -> Vec<ClassifiedSeam> {
        let mut classified = byte_bound_corpus();
        classified[big] = classified_at(
            &format!("src/f{big}.rs"),
            &format!("f{big}::{}", "x".repeat(40_000)),
            big + 1,
            SeamGripClass::Ungripped,
        );
        classified
    }

    fn one_entry_bytes(classified: &[ClassifiedSeam]) -> usize {
        render_seam_json(&classified[0], &canonical_gap_identities(classified)).len()
    }

    /// #12151: an oversized seam in the middle of the current-change group is
    /// skipped and counted, and the later current-change seam is kept. Negative
    /// experiments: the pre-fix `break` keeps only `f0`; dropping the
    /// stop-after-a-skipped-group rule lets the rest group fill the leftover.
    #[test]
    fn byte_bound_skips_an_oversized_seam_without_starving_later_ones_or_crossing_groups()
    -> Result<(), String> {
        let classified = corpus_with_oversized(1);
        let one = one_entry_bytes(&classified);
        let budget = one * 7 / 2;
        let canonical_gaps = canonical_gap_identities(&classified);
        let big = render_seam_json(&classified[1], &canonical_gaps).len();
        assert!(big > budget, "fixture: seam 1 must exceed the budget");
        let bound = bound_repo_exposure_seams(
            &classified,
            Some((budget, SeamLimitSource::Configured)),
            |entry| {
                let file = entry.seam.file();
                file.ends_with("f0.rs") || file.ends_with("f1.rs") || file.ends_with("f2.rs")
            },
            &[],
        )
        .ok_or("must be cut")?;
        let document = render_bounded(&classified, None, Some(&bound))?;
        // f3 would still fit, but it is lower priority than the skipped seam.
        assert_eq!(seam_files(&document), ["src/f0.rs", "src/f2.rs"]);
        assert_eq!(bound.skipped_over_budget, 1);
        let entry = &document["limitations"][0];
        assert_eq!(entry["seams_rendered"], 2);
        assert_eq!(entry["seams_total"], 6);
        assert_eq!(entry["seams_skipped_over_budget"], 1);

        // Discriminating negative: with no oversized seam nothing is skipped,
        // and the rest group fills the budget.
        let plain = byte_bound_corpus();
        let bound = bound_repo_exposure_seams(
            &plain,
            Some((one * 7 / 2, SeamLimitSource::Configured)),
            |entry| entry.seam.file().ends_with("f0.rs"),
            &[],
        )
        .ok_or("must be cut")?;
        assert_eq!(bound.skipped_over_budget, 0);
        assert_eq!(bound.rendered(), 3);
        Ok(())
    }

    /// #12152: after the current change, the cut keeps seams by pilot rank,
    /// not inventory order, and still writes the kept seams in inventory
    /// order. Negative experiments: ignoring `ranked` keeps `f0`; writing in
    /// rank order would list `f5` before `f4`.
    #[test]
    fn byte_bound_keeps_the_top_ranked_seam_even_when_it_is_last_in_inventory() -> Result<(), String>
    {
        let classified = byte_bound_corpus();
        let one = one_entry_bytes(&classified);
        let ranked = [&classified[5], &classified[4]];
        let bound = bound_repo_exposure_seams(
            &classified,
            Some((one * 3 / 2, SeamLimitSource::Default)),
            |_| false,
            &ranked,
        )
        .ok_or("must be cut")?;
        let document = render_bounded(&classified, None, Some(&bound))?;
        assert_eq!(seam_files(&document), ["src/f5.rs"]);
        assert_eq!(
            document["limitations"][0]["selection_order"],
            "current_change_then_pilot_rank_then_inventory"
        );

        let bound = bound_repo_exposure_seams(
            &classified,
            Some((one * 5 / 2, SeamLimitSource::Default)),
            |_| false,
            &ranked,
        )
        .ok_or("must be cut")?;
        let document = render_bounded(&classified, None, Some(&bound))?;
        assert_eq!(seam_files(&document), ["src/f4.rs", "src/f5.rs"]);

        // The current change still leads the ranking.
        let bound = bound_repo_exposure_seams(
            &classified,
            Some((one * 3 / 2, SeamLimitSource::Default)),
            |entry| entry.seam.file().ends_with("f2.rs"),
            &ranked,
        )
        .ok_or("must be cut")?;
        let document = render_bounded(&classified, None, Some(&bound))?;
        assert_eq!(seam_files(&document), ["src/f2.rs"]);

        // An oversized top-ranked seam is skipped, the next ranked seam is
        // kept, and the unranked seams behind them are not reached.
        let classified = corpus_with_oversized(5);
        let one = one_entry_bytes(&classified);
        let ranked = [&classified[5], &classified[4]];
        let bound = bound_repo_exposure_seams(
            &classified,
            Some((one * 5 / 2, SeamLimitSource::Default)),
            |_| false,
            &ranked,
        )
        .ok_or("must be cut")?;
        let document = render_bounded(&classified, None, Some(&bound))?;
        assert_eq!(seam_files(&document), ["src/f4.rs"]);
        assert_eq!(bound.skipped_over_budget, 1);
        Ok(())
    }

    /// #12153: a bounded document names the population its `metrics` count
    /// and carries the true total beside the kept count; an unbounded one is
    /// unchanged.
    #[test]
    fn byte_bounded_metrics_name_their_population_and_the_true_total() -> Result<(), String> {
        let classified = byte_bound_corpus();
        let one = one_entry_bytes(&classified);
        let bound = bound_repo_exposure_seams(
            &classified,
            Some((one * 5 / 2, SeamLimitSource::Default)),
            |_| false,
            &[],
        )
        .ok_or("must be cut")?;
        let document = render_bounded(&classified, None, Some(&bound))?;
        assert_eq!(document["metrics"]["seams_total"], 2);
        assert_eq!(document["metrics"]["population"], "byte_bound_kept_seams");
        assert_eq!(document["metrics"]["population_seams_total"], 6);

        let unbounded = render_bounded(&classified, None, None)?;
        assert!(unbounded["metrics"].get("population").is_none());
        assert!(unbounded["metrics"].get("population_seams_total").is_none());
        Ok(())
    }

    /// #12153: `SeamJsons` indexes `prefix` by position in `order`. Narrowing
    /// to a subset must not keep the full set's prerendered entries, or
    /// position 0 of the subset would render seam 0. Negative experiment:
    /// setting `order` without clearing `prefix` writes `f0` for `f3`.
    #[test]
    fn seam_jsons_subset_never_serves_a_stale_prefix_entry() {
        let classified = byte_bound_corpus();
        let mut seams = SeamJsons::new(&classified, usize::MAX);
        assert_eq!(seams.prefix.len(), classified.len(), "fixture: prerendered");
        seams.restrict_to(&[3]);
        assert!(seams.prefix.is_empty());
        let rendered = seams.with_entry(0, str::to_string);
        assert!(rendered.contains("src/f3.rs"), "{rendered}");
        assert!(!rendered.contains("src/f0.rs"), "{rendered}");
    }

    #[test]
    fn byte_bound_counts_the_separators_the_writer_adds_at_the_exact_threshold()
    -> Result<(), String> {
        let classified = byte_bound_corpus();
        let canonical_gaps = canonical_gap_identities(&classified);
        let sizes: Vec<usize> = classified
            .iter()
            .map(|entry| render_seam_json(entry, &canonical_gaps).len())
            .collect();
        let three = sizes[..3].iter().sum::<usize>() + 2 * SEAM_SEPARATOR_BYTES;
        for (budget, expect) in [(three, 3usize), (three - 1, 2)] {
            let bound = bound_repo_exposure_seams(
                &classified,
                Some((budget, SeamLimitSource::Configured)),
                |_| false,
                &[],
            )
            .ok_or("must be cut")?;
            assert_eq!(bound.rendered(), expect, "budget {budget}");
            assert!(bound.bytes <= budget);
            let document = render_bounded(&classified, None, Some(&bound))?;
            assert_eq!(document["seams"].as_array().map(Vec::len), Some(expect));
            let written: usize = bound.kept.iter().map(|i| sizes[*i]).sum::<usize>()
                + (expect - 1) * SEAM_SEPARATOR_BYTES;
            assert_eq!(written, bound.bytes);
        }
        Ok(())
    }

    #[test]
    fn byte_bounded_predicate_reads_status_or_category_and_nothing_else() {
        use serde_json::json;
        assert!(is_byte_bounded_repo_exposure(
            &json!({"run_status": "limited_repo_exposure_bytes"})
        ));
        assert!(is_byte_bounded_repo_exposure(&json!({
            "run_status": "seam_limit_applied",
            "limitations": [{"category": "pilot_repo_exposure_byte_bound_applied"}]
        })));
        assert!(!is_byte_bounded_repo_exposure(&json!({
            "run_status": "seam_limit_applied",
            "limitations": [{"category": "pilot_seam_budget_applied"}]
        })));
        assert!(!is_byte_bounded_repo_exposure(
            &json!({"run_status": "complete"})
        ));
    }

    /// #12150: the seam-cut predicate reads what the writer really emits for
    /// each limit shape, and nothing else. Negative experiments: matching on
    /// `run_status` alone would flag a declined-files document; dropping the
    /// category arm would lose the control name for `Both`.
    #[test]
    fn seam_limit_cut_controls_read_the_written_documents() -> Result<(), String> {
        let classified = vec![weakly_gripped_classified()];
        let info = SeamLimitInfo {
            analyzed: 1,
            total: 3,
            source: SeamLimitSource::Default,
        };
        let controls = |limit: Option<RepoExposureLimit<'_>>| -> Result<Vec<&'static str>, String> {
            let json = render_repo_exposure_json_with_limit(&classified, limit, None, None, None);
            let document: Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
            Ok(seam_limit_cut_controls(&document))
        };
        assert_eq!(
            controls(Some(RepoExposureLimit::Inventory(&info)))?,
            vec!["RIPR_REPO_EXPOSURE_SEAM_LIMIT"]
        );
        assert_eq!(
            controls(Some(RepoExposureLimit::PilotBudget(&info)))?,
            vec!["RIPR_PILOT_SEAM_BUDGET"]
        );
        assert_eq!(
            controls(Some(RepoExposureLimit::Both(&info, &info)))?,
            vec!["RIPR_PILOT_SEAM_BUDGET", "RIPR_REPO_EXPOSURE_SEAM_LIMIT"]
        );
        // Discriminating negatives: an uncut document and a declined-files one.
        assert!(controls(None)?.is_empty());
        let declined = serde_json::json!({
            "run_status": "limited_declined_files",
            "limitations": [{"category": "rust_files_declined"}]
        });
        assert!(seam_limit_cut_controls(&declined).is_empty());
        // A cut file whose limitation entry is missing is still a cut.
        assert_eq!(
            seam_limit_cut_controls(&serde_json::json!({"run_status": "seam_limit_applied"})).len(),
            2
        );
        Ok(())
    }

    /// #12226: a document the real writer cut both ways (pilot seam budget and
    /// byte bound), for the consumer refusal tests.
    pub(crate) fn both_cuts_document_from_writer() -> Result<String, String> {
        let classified = byte_bound_corpus();
        let one = one_entry_bytes(&classified);
        let bound = bound_repo_exposure_seams(
            &classified,
            Some((one * 5 / 2, SeamLimitSource::Default)),
            |_| false,
            &[],
        )
        .ok_or("fixture: byte bound must cut")?;
        let info = SeamLimitInfo {
            analyzed: 6,
            total: 9,
            source: SeamLimitSource::Default,
        };
        Ok(render_repo_exposure_json_bounded(
            &classified,
            Some(RepoExposureLimit::PilotBudget(&info)),
            None,
            None,
            None,
            Some(&bound),
        ))
    }

    /// #12226: the control list for a both-cuts file leads with the byte
    /// budget and adds every seam control the real writer emitted; a byte-only
    /// and a seam-only document are told apart by the caller's two predicates.
    #[test]
    fn both_cuts_document_lists_the_byte_and_seam_controls() -> Result<(), String> {
        let json = both_cuts_document_from_writer()?;
        let document: Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
        assert!(
            is_byte_bounded_repo_exposure(&document),
            "fixture: byte cut"
        );
        let seam_cut = seam_limit_cut_controls(&document);
        assert_eq!(
            seam_cut,
            vec!["RIPR_PILOT_SEAM_BUDGET"],
            "fixture: seam cut"
        );
        assert_eq!(
            byte_and_seam_cut_controls(&seam_cut),
            vec!["RIPR_PILOT_REPO_EXPOSURE_BYTES", "RIPR_PILOT_SEAM_BUDGET"]
        );
        // Negative: a byte-only document has no seam cut to add.
        let classified = byte_bound_corpus();
        let one = one_entry_bytes(&classified);
        let bound = bound_repo_exposure_seams(
            &classified,
            Some((one * 5 / 2, SeamLimitSource::Default)),
            |_| false,
            &[],
        )
        .ok_or("must be cut")?;
        let byte_only = render_bounded(&classified, None, Some(&bound))?;
        assert!(is_byte_bounded_repo_exposure(&byte_only));
        assert!(seam_limit_cut_controls(&byte_only).is_empty());
        Ok(())
    }

    #[test]
    fn byte_bound_zero_budget_keeps_no_seam_and_still_discloses() -> Result<(), String> {
        let classified = byte_bound_corpus();
        let bound = bound_repo_exposure_seams(
            &classified,
            Some((1, SeamLimitSource::Configured)),
            |_| false,
            &[],
        )
        .ok_or("must be cut")?;
        let document = render_bounded(&classified, None, Some(&bound))?;
        assert_eq!(document["seams"].as_array().map(Vec::len), Some(0));
        assert_eq!(document["limitations"][0]["seams_rendered"], 0);
        assert_eq!(document["run_status"], REPO_EXPOSURE_BYTE_BOUND_RUN_STATUS);
        Ok(())
    }

    #[test]
    fn byte_bound_keeps_the_seam_limit_status_and_adds_its_own_entry() -> Result<(), String> {
        use crate::analysis::SeamLimitInfo;
        let classified = byte_bound_corpus();
        let info = SeamLimitInfo {
            analyzed: 6,
            total: 60,
            source: SeamLimitSource::Default,
        };
        let bound = bound_repo_exposure_seams(
            &classified,
            Some((1, SeamLimitSource::Default)),
            |_| false,
            &[],
        )
        .ok_or("must be cut")?;
        let document = render_bounded(
            &classified,
            Some(RepoExposureLimit::PilotBudget(&info)),
            Some(&bound),
        )?;
        assert_eq!(document["run_status"], "seam_limit_applied");
        let categories: Vec<&str> = document["limitations"]
            .as_array()
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| entry["category"].as_str())
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(
            categories,
            [
                "pilot_seam_budget_applied",
                "pilot_repo_exposure_byte_bound_applied"
            ]
        );
        Ok(())
    }

    #[test]
    fn pilot_repo_exposure_byte_budget_resolves_default_configured_and_opt_out() {
        use std::env::VarError::NotPresent;
        assert_eq!(
            pilot_repo_exposure_byte_budget_from_env(Err(NotPresent)),
            Ok(Some((
                DEFAULT_PILOT_REPO_EXPOSURE_BYTES,
                SeamLimitSource::Default
            )))
        );
        assert_eq!(
            pilot_repo_exposure_byte_budget_from_env(Ok("4096".to_string())),
            Ok(Some((4096, SeamLimitSource::Configured)))
        );
        assert_eq!(
            pilot_repo_exposure_byte_budget_from_env(Ok("0".to_string())),
            Ok(None)
        );
        for invalid in ["many", "-1", "+0", "00", "", "1.5"] {
            assert!(
                pilot_repo_exposure_byte_budget_from_env(Ok(invalid.to_string()))
                    .is_err_and(|error| error.starts_with("RIPR_PILOT_REPO_EXPOSURE_BYTES `")),
                "`{invalid}` must fail closed, not fall back to a default or the opt-out"
            );
        }
    }
}

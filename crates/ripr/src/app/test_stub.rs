//! `ripr agent stub`: resolve one gap to a compiling Rust test stub and,
//! when asked, write it.
//!
//! The producer is `analysis::test_stub`; this use case owns seam selection
//! (by seam id or by the `file:line` a `ripr check` finding prints), the
//! source read the producer works from, and the guarded write.
//!
//! It also owns the decision `ripr check` prints under its selected finding
//! (#5471): the route is printed only when this same resolver produces a
//! stub for that location, so the printed command and its answer cannot
//! disagree.

use crate::agent::loop_commands::{bound_root, display_path, root_path_display, shell_arg};
use crate::analysis;
use crate::analysis::owner_fn_line_span;
use crate::analysis::repair_route::repair_packet_eligibility;
use crate::analysis::seams::SeamGripClass;
use crate::analysis::seams::SeamKind;
use crate::analysis::test_stub::{
    RustTestStub, TestStubPlacement, TestStubRefusal, rust_test_stub,
    rust_test_stub_for_classified_seam,
};
use crate::analysis::{ClassifiedSeam, RepoSeam};
use crate::app::CheckOutput;
use crate::app::agent_brief::AgentBriefPolicy;
use crate::config::RiprConfig;
use crate::domain::{ExposureClass, Finding, LanguageId};
use crate::output::agent_seam_packets::{
    recommended_test_for, recommended_test_is_repair_edit_target,
};
use std::path::{Path, PathBuf};

/// Which gap the caller means.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TestStubSelector {
    SeamId(String),
    /// `file:line`, as `ripr check` prints a finding location. `kind` is
    /// the finding's probe family (`--kind`), when the caller has one.
    At {
        file: String,
        line: usize,
        kind: Option<String>,
    },
}

impl TestStubSelector {
    /// Attach `--kind FAMILY` to an `--at` selector. The family is the probe
    /// family `ripr check` reports (`predicate`, `return_value`,
    /// `error_path`, `match_arm`, ...); a name with no seam kind is refused.
    pub(crate) fn with_kind(self, family: &str) -> Result<Self, String> {
        let Self::At { file, line, .. } = self else {
            return Err("agent stub --kind applies to --at only".to_string());
        };
        if analysis::seam_kind_for_probe_family(family).is_none() {
            return Err(format!(
                "--kind expects a probe family such as predicate, return_value, error_path, match_arm, call_deletion, field_construction, or side_effect, got {family:?}"
            ));
        }
        Ok(Self::At {
            file,
            line,
            kind: Some(family.to_string()),
        })
    }

    pub(crate) fn parse_at(value: &str) -> Result<Self, String> {
        let (file, line) = value
            .rsplit_once(':')
            .ok_or_else(|| format!("--at expects FILE:LINE, got {value:?}"))?;
        let line = line.parse::<usize>().map_err(|_parse_err| {
            format!("--at expects FILE:LINE with a line number, got {value:?}")
        })?;
        if file.trim().is_empty() || line == 0 {
            return Err(format!("--at expects FILE:LINE, got {value:?}"));
        }
        Ok(Self::At {
            file: file.to_string(),
            line,
            kind: None,
        })
    }
}

#[derive(Debug)]
pub(crate) struct TestStubResolution {
    pub(crate) seam_id: String,
    pub(crate) owner: String,
    /// The owner file text the stub was computed from; the write re-reads
    /// and refuses when the file changed in between.
    pub(crate) source: String,
    pub(crate) outcome: Result<RustTestStub, TestStubRefusal>,
    /// Inventory grip class when classification named this seam.
    /// `--at --kind` selects from a file parse without classifying; the
    /// use case looks the class up afterward so the stub can disclose it
    /// (#7290). `None` when that lookup did not find the seam.
    pub(crate) grip_class: Option<SeamGripClass>,
    /// Set when `--at FILE:LINE` named a line that holds no gap and the
    /// nearest gap in the same function was stubbed instead (#6298), so the
    /// substitution is stated rather than left to the generated comment.
    pub(crate) at_note: Option<String>,
    /// The repair journey this stub feeds (#8727): the documented
    /// `ripr agent repair` start for the same seam plus the repair packet's
    /// admitted edit surface, projected from the classified entry that
    /// produced the stub. `None` when the route cannot be established
    /// honestly — a file-parse (`--at --kind`) resolution carries no
    /// classified entry, or the fail-closed repair-route flip does not hold —
    /// so no route is invented.
    pub(crate) next_repair: Option<StubNextRepair>,
    /// The line of the seam this resolution stubs. It differs from a
    /// requested `--at` line when the nearest gap was substituted.
    pub(crate) seam_line: usize,
    /// Set when a related test already reaches the boundary and only its
    /// assertion is weak (#10683): `check`'s wording plus that test. The CLI
    /// reports this instead of a second test skeleton.
    pub(crate) weak_oracle_guidance: Option<String>,
}

/// The repair journey one generated stub feeds (#8727). Both placements are
/// independent derivations over the same classified seam: the stub's
/// placement comes from the stub producer's owner-shape rules, the admitted
/// surface from the repair packet's recommended-test authority. Comparing
/// them at generation time turns the before phase's later dirty-premise
/// refusal into a stated, named choice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StubNextRepair {
    /// The documented start of the repair transaction for the same seam.
    pub(crate) start_command: String,
    /// The file the repair packet admits as its edit surface; the stub's
    /// test must land here for the repair's before phase to accept the edit.
    pub(crate) allowed_edit_surface: String,
    /// Set when the stub's chosen placement file differs from the packet's
    /// admitted surface: writing the stub where the stub says would make the
    /// repair's before phase refuse the edit at the dirty-premise gate.
    pub(crate) placement_divergence: Option<StubPlacementDivergence>,
}

/// Two independently derived placements that disagree at stub generation
/// time, both named so the user can place the test where the repair admits
/// it instead of discovering the refusal after the write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StubPlacementDivergence {
    /// Where the stub is placed: an inline test module of a production file,
    /// or an integration test file.
    pub(crate) stub_file: String,
    /// The only file the repair packet's edit cage admits.
    pub(crate) packet_file: String,
}

#[derive(Debug)]
pub(crate) enum TestStubError {
    /// The selector named no gap ripr can stub; the message says where to
    /// look instead.
    NotFound(String),
    /// Several candidate files or disjoint tied seams match the selector;
    /// no seam was selected and the message names how to disambiguate.
    Ambiguous {
        message: String,
        seam_ids: Vec<String>,
    },
    /// Stub generation supports Rust source paths only. This says nothing
    /// about whether another language's check finding exists or is exposed.
    UnsupportedLanguage(String),
    Operational(String),
}

pub(crate) fn resolve_test_stub(
    root: &Path,
    config: &RiprConfig,
    selector: &TestStubSelector,
) -> Result<TestStubResolution, TestStubError> {
    match selector {
        // A seam id is repo-wide, so it is looked up in the full inventory.
        TestStubSelector::SeamId(id) => {
            let (classified, _) = analysis::inventory_classified_seams_at_with_config(root, config)
                .map_err(TestStubError::Operational)?;
            let entry = classified
                .iter()
                .find(|entry| entry.seam.id().as_str() == id)
                .ok_or_else(|| TestStubError::NotFound(format!("seam_id {id} was not found")))?;
            let source = read_owner_source(root, &entry.seam)?;
            Ok(resolution_for(root, config, entry, source))
        }
        TestStubSelector::At { file, line, kind } => {
            let mut seam_index = None;
            let mut resolution =
                resolve_at_location(root, config, file, *line, kind.as_deref(), &mut seam_index)?;
            if let Some((class, guidance)) =
                inventory_entry_facts(root, config, file, &resolution.seam_id)
            {
                resolution.grip_class.get_or_insert(class);
                resolution.weak_oracle_guidance =
                    resolution.weak_oracle_guidance.take().or(guidance);
            }
            Ok(resolution)
        }
    }
}

/// A validated probe-family spelling paired with its selection kind.
#[derive(Clone, Copy)]
struct AtKind<'a> {
    family: &'a str,
    seam_kind: SeamKind,
}

/// The one `--at FILE:LINE` resolver, shared by `ripr agent stub --at` and
/// the route `ripr check` prints (#5471).
///
/// With `kind` (the finding's probe family, `--kind`) and a `file` under
/// `root`, the candidates are that file's seams from a parse of the file
/// alone: no workspace index, test evidence, or classification, so the
/// finding `check` reported is not re-judged by a second classifier (which
/// is how the route and the command disagreed). `check` already decided the
/// location is a gap of that kind; the stub needs only the seam's shape and
/// the owner source, and is placed inline (an integration-file placement
/// needs classified evidence, so it stays on `--seam-id`). Only seams of the
/// matching seam kind are tried, in line-then-nearest order, so a line
/// holding a boundary and an error variant stubs the one `check` reported
/// and never a seam of another kind.
///
/// Without `kind` nothing has vouched for the location, so a file under
/// `root` is classified in a scan scoped to that file and only its reported
/// gaps are tried: a bare `--at` never stubs a seam the tests already pin.
///
/// A suffix that names no file under `root` (`src/lib.rs` for
/// `crates/a/src/lib.rs`) keeps the repo-wide classified lookup so its
/// more-than-one-file refusal still applies; `check` never prints that form.
fn resolve_at_location(
    root: &Path,
    config: &RiprConfig,
    file: &str,
    line: usize,
    kind: Option<&str>,
    seam_index: &mut Option<analysis::FileSeamIndex>,
) -> Result<TestStubResolution, TestStubError> {
    let path = Path::new(file);
    let source_language = analysis::route(path);
    if source_language != Some(LanguageId::Rust) {
        // The router groups TS/JS under one adapter; preserve the source
        // language name without duplicating its extension inventory.
        let language = if path
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .and_then(analysis::ts_js_source_kind)
            == Some(analysis::TsJsSourceKind::JavaScript)
        {
            Some(LanguageId::JavaScript)
        } else {
            source_language
        };
        let language = language.map(LanguageId::display_name).unwrap_or("non-Rust");
        let root = shell_arg(&bound_root(&root.to_string_lossy()));
        return Err(TestStubError::UnsupportedLanguage(format!(
            "stub generation supports Rust files only; {file} is a {language} path. For a reported gap, use its `ripr check` test file/node and verify command when available. Add or strengthen the test by hand, run the verify command or your repository's test command, then rerun `ripr check --root {root}` to refresh the finding."
        )));
    }
    let kind = kind.and_then(|family| {
        analysis::seam_kind_for_probe_family(family).map(|seam_kind| AtKind { family, seam_kind })
    });
    match (scoped_file(root, file), kind) {
        (Some(relative), Some(_)) => {
            let seams = analysis::scoped_file_seams(root, config, &relative, seam_index)
                .map_err(TestStubError::Operational)?;
            let candidates = seams.iter().map(AtCandidate::Shape).collect::<Vec<_>>();
            resolve_at(root, config, candidates, file, line, kind)
        }
        (Some(relative), None) => {
            let scoped = analysis::inventory_diff_scoped_classified_seams_at_with_config(
                root,
                config,
                &[relative],
                &[],
            )
            .map_err(TestStubError::Operational)?;
            resolve_at(
                root,
                config,
                classified_candidates(&scoped.classified),
                file,
                line,
                kind,
            )
        }
        (None, _) => {
            let (classified, _) = analysis::inventory_classified_seams_at_with_config(root, config)
                .map_err(TestStubError::Operational)?;
            resolve_at(
                root,
                config,
                classified_candidates(&classified),
                file,
                line,
                kind,
            )
        }
    }
}

/// One `--at` candidate: a seam shape from a single-file parse, or a
/// classified seam from the repo-wide inventory.
#[derive(Clone, Copy)]
enum AtCandidate<'a> {
    Shape(&'a RepoSeam),
    Classified(&'a ClassifiedSeam),
}

impl<'a> AtCandidate<'a> {
    fn seam(self) -> &'a RepoSeam {
        match self {
            Self::Shape(seam) => seam,
            Self::Classified(entry) => &entry.seam,
        }
    }

    fn resolution(self, root: &Path, config: &RiprConfig, source: String) -> TestStubResolution {
        match self {
            Self::Shape(seam) => {
                let outcome = rust_test_stub(seam, None, &source);
                TestStubResolution {
                    seam_id: seam.id().as_str().to_string(),
                    owner: seam.owner().to_string(),
                    source,
                    outcome,
                    grip_class: None,
                    at_note: None,
                    // A file-parse selection carries no classified entry, so
                    // the repair packet's surface cannot be established; no
                    // route is invented (#8727).
                    next_repair: None,
                    seam_line: seam.display_line(),
                    weak_oracle_guidance: None,
                }
            }
            Self::Classified(entry) => resolution_for(root, config, entry, source),
        }
    }
}

/// The reported gaps of a classified inventory as `--at` candidates.
fn classified_candidates(classified: &[ClassifiedSeam]) -> Vec<AtCandidate<'_>> {
    classified
        .iter()
        .filter(|entry| entry.class.is_headline_eligible())
        .map(AtCandidate::Classified)
        .collect()
}

/// `file` as a root-relative path when it names a regular file under
/// `root`; `None` otherwise. Both sides are canonicalized, so `..` or a
/// symlink cannot point `--at` (and `--write`) at a file outside the root.
fn scoped_file(root: &Path, file: &str) -> Option<PathBuf> {
    let normalized = file.replace('\\', "/");
    let path = Path::new(normalized.trim_start_matches("./"));
    let canonical_root = root.canonicalize().ok()?;
    let target = if path.is_absolute() {
        path.canonicalize().ok()?
    } else {
        root.join(path).canonicalize().ok()?
    };
    let relative = target.strip_prefix(&canonical_root).ok()?.to_path_buf();
    (!relative.as_os_str().is_empty() && target.is_file()).then_some(relative)
}

/// What `ripr check` prints under its selected finding for the test-stub
/// route (#5471).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StubRouteDecision {
    /// The shared resolver produces a stub here: print
    /// `ripr agent stub --at FILE:LINE --kind FAMILY`.
    Stub {
        file: String,
        line: usize,
        kind: &'static str,
    },
    /// The shared resolver refuses: print its reason instead of a command
    /// that would only refuse.
    Refused { reason: String },
    /// An existing test already reaches the gap and only needs a stronger
    /// assertion; the guidance names that test, so it is already the next
    /// step and no hand-written-test step is added after it (#10710).
    WeakOracle { guidance: String },
    /// Unvalidated syntax selectors from the resolver; classification or
    /// stub generation can still refuse them. No seam or repair is selected.
    Ambiguous {
        reason: String,
        seam_ids: Vec<String>,
    },
    /// Several seams share the finding's line and ripr could not check which
    /// of them produces a stub (too many to validate, the inventory could not
    /// be classified, or a seam was missing from it). No command is printed;
    /// the plain reason lists no unvalidated seam IDs.
    Withheld { reason: String },
    /// Several seams share the finding's line, but exactly one of them
    /// produces a stub (#10710): print `ripr agent stub --seam-id ID`, the
    /// command that was just resolved to a stub.
    SeamStub { seam_id: String },
}

/// The most ambiguous seams checked for a stub before the choices are
/// withheld.
const AMBIGUOUS_VALIDATION_LIMIT: usize = 8;

/// What `ripr agent stub --seam-id <id>` does for one seam. A weak-oracle
/// result is its own outcome: the stub refuses because an existing test needs
/// its assertion fixed, which is not a "write one by hand" refusal.
#[derive(Clone, Debug, PartialEq, Eq)]
enum SeamStubOutcome {
    Stub,
    Refused(String),
    WeakOracle(String),
    /// The seam is not in the classified inventory, which `agent stub` words
    /// as "not found"; nothing is claimed about whether it is a gap.
    Missing,
}

/// The resolver's ambiguity message without its unvalidated `pass one of:
/// --seam-id ...` tail: only validated seam IDs may be named on screen.
fn plain_ambiguity_reason(reason: &str) -> String {
    reason
        .split("; pass one of")
        .next()
        .unwrap_or(reason)
        .trim()
        .to_string()
}

/// The reason for an ambiguity ripr could not narrow to a validated stub.
fn withheld_ambiguity(reason: &str) -> StubRouteDecision {
    StubRouteDecision::Withheld {
        reason: format!(
            "{}; ripr could not check which of them your change touches",
            plain_ambiguity_reason(reason)
        ),
    }
}

/// What `ripr agent stub --seam-id <id>` does for one seam. This is the resolution `resolve_test_stub` runs for a seam
/// ID (`resolution_for` over the classified inventory), not a second
/// validator, so a command printed on `Ok` is a command that succeeds.
fn seam_stub_refusal(
    root: &Path,
    config: &RiprConfig,
    classified: &[ClassifiedSeam],
    seam_id: &str,
) -> SeamStubOutcome {
    let Some(entry) = classified
        .iter()
        .find(|entry| entry.seam.id().as_str() == seam_id)
    else {
        return SeamStubOutcome::Missing;
    };
    let source = match read_owner_source(root, &entry.seam) {
        Ok(source) => source,
        Err(error) => return SeamStubOutcome::Refused(format!("{error:?}")),
    };
    let resolution = resolution_for(root, config, entry, source);
    // The weak-oracle guidance wins, as it does in `agent stub` itself.
    match (resolution.weak_oracle_guidance, resolution.outcome) {
        (Some(guidance), _) => SeamStubOutcome::WeakOracle(guidance),
        (None, Err(refusal)) => SeamStubOutcome::Refused(refusal.reason().to_string()),
        (None, Ok(_)) => SeamStubOutcome::Stub,
    }
}

/// Fold the per-seam outcomes into one route decision. When no seam produces
/// a stub, all-weak-oracle candidates are a `WeakOracle` decision (strengthen
/// the existing test; no hand-written step follows), otherwise the first
/// hard refusal is shown.
fn fold_seam_outcomes(
    outcomes: Vec<(String, SeamStubOutcome)>,
    reason: String,
    narrowed: bool,
) -> StubRouteDecision {
    let mut stubbable = Vec::new();
    let mut first_refusal = None;
    let mut first_weak = None;
    for (seam_id, outcome) in outcomes {
        match outcome {
            SeamStubOutcome::Stub => stubbable.push(seam_id),
            SeamStubOutcome::Refused(why) => {
                first_refusal.get_or_insert(why);
            }
            SeamStubOutcome::WeakOracle(guidance) => {
                first_weak.get_or_insert(guidance);
            }
            SeamStubOutcome::Missing => {}
        }
    }
    match stubbable.len() {
        0 => match (first_weak, first_refusal) {
            (Some(guidance), None) => StubRouteDecision::WeakOracle { guidance },
            (_, Some(why)) => StubRouteDecision::Refused { reason: why },
            (None, None) => withheld_ambiguity(&reason),
        },
        // Only a seam tied to the finding's changed expression is printed as
        // the one command. When the expression did not pick a seam, a lone
        // survivor is still just one of the seams on the line (it may be the
        // unchanged field), so it stays a choice.
        1 if narrowed => StubRouteDecision::SeamStub {
            seam_id: stubbable.swap_remove(0),
        },
        _ => StubRouteDecision::Ambiguous {
            reason: plain_ambiguity_reason(&reason),
            seam_ids: stubbable,
        },
    }
}

/// #10710: an ambiguous `--at` line names several seams, but the finding
/// changed one expression (`P { x: 1, y: 3 }` holds one seam per field and
/// only `y: 3` changed). Narrow the choices to the seam whose own expression
/// is the finding's changed expression, then keep only the seams whose
/// `ripr agent stub --seam-id` command produces a stub (the classified
/// resolution, `seam_stub_refusal`). One survivor becomes that command,
/// several stay a choice among survivors, and none becomes the first refusal.
/// No command is printed when the seams cannot be classified, or when more
/// than `AMBIGUOUS_VALIDATION_LIMIT` seams share the line: validating each
/// costs a classified resolution, and a long list is not a useful choice, so
/// the refusal is shown instead of unchecked commands.
fn narrowed_ambiguity(
    root: &Path,
    config: &RiprConfig,
    file: &str,
    expression: &str,
    reason: String,
    seam_ids: Vec<String>,
    seam_index: &mut Option<analysis::FileSeamIndex>,
) -> StubRouteDecision {
    let squash = |text: &str| text.split_whitespace().collect::<String>();
    let wanted = squash(expression);
    let mut candidates = seam_ids.clone();
    let mut narrowed = false;
    if let Some(relative) = scoped_file(root, file)
        && let Ok(seams) = analysis::scoped_file_seams(root, config, &relative, seam_index)
    {
        let mut matching = seams.iter().filter(|seam| {
            seam_ids.iter().any(|id| id == seam.id().as_str())
                && squash(seam.expression()) == wanted
        });
        if let (Some(seam), None) = (matching.next(), matching.next()) {
            candidates = vec![seam.id().as_str().to_string()];
            narrowed = true;
        }
    }
    validate_ambiguity(root, config, candidates, narrowed, reason, || {
        analysis::inventory_classified_seams_at_with_config(root, config)
            .map(|(classified, _)| classified)
    })
}

/// The validation half of `narrowed_ambiguity`, with the classified inventory
/// supplied by `load_inventory` so each withhold path is reachable on its own:
/// too many candidates (the inventory is never loaded), an inventory that
/// cannot be classified, and candidates the inventory does not hold (every
/// outcome `Missing`).
fn validate_ambiguity(
    root: &Path,
    config: &RiprConfig,
    candidates: Vec<String>,
    narrowed: bool,
    reason: String,
    load_inventory: impl FnOnce() -> Result<Vec<ClassifiedSeam>, String>,
) -> StubRouteDecision {
    if candidates.len() > AMBIGUOUS_VALIDATION_LIMIT {
        return withheld_ambiguity(&reason);
    }
    let Ok(classified) = load_inventory() else {
        return withheld_ambiguity(&reason);
    };
    let outcomes = candidates
        .into_iter()
        .map(|seam_id| {
            let outcome = seam_stub_refusal(root, config, &classified, &seam_id);
            (seam_id, outcome)
        })
        .collect();
    fold_seam_outcomes(outcomes, reason, narrowed)
}

/// A route decision bound to the finding it was computed for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StubRoute {
    pub(crate) finding_id: String,
    pub(crate) decision: StubRouteDecision,
}

/// The `FILE:LINE` and probe family a stub route names for `finding`, the
/// file relative to `root` because `--at` resolves against `--root`. `None`
/// when the finding is not a non-exposed Rust gap in a family the stub
/// producer handles.
pub(crate) fn stub_route_location(
    finding: &Finding,
    root: &Path,
) -> Option<(String, usize, &'static str)> {
    let location = &finding.probe.location.file;
    let eligible = finding.class != ExposureClass::Exposed
        && finding.probe.offers_stub_route()
        && location.extension().and_then(|ext| ext.to_str()) == Some("rs");
    if !eligible {
        return None;
    }
    let relative = location.strip_prefix(root).unwrap_or(location);
    let file = crate::output::path::display_path(relative);
    Some((
        file.trim_start_matches("./").to_string(),
        finding.probe.location.line,
        finding.probe.family.as_str(),
    ))
}

/// The route decision for one finding, from the resolver
/// `ripr agent stub --at` runs. `None` when the finding gets no route or the
/// resolver finds no seam there; nothing is printed then.
pub(crate) fn stub_route_for_finding(
    root: &Path,
    config: &RiprConfig,
    finding: &Finding,
    seam_index: &mut Option<analysis::FileSeamIndex>,
) -> Option<StubRoute> {
    let (file, line, kind) = stub_route_location(finding, root)?;
    if let Some(name) = finding.probe.changed_const_name() {
        let decision = const_consumer_route(root, config, &file, line, name, seam_index)?;
        return Some(StubRoute {
            finding_id: finding.id.clone(),
            decision,
        });
    }
    let mut resolution =
        match resolve_at_location(root, config, &file, line, Some(kind), seam_index) {
            Ok(resolution) => resolution,
            Err(TestStubError::Ambiguous { message, seam_ids }) if !seam_ids.is_empty() => {
                return Some(StubRoute {
                    finding_id: finding.id.clone(),
                    decision: narrowed_ambiguity(
                        root,
                        config,
                        &file,
                        &finding.probe.expression,
                        message,
                        seam_ids,
                        seam_index,
                    ),
                });
            }
            Err(_) => return None,
        };
    // #10683: `agent stub` refuses a weak-oracle boundary seam (an existing
    // test already reaches it); never advertise a stub command it will refuse.
    // #8547: the guidance producer only accepts PredicateBoundary. The
    // --kind resolver above selects that same mapped seam kind, so other
    // families cannot gain this pointer from a full-workspace inventory.
    // Keep the explicit stub command's lookup: it also discloses grip class.
    if resolution.weak_oracle_guidance.is_none()
        && analysis::seam_kind_for_probe_family(kind) == Some(SeamKind::PredicateBoundary)
    {
        resolution.weak_oracle_guidance =
            inventory_entry_facts(root, config, &file, &resolution.seam_id)
                .and_then(|(_, guidance)| guidance);
    }
    if let Some(guidance) = resolution.weak_oracle_guidance.take() {
        return Some(StubRoute {
            finding_id: finding.id.clone(),
            decision: StubRouteDecision::WeakOracle { guidance },
        });
    }
    let decision = match resolution.outcome {
        // `--at` stubs the nearest gap when the finding's line holds none
        // (#6298). Print the line that stub is for, and only when asking for
        // that line resolves to the same seam, so the command shown is the
        // command that produces the stub it promises (#8444).
        Ok(_) if resolution.at_note.is_some() => {
            let seam_line = verified_seam_line(root, config, &file, kind, &resolution, seam_index)?;
            StubRouteDecision::Stub {
                file,
                line: seam_line,
                kind,
            }
        }
        Ok(_) => StubRouteDecision::Stub { file, line, kind },
        Err(refusal) => StubRouteDecision::Refused {
            reason: refusal.reason().to_string(),
        },
    };
    Some(StubRoute {
        finding_id: finding.id.clone(),
        decision,
    })
}

/// The line to print for a substituted `--at` resolution: the seam's own line,
/// only when asking for that line resolves to the same seam without a
/// substitution note, so the printed command produces the stub it promises
/// (#8444).
fn verified_seam_line(
    root: &Path,
    config: &RiprConfig,
    file: &str,
    kind: &str,
    resolution: &TestStubResolution,
    seam_index: &mut Option<analysis::FileSeamIndex>,
) -> Option<usize> {
    let seam_line = resolution.seam_line;
    resolve_at_location(root, config, file, seam_line, Some(kind), seam_index)
        .is_ok_and(|exact| exact.seam_id == resolution.seam_id && exact.at_note.is_none())
        .then_some(seam_line)
}

/// Seam kinds a constant's consumer is tried as, most common first.
const CONSUMER_KINDS: [&str; 4] = ["return_value", "predicate", "error_path", "match_arm"];

/// The most consuming functions of a changed constant tried for a stub.
const CONSUMER_FN_LIMIT: usize = 6;

/// A changed `const`/`static` has no seam, so the route goes to the first
/// function in the same file that names it: a stub for that function is the
/// place to pin the constant's effect. The first consumer that yields a stub
/// wins; otherwise the first refusal names why. `None` when no consumer is
/// found or none resolves, so nothing is printed.
fn const_consumer_route(
    root: &Path,
    config: &RiprConfig,
    file: &str,
    const_line: usize,
    name: &str,
    seam_index: &mut Option<analysis::FileSeamIndex>,
) -> Option<StubRouteDecision> {
    let source = std::fs::read_to_string(root.join(file)).ok()?;
    let lines = source.lines().collect::<Vec<_>>();
    let code = code_lines_of(&source);
    // Test modules (the parser's governed `cfg(test)` inventory) are never
    // stub targets; an unparsed file has no consumer to find anyway.
    let test_start = analysis::governed_cfg_test_modules(&source)?
        .iter()
        .map(|module| source[..module.item_start].matches('\n').count() + 1)
        .min()
        .unwrap_or(lines.len() + 1);
    // The same name declared twice (an associated const per impl, a const
    // generic, a local `const`) cannot be told apart by spelling: no route.
    if code
        .iter()
        .filter(|text| declares_constant(text, name))
        .count()
        > 1
    {
        return None;
    }
    let mut seen_fns: Vec<(usize, usize)> = Vec::new();
    let mut tried = 0;
    let mut first_refusal = None;
    for index in 0..lines.len() {
        let line = index + 1;
        if line == const_line
            || line >= test_start
            || !code
                .get(index)
                .is_some_and(|code| names_identifier(code, name))
        {
            continue;
        }
        if seen_fns
            .iter()
            .any(|&(start, end)| (start..=end).contains(&line))
        {
            continue;
        }
        let Some(span) = owner_fn_line_span(&source, line) else {
            continue;
        };
        seen_fns.push(span);
        if is_test_fn(&lines, span.0) {
            continue;
        }
        tried += 1;
        if tried > CONSUMER_FN_LIMIT {
            break;
        }
        for kind in CONSUMER_KINDS {
            let Ok(resolution) =
                resolve_at_location(root, config, file, line, Some(kind), seam_index)
            else {
                continue;
            };
            match resolution.outcome {
                Ok(_) => {
                    // The reference line may not hold the seam; print the
                    // seam's own line, or skip a consumer that cannot be
                    // reproduced exactly (#8444).
                    let stub_line = if resolution.at_note.is_some() {
                        match verified_seam_line(root, config, file, kind, &resolution, seam_index)
                        {
                            Some(seam_line) => seam_line,
                            None => continue,
                        }
                    } else {
                        line
                    };
                    return Some(StubRouteDecision::Stub {
                        file: file.to_string(),
                        line: stub_line,
                        kind,
                    });
                }
                Err(refusal) => {
                    first_refusal.get_or_insert(StubRouteDecision::Refused {
                        reason: refusal.reason().to_string(),
                    });
                }
            }
        }
    }
    first_refusal
}

/// `source` with string, char and raw-string literals and comments blanked,
/// keeping every newline so line numbers hold. Block comments nest and
/// strings, raw strings and block comments may span lines; a leading `*` is
/// code (a dereference) unless a block comment is open. A `'` that opens no
/// char literal is a lifetime and stays.
fn code_lines_of(source: &str) -> Vec<String> {
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let (mut block_depth, mut in_string, mut in_line_comment) = (0usize, false, false);
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c == '\n' {
            in_line_comment = false;
            out.push('\n');
        } else if in_line_comment {
            out.push(' ');
        } else if block_depth > 0 {
            if c == '*' && next == Some('/') {
                block_depth -= 1;
                out.push_str("  ");
                i += 1;
            } else if c == '/' && next == Some('*') {
                block_depth += 1;
                out.push_str("  ");
                i += 1;
            } else {
                out.push(' ');
            }
        } else if in_string {
            if c == '\\' && next.is_some_and(|next| next != '\n') {
                out.push_str("  ");
                i += 1;
            } else {
                in_string = c != '"';
                out.push(' ');
            }
        } else if c == '"' {
            in_string = true;
            out.push(' ');
        } else if c == '/' && next == Some('/') {
            in_line_comment = true;
            out.push_str("  ");
            i += 1;
        } else if c == '/' && next == Some('*') {
            block_depth += 1;
            out.push_str("  ");
            i += 1;
        } else if let Some(end) = raw_string_end(&chars, i).or_else(|| char_literal_end(&chars, i))
        {
            for &blanked in &chars[i..end] {
                out.push(if blanked == '\n' { '\n' } else { ' ' });
            }
            i = end - 1;
        } else {
            out.push(c);
        }
        i += 1;
    }
    out.lines().map(str::to_string).collect()
}

/// The exclusive end of a raw string (`r"…"`, `r#"…"#`, `br"…"`) starting at
/// `at`, running to the end of the source when it never closes.
fn raw_string_end(chars: &[char], at: usize) -> Option<usize> {
    let after_prefix = match (chars[at], chars.get(at + 1)) {
        ('r', _) => at + 1,
        ('b', Some('r')) => at + 2,
        _ => return None,
    };
    if at > 0 && (chars[at - 1].is_alphanumeric() || chars[at - 1] == '_') {
        return None;
    }
    let hashes = chars[after_prefix..]
        .iter()
        .take_while(|&&c| c == '#')
        .count();
    if chars.get(after_prefix + hashes) != Some(&'"') {
        return None;
    }
    let body = after_prefix + hashes + 1;
    let closer = (body..chars.len()).find(|&k| {
        chars[k] == '"' && (1..=hashes).all(|offset| chars.get(k + offset) == Some(&'#'))
    });
    Some(closer.map_or(chars.len(), |k| k + 1 + hashes))
}

/// The exclusive end of a char literal (`'x'`, `'\\''`, `'\\u{1F600}'`)
/// starting at `at`; `None` for a lifetime.
fn char_literal_end(chars: &[char], at: usize) -> Option<usize> {
    if chars[at] != '\'' {
        return None;
    }
    match (chars.get(at + 1)?, chars.get(at + 2)) {
        ('\\', _) => (at + 3..chars.len().min(at + 12))
            .find(|&k| chars[k] == '\'')
            .map(|k| k + 1),
        (c, Some('\'')) if *c != '\'' && *c != '\n' => Some(at + 3),
        _ => None,
    }
}

/// Whether the function starting at `start` (1-based) is a test: one of the
/// attribute lines directly above it names `test`, `rstest` or `test_case`.
fn is_test_fn(lines: &[&str], start: usize) -> bool {
    let above = start.saturating_sub(1).min(lines.len());
    lines[..above]
        .iter()
        .rev()
        .take(8)
        .map(|text| text.trim())
        .take_while(|text| {
            !text.is_empty()
                && (text.starts_with("#[")
                    || text.starts_with("//")
                    || !text.ends_with(['{', '}', ';']))
        })
        .filter(|text| text.starts_with("#["))
        .any(|text| {
            let path = text[2..].split(['(', ']', '=']).next().unwrap_or("").trim();
            matches!(
                path.rsplit("::").next(),
                Some("test" | "rstest" | "test_case")
            )
        })
}

/// Whether `text` declares `name` as a `const` or `static`, as an item, a
/// local or a const generic parameter.
fn declares_constant(text: &str, name: &str) -> bool {
    let words = text
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    words
        .windows(2)
        .any(|window| matches!(window, ["const" | "static", declared] if *declared == name))
        || words
            .windows(3)
            .any(|window| matches!(window, ["static", "mut", declared] if *declared == name))
}

/// Whether `text` reads `name` as a whole identifier: not a field access (a
/// range `0..CAP` is a read), and
/// not behind a path other than `Self::`, `self::`, `crate::` or `super::`
/// (another module's or type's item of the same spelling).
fn names_identifier(text: &str, name: &str) -> bool {
    let is_ident = |c: char| c.is_alphanumeric() || c == '_';
    text.match_indices(name).any(|(at, _)| {
        let before = &text[..at];
        if before.chars().next_back().is_some_and(is_ident)
            || text[at + name.len()..].chars().next().is_some_and(is_ident)
        {
            return false;
        }
        if before.ends_with('.') && !before.ends_with("..") {
            return false;
        }
        match before.strip_suffix("::") {
            Some(path) => {
                let segment = path
                    .rsplit(|c: char| !is_ident(c))
                    .next()
                    .unwrap_or_default();
                matches!(segment, "Self" | "self" | "crate" | "super")
            }
            None => true,
        }
    })
}

/// Resolve the selected route plus typed ambiguities for the eligible rows
/// inside the same bounded Hidden window the default human report displays.
/// Uses the stub command's root config, disk guard and resolver. A shared seam
/// index is reused when it covers a file; alternating files may replace it.
pub(crate) fn check_stub_routes(
    root: &Path,
    check_config: &RiprConfig,
    output: &CheckOutput,
    seam_index: &mut Option<analysis::FileSeamIndex>,
) -> Vec<StubRoute> {
    let (selected, hidden) = crate::output::human::stub_route_subjects(output, check_config);
    if selected.is_none() && hidden.is_empty() {
        return Vec::new();
    }
    let Ok(config) = crate::config::load_for_root(root) else {
        return Vec::new();
    };
    selected
        .into_iter()
        .map(|finding| (finding, true))
        .chain(hidden.into_iter().map(|finding| (finding, false)))
        .filter_map(|(finding, selected)| {
            let (file, line, _) = stub_route_location(finding, root)?;
            if !finding_text_is_on_disk(root, &file, line, &finding.probe.expression) {
                return None;
            }
            if selected {
                stub_route_for_finding(root, &config, finding, seam_index)
            } else {
                hidden_stub_ambiguity(root, &config, finding, seam_index)
            }
        })
        .collect()
}

/// Hidden rows display only syntax ambiguity. Do not finish a unique route's
/// classified inventory/weak-oracle lookup merely to discard its result.
fn hidden_stub_ambiguity(
    root: &Path,
    config: &RiprConfig,
    finding: &Finding,
    seam_index: &mut Option<analysis::FileSeamIndex>,
) -> Option<StubRoute> {
    let (file, line, kind) = stub_route_location(finding, root)?;
    // A changed constant has no seam of its own; its selected route may
    // investigate consumers, but hidden syntax choices must not widen scope.
    analysis::seam_kind_for_probe_family(kind)?;
    // Keep this the single-file shape path. A missing/out-of-root file must
    // not fall back to the resolver's repo-wide classified suffix lookup.
    scoped_file(root, &file)?;
    match resolve_at_location(root, config, &file, line, Some(kind), seam_index) {
        Err(TestStubError::Ambiguous { message, seam_ids }) if !seam_ids.is_empty() => {
            // #10710: a hidden row prints only choices that produce a stub; a
            // row whose every choice refuses prints nothing.
            match narrowed_ambiguity(
                root,
                config,
                &file,
                &finding.probe.expression,
                message,
                seam_ids,
                seam_index,
            ) {
                StubRouteDecision::Refused { .. } | StubRouteDecision::Withheld { .. } => None,
                decision => Some(StubRoute {
                    finding_id: finding.id.clone(),
                    decision,
                }),
            }
        }
        _ => None,
    }
}

/// Whether the finding's expression is still on disk in the function the
/// resolver will search (the finding line alone outside a function). A
/// `--diff` patch (a file or stdin) can disagree with the checkout, and a
/// removed-only finding names text the disk no longer holds; the resolver
/// reads the disk, so either would route to a seam the finding never named.
/// The function, not the exact line, is the scope because hunk headers can
/// be off by a line while the text is the same; whitespace is ignored so a
/// multi-line expression still matches.
fn finding_text_is_on_disk(root: &Path, file: &str, line: usize, expression: &str) -> bool {
    let squash = |text: &str| text.split_whitespace().collect::<String>();
    let wanted = squash(expression);
    if wanted.is_empty() || line == 0 {
        return false;
    }
    let Ok(source) = std::fs::read_to_string(root.join(file)) else {
        return false;
    };
    let (start, end) = owner_fn_line_span(&source, line).unwrap_or((line, line));
    let scope = source
        .lines()
        .skip(start.saturating_sub(1))
        .take(end.saturating_sub(start) + 1)
        .collect::<Vec<_>>()
        .join(" ");
    squash(&scope).contains(&wanted)
}

/// A `ripr check` finding line is the changed line, which is not always a
/// seam's own line. Candidates are the seams on that line, then the seams in
/// the same function nearest first, only those of seam kind `kind` when it
/// is given; the first one that yields a stub wins, and when none does, the
/// first candidate's refusal names why.
fn resolve_at(
    root: &Path,
    config: &RiprConfig,
    candidates: Vec<AtCandidate<'_>>,
    file: &str,
    line: usize,
    kind: Option<AtKind<'_>>,
) -> Result<TestStubResolution, TestStubError> {
    // An empty list says nothing about its source; only the `--kind` path
    // gathers shape candidates, so without a kind it was a gap list.
    let noun = match (candidates.first(), kind) {
        (Some(AtCandidate::Classified(_)), _) | (None, None) => "reported gap",
        _ => "seam ripr can stub",
    };
    let in_file = candidates
        .into_iter()
        .filter(|candidate| paths_match(candidate.seam().file(), file))
        .collect::<Vec<_>>();
    let mut files = in_file
        .iter()
        .map(|candidate| candidate.seam().file().to_string_lossy().replace('\\', "/"))
        .collect::<Vec<_>>();
    files.sort();
    files.dedup();
    if files.len() > 1 {
        // A suffix like `src/lib.rs` can name several crates' files; reading
        // one file and applying another file's seam offsets would build a
        // stub for the wrong function.
        return Err(TestStubError::Ambiguous {
            message: format!(
                "{file} matches more than one file with gaps ({}); pass one of those paths or a seam ID",
                files.join(", ")
            ),
            seam_ids: Vec::new(),
        });
    }
    let Some(first) = in_file.first() else {
        // Name the parse failure: a file ripr cannot parse has no seams,
        // which is a static limit, not evidence that nothing changed.
        let unparsed = std::fs::read_to_string(root.join(file))
            .is_ok_and(|source| !analysis::rust_source_parses_cleanly(&source));
        let why = if unparsed {
            "; the file did not parse cleanly as current-edition Rust, so ripr has no seams for it"
        } else {
            ""
        };
        return Err(TestStubError::NotFound(format!(
            "no {noun} is in {file}{why}"
        )));
    };
    let source = read_owner_source(root, first.seam())?;
    let span = owner_fn_line_span(&source, line);
    let mut candidates = in_file
        .iter()
        .copied()
        .filter(|candidate| {
            let seam_line = candidate.seam().display_line();
            seam_line == line
                || span.is_some_and(|(start, end)| start <= seam_line && seam_line <= end)
        })
        .collect::<Vec<_>>();
    // With `--kind`, only a seam of the finding's kind speaks for it:
    // another kind's stub targets a different behavior (#6298) and its
    // refusal names the wrong blocker.
    if let Some(kind) = kind {
        candidates.retain(|candidate| candidate.seam().kind() == kind.seam_kind);
    }
    candidates.sort_by_key(|candidate| candidate.seam().display_line().abs_diff(line));
    // A single-file parse lists every seam, not only the reported gaps, so
    // two disjoint seams of one kind that rank equally (`a > 10 && b > 20`)
    // cannot be told apart by `--at` and `--kind`. Picking the first could
    // stub a boundary the tests already pin, so refuse and name the seam
    // IDs. Nested seams (`return Err(E)` around `E`) are one behavior.
    if let [lead, rest @ ..] = candidates.as_slice()
        && matches!(lead, AtCandidate::Shape(_))
    {
        let rank = |candidate: &AtCandidate<'_>| {
            (
                candidate.seam().kind(),
                candidate.seam().display_line().abs_diff(line),
            )
        };
        let tied = std::iter::once(lead)
            .chain(
                rest.iter()
                    .filter(|candidate| rank(candidate) == rank(lead)),
            )
            .map(|candidate| candidate.seam())
            .collect::<Vec<_>>();
        let disjoint = tied.iter().enumerate().any(|(index, a)| {
            tied[index + 1..]
                .iter()
                .any(|b| !seam_spans_nest(a, b) && !seam_spans_nest(b, a))
        });
        if disjoint {
            let ids = tied
                .iter()
                .map(|seam| format!("--seam-id {}", seam.id().as_str()))
                .collect::<Vec<_>>();
            return Err(TestStubError::Ambiguous {
                message: format!(
                    "{file}:{} holds more than one {} seam, so --at cannot pick one; pass one of: {}",
                    lead.seam().display_line(),
                    lead.seam().kind().as_str(),
                    ids.join(", ")
                ),
                seam_ids: tied
                    .iter()
                    .map(|seam| seam.id().as_str().to_string())
                    .collect(),
            });
        }
    }
    let mut first_refusal = None;
    for candidate in candidates {
        let seam_line = candidate.seam().display_line();
        let mut resolution = candidate.resolution(root, config, source.clone());
        // A finding line is the changed line, which need not hold a gap of
        // its own; when the stub is for a different line, say so (#6298).
        if seam_line != line {
            resolution.at_note = Some(format!(
                "note: {file}:{line} has no stubbable gap; this stub is for the nearest one in `{}`, {file}:{seam_line}",
                resolution.owner
            ));
        }
        if resolution.outcome.is_ok() {
            return Ok(resolution);
        }
        first_refusal.get_or_insert(resolution);
    }
    if let Some(resolution) = first_refusal {
        return Ok(resolution);
    }
    let mut nearby = in_file;
    if let Some(kind) = kind {
        nearby.retain(|candidate| candidate.seam().kind() == kind.seam_kind);
    }
    nearby.sort_by_key(|candidate| candidate.seam().display_line().abs_diff(line));
    let listed = nearby
        .iter()
        .take(5)
        .map(|candidate| {
            format!(
                "{file}:{} (--seam-id {})",
                candidate.seam().display_line(),
                candidate.seam().id().as_str()
            )
        })
        .collect::<Vec<_>>();
    let noun = match kind {
        Some(kind) => format!("{} {noun}", kind.family),
        None => noun.to_string(),
    };
    let nearest = if listed.is_empty() {
        "none".to_string()
    } else {
        listed.join(", ")
    };
    Err(TestStubError::NotFound(format!(
        "no {noun} is in the function at {file}:{line}; nearest: {nearest}"
    )))
}

/// Whether `inner`'s source span lies within `outer`'s.
fn seam_spans_nest(outer: &RepoSeam, inner: &RepoSeam) -> bool {
    outer.byte_offset() <= inner.byte_offset()
        && inner.byte_offset() + inner.expression().len()
            <= outer.byte_offset() + outer.expression().len()
}

fn read_owner_source(root: &Path, seam: &RepoSeam) -> Result<String, TestStubError> {
    std::fs::read_to_string(root.join(seam.file())).map_err(|error| {
        TestStubError::Operational(format!("failed to read {}: {error}", seam.file().display()))
    })
}

fn resolution_for(
    root: &Path,
    config: &RiprConfig,
    entry: &ClassifiedSeam,
    source: String,
) -> TestStubResolution {
    let outcome = rust_test_stub_for_classified_seam(root, entry, &source);
    let next_repair = outcome
        .as_ref()
        .ok()
        .and_then(|stub| next_repair_for(root, config, entry, stub));
    TestStubResolution {
        seam_id: entry.seam.id().as_str().to_string(),
        owner: entry.seam.owner().to_string(),
        source,
        outcome,
        grip_class: Some(entry.class),
        at_note: None,
        next_repair,
        seam_line: entry.seam.display_line(),
        weak_oracle_guidance: super::task_first::weak_oracle_guidance(entry),
    }
}

/// The repair journey projection for one produced stub (#8727), or `None`
/// when the repair route cannot be established honestly. The fail-closed
/// flip is the same one the pilot's offered repair start uses
/// (`repair_packet_eligibility` plus `recommended_test_is_repair_edit_target`):
/// when it does not hold, the before phase would only refuse, so no start
/// command is offered. The packet renderer also refuses policy-omitted seams
/// (`AgentBriefPolicy::omission_reason_for_class`, applied in
/// `render_agent_packet_for_seam_with_report` before any packet is produced),
/// so a seam whose class is configured off gets no start command either —
/// the offered journey must not be one the packet would refuse. The surface
/// names the same `recommended_test_for` derivation the packet render writes
/// into `allowed_edit_surface` (or the edit cage's `recommended_test.file`
/// fallback), so it is the file the before-phase cage actually admits.
fn next_repair_for(
    root: &Path,
    config: &RiprConfig,
    entry: &ClassifiedSeam,
    stub: &RustTestStub,
) -> Option<StubNextRepair> {
    if AgentBriefPolicy::from_config(config)
        .omission_reason_for_class(entry.class)
        .is_some()
    {
        return None;
    }
    if !repair_packet_eligibility(entry).eligible()
        || !recommended_test_is_repair_edit_target(entry)
    {
        return None;
    }
    let surface = recommended_test_for(entry).file;
    // Compare as paths, not strings: the placement is a checkout-relative
    // path whose separators follow the host, the surface a stable `/`-joined
    // derivation, and both may sit under any worktree root.
    let placement_divergence =
        (!stub.placement.file().ends_with(&surface)).then(|| StubPlacementDivergence {
            stub_file: display_path(stub.placement.file()),
            packet_file: surface.clone(),
        });
    Some(StubNextRepair {
        start_command: format!(
            "ripr agent repair --root {} --seam-id {} --phase before",
            // Render the root with `root_path_display` before binding it, as
            // the CLI recovery paths do: `display_path` would rewrite a
            // literal backslash in a Unix checkout into a separator and the
            // pasted command would target another directory (#4287).
            shell_arg(&bound_root(&root_path_display(root))),
            shell_arg(entry.seam.id().as_str()),
        ),
        allowed_edit_surface: surface,
        placement_divergence,
    })
}

#[cfg(test)]
std::thread_local! {
    static INVENTORY_ENTRY_FACTS_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Inventory class for a seam selected from a file parse (`--at --kind`).
/// Classification is not used to pick the seam (#5471); this lookup is
/// disclosure only, plus the weak-oracle pointer (#10683). A failed or empty
/// inventory leaves `None`.
fn inventory_entry_facts(
    root: &Path,
    config: &RiprConfig,
    file: &str,
    seam_id: &str,
) -> Option<(SeamGripClass, Option<String>)> {
    #[cfg(test)]
    INVENTORY_ENTRY_FACTS_CALLS.with(|count| count.set(count.get() + 1));
    let relative = scoped_file(root, file)?;
    let scoped = analysis::inventory_diff_scoped_classified_seams_at_with_config(
        root,
        config,
        &[relative],
        &[],
    )
    .ok()?;
    scoped
        .classified
        .iter()
        .find(|entry| entry.seam.id().as_str() == seam_id)
        .map(|entry| (entry.class, super::task_first::weak_oracle_guidance(entry)))
}

/// Exact or path-suffix match in either direction, after normalizing
/// separators and a leading `./`, the same leniency `ripr explain` applies.
fn paths_match(stored: &Path, wanted: &str) -> bool {
    let normalize = |text: &str| {
        text.replace('\\', "/")
            .trim_start_matches("./")
            .trim_matches('/')
            .to_string()
    };
    let stored = normalize(&analysis::stable_path_text(stored));
    let wanted = normalize(wanted);
    !stored.is_empty()
        && !wanted.is_empty()
        && (stored == wanted
            || stored.ends_with(&format!("/{wanted}"))
            || wanted.ends_with(&format!("/{stored}")))
}

/// 1-based line of a byte offset in `source`.
pub(crate) fn line_of_offset(source: &str, offset: usize) -> usize {
    source
        .get(..offset)
        .map(|prefix| prefix.matches('\n').count() + 1)
        .unwrap_or(1)
}

/// Apply the stub: insert into the owner file, or create the new
/// integration file. Refuses when the owner file changed since the stub was
/// computed, or the new file already exists.
pub(crate) fn write_test_stub(
    root: &Path,
    resolution: &TestStubResolution,
    stub: &RustTestStub,
) -> Result<PathBuf, String> {
    let relative = stub.placement.file();
    let path = root.join(relative);
    // A committed `tests -> /elsewhere` link must not redirect the write or
    // the directory creation below (#11477 item 4); checked before either.
    crate::atomic_file::ensure_path_within_root(root, &path, "test stub")?;
    match &stub.placement {
        TestStubPlacement::ExistingInlineModule { offset, .. }
        | TestStubPlacement::NewInlineModule { offset, .. } => {
            let current = std::fs::read_to_string(&path)
                .map_err(|error| format!("failed to read {}: {error}", relative.display()))?;
            if current != resolution.source {
                return Err(format!(
                    "{} changed after the stub was computed; rerun `ripr agent stub`",
                    relative.display()
                ));
            }
            let (head, tail) = current.split_at_checked(*offset).ok_or_else(|| {
                format!(
                    "stub offset {offset} is outside {}; rerun `ripr agent stub`",
                    relative.display()
                )
            })?;
            let text = match_line_endings(&stub.text, &current);
            let updated = format!("{head}{text}{tail}");
            crate::atomic_file::write(&path, updated.as_bytes(), "test stub")?;
        }
        TestStubPlacement::ExistingIntegrationFile { .. } => {
            let current = std::fs::read_to_string(&path)
                .map_err(|error| format!("failed to read {}: {error}", relative.display()))?;
            let separator = if current.ends_with('\n') || current.is_empty() {
                ""
            } else {
                "\n"
            };
            let updated = format!("{current}{separator}{}", stub.text);
            crate::atomic_file::write(&path, updated.as_bytes(), "test stub")?;
        }
        TestStubPlacement::NewIntegrationFile { .. } => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
            }
            crate::atomic_file::create_new(&path, stub.text.as_bytes()).map_err(|error| {
                let error = match error {
                    crate::atomic_file::CreateNewError::Staging(error)
                    | crate::atomic_file::CreateNewError::Link(error) => error,
                };
                format!("failed to create {}: {error}", relative.display())
            })?;
        }
    }
    Ok(relative.to_path_buf())
}

/// The revalidated disposition of a stub that resolved as `already_present`
/// when `--write` would otherwise skip the file edit (#8990 review X3).
#[derive(Debug)]
pub(crate) enum AlreadyPresentRevalidation {
    /// Current bytes still hold the identical stub: the write stays a no-op.
    /// Carries the fresh resolution, so the report names the placement and
    /// run command the re-read found rather than the pre-edit ones.
    Current(Box<TestStubResolution>),
    /// The placement no longer holds the stub: this fresh resolution's stub
    /// is not `already_present`, so the caller writes it through the normal
    /// guarded path instead of reporting success from bytes the resolver
    /// read before a concurrent edit.
    Rewrite(Box<TestStubResolution>),
}

/// Re-resolve a seam whose stub resolved as `already_present`, immediately
/// before the CLI skips the write on that strength (#8990 review X3). The
/// first resolution read the placement file before this moment; a concurrent
/// editor can add or remove the stub in between, and the idempotent report
/// must never claim "already written" from bytes that were not re-read. A
/// seam that no longer resolves is a changed workspace, not a vanished
/// claim: the caller refuses with the rerun message, the same shape
/// `write_test_stub` uses for an owner file that changed after resolution.
pub(crate) fn revalidate_already_present_stub(
    root: &Path,
    config: &RiprConfig,
    selector: &TestStubSelector,
) -> Result<AlreadyPresentRevalidation, String> {
    let fresh = resolve_test_stub(root, config, selector).map_err(|error| match error {
        TestStubError::Operational(message)
        | TestStubError::NotFound(message)
        | TestStubError::Ambiguous { message, .. }
        | TestStubError::UnsupportedLanguage(message) => {
            format!("{message}; the workspace changed after the stub was computed, so rerun `ripr agent stub`")
        }
    })?;
    match &fresh.outcome {
        Ok(stub) if stub.already_present => {
            Ok(AlreadyPresentRevalidation::Current(Box::new(fresh)))
        }
        Ok(_) => Ok(AlreadyPresentRevalidation::Rewrite(Box::new(fresh))),
        Err(refusal) => Err(format!(
            "the workspace changed after the stub was computed ({}: {}); rerun `ripr agent stub`",
            refusal.as_str(),
            refusal.reason()
        )),
    }
}

/// The stub is produced with `\n` line breaks. Splice it into a file that
/// is predominantly CRLF with CRLF breaks so the write does not leave mixed
/// line endings behind.
fn match_line_endings(text: &str, file: &str) -> String {
    let crlf = file.matches("\r\n").count();
    let lf = file.matches('\n').count();
    if crlf > 0 && crlf * 2 > lf {
        text.replace("\r\n", "\n").replace('\n', "\r\n")
    } else {
        text.to_string()
    }
}

/// The root-relative manifest of the Cargo package that will own the stub:
/// the nearest `Cargo.toml` with a `[package]` table at or above the stub's
/// file, bounded by `root`. `None` when no package owns it, so no run command
/// is offered rather than one that would test a different package.
pub(crate) fn package_manifest_for(root: &Path, stub: &RustTestStub) -> Option<PathBuf> {
    stub.placement.file().ancestors().skip(1).find_map(|dir| {
        let manifest = dir.join("Cargo.toml");
        let text = std::fs::read_to_string(root.join(&manifest)).ok()?;
        let parsed = text.parse::<toml::Table>().ok()?;
        parsed.contains_key("package").then_some(manifest)
    })
}

/// The cargo target name that runs the integration file `stub` is placed in:
/// an explicit `[[test]]` entry whose `path` is that file (its `name` wins, so
/// `name = "pricing"` for `tests/discount.rs` yields `pricing`), otherwise the
/// auto-discovered name (`tests/<stem>.rs` or `tests/<dir>/main.rs`) when
/// `autotests` is not disabled. `None` when the name cannot be established:
/// non-integration placement, no owning package, an unparsable manifest,
/// `autotests = false` without an explicit entry, an explicit entry that
/// shadows the inferred name, or `required-features` that a bare
/// `cargo test --test` would not enable.
#[cfg(test)]
fn integration_target_name(root: &Path, manifest: &Path, stub: &RustTestStub) -> Option<String> {
    match integration_target_lookup(root, manifest, stub) {
        TargetLookup::Name(name) => Some(name),
        TargetLookup::NotRegistered | TargetLookup::Unresolved => None,
    }
}

/// Outcome of resolving an integration file's cargo test target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TargetLookup {
    /// The file is the target with this name.
    Name(String),
    /// Manifest evidence shows the file is not registered under a name ripr
    /// can establish (discovery disabled, outside `tests/`, or a shadowed
    /// name). Ambiguous layouts are `Unresolved`, not this.
    NotRegistered,
    /// The manifest, edition or an entry could not be resolved, or the target
    /// is registered but not runnable by a bare command.
    Unresolved,
}

pub(crate) fn integration_target_lookup(
    root: &Path,
    manifest: &Path,
    stub: &RustTestStub,
) -> TargetLookup {
    lookup_inner(root, manifest, stub).unwrap_or(TargetLookup::Unresolved)
}

fn lookup_inner(root: &Path, manifest: &Path, stub: &RustTestStub) -> Option<TargetLookup> {
    use TargetLookup::{Name, NotRegistered, Unresolved};
    let (TestStubPlacement::NewIntegrationFile { file }
    | TestStubPlacement::ExistingIntegrationFile { file }) = &stub.placement
    else {
        return Some(Unresolved);
    };
    let text = std::fs::read_to_string(root.join(manifest)).ok()?;
    let parsed = text.parse::<toml::Table>().ok()?;
    let package = parsed.get("package")?.as_table()?;
    let package_dir = manifest.parent().unwrap_or(Path::new(""));
    let relative = normalize_components(file.strip_prefix(package_dir).ok()?)?;
    let explicit: Vec<&toml::Table> = parsed
        .get("test")
        .and_then(toml::Value::as_array)
        .map(|entries| entries.iter().filter_map(toml::Value::as_table).collect())
        .unwrap_or_default();
    let entry_name = |entry: &toml::Table| -> Option<String> {
        entry
            .get("name")
            .and_then(toml::Value::as_str)
            .map(ToOwned::to_owned)
    };
    // Cargo prefers tests/<n>.rs over tests/<n>/main.rs when both exist.
    let sibling_file_exists = |name: &str| {
        root.join(package_dir)
            .join("tests")
            .join(format!("{name}.rs"))
            .is_file()
    };
    let dir_main_exists = |name: &str| {
        root.join(package_dir)
            .join("tests")
            .join(name)
            .join("main.rs")
            .is_file()
    };
    // A present `path` is the only candidate (an unusable one matches
    // nothing); the name-based default applies only when `path` is absent.
    let entry_matches = |entry: &toml::Table| -> bool {
        match entry.get("path") {
            Some(path) => path
                .as_str()
                .and_then(|path| normalize_components(Path::new(path)))
                .is_some_and(|path| path == relative),
            None => entry_name(entry).is_some_and(|name| {
                relative == format!("tests/{name}.rs")
                    || relative == format!("tests/{name}/main.rs")
            }),
        }
    };
    let runnable = |entry: &toml::Table| {
        // `harness = false` targets are not run by a plain test command.
        entry
            .get("harness")
            .and_then(toml::Value::as_bool)
            .unwrap_or(true)
    };
    let enabled = |entry: &toml::Table| {
        entry
            .get("required-features")
            .and_then(toml::Value::as_array)
            .is_none_or(Vec::is_empty)
    };
    if let Some(entry) = explicit.iter().find(|entry| entry_matches(entry)) {
        // Cargo drops a name-only entry when both layouts exist.
        if entry.get("path").is_none()
            && entry_name(entry)
                .is_some_and(|name| sibling_file_exists(&name) && dir_main_exists(&name))
        {
            return Some(Unresolved);
        }
        return Some(
            match entry_name(entry)
                .filter(|name| !name.trim().is_empty())
                .filter(|_| enabled(entry) && runnable(entry))
            {
                Some(name) => Name(name),
                None => Unresolved,
            },
        );
    }
    // Cargo's default for `autotests`: on for edition 2018+, but in edition
    // 2015 any explicit `[[test]]` turns legacy auto-discovery off unless
    // `autotests = true` is written. Uncertain edition fails closed.
    let autotests = match package.get("autotests") {
        None => explicit.is_empty() || package_edition(root, manifest, package)? >= 2018,
        Some(value) => value.as_bool()?,
    };
    if !autotests {
        return Some(NotRegistered);
    }
    let Some(tail) = relative.strip_prefix("tests/") else {
        return Some(NotRegistered);
    };
    let inferred = match tail.split('/').collect::<Vec<_>>()[..] {
        [name] if name.ends_with(".rs") => name.trim_end_matches(".rs"),
        [dir, "main.rs"] => dir,
        _ => return Some(NotRegistered),
    };
    // Both tests/<n>.rs and tests/<n>/main.rs: cargo reports a duplicate target.
    if sibling_file_exists(inferred) && dir_main_exists(inferred) {
        return Some(Unresolved);
    }
    // An explicit entry with the inferred name but another path replaces the
    // auto-discovered target, so this file is not reachable under that name.
    if inferred.is_empty()
        || explicit
            .iter()
            .any(|entry| entry_name(entry).as_deref() == Some(inferred))
    {
        return Some(NotRegistered);
    }
    Some(Name(inferred.to_string()))
}

/// The package's effective edition year: its own `edition`, or the workspace
/// root's `[workspace.package] edition` for `edition.workspace = true`, or 2015
/// when absent. `None` when it cannot be resolved.
fn package_edition(root: &Path, manifest: &Path, package: &toml::Table) -> Option<u32> {
    let parse = |value: &toml::Value| value.as_str()?.trim().parse::<u32>().ok();
    match package.get("edition") {
        None => Some(2015),
        Some(value @ toml::Value::String(_)) => parse(value),
        Some(toml::Value::Table(table)) if table.get("workspace")?.as_bool()? => {
            let start = manifest.parent().unwrap_or(Path::new(""));
            // Cargo reads only one workspace: the explicit `package.workspace`
            // pointer, else the first ancestor (the manifest's own directory
            // included) that has a `[workspace]` table. If that workspace has
            // no `[workspace.package] edition`, the inheritance is invalid.
            let workspace_manifest = |dir: &Path| -> Option<toml::Table> {
                std::fs::read_to_string(root.join(dir).join("Cargo.toml"))
                    .ok()?
                    .parse::<toml::Table>()
                    .ok()
            };
            let workspace_dir: PathBuf = match package.get("workspace") {
                Some(pointer) => {
                    let pointed = start.join(pointer.as_str()?);
                    let pointed = pointed.strip_prefix(".").unwrap_or(&pointed);
                    PathBuf::from(normalize_components(pointed)?)
                }
                None => start
                    .ancestors()
                    .find(|dir| {
                        workspace_manifest(dir)
                            .is_some_and(|parsed| parsed.contains_key("workspace"))
                    })?
                    .to_path_buf(),
            };
            let parsed = workspace_manifest(&workspace_dir)?;
            parsed
                .get("workspace")?
                .as_table()?
                .get("package")?
                .as_table()?
                .get("edition")
                .and_then(parse)
        }
        Some(_) => None,
    }
}

/// `/`-joined components of a relative path with `.` and in-package `..`
/// resolved lexically; `None` for a `..` that escapes, roots or prefixes.
fn normalize_components(path: &Path) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::Normal(part) => parts.push(part.to_str()?.to_string()),
            std::path::Component::CurDir => {}
            // Lexical `..` stays inside the package or the path is unusable.
            std::path::Component::ParentDir => {
                parts.pop()?;
            }
            _ => return None,
        }
    }
    Some(parts.join("/"))
}

/// The command that runs just this test after the stub is in place.
/// `manifest` is the package manifest as the caller should spell it and
/// `target` the effective cargo test target from [`integration_target_name`].
/// `None` when an integration stub's target name is not established: a
/// command naming the file stem could fail, so none is offered.
pub(crate) fn run_command(
    manifest: &str,
    stub: &RustTestStub,
    target: Option<&str>,
) -> Option<String> {
    match &stub.placement {
        TestStubPlacement::NewIntegrationFile { .. }
        | TestStubPlacement::ExistingIntegrationFile { .. } => Some(format!(
            "cargo test --manifest-path {manifest} --test {} {}",
            target?, stub.test_name
        )),
        _ => Some(format!(
            "cargo test --manifest-path {manifest} {}",
            stub.test_name
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_lines_blank_strings_and_comments_but_keep_dereferences() {
        let uses =
            |source: &str, line: usize| names_identifier(&code_lines_of(source)[line - 1], "CAP");
        assert!(!uses("let s = \"CAP\";", 1));
        assert!(!uses("let s = \"a\\\"CAP\";", 1));
        assert!(!uses("x + 1 // CAP", 1));
        assert!(!uses("/* CAP */", 1));
        assert!(!uses("/*\n * the CAP applies\n */\nlet x = 1;", 2));
        assert!(!uses("/* a /* b */ CAP */", 1));
        assert!(!uses("let s = \"line one\nCAP two\";", 2));
        assert!(uses("x.min(CAP) // cap", 1));
        assert!(uses("let s = \"x\"; y.min(CAP)", 1));
        assert!(uses("*total += CAP;", 1));
        assert!(uses("let y = a\n    * CAP;", 2));
        assert!(uses("/* c */ CAP", 1));
        // char literals, escapes and raw strings do not desync the scan
        assert!(!uses("let q = '\"'; // CAP", 1));
        assert!(uses("let q = '\"';\nx.min(CAP)", 2));
        assert!(!uses("let q = '\\'';\nlet s = \"CAP\";", 2));
        assert!(uses("let r = r\"a\\\";\nx.min(CAP)", 2));
        assert!(!uses("let r = r#\"say \"CAP\"\"#;", 1));
        assert!(!uses("let r = br\"CAP\";", 1));
        assert!(uses("fn f<'a>(x: &'a u64) -> u64 { x.min(CAP) }", 1));
        assert_eq!(code_lines_of("a\n// b\nc").len(), 3);
    }

    #[test]
    fn names_identifier_matches_whole_identifiers_only() {
        assert!(names_identifier("over.min(CAP) * rate", "CAP"));
        assert!(names_identifier("CAP", "CAP"));
        assert!(!names_identifier("over.min(CAPS)", "CAP"));
        assert!(!names_identifier("MAX_CAP + 1", "CAP"));
        assert!(!names_identifier("cap", "CAP"));
    }

    #[test]
    fn names_identifier_rejects_fields_and_foreign_paths() {
        assert!(names_identifier("Self::CAP + 1", "CAP"));
        assert!(names_identifier("crate::CAP", "CAP"));
        assert!(names_identifier("super::CAP", "CAP"));
        assert!(!names_identifier("other::CAP", "CAP"));
        assert!(!names_identifier("a::b::CAP", "CAP"));
        assert!(!names_identifier("self.CAP", "CAP"));
        assert!(names_identifier("for i in 0..CAP {", "CAP"));
        assert!(names_identifier("for i in 0..=CAP {", "CAP"));
        assert!(names_identifier("&buf[..CAP]", "CAP"));
    }

    #[test]
    fn declares_constant_sees_items_locals_and_const_generics() {
        assert!(declares_constant("pub const CAP: u64 = 5;", "CAP"));
        assert!(declares_constant("static mut CAP: u64 = 5;", "CAP"));
        assert!(declares_constant("fn f<const CAP: usize>() {}", "CAP"));
        assert!(!declares_constant("const CAPS: u64 = 5;", "CAP"));
        assert!(!declares_constant("let x = CAP;", "CAP"));
    }

    #[test]
    fn is_test_fn_reads_the_attributes_directly_above() {
        let lines = |text: &'static str| text.lines().collect::<Vec<_>>();
        let tokio = lines("#[tokio::test]\nasync fn t() {}");
        assert!(is_test_fn(&tokio, 2));
        let stacked = lines(
            "#[test]\n#[should_panic(\n    expected = \"x\"\n)]\n/// doc\n#[ignore]\nfn t() {}",
        );
        assert!(is_test_fn(&stacked, 7));
        let doc_ends_in_brace =
            lines("#[test]\n/// takes the `{` branch;\n#[cfg(feature = \"x\")]\nfn t() {}");
        assert!(is_test_fn(&doc_ends_in_brace, 4));
        let neighbour = lines("#[test]\nfn t() {}\nfn prod() {}");
        assert!(!is_test_fn(&neighbour, 3));
        assert!(!is_test_fn(&lines("#[inline]\nfn f() {}"), 2));
    }

    #[test]
    fn at_selector_parses_file_and_line_and_refuses_junk() {
        assert_eq!(
            TestStubSelector::parse_at("src/lib.rs:12"),
            Ok(TestStubSelector::At {
                file: "src/lib.rs".to_string(),
                line: 12,
                kind: None
            })
        );
        for junk in ["src/lib.rs", "src/lib.rs:x", ":3", "src/lib.rs:0"] {
            let error = TestStubSelector::parse_at(junk).err();
            assert!(error.is_some(), "{junk}: expected a refusal, got Ok");
            let message = error.unwrap_or_default();
            assert!(message.contains("FILE:LINE"), "{junk}: {message}");
            assert!(
                message.contains(&format!("{junk:?}")),
                "{junk}: dropped the typed value: {message}"
            );
            assert!(
                !message.contains("invalid digit"),
                "{junk}: restored std parse jargon: {message}"
            );
        }
    }

    #[test]
    fn path_match_accepts_suffixes_in_either_direction() {
        assert!(paths_match(Path::new("crates/a/src/lib.rs"), "src/lib.rs"));
        assert!(paths_match(
            Path::new("src/lib.rs"),
            "./crates/a/src/lib.rs"
        ));
        assert!(paths_match(Path::new("src\\lib.rs"), "src/lib.rs"));
        assert!(!paths_match(Path::new("src/lib.rs"), "lib.rs.bak"));
        assert!(!paths_match(Path::new("src/mylib.rs"), "lib.rs"));
    }

    fn classified_at(file: &str, line: usize, class: SeamGripClass) -> ClassifiedSeam {
        use crate::analysis::seams::{ExpectedSink, RepoSeam, RequiredDiscriminator, SeamKind};
        use crate::analysis::test_grip_evidence::TestGripEvidence;
        use crate::domain::{Confidence, StageEvidence, StageState};
        let stage = || StageEvidence::new(StageState::Yes, Confidence::Medium, "test stage");
        let seam = RepoSeam::new(
            file,
            "owner",
            SeamKind::PredicateBoundary,
            0,
            line,
            "a >= b",
            RequiredDiscriminator::BoundaryValue {
                description: "a == b".to_string(),
            },
            ExpectedSink::ReturnValue,
        );
        ClassifiedSeam {
            evidence: TestGripEvidence {
                seam_id: seam.id().clone(),
                related_tests: Vec::new(),
                reach: stage(),
                activate: stage(),
                propagate: stage(),
                observe: stage(),
                discriminate: stage(),
                observed_values: Vec::new(),
                missing_discriminators: Vec::new(),
                statically_contradicted_related_tests: 0,
                new_test_target: None,
            },
            seam,
            class,
        }
    }

    fn ungripped(file: &str, line: usize) -> ClassifiedSeam {
        classified_at(file, line, SeamGripClass::Ungripped)
    }

    fn scratch_stub_crate(label: &str, source: &str) -> Result<PathBuf, String> {
        let root = std::env::temp_dir().join(format!(
            "ripr-stub-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_nanos()
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

    /// #10710: the narrowed seam is only offered when `ripr agent stub
    /// --seam-id` itself produces a stub. Here the changed field's seam is
    /// already strongly gripped by a whole-value assertion, so the classified
    /// resolution refuses it and no stub command is printed; a gap in the
    /// same shape (the test asserts one field only) does print it.
    #[cfg(feature = "lang-rust")]
    #[test]
    fn narrowed_field_seam_is_printed_only_when_the_seam_id_stub_succeeds() -> Result<(), String> {
        const STRUCT: &str = "#[derive(Debug, PartialEq)]\npub struct P {\n    pub x: u32,\n    pub y: u32,\n}\n\npub fn mk() -> P {\n    P { x: 1, y: 3 }\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n";
        let gripped = format!(
            "{STRUCT}    #[test]\n    fn whole_value() {{\n        assert_eq!(mk(), P {{ x: 1, y: 3 }});\n    }}\n}}\n"
        );
        let weak = format!(
            "{STRUCT}    #[test]\n    fn one_field() {{\n        assert_eq!(mk().x, 1);\n    }}\n}}\n"
        );
        let decide = |label: &str, source: &str| -> Result<StubRouteDecision, String> {
            let root = scratch_stub_crate(label, source)?;
            let config = RiprConfig::default();
            let relative = PathBuf::from("src/lib.rs");
            let mut index = None;
            let seams = analysis::scoped_file_seams(&root, &config, &relative, &mut index)?;
            let ids = seams
                .iter()
                .filter(|seam| seam.kind() == SeamKind::FieldConstruction)
                .map(|seam| seam.id().as_str().to_string())
                .collect::<Vec<_>>();
            assert!(ids.len() >= 2, "precondition, one seam per field: {ids:?}");
            let decision = narrowed_ambiguity(
                &root,
                &config,
                "src/lib.rs",
                "y: 3",
                "several seams".to_string(),
                ids,
                &mut index,
            );
            let _ = std::fs::remove_dir_all(&root);
            Ok(decision)
        };
        let decision = decide("narrow-weak", &weak)?;
        assert!(
            matches!(decision, StubRouteDecision::SeamStub { .. }),
            "the changed field of a weak test gets its seam-id stub: {decision:?}"
        );
        let decision = decide("narrow-gripped", &gripped)?;
        assert!(
            matches!(decision, StubRouteDecision::Refused { .. }),
            "a strongly gripped seam must not print a stub command: {decision:?}"
        );
        Ok(())
    }

    /// A narrowed seam that is weak-oracle (not a hard refusal) must not
    /// become a plain `Refused`, which would print the hand-written step.
    #[test]
    fn fold_keeps_weak_oracle_distinct_from_refusal() {
        let weak = |id: &str| (id.to_string(), SeamStubOutcome::WeakOracle("fix it".into()));
        let hard = |id: &str| (id.to_string(), SeamStubOutcome::Refused("no".into()));
        let ok = |id: &str| (id.to_string(), SeamStubOutcome::Stub);
        let why = || "several".to_string();
        assert_eq!(
            fold_seam_outcomes(vec![weak("a")], why(), true),
            StubRouteDecision::WeakOracle {
                guidance: "fix it".into()
            }
        );
        assert_eq!(
            fold_seam_outcomes(vec![weak("a"), weak("b")], why(), false),
            StubRouteDecision::WeakOracle {
                guidance: "fix it".into()
            }
        );
        assert_eq!(
            fold_seam_outcomes(vec![weak("a"), hard("b")], why(), false),
            StubRouteDecision::Refused {
                reason: "no".into()
            }
        );
        assert_eq!(
            fold_seam_outcomes(vec![hard("a")], why(), true),
            StubRouteDecision::Refused {
                reason: "no".into()
            }
        );
        // A seam missing from the inventory is not a "not a gap" claim, and a
        // withheld ambiguity never lists the resolver's unvalidated seam IDs.
        let raw = "src/lib.rs:12 holds more than one field_construction seam, so --at cannot pick one; pass one of: --seam-id a, --seam-id b";
        let withheld = fold_seam_outcomes(
            vec![("a".to_string(), SeamStubOutcome::Missing)],
            raw.to_string(),
            true,
        );
        let reason = match withheld {
            StubRouteDecision::Withheld { reason } => reason,
            other => format!("UNEXPECTED {other:?}"),
        };
        assert!(
            reason.contains("cannot pick one")
                && !reason.contains("pass one of")
                && !reason.contains("--seam-id"),
            "{reason}"
        );
        // An empty validated set can never be a choice: a Missing seam beside a
        // hard refusal is that refusal, and beside a weak oracle is the
        // strengthen-the-test guidance, never an Ambiguous with no commands.
        let missing = || ("m".to_string(), SeamStubOutcome::Missing);
        assert_eq!(
            fold_seam_outcomes(vec![missing(), hard("a")], raw.to_string(), true),
            StubRouteDecision::Refused {
                reason: "no".into()
            }
        );
        assert!(
            matches!(
                fold_seam_outcomes(vec![missing(), weak("a")], raw.to_string(), true),
                StubRouteDecision::WeakOracle { .. }
            ),
            "missing + weak oracle is weak-oracle guidance"
        );
        let choice = fold_seam_outcomes(vec![ok("a"), ok("b")], raw.to_string(), false);
        let reason = match choice {
            StubRouteDecision::Ambiguous { reason, .. } => reason,
            other => format!("UNEXPECTED {other:?}"),
        };
        assert!(
            !reason.starts_with("UNEXPECTED") && !reason.contains("pass one of"),
            "{reason}"
        );
        assert_eq!(
            fold_seam_outcomes(vec![weak("a"), ok("b")], why(), true),
            StubRouteDecision::SeamStub {
                seam_id: "b".into()
            }
        );
    }

    /// Each withhold path of `validate_ambiguity` returns a plain reason with
    /// no unvalidated seam IDs and no command, and each is pinned separately
    /// by injecting the inventory: too many candidates (the loader must not
    /// run), an inventory that cannot be classified, and candidates the
    /// inventory does not hold. No directory is created.
    #[test]
    fn narrowed_ambiguity_withholds_with_a_plain_reason() {
        let raw = "src/lib.rs:12 holds more than one field_construction seam, so --at cannot pick one; pass one of: --seam-id s0, --seam-id s1";
        let root = Path::new("no-such-root-10710");
        let config = RiprConfig::default();
        let ids = |count: usize| (0..count).map(|i| format!("s{i}")).collect::<Vec<_>>();
        type Inventory = Option<Result<Vec<ClassifiedSeam>, String>>;
        let cases: Vec<(&str, usize, Inventory)> = vec![
            ("too many", AMBIGUOUS_VALIDATION_LIMIT + 1, None),
            (
                "unclassifiable",
                2,
                Some(Err("cannot classify".to_string())),
            ),
            ("missing from inventory", 2, Some(Ok(Vec::new()))),
        ];
        for (label, count, inventory) in cases {
            let mut loaded = false;
            let decision =
                validate_ambiguity(root, &config, ids(count), false, raw.to_string(), || {
                    loaded = true;
                    inventory.unwrap_or_else(|| Err("must not load".to_string()))
                });
            assert_eq!(loaded, label != "too many", "{label}: inventory load");
            let reason = match decision {
                StubRouteDecision::Withheld { reason } => reason,
                other => format!("UNEXPECTED {other:?}"),
            };
            assert!(
                reason.contains("cannot pick one")
                    && reason.ends_with("ripr could not check which of them your change touches")
                    && !reason.contains("pass one of")
                    && !reason.contains("--seam-id"),
                "{label}: {reason}"
            );
        }
    }

    #[cfg(feature = "lang-rust")]
    #[test]
    fn limited_journey_hidden_unique_stub_is_not_projected() -> Result<(), String> {
        let root = scratch_stub_crate(
            "hidden-unique",
            "pub fn bulk(qty: u32) -> bool {\n    qty >= 10\n}\n",
        )?;
        let diff = root.join("change.patch");
        std::fs::write(
            &diff,
            "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -2 +2 @@\n-    qty > 10\n+    qty >= 10\n",
        )
        .map_err(|error| error.to_string())?;
        let config = RiprConfig::default();
        let output = crate::app::check_workspace(crate::app::CheckInput {
            root: root.clone(),
            diff_file: Some(diff),
            ..Default::default()
        })?;
        let (selected, hidden) = crate::output::human::stub_route_subjects(&output, &config);
        assert!(selected.is_some(), "nonempty selected subject");
        let [hidden] = hidden.as_slice() else {
            return Err(format!(
                "expected one hidden stub-route subject, got {}",
                hidden.len()
            ));
        };
        // The changed boundary predicate is the selected top gap; the owner's
        // return-value sink is the one hidden unique subject this control
        // routes without a classified-inventory lookup.
        assert_eq!(hidden.probe.family.as_str(), "return_value");
        let mut index = None;
        let direct = stub_route_for_finding(&root, &config, hidden, &mut index)
            .ok_or("the hidden subject must independently resolve")?;
        assert!(matches!(direct.decision, StubRouteDecision::Stub { .. }));
        let original_count = INVENTORY_ENTRY_FACTS_CALLS.with(|count| count.replace(0));
        let mut selected_index = None;
        let selected_route = stub_route_for_finding(
            &root,
            &config,
            selected.ok_or("missing selected subject")?,
            &mut selected_index,
        )
        .ok_or("selected subject must keep its full route")?;
        let selected_inventory_calls = INVENTORY_ENTRY_FACTS_CALLS.with(|count| count.get());
        assert!(
            selected_inventory_calls > 0,
            "positive classified-lookup control"
        );
        INVENTORY_ENTRY_FACTS_CALLS.with(|count| count.set(0));
        let routes = check_stub_routes(&root, &config, &output, &mut index);
        assert_eq!(
            INVENTORY_ENTRY_FACTS_CALLS.with(|count| count.get()),
            selected_inventory_calls,
            "hidden unique route must add no classified inventory lookup"
        );
        assert!(
            routes.contains(&selected_route),
            "selected full decision preserved"
        );
        INVENTORY_ENTRY_FACTS_CALLS.with(|count| count.set(original_count));
        assert!(routes.iter().all(|route| route.finding_id != hidden.id));
        assert!(
            routes.iter().any(|route| Some(route.finding_id.as_str())
                == selected.map(|finding| finding.id.as_str()))
        );
        assert_eq!(
            std::fs::read_to_string(root.join("src/lib.rs")).map_err(|error| error.to_string())?,
            "pub fn bulk(qty: u32) -> bool {\n    qty >= 10\n}\n"
        );
        std::fs::remove_dir_all(root).map_err(|error| error.to_string())
    }

    #[test]
    fn at_selector_refuses_a_suffix_that_names_two_files_with_gaps() {
        let classified = vec![
            ungripped("crates/a/src/lib.rs", 2),
            ungripped("crates/b/src/lib.rs", 2),
        ];
        let config = RiprConfig::default();
        let result = resolve_at(
            Path::new("/nonexistent"),
            &config,
            classified_candidates(&classified),
            "src/lib.rs",
            2,
            None,
        );
        assert!(
            matches!(&result, Err(TestStubError::Ambiguous { message, .. })
                if message.contains("crates/a/src/lib.rs, crates/b/src/lib.rs")
                    && message.contains("seam ID")),
            "{:?}",
            result.err()
        );
        // One full path is unambiguous and reaches the source read.
        let one = resolve_at(
            Path::new("/nonexistent"),
            &config,
            classified_candidates(&classified),
            "crates/a/src/lib.rs",
            2,
            None,
        );
        assert!(
            matches!(&one, Err(TestStubError::Operational(_))),
            "{:?}",
            one.err()
        );
    }

    #[test]
    fn run_command_targets_the_package_that_owns_the_stub() -> Result<(), String> {
        use crate::analysis::test_stub::TestStubPlacement;
        let root = std::env::temp_dir().join(format!(
            "ripr-stub-manifest-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("crates/a/src")).map_err(|error| error.to_string())?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/a\"]\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(
            root.join("crates/a/Cargo.toml"),
            "[package]\nname = \"a\"\n",
        )
        .map_err(|error| error.to_string())?;
        let stub = |placement| RustTestStub {
            placement,
            test_name: "owner_boundary".to_string(),
            text: String::new(),
            already_present: false,
            fill_ins: Vec::new(),
            derived_inputs: Vec::new(),
        };
        let inline = stub(TestStubPlacement::NewInlineModule {
            file: PathBuf::from("crates/a/src/lib.rs"),
            offset: 0,
        });
        let integration = stub(TestStubPlacement::NewIntegrationFile {
            file: PathBuf::from("crates/a/tests/owner.rs"),
        });
        let orphan = stub(TestStubPlacement::NewInlineModule {
            file: PathBuf::from("scripts/tool.rs"),
            offset: 0,
        });
        let inline_manifest = package_manifest_for(&root, &inline);
        let integration_manifest = package_manifest_for(&root, &integration);
        let orphan_manifest = package_manifest_for(&root, &orphan);
        let _ = std::fs::remove_dir_all(&root);

        // The workspace manifest at the root has no [package]; it is skipped.
        assert_eq!(inline_manifest, Some(PathBuf::from("crates/a/Cargo.toml")));
        assert_eq!(
            integration_manifest,
            Some(PathBuf::from("crates/a/Cargo.toml"))
        );
        assert_eq!(orphan_manifest, None);
        assert_eq!(
            run_command("crates/a/Cargo.toml", &inline, None).as_deref(),
            Some("cargo test --manifest-path crates/a/Cargo.toml owner_boundary")
        );
        assert_eq!(
            run_command("crates/a/Cargo.toml", &integration, Some("owner")).as_deref(),
            Some("cargo test --manifest-path crates/a/Cargo.toml --test owner owner_boundary")
        );
        assert_eq!(run_command("crates/a/Cargo.toml", &integration, None), None);
        Ok(())
    }

    #[test]
    fn integration_target_name_follows_cargo_target_registration() -> Result<(), String> {
        use crate::analysis::test_stub::TestStubPlacement;
        let root = std::env::temp_dir().join(format!(
            "ripr-stub-target-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("pkg")).map_err(|error| error.to_string())?;
        let stub = |file: &str| RustTestStub {
            placement: TestStubPlacement::ExistingIntegrationFile {
                file: PathBuf::from(file),
            },
            test_name: "t".to_string(),
            text: String::new(),
            already_present: false,
            fill_ins: Vec::new(),
            derived_inputs: Vec::new(),
        };
        let got = |tail: &str, file: &str| -> Result<Option<String>, String> {
            std::fs::write(
                root.join("pkg/Cargo.toml"),
                format!(
                    "[package]\nname = \"pkg\"\n{}{tail}",
                    if tail.contains("edition") || tail.starts_with("[[test]]\nname = \"other\"") {
                        ""
                    } else {
                        "edition = \"2021\"\n"
                    }
                ),
            )
            .map_err(|error| error.to_string())?;
            Ok(integration_target_name(
                &root,
                Path::new("pkg/Cargo.toml"),
                &stub(file),
            ))
        };
        let some = |name: &str| Some(name.to_string());
        let result = (|| {
            // auto-discovered: file stem, or the directory of `main.rs`
            assert_eq!(got("", "pkg/tests/discount.rs")?, some("discount"));
            assert_eq!(got("", "pkg/tests/grp/main.rs")?, some("grp"));
            assert_eq!(got("", "pkg/tests/grp/helper.rs")?, None);
            assert_eq!(got("", "pkg/src/lib.rs")?, None);
            // an explicit entry renames the target: its name, not the stem
            let renamed = "[[test]]\nname = \"pricing\"\npath = \"tests/discount.rs\"\n";
            assert_eq!(got(renamed, "pkg/tests/discount.rs")?, some("pricing"));
            // path spelling is normalized
            let dotted = "[[test]]\nname = \"pricing\"\npath = \"./tests/discount.rs\"\n";
            assert_eq!(got(dotted, "pkg/tests/discount.rs")?, some("pricing"));
            // an entry without `path` defaults to tests/<name>.rs
            let default_path = "[[test]]\nname = \"discount\"\n";
            assert_eq!(
                got(default_path, "pkg/tests/discount.rs")?,
                some("discount")
            );
            // a sibling file keeps its own stem when another file is renamed
            assert_eq!(got(renamed, "pkg/tests/other.rs")?, some("other"));
            // `autotests = false`: only an explicit entry registers a target
            let off = "autotests = false\n";
            assert_eq!(got(off, "pkg/tests/discount.rs")?, None);
            let off_explicit = format!("{off}{renamed}");
            assert_eq!(
                got(&off_explicit, "pkg/tests/discount.rs")?,
                some("pricing")
            );
            // an explicit entry stealing the inferred name leaves the file unreachable
            let stolen = "[[test]]\nname = \"discount\"\npath = \"tests/elsewhere.rs\"\n";
            assert_eq!(got(stolen, "pkg/tests/discount.rs")?, None);
            // a target that needs features is not runnable by a bare command
            let gated = format!("{renamed}required-features = [\"extra\"]\n");
            assert_eq!(got(&gated, "pkg/tests/discount.rs")?, None);
            // `harness = false` is not run by a plain test command
            let no_harness = format!("{renamed}harness = false\n");
            assert_eq!(got(&no_harness, "pkg/tests/discount.rs")?, None);
            // a pathless entry may live at tests/<name>/main.rs
            let dir_default = "[[test]]\nname = \"grp\"\n";
            assert_eq!(got(dir_default, "pkg/tests/grp/main.rs")?, some("grp"));
            assert_eq!(got(dir_default, "pkg/tests/grp.rs")?, some("grp"));
            // edition 2015: an explicit entry disables legacy auto-discovery
            let other = "[[test]]\nname = \"other\"\npath = \"tests/other.rs\"\n";
            let e2015 = format!("edition = \"2015\"\n{other}");
            assert_eq!(got(&e2015, "pkg/tests/discount.rs")?, None);
            assert_eq!(got(other, "pkg/tests/discount.rs")?, None);
            // ...unless autotests = true is explicit
            let e2015_on = format!("edition = \"2015\"\nautotests = true\n{other}");
            assert_eq!(got(&e2015_on, "pkg/tests/discount.rs")?, some("discount"));
            // 2015 with no explicit entry still auto-discovers
            let bare = got("edition = \"2015\"\n", "pkg/tests/discount.rs")?;
            assert_eq!(bare, some("discount"));
            // 2018+ with an explicit entry still auto-discovers
            let e2018 = format!("edition = \"2018\"\n{other}");
            assert_eq!(got(&e2018, "pkg/tests/discount.rs")?, some("discount"));
            // inherited edition comes from the workspace root
            let inherit = format!("edition.workspace = true\n{other}");
            let ws = |edition: &str| {
                std::fs::write(
                    root.join("Cargo.toml"),
                    format!("[workspace]\nmembers = [\"pkg\"]\n[workspace.package]\nedition = \"{edition}\"\n"),
                )
                .map_err(|error| error.to_string())
            };
            ws("2021")?;
            assert_eq!(got(&inherit, "pkg/tests/discount.rs")?, some("discount"));
            ws("2015")?;
            assert_eq!(got(&inherit, "pkg/tests/discount.rs")?, None);
            // unresolvable inheritance fails closed
            std::fs::remove_file(root.join("Cargo.toml")).map_err(|error| error.to_string())?;
            assert_eq!(got(&inherit, "pkg/tests/discount.rs")?, None);
            // an unusable `path` matches nothing: never fall back to the name
            for bad in ["../shared/d.rs", "/abs/d.rs"] {
                let entry = format!("[[test]]\nname = \"discount\"\npath = \"{bad}\"\n");
                assert_eq!(got(&entry, "pkg/tests/discount.rs")?, None);
            }
            // an unparseable edition only matters when explicit entries exist
            let future = "edition = \"future\"\n";
            assert_eq!(got(future, "pkg/tests/discount.rs")?, some("discount"));
            assert_eq!(
                got(&format!("{future}{other}"), "pkg/tests/discount.rs")?,
                None
            );
            // autotests = false decides without needing the edition
            let off_inherit = format!("edition.workspace = true\nautotests = false\n{other}");
            assert_eq!(got(&off_inherit, "pkg/tests/discount.rs")?, None);
            // a root package inheriting from its own manifest's [workspace.package]
            std::fs::write(
                root.join("Cargo.toml"),
                format!(
                    "[package]\nname = \"rootpkg\"\nedition.workspace = true\n{other}[workspace]\n[workspace.package]\nedition = \"2021\"\n"
                ),
            )
            .map_err(|error| error.to_string())?;
            let root_stub = stub("tests/discount.rs");
            assert_eq!(
                integration_target_name(&root, Path::new("Cargo.toml"), &root_stub),
                some("discount")
            );
            // both tests/<n>.rs and tests/<n>/main.rs: cargo reports a duplicate
            std::fs::create_dir_all(root.join("pkg/tests/grp"))
                .map_err(|error| error.to_string())?;
            std::fs::write(root.join("pkg/tests/grp/main.rs"), "")
                .map_err(|error| error.to_string())?;
            std::fs::write(root.join("pkg/tests/grp.rs"), "").map_err(|error| error.to_string())?;
            assert_eq!(got("", "pkg/tests/grp/main.rs")?, None);
            assert_eq!(got("", "pkg/tests/grp.rs")?, None);
            // in-package `..` resolves lexically; an escaping one matches nothing
            let dotdot = "[[test]]\nname = \"pricing\"\npath = \"tests/../tests/discount.rs\"\n";
            assert_eq!(got(dotdot, "pkg/tests/discount.rs")?, some("pricing"));
            let escape = "[[test]]\nname = \"discount\"\npath = \"../x/../tests/discount.rs\"\n";
            assert_eq!(got(escape, "pkg/tests/discount.rs")?, None);
            // a name-only entry is ambiguous when both layouts exist
            std::fs::create_dir_all(root.join("pkg/tests/amb"))
                .map_err(|error| error.to_string())?;
            std::fs::write(root.join("pkg/tests/amb.rs"), "").map_err(|error| error.to_string())?;
            std::fs::write(root.join("pkg/tests/amb/main.rs"), "")
                .map_err(|error| error.to_string())?;
            let amb = "[[test]]\nname = \"amb\"\n";
            assert_eq!(got(amb, "pkg/tests/amb.rs")?, None);
            assert_eq!(got(amb, "pkg/tests/amb/main.rs")?, None);
            // positive vs unresolved lookups
            std::fs::write(
                root.join("pkg/Cargo.toml"),
                "[package]\nname = \"pkg\"\nautotests = false\n",
            )
            .map_err(|error| error.to_string())?;
            let lookup = |file: &str| {
                integration_target_lookup(&root, Path::new("pkg/Cargo.toml"), &stub(file))
            };
            assert_eq!(lookup("pkg/tests/discount.rs"), TargetLookup::NotRegistered);
            // registered but not runnable by a bare command: unresolved
            for tail in ["harness = false\n", "required-features = [\"extra\"]\n"] {
                std::fs::write(
                    root.join("pkg/Cargo.toml"),
                    format!(
                        "[package]\nname = \"pkg\"\n[[test]]\nname = \"pricing\"\npath = \"tests/discount.rs\"\n{tail}"
                    ),
                )
                .map_err(|error| error.to_string())?;
                assert_eq!(lookup("pkg/tests/discount.rs"), TargetLookup::Unresolved);
            }
            std::fs::write(
                root.join("pkg/Cargo.toml"),
                "[package]\nname = \"pkg\"\nedition = \"future\"\n[[test]]\nname = \"o\"\npath = \"tests/o.rs\"\n",
            )
            .map_err(|error| error.to_string())?;
            assert_eq!(lookup("pkg/tests/discount.rs"), TargetLookup::Unresolved);
            // only the nearest workspace counts: an inner `[workspace]` without
            // an edition does not fall through to an outer one
            let write = |path: &str, text: &str| -> Result<(), String> {
                if let Some(parent) = Path::new(path).parent() {
                    std::fs::create_dir_all(root.join(parent))
                        .map_err(|error| error.to_string())?;
                }
                std::fs::write(root.join(path), text).map_err(|error| error.to_string())
            };
            write(
                "Cargo.toml",
                "[workspace]\nmembers = [\"pkg\"]\n[workspace.package]\nedition = \"2021\"\n",
            )?;
            assert_eq!(got(&inherit, "pkg/tests/discount.rs")?, some("discount"));
            write("pkg/Cargo.toml", "[workspace]\n")?;
            let inner = format!("[package]\nname = \"in\"\nedition.workspace = true\n{other}");
            write("pkg/in/Cargo.toml", &inner)?;
            let nested = integration_target_lookup(
                &root,
                Path::new("pkg/in/Cargo.toml"),
                &stub("pkg/in/tests/discount.rs"),
            );
            assert_eq!(nested, TargetLookup::Unresolved);
            // an explicit `package.workspace` pointer selects the outer workspace
            let pointed = format!(
                "[package]\nname = \"in\"\nworkspace = \"../..\"\nedition.workspace = true\n{other}"
            );
            write("pkg/in/Cargo.toml", &pointed)?;
            let by_pointer = integration_target_lookup(
                &root,
                Path::new("pkg/in/Cargo.toml"),
                &stub("pkg/in/tests/discount.rs"),
            );
            assert_eq!(by_pointer, TargetLookup::Name("discount".to_string()));
            Ok::<(), String>(())
        })();
        let _ = std::fs::remove_dir_all(&root);
        result
    }

    #[test]
    fn kind_selects_a_probe_family_and_applies_to_at_only() {
        let at =
            TestStubSelector::parse_at("src/lib.rs:2").and_then(|at| at.with_kind("error_path"));
        assert_eq!(
            at,
            Ok(TestStubSelector::At {
                file: "src/lib.rs".to_string(),
                line: 2,
                kind: Some("error_path".to_string()),
            })
        );
        assert!(matches!(
            TestStubSelector::parse_at("src/lib.rs:2")
                .and_then(|at| at.with_kind("static_unknown")),
            Err(message) if message.contains("--kind expects a probe family")
        ));
        assert!(matches!(
            TestStubSelector::SeamId("abc".to_string()).with_kind("predicate"),
            Err(message) if message.contains("--at only")
        ));
    }

    /// #5471: a finding line can hold seams of several kinds. The finding's
    /// probe family picks the one `check` reported; without it the seam on
    /// the line wins.
    #[test]
    fn kind_ranks_the_matching_seam_ahead_of_a_nearer_one() -> Result<(), String> {
        let root = std::env::temp_dir().join(format!(
            "ripr-stub-kind-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"kind\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(
            root.join("src/lib.rs"),
            "pub fn check(input: &str) -> Result<u8, E> {\n    if input.is_empty() {\n        return Err(E::Long);\n    }\n    Ok(1)\n}\n\n#[derive(Debug, PartialEq)]\npub enum E {\n    Long,\n}\n",
        )
        .map_err(|error| error.to_string())?;
        let config = RiprConfig::default();
        let kind_of = |kind: Option<&str>| -> Result<(String, bool), String> {
            let resolution = resolve_at_location(&root, &config, "src/lib.rs", 2, kind, &mut None)
                .map_err(|error| format!("{error:?}"))?;
            let seams =
                analysis::scoped_file_seams(&root, &config, Path::new("src/lib.rs"), &mut None)?;
            let seam = seams
                .iter()
                .find(|seam| seam.id().as_str() == resolution.seam_id)
                .ok_or("resolved seam is in the file")?;
            Ok((seam.kind().as_str().to_string(), resolution.outcome.is_ok()))
        };
        let plain = kind_of(None);
        let error_path = kind_of(Some("error_path"));
        let predicate = kind_of(Some("predicate"));
        let _ = std::fs::remove_dir_all(&root);
        let boundary = SeamKind::PredicateBoundary.as_str().to_string();
        let variant = SeamKind::ErrorVariant.as_str().to_string();
        assert_eq!(plain?, (boundary.clone(), true));
        assert_eq!(error_path?, (variant, true));
        assert_eq!(predicate?, (boundary, true));
        Ok(())
    }

    #[test]
    fn scoped_file_names_only_a_file_under_the_root() -> Result<(), String> {
        let root = std::env::temp_dir().join(format!(
            "ripr-stub-scoped-file-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("crates/a/src")).map_err(|error| error.to_string())?;
        std::fs::write(root.join("crates/a/src/lib.rs"), "").map_err(|error| error.to_string())?;
        let exact = scoped_file(&root, "crates/a/src/lib.rs");
        let dotted = scoped_file(&root, "./crates/a/src/lib.rs");
        let windows = scoped_file(&root, "crates\\a\\src\\lib.rs");
        let absolute = scoped_file(&root, &root.join("crates/a/src/lib.rs").to_string_lossy());
        // A suffix is not a file under the root: it keeps the repo-wide
        // lookup and its more-than-one-file refusal.
        let suffix = scoped_file(&root, "src/lib.rs");
        let directory = scoped_file(&root, "crates/a/src");
        // A path that leaves the root, by `..` or a symlink, is not under it.
        let outside = root.with_extension("outside.rs");
        std::fs::write(&outside, "").map_err(|error| error.to_string())?;
        let outside_name = outside
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        let dotdot = scoped_file(&root, &format!("crates/../../{outside_name}"));
        #[cfg(unix)]
        let linked = {
            std::os::unix::fs::symlink(&outside, root.join("crates/a/src/link.rs"))
                .map_err(|error| error.to_string())?;
            scoped_file(&root, "crates/a/src/link.rs")
        };
        let _ = std::fs::remove_file(&outside);
        let _ = std::fs::remove_dir_all(&root);
        let expected = Some(PathBuf::from("crates/a/src/lib.rs"));
        assert_eq!(exact, expected);
        assert_eq!(dotted, expected);
        assert_eq!(windows, expected);
        assert_eq!(absolute, expected);
        assert_eq!(suffix, None);
        assert_eq!(directory, None);
        assert_eq!(dotdot, None);
        #[cfg(unix)]
        assert_eq!(linked, None);
        Ok(())
    }

    #[test]
    fn line_of_offset_is_one_based() {
        assert_eq!(line_of_offset("a\nb\nc", 0), 1);
        assert_eq!(line_of_offset("a\nb\nc", 2), 2);
        assert_eq!(line_of_offset("a\nb\nc", 4), 3);
    }

    #[test]
    fn classified_candidates_omit_strongly_gripped_seams() {
        let classified = vec![
            ungripped("src/lib.rs", 2),
            classified_at("src/lib.rs", 4, SeamGripClass::StronglyGripped),
        ];
        let candidates = classified_candidates(&classified);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].seam().display_line(), 2);
    }

    /// #7290: `--at --kind` selects from a file parse, so a strongly-gripped
    /// seam still yields a stub. The use case must still look up the
    /// inventory class so the CLI can disclose it.
    #[test]
    fn at_kind_stub_discloses_a_strongly_gripped_inventory_class() -> Result<(), String> {
        const SOURCE: &str = "pub fn clamp_to(x: i32) -> i32 {\n    if x >= 100 { 100 } else { x }\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn at_boundary_clamps() {\n        assert_eq!(clamp_to(100), 100);\n        assert_eq!(clamp_to(99), 99);\n        assert_eq!(clamp_to(500), 100);\n    }\n}\n";
        let root = scratch_stub_crate("strongly-gripped", SOURCE)?;
        let config = RiprConfig::default();
        let inventory = analysis::inventory_diff_scoped_classified_seams_at_with_config(
            &root,
            &config,
            &[PathBuf::from("src/lib.rs")],
            &[],
        )
        .map_err(|error| error.to_string())?;
        let fixture = inventory
            .classified
            .iter()
            .find(|entry| entry.seam.display_line() == 2)
            .ok_or_else(|| {
                format!(
                    "fixture must classify the comparison on line 2: {:?}",
                    inventory
                        .classified
                        .iter()
                        .map(|entry| (entry.seam.display_line(), entry.class.as_str()))
                        .collect::<Vec<_>>()
                )
            })?;
        assert_eq!(
            fixture.class,
            SeamGripClass::StronglyGripped,
            "fixture must already be strongly gripped before the stub lookup"
        );
        let selector = TestStubSelector::parse_at("src/lib.rs:2")?.with_kind("predicate")?;
        let resolution =
            resolve_test_stub(&root, &config, &selector).map_err(|error| format!("{error:?}"))?;
        let outcome_ok = resolution.outcome.is_ok();
        let grip = resolution.grip_class;
        let _ = std::fs::remove_dir_all(&root);
        assert!(outcome_ok, "stub as scaffold stays ready");
        assert_eq!(grip, Some(SeamGripClass::StronglyGripped));
        Ok(())
    }

    /// #6298: `--at` on a line that holds no gap stubs the nearest gap in the
    /// function, and the resolution says so; a line that holds a gap says
    /// nothing, so the note never fires on an exact match.
    #[test]
    fn at_a_line_without_a_gap_names_the_nearest_gap_it_stubbed() -> Result<(), String> {
        const SOURCE: &str = "pub struct Size(pub u64);
impl Size {
    pub fn whole(&self, unit: u64) -> Option<u64> {
        if unit == 0 || self.0 % unit != 0 {
            return None;
        }
        Some(self.0 / unit)
    }
}
";
        let root = scratch_stub_crate("at-note", SOURCE)?;
        let config = RiprConfig::default();
        let resolve = |line: usize, family: &str| {
            let selector =
                TestStubSelector::parse_at(&format!("src/lib.rs:{line}"))?.with_kind(family)?;
            resolve_test_stub(&root, &config, &selector).map_err(|error| format!("{error:?}"))
        };
        let exact = resolve(7, "return_value")?;
        let off = resolve(8, "return_value")?;
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(exact.at_note, None, "an exact line match needs no note");
        assert!(
            off.outcome.is_ok(),
            "the nearest gap is still stubbed: {:?}",
            off.outcome.err()
        );
        let note = off.at_note.ok_or("a substituted line must carry a note")?;
        assert!(
            note.starts_with("note: src/lib.rs:8 has no stubbable gap")
                && note.ends_with(", src/lib.rs:7"),
            "{note}"
        );
        Ok(())
    }

    /// A genuine open gap still yields a ready stub, and its inventory
    /// class is not the strongly-gripped omission case.
    #[test]
    fn at_kind_stub_on_an_open_gap_is_not_strongly_gripped() -> Result<(), String> {
        const SOURCE: &str =
            "pub fn clamp_to(x: i32) -> i32 {\n    if x >= 100 { 100 } else { x }\n}\n";
        let root = scratch_stub_crate("open-gap", SOURCE)?;
        let config = RiprConfig::default();
        let selector = TestStubSelector::parse_at("src/lib.rs:2")?.with_kind("predicate")?;
        let resolution =
            resolve_test_stub(&root, &config, &selector).map_err(|error| format!("{error:?}"))?;
        let outcome_ok = resolution.outcome.is_ok();
        let grip = resolution.grip_class;
        // #8727: a file-parse (`--at --kind`) selection carries no classified
        // entry, so the repair packet's surface cannot be established and no
        // route is invented.
        let next_repair = resolution.next_repair.is_none();
        let _ = std::fs::remove_dir_all(&root);
        assert!(outcome_ok, "an open gap still yields a stub: {grip:?}");
        assert!(next_repair, "--at --kind must not invent a repair route");
        assert_ne!(grip, Some(SeamGripClass::StronglyGripped));
        Ok(())
    }

    /// One classified gap whose repair route holds: an exact missing
    /// boundary discriminator and a producer-owned integration test
    /// proposal. `line` is display-only; `offset` must point at the seam
    /// expression inside `source`.
    fn repair_ready_entry(line: usize, offset: usize, owner: &str) -> ClassifiedSeam {
        use crate::analysis::new_test_target::{
            NewTestKind, NewTestProposalProvenance, NewTestTargetAdmission, NewTestTargetProposal,
        };
        use crate::analysis::seams::{ExpectedSink, RequiredDiscriminator};
        use crate::domain::MissingDiscriminatorFact;
        let mut entry = classified_at("src/lib.rs", line, SeamGripClass::WeaklyGripped);
        entry.seam = RepoSeam::new(
            "src/lib.rs",
            owner,
            SeamKind::PredicateBoundary,
            offset,
            line,
            "x >= 100",
            RequiredDiscriminator::BoundaryValue {
                description: "x >= 100".to_string(),
            },
            ExpectedSink::ReturnValue,
        );
        entry.evidence.seam_id = entry.seam.id().clone();
        entry
            .evidence
            .missing_discriminators
            .push(MissingDiscriminatorFact {
                value: "x >= 100".to_string(),
                reason: "observed values do not include the boundary case".to_string(),
                flow_sink: None,
            });
        entry.evidence.new_test_target = Some(NewTestTargetAdmission {
            proposal: Some(NewTestTargetProposal {
                kind: NewTestKind::Integration,
                file: PathBuf::from("tests/clamp_tests.rs"),
                owner: owner.to_string(),
                provenance: NewTestProposalProvenance::ProducerOwned,
            }),
            region: None,
            blocker: None,
            owner_inline_region: None,
        });
        entry
    }

    const FREE_FN_SOURCE: &str =
        "pub fn clamp_to(x: i32) -> i32 {\n    if x >= 100 { 100 } else { x }\n}\n";
    const IMPL_METHOD_SOURCE: &str = "pub struct Clamp;\nimpl Clamp {\n    pub fn clamp_to(&self, x: i32) -> i32 {\n        if x >= 100 { 100 } else { x }\n    }\n}\n";

    fn seam_offset(source: &str) -> Result<usize, String> {
        source
            .find("x >= 100")
            .ok_or_else(|| "fixture source lost its seam expression".to_string())
    }

    /// #8727: a repair-ready stub names the repair journey it feeds — the
    /// documented `ripr agent repair` start and the packet's admitted edit
    /// surface. A free crate-root owner takes the producer-admitted
    /// integration file, so the two independent derivations agree and no
    /// divergence is carried.
    #[test]
    fn next_repair_names_the_start_and_the_agreeing_surface() -> Result<(), String> {
        let root = scratch_stub_crate("next-repair-agree", FREE_FN_SOURCE)?;
        let entry = repair_ready_entry(2, seam_offset(FREE_FN_SOURCE)?, "clamp_to");
        let seam_id = entry.seam.id().as_str().to_string();
        let resolution = resolution_for(
            &root,
            &RiprConfig::default(),
            &entry,
            FREE_FN_SOURCE.to_string(),
        );
        // #8727 review: filename agreement is the projection's own rule, but
        // the before phase's edit cage is the authority that must admit the
        // written test; an agreeing placement must pass the target predicate
        // the repair start itself uses.
        let stub = resolution
            .outcome
            .as_ref()
            .map_err(|refusal| format!("fixture seam must stub: {refusal:?}"))?;
        assert!(
            recommended_test_is_repair_edit_target(&entry),
            "an agreeing surface must be one the repair route admits"
        );
        let next = resolution
            .next_repair
            .ok_or("a repair-ready stub must name its repair journey")?;
        assert_eq!(
            next.start_command,
            format!(
                "ripr agent repair --root {} --seam-id {} --phase before",
                shell_arg(&bound_root(&root_path_display(&root))),
                shell_arg(&seam_id),
            ),
            "the start command must name the same seam and root"
        );
        assert_eq!(next.allowed_edit_surface, "tests/clamp_tests.rs");
        assert_eq!(
            next.placement_divergence, None,
            "a free owner lands the stub in the packet's own surface"
        );
        assert!(
            stub.placement
                .file()
                .ends_with(next.allowed_edit_surface.as_str()),
            "the admitted surface must cover the stub's placement file: {:?}",
            stub.placement.file()
        );
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    /// #8727: the divergence the two derivations actually permit. The
    /// producer's target selection proposes an integration file, but an
    /// `impl`-method owner is not one an integration test can name, so the
    /// stub falls back to an inline module of the production file while the
    /// repair packet admits only the integration file. Generation time must
    /// name both files — the before phase's dirty-premise gate would
    /// otherwise refuse the written stub without ever mentioning it.
    #[test]
    fn next_repair_warns_when_the_stub_lands_outside_the_packet_surface() -> Result<(), String> {
        let root = scratch_stub_crate("next-repair-diverge", IMPL_METHOD_SOURCE)?;
        let entry = repair_ready_entry(4, seam_offset(IMPL_METHOD_SOURCE)?, "Clamp::clamp_to");
        let resolution = resolution_for(
            &root,
            &RiprConfig::default(),
            &entry,
            IMPL_METHOD_SOURCE.to_string(),
        );
        let stub = resolution
            .outcome
            .as_ref()
            .map_err(|refusal| format!("fixture seam must stub: {refusal:?}"))?;
        assert!(
            matches!(stub.placement, TestStubPlacement::NewInlineModule { .. }),
            "an impl-method owner keeps the stub inline: {:?}",
            stub.placement
        );
        let next = resolution
            .next_repair
            .ok_or("a repair-ready stub must name its repair journey")?;
        assert_eq!(next.allowed_edit_surface, "tests/clamp_tests.rs");
        let divergence = next
            .placement_divergence
            .ok_or("diverging placements must be named at generation time")?;
        assert_eq!(divergence.stub_file, "src/lib.rs");
        assert_eq!(divergence.packet_file, "tests/clamp_tests.rs");
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    /// #8727 fail-closed honesty: the stub producer does not wait on the
    /// repair-packet flip, but the repair journey must. With no safe test
    /// target the route is not eligible, so no start command is offered —
    /// offering one would send the user into a refusal.
    #[test]
    fn next_repair_stays_silent_when_the_repair_route_is_not_eligible() -> Result<(), String> {
        let root = scratch_stub_crate("next-repair-ineligible", FREE_FN_SOURCE)?;
        let mut entry = repair_ready_entry(2, seam_offset(FREE_FN_SOURCE)?, "clamp_to");
        entry.evidence.new_test_target = None;
        let resolution = resolution_for(
            &root,
            &RiprConfig::default(),
            &entry,
            FREE_FN_SOURCE.to_string(),
        );
        let outcome_ok = resolution.outcome.is_ok();
        let next_repair = resolution.next_repair;
        let _ = std::fs::remove_dir_all(&root);
        assert!(outcome_ok, "the stub itself does not wait on the flip");
        assert_eq!(
            next_repair, None,
            "no eligible repair route, no invented start command"
        );
        Ok(())
    }

    /// #8727 review: a seam whose class is configured off still yields a
    /// ready stub (the producer does not gate on agent policy), but the
    /// packet renderer refuses policy-omitted seams before producing any
    /// packet, so the projected start command would only meet that refusal.
    /// The same `AgentBriefPolicy::omission_reason_for_class` gate the packet
    /// renderer applies must suppress the block.
    #[test]
    fn next_repair_stays_silent_when_the_class_is_configured_off() -> Result<(), String> {
        let root = scratch_stub_crate("next-repair-configured-off", FREE_FN_SOURCE)?;
        let entry = repair_ready_entry(2, seam_offset(FREE_FN_SOURCE)?, "clamp_to");
        let config =
            crate::config::tests_only_parse("[severity.seams]\nweakly_gripped = \"off\"\n")?;
        let resolution = resolution_for(&root, &config, &entry, FREE_FN_SOURCE.to_string());
        let outcome_ok = resolution.outcome.is_ok();
        let next_repair = resolution.next_repair;
        let _ = std::fs::remove_dir_all(&root);
        assert!(outcome_ok, "the stub itself does not wait on agent policy");
        assert_eq!(
            next_repair, None,
            "a policy-omitted seam must not be offered a repair start"
        );
        Ok(())
    }

    /// #11477 item 4: a committed `tests -> /elsewhere` link must not redirect
    /// `agent stub --write`; a real `tests/` directory still receives the file.
    #[cfg(unix)]
    #[test]
    fn write_test_stub_refuses_symlinked_tests_dir() -> Result<(), String> {
        let base = std::env::temp_dir().join(format!("ripr-stub-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let ws = base.join("ws");
        let outside = base.join("outside");
        std::fs::create_dir_all(&ws).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&outside).map_err(|e| e.to_string())?;
        let stub = RustTestStub {
            placement: TestStubPlacement::NewIntegrationFile {
                file: PathBuf::from("tests/clamp_tests.rs"),
            },
            test_name: "clamp_to_boundary".to_string(),
            text: "#[test]\nfn t() {}\n".to_string(),
            already_present: false,
            fill_ins: Vec::new(),
            derived_inputs: Vec::new(),
        };
        let resolution = TestStubResolution {
            seam_id: "seam".to_string(),
            owner: "clamp_to".to_string(),
            source: String::new(),
            outcome: Ok(stub.clone()),
            grip_class: None,
            at_note: None,
            next_repair: None,
            seam_line: 1,
            weak_oracle_guidance: None,
        };
        std::os::unix::fs::symlink(&outside, ws.join("tests")).map_err(|e| e.to_string())?;
        let refused = write_test_stub(&ws, &resolution, &stub);
        let leaked = outside.join("clamp_tests.rs").exists();
        std::fs::remove_file(ws.join("tests")).map_err(|e| e.to_string())?;
        let written = write_test_stub(&ws, &resolution, &stub);
        let landed = ws.join("tests/clamp_tests.rs").exists();
        let _ = std::fs::remove_dir_all(&base);
        assert!(refused.is_err_and(|e| e.contains("resolves outside the workspace")));
        assert!(!leaked, "nothing may be written through the link");
        assert!(written.is_ok() && landed, "real tests/ dir must still work");
        Ok(())
    }

    /// #8727 review: the start command renders the selected root exactly as
    /// the filesystem spells it. On Unix a `\` is an ordinary filename
    /// character; rewriting it into a separator before binding would make
    /// the pasted command target another directory (#4287). The unit form is
    /// a plain string path — no checkout with such a name is created.
    #[test]
    fn next_repair_start_preserves_a_literal_backslash_unix_root() -> Result<(), String> {
        let root = Path::new("/tmp/team\\repo");
        let entry = repair_ready_entry(2, 0, "clamp_to");
        let stub = RustTestStub {
            placement: TestStubPlacement::NewIntegrationFile {
                file: PathBuf::from("tests/clamp_tests.rs"),
            },
            test_name: "clamp_to_boundary".to_string(),
            text: String::new(),
            already_present: false,
            fill_ins: Vec::new(),
            derived_inputs: Vec::new(),
        };
        let next = next_repair_for(root, &RiprConfig::default(), &entry, &stub)
            .ok_or("a repair-ready stub must name its repair journey")?;
        if cfg!(unix) {
            assert!(
                next.start_command.contains("team\\repo"),
                "a literal backslash in a Unix root must survive: {}",
                next.start_command
            );
        } else {
            assert!(
                !next.start_command.contains("team\\repo"),
                "Windows treats `\\` as a separator: {}",
                next.start_command
            );
            assert!(
                next.start_command.contains("team/repo"),
                "the root must render with stable separators: {}",
                next.start_command
            );
        }
        Ok(())
    }
}

#[cfg(all(test, feature = "lang-rust"))]
#[path = "test_stub_inventory_tests.rs"]
mod inventory_lookup_tests;

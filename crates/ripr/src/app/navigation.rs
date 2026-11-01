use super::test_stub::{StubRoute, StubRouteDecision};
use super::{CheckInput, Mode};
use crate::agent::loop_commands::{bound_root, bound_root_path, root_path_display, shell_arg};
use std::path::Path;

/// What tree content a check run analyzed (#6304, #7257): the live working
/// tree (explicit `--worktree` or the dirty-workspace default), a fixed
/// supplied scope (`--diff`, `--candidate-tree`), or committed history (an
/// explicit or resolved base, including `--committed`). The canonical triage
/// adapter binds the DTO diff-source from this, never by deriving it from
/// `output.base`: both committed and worktree runs resolve a base, while a
/// supplied scope has none — base presence identifies neither mode. Bind it
/// from the effective analysis source, not from the `--worktree` flag alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CheckDiffProvenance {
    Worktree,
    SuppliedScope,
    CommittedHistory,
}

impl CheckDiffProvenance {
    /// Provenance of the tree this check actually analyzed (#7257).
    ///
    /// `worktree_run` is the effective live source already used for analysis
    /// (explicit `--worktree` **or** the dirty-workspace default). Selecting
    /// from the `--worktree` flag alone misattributes that dirty default as
    /// committed history. `--diff` / `--candidate-tree` stay a supplied
    /// scope even when the checkout is dirty. Explicit `--committed` keeps
    /// `worktree_run` false.
    pub(crate) fn from_effective_source(worktree_run: bool, supplied_scope: bool) -> Self {
        if worktree_run {
            Self::Worktree
        } else if supplied_scope {
            Self::SuppliedScope
        } else {
            Self::CommittedHistory
        }
    }
}

/// The repair route the check pipeline computed for its selected finding
/// (#8166 Slice C): the seam identity `ripr repair` accepts for that
/// finding, established by the producer-owned repair-admission gate in
/// `app::task_first`. Absent means no repair line is printed: ineligible,
/// ambiguous, truncated, and unbound findings get no route.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RepairRoute {
    pub(crate) finding_id: String,
    /// The exact seam ID the repair resolver accepts for this finding.
    pub(crate) seam_id: String,
    /// The production file whose uncommitted changes make `ripr repair`
    /// refuse this seam today (its inline `#[cfg(test)]` module is the edit
    /// target and starts only from the committed file). When set, the check
    /// screen states the commit-or-stash requirement instead of offering the
    /// repair start as a runnable next action.
    pub(crate) uncommitted_target: Option<String>,
}

/// The gap-bound repair start the check pipeline computed for a TypeScript
/// finding whose repair packet validates (#9103, RIPR-SPEC-0311): the
/// canonical gap id and the diff base this run analyzed, which the printed
/// `ripr agent repair --gap-id ... --base ... --phase before` start replays.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GapRepairRoute {
    pub(crate) finding_id: String,
    pub(crate) gap_id: String,
    pub(crate) base: String,
    /// Tracked production files that differ from HEAD in the analyzed tree
    /// (#10698). The start refuses to begin while any exist, so the check
    /// screen prints their commit before the start. Empty on a clean tree.
    pub(crate) uncommitted_files: Vec<String>,
}

impl GapRepairRoute {
    /// The commit step that must precede the gap-bound start when the analyzed
    /// tree has uncommitted production edits (#10698). `None` on a clean tree.
    ///
    /// The command runs from any directory: `git -C <root>` anchors it at the
    /// analyzed root, `--literal-pathspecs` keeps names like `src/[ab].py`
    /// literal, and the files are shell-quoted and spelled relative to that root.
    pub(crate) fn commit_prerequisite(&self, root_display: &str) -> Option<String> {
        if self.uncommitted_files.is_empty() {
            return None;
        }
        let files = self
            .uncommitted_files
            .iter()
            .map(|file| shell_arg(file))
            .collect::<Vec<_>>()
            .join(" ");
        Some(format!(
            "git -C {} --literal-pathspecs commit -m \"<message>\" -- {files}",
            shell_arg(root_display)
        ))
    }
}

/// Why the check pipeline offered no repair start for its selected finding
/// (#8729). Producer-owned at the decision site in
/// `app::task_first::check_repair_route`, so the rendered advisory names
/// the state instead of leaving the repair line's absence silent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RepairRouteRefusal {
    GapRouteUnavailable {
        reason: &'static str,
    },
    /// The run's analysis is incomplete (#11136): the after phase refuses a
    /// before artifact like that, so no gap-bound start is printed.
    /// `limitation` names the cause and the recovery.
    AnalysisIncomplete {
        limitation: String,
    },
    /// The finding has no canonical gap to bind a seam from.
    NoCanonicalGap,
    /// A TypeScript/JavaScript preview finding whose repair packet is not
    /// complete: the delegated loop starts from a complete packet's gap id,
    /// not from a Rust seam, so the absent seam is not the reason.
    PreviewPacketNotComplete {
        language: String,
        reason: String,
    },
    /// The finding's file belongs to a language adapter whose findings the
    /// seam inventory behind `ripr repair` cannot bind (#9102). The
    /// inventory is built from the Rust index only, so no seam exists for
    /// this file; a terminal state, not a stale id.
    NoSeamInventory {
        language: String,
        file: String,
    },
    /// The root's typed config could not be loaded.
    ConfigUnreadable,
    /// Classifying seams in the finding's file failed.
    ClassificationFailed,
    /// The repo seam-limit authority was unreadable, or the classified
    /// candidate set exceeded the configured limit.
    SeamLimit,
    /// No classified seam in the finding's file binds this finding.
    /// `seams_in_file` is how many classified seams the file holds, so the
    /// advisory never says "no seam" where `ripr repair` lists seams (#10702).
    NoBindingSeam {
        seams_in_file: usize,
    },
    /// The finding itself is `exposed`: the related tests already notice the
    /// change, so there is nothing to repair whatever the seam view says
    /// (#10702).
    FindingAlreadyExposed,
    /// `check` reads a gap or limitation for this change, while the
    /// repository-wide seam view `ripr repair` reads already counts the bound
    /// seam as gripped. The advisory names that one difference instead of
    /// quoting repair's "no gap remains" beside check's gap (#10702).
    SeamGrippedFindingNot,
    /// Several classified seams bind this finding; the check screen never
    /// picks one silently.
    SeveralBindingSeams,
    /// The one binding seam failed the shared repair-admission gate.
    Ineligible {
        reason: String,
        /// The gate's refusal kind, so the check screen words a not-ready
        /// route without parsing `reason` (#9244).
        kind: super::task_first::RefusalKind,
        /// The producer-owned missing-evidence list behind a not-ready route.
        missing: Vec<String>,
        /// A related test already supplies the changed boundary input and
        /// only its oracle is weak (#10622).
        boundary_input_observed: bool,
    },
}

impl RepairRouteRefusal {
    /// The one-line advisory for the check triage screen (#8729).
    /// `full_gate_command` is the bound `ripr repair <finding-id>` command
    /// for this navigation. The seam-budget arm names its own recovery instead: the full
    /// gate re-checks the same truncated authority, so pointing there would
    /// repeat the refusal. A finding with no canonical gap (#9244) never
    /// reaches a seam, so the gate would answer with the same refusal; it
    /// names the Next step already on the screen instead. A finding no seam
    /// binds to (#9244) gets the same recovery: with no binding seam the check
    /// screen has nothing for the gate to start from, so it names the Next step.
    pub(crate) fn advisory(&self, full_gate_command: &str) -> String {
        let gate = || format!("run `{full_gate_command}` for the full gate's answer");
        match self {
            Self::GapRouteUnavailable { reason } => format!(
                "no repair start offered: {reason}; inspect the current workspace and rerun `ripr check`"
            ),
            Self::AnalysisIncomplete { limitation } => format!(
                "no repair start offered: the analysis is incomplete ({limitation}), so a repair started now could not be verified afterward; resolve that, rerun `ripr check`, or add the missing test by hand from the packet above"
            ),
            Self::NoCanonicalGap => {
                "no repair start offered: this finding has no gap ID, so there is \
                 no gap to start a repair from; follow the Next step above"
                    .to_string()
            }
            Self::PreviewPacketNotComplete { language, reason } => format!(
                "no repair start offered: ripr has no complete repair packet for this {language} \
                 finding ({reason}); add or strengthen the test by hand as the Next step above \
                 says, then rerun `ripr check`"
            ),
            Self::NoSeamInventory { language, file } => format!(
                "no repair start offered: {}",
                super::task_first::no_seam_inventory_reason_plain(language, file)
            ),
            Self::ConfigUnreadable => "no repair start offered: the root's config could not be \
                 read; fix the ripr.toml error at the root, then rerun `ripr check`"
                .to_string(),
            Self::ClassificationFailed => format!(
                "no repair start offered: classifying the finding's seams failed; \
                 `{full_gate_command}` may print the underlying error"
            ),
            Self::SeamLimit => {
                format!(
                    "no repair start offered: the seam budget left the repair candidate set unreadable or truncated; {}",
                    super::task_first::WIDEN_SEAM_BUDGET
                )
            }
            Self::NoBindingSeam { seams_in_file: 0 } => {
                "no repair start offered: ripr found no spot in this \
                 finding's file to bind it to, so there is no spot to start a repair from; \
                 follow the Next step above"
                    .to_string()
            }
            Self::NoBindingSeam { seams_in_file } => format!(
                "no repair start offered: ripr found {seams_in_file} {} in this finding's \
                 file, but could not tie one to this finding's changed line, so it cannot pick one \
                 for you; `ripr pilot` lists them, or follow the Next step above",
                if *seams_in_file == 1 { "spot" } else { "spots" }
            ),
            Self::FindingAlreadyExposed => "no repair start offered: nothing to repair here; \
                 the related tests already notice this change (exposed)"
                .to_string(),
            Self::SeamGrippedFindingNot => "no repair start offered: `ripr check` reads a gap \
                 or limit for this change, but `ripr repair` judges the surrounding code as a whole and finds \
                 its tests already notice changes there (exposed), so it has no gap to start \
                 from. The two read different evidence (your changed lines versus the surrounding \
                 code as a whole) and ripr does not start a repair on conflicting evidence; follow the \
                 Next step above"
                .to_string(),
            Self::SeveralBindingSeams => {
                "no repair start offered: several seams bind this finding \
                 and the check screen never picks one; `ripr repair` needs the ID of the seam \
                 you choose"
                    .to_string()
            }
            Self::Ineligible {
                kind: super::task_first::RefusalKind::RouteNotReady,
                missing,
                boundary_input_observed,
                ..
            } => {
                let observed_boundary = *boundary_input_observed
                    && missing
                        .iter()
                        .any(|item| super::task_first::is_boundary_value_discriminator(item));
                if observed_boundary {
                    // #10622: the input is already there; only the assertion
                    // is weak, so do not say the boundary evidence is missing.
                    // #10644: plain phrases for the human reader; the machine
                    // `missing` list stays internal.
                    let others: Vec<String> = missing
                        .iter()
                        .filter(|item| !super::task_first::is_boundary_value_discriminator(item))
                        .map(|item| super::task_first::humanize_route_evidence(item))
                        .collect();
                    let also = if others.is_empty() {
                        String::new()
                    } else {
                        format!(" It is also still missing: {}.", others.join("; "))
                    };
                    format!(
                        "no repair start offered: a related test already reaches the changed \
                         boundary input, but ripr grades its assertion as a weak check, so ripr \
                         cannot build a repair packet for this finding yet.{also} \
                         Follow the Next step above, then rerun `ripr check`"
                    )
                } else if missing.is_empty() {
                    "no repair start offered: ripr cannot build a repair packet for this \
                     finding yet because the static evidence is not complete enough to name a \
                     safe test target; follow the Next step above, then rerun `ripr check`"
                        .to_string()
                } else {
                    // #10644: name each missing fact in plain words on this
                    // human screen; the underlying list keeps internal codes.
                    let plain: Vec<String> = missing
                        .iter()
                        .map(|item| super::task_first::humanize_route_evidence(item))
                        .collect();
                    format!(
                        "no repair start offered: ripr cannot build a repair packet for this \
                         finding yet because it is still missing: {}; follow the Next step \
                         above, then rerun `ripr check`",
                        plain.join("; ")
                    )
                }
            }
            Self::Ineligible { reason, .. } => {
                format!("no repair start offered: {reason}; {}", gate())
            }
        }
    }
}

/// What the check pipeline decided about the repair route for its selected
/// finding (#8729): the route to print, a named refusal to disclose, or a
/// deliberate silence. Silence covers a static-limited finding — its
/// limitation guidance already renders on the same screen — and runs with
/// no selected finding at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CheckRepairRoute {
    Route(RepairRoute),
    /// A TypeScript finding with a validated repair packet: a gap-bound
    /// start instead of a seam-bound one (#9103).
    GapRoute(GapRepairRoute),
    Refused(RepairRouteRefusal),
    Silent,
}

/// Copy-pasteable sibling commands for a selected finding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FindingNavigation {
    explain_prefix: String,
    context_prefix: String,
    list_prefix: String,
    stub_prefix: String,
    repair_suffix: String,
    /// The analyzed root as the printed commands spell it (`.` for legacy).
    root_display: String,
    /// Resolver decisions for the selected finding and typed ambiguities
    /// within the displayed hidden window, each keyed by finding identity.
    stub_routes: Vec<StubRoute>,
    /// The repair route the check pipeline computed for its selected
    /// finding (#8166 Slice C). Absent means no route is printed.
    repair_route: Option<RepairRoute>,
    /// The gap-bound repair start for a TypeScript finding (#9103). Absent
    /// unless the check pipeline validated the finding's repair packet.
    gap_repair_route: Option<GapRepairRoute>,
    /// The named refusal when the check pipeline selected a finding but
    /// offered it no repair start (#8729). Absent on a computed route and
    /// on deliberate silence (static-limited findings, no selected
    /// finding).
    repair_route_refusal: Option<RepairRouteRefusal>,
}

impl FindingNavigation {
    pub(crate) fn legacy() -> Self {
        Self {
            explain_prefix: "ripr explain".to_string(),
            context_prefix: "ripr context".to_string(),
            list_prefix: "ripr check".to_string(),
            stub_prefix: "ripr agent stub".to_string(),
            repair_suffix: String::new(),
            root_display: ".".to_string(),
            stub_routes: Vec::new(),
            repair_route: None,
            gap_repair_route: None,
            repair_route_refusal: None,
        }
    }

    /// Carry the check pipeline's stub-route decision to the renderer.
    pub(crate) fn with_stub_routes(mut self, stub_routes: Vec<StubRoute>) -> Self {
        self.stub_routes = stub_routes;
        self
    }

    /// Carry the check pipeline's repair-route outcome to the renderer
    /// (#8729): a route prints the start command, a refusal prints the
    /// advisory naming why, and silence prints nothing.
    pub(crate) fn with_repair_route_outcome(mut self, outcome: CheckRepairRoute) -> Self {
        match outcome {
            CheckRepairRoute::Route(route) => self.repair_route = Some(route),
            CheckRepairRoute::GapRoute(route) => self.gap_repair_route = Some(route),
            CheckRepairRoute::Refused(refusal) => self.repair_route_refusal = Some(refusal),
            CheckRepairRoute::Silent => {}
        }
        self
    }

    /// The named refusal for the check screen's selected finding, when the
    /// pipeline computed one (#8729).
    pub(crate) fn repair_route_refusal(&self) -> Option<&RepairRouteRefusal> {
        self.repair_route_refusal.as_ref()
    }

    /// The repair route for `finding_id`, when one was computed for that
    /// finding.
    pub(crate) fn repair_route_for(&self, finding_id: &str) -> Option<&RepairRoute> {
        self.repair_route
            .as_ref()
            .filter(|route| route.finding_id == finding_id)
    }

    /// The gap-bound repair route for `finding_id`, when one was computed.
    pub(crate) fn gap_repair_route_for(&self, finding_id: &str) -> Option<&GapRepairRoute> {
        self.gap_repair_route
            .as_ref()
            .filter(|route| route.finding_id == finding_id)
    }

    /// The gap-bound `ripr agent repair` start (#9103): the canonical gap and
    /// the diff base replace the seam identity, then `--root` as `repair`.
    pub(crate) fn gap_repair_command(&self, route: &GapRepairRoute) -> String {
        format!(
            "ripr agent repair --gap-id {} --base {}{} --phase before",
            shell_arg(&route.gap_id),
            shell_arg(&route.base),
            self.repair_suffix
        )
    }

    /// The commit step that must precede the gap-bound start when check
    /// analyzed uncommitted production edits (#10698): the start refuses
    /// while tracked production files differ from HEAD.
    pub(crate) fn gap_repair_prerequisite(&self, route: &GapRepairRoute) -> Option<String> {
        route.commit_prerequisite(&self.root_display)
    }

    /// The `ripr repair` start for one seam identity. Repair reads the live
    /// tree at the bound root, so unlike explain/context it carries no
    /// diff scope: the item comes first, then `--root`.
    pub(crate) fn repair_command(&self, seam_id: &str) -> String {
        format!("ripr repair {}{}", shell_arg(seam_id), self.repair_suffix)
    }

    /// The stub-route decision for `finding_id`, when one was computed for
    /// that finding.
    pub(crate) fn stub_route_for(&self, finding_id: &str) -> Option<&StubRouteDecision> {
        self.stub_routes
            .iter()
            .find(|route| route.finding_id == finding_id)
            .map(|route| &route.decision)
    }

    pub(crate) fn explain_command(&self, selector: &str) -> String {
        format!("{} {}", self.explain_prefix, shell_arg(selector))
    }

    pub(crate) fn context_command(&self, selector: &str) -> String {
        format!("{} --at {}", self.context_prefix, shell_arg(selector))
    }

    /// The one-step `ripr agent stub` route from a finding location to a
    /// runnable test (#5355). It reads the working tree at the same root.
    /// `kind` is the finding's probe family, so the stub targets the seam
    /// the finding reported when its line holds several (#5471).
    pub(crate) fn stub_seam_command(&self, seam_id: &str) -> String {
        format!("{} --seam-id {}", self.stub_prefix, shell_arg(seam_id))
    }

    pub(crate) fn stub_command(&self, file: &str, line: usize, kind: &str) -> String {
        format!(
            "{} --at {} --kind {}",
            self.stub_prefix,
            shell_arg(&format!("{file}:{line}")),
            shell_arg(kind)
        )
    }

    /// The `ripr check --json` command that lists finding ids for the same
    /// scope (root, base or diff, `--worktree`, mode), so a selector miss
    /// recovers against the analysis it asked about. Only meaningful for a
    /// fresh scope: `ripr check` has no `--from`.
    pub(crate) fn list_command(&self) -> String {
        format!("{} --json", self.list_prefix)
    }
}

/// The drill-in guidance the human surfaces print (#4321). The block must
/// never vanish silently: every check, including a `--worktree` run without
/// an artifact, prints sibling commands that replay its own input identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FindingDrillIn {
    /// Copy-pasteable `ripr explain` / `ripr context` commands that preserve
    /// this run's input identity (`--worktree` included).
    Commands(FindingNavigation),
}

/// Build sibling commands that preserve the input identity needed to replay a
/// finding. An artifact is authoritative for its diff source; otherwise the
/// explicit diff or base is carried forward.
pub(crate) fn finding_navigation(
    input: &CheckInput,
    artifact_path: Option<&Path>,
    mode_explicit: bool,
) -> FindingNavigation {
    finding_navigation_with_worktree(input, artifact_path, mode_explicit, false)
}

/// [`finding_navigation`] for a run that may have analyzed the working tree:
/// `worktree` carries `--worktree` forward so the drill-in commands see the
/// same uncommitted edits as the check that listed the finding.
pub(crate) fn finding_navigation_with_worktree(
    input: &CheckInput,
    artifact_path: Option<&Path>,
    mode_explicit: bool,
    worktree: bool,
) -> FindingNavigation {
    let args = navigation_args(input, artifact_path, mode_explicit, worktree);
    // `ripr check` has no `--from`: the listing always re-runs the original
    // scope, even when the drill-in commands replay a written artifact.
    let list_args = navigation_args(input, None, mode_explicit, worktree);
    FindingNavigation {
        explain_prefix: format!("ripr explain {args}"),
        context_prefix: format!("ripr context {args}"),
        list_prefix: format!("ripr check {list_args}"),
        stub_prefix: format!(
            "ripr agent stub --root {}",
            shell_arg(&bound_root(&input.root.display().to_string()))
        ),
        repair_suffix: format!(
            " --root {}",
            shell_arg(&bound_root(&input.root.display().to_string()))
        ),
        root_display: bound_root(&input.root.display().to_string()),
        stub_routes: Vec::new(),
        repair_route: None,
        gap_repair_route: None,
        repair_route_refusal: None,
    }
}

/// A `--diff`, `--from` or `--perl-facts` input resolves against the process
/// working directory, independently of `--root`, so a drill-in pasted from another
/// directory would read a different file (or none). The stdin sentinel `-`
/// stays as typed.
fn bound_input_file(path: &Path) -> String {
    if path == Path::new("-") {
        return "-".to_string();
    }
    root_path_display(&bound_root_path(path))
}

fn navigation_args(
    input: &CheckInput,
    artifact_path: Option<&Path>,
    mode_explicit: bool,
    worktree: bool,
) -> String {
    // The drill-in is pasted after the listing, often from another directory,
    // so it names the repository the check resolved, not the typed relative
    // spelling (#3948).
    let mut args = vec![format!(
        "--root {}",
        shell_arg(&bound_root(&input.root.display().to_string()))
    )];

    if let Some(artifact_path) = artifact_path {
        args.push(format!(
            "--from {}",
            shell_arg(&bound_input_file(artifact_path))
        ));
    } else if let Some(diff_file) = input.diff_file.as_deref() {
        args.push(format!(
            "--diff {}",
            shell_arg(&bound_input_file(diff_file))
        ));
    } else {
        if let Some(base) = input.base.as_deref() {
            args.push(format!("--base {}", shell_arg(base)));
        }
        if worktree {
            args.push("--worktree".to_string());
        } else if input.git_candidate.is_none() {
            // All live CLI siblings default to the dirty worktree. A command
            // for committed input must therefore carry its explicit source.
            args.push("--committed".to_string());
        }
    }

    if mode_explicit || input.mode != Mode::Draft {
        args.push(format!("--mode {}", shell_arg(input.mode.as_str())));
    }
    if !input.include_unchanged_tests {
        args.push("--no-unchanged-tests".to_string());
    }
    if let Some(perl_facts_path) = input.perl_facts_path.as_deref() {
        args.push(format!(
            "--perl-facts {}",
            shell_arg(&bound_input_file(perl_facts_path))
        ));
    }
    if let Some(suppression_policy) = input.suppression_policy.as_deref() {
        args.push(format!(
            "--suppression-policy {}",
            shell_arg(&suppression_policy.display().to_string())
        ));
    }

    args.join(" ")
}

#[cfg(test)]
mod tests {
    use super::super::task_first::RefusalKind;
    use super::*;
    use std::path::PathBuf;

    /// #10698: the printed commit step works from an unrelated working
    /// directory and commits a tracked file whose name looks like a glob.
    #[test]
    fn commit_prerequisite_commits_literal_paths_from_a_decoy_directory() -> Result<(), String> {
        use crate::testing::fixture_git::fixture_git_ok as run_git;

        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| format!("test clock failed: {error}"))?
            .as_nanos();
        let base =
            std::env::temp_dir().join(format!("ripr-commit-hint-{}-{stamp}", std::process::id()));
        let root = base.join("repo with space");
        let decoy = base.join("decoy");
        std::fs::create_dir_all(root.join("src")).map_err(|error| format!("mkdir: {error}"))?;
        std::fs::create_dir_all(&decoy).map_err(|error| format!("mkdir: {error}"))?;
        let result = (|| -> Result<(), String> {
            run_git(&root, &["init"])?;
            run_git(
                &root,
                &["config", "user.email", "ripr-test@example.invalid"],
            )?;
            run_git(&root, &["config", "user.name", "RIPR Test"])?;
            for file in ["src/[ab].py", "src/a.py", "src/b.py"] {
                std::fs::write(root.join(file), "x = 1\n").map_err(|e| format!("write: {e}"))?;
            }
            run_git(&root, &["add", "."])?;
            run_git(&root, &["commit", "--no-gpg-sign", "-m", "initial"])?;
            for file in ["src/[ab].py", "src/a.py", "src/b.py"] {
                std::fs::write(root.join(file), "x = 2\n").map_err(|e| format!("write: {e}"))?;
            }
            let route = GapRepairRoute {
                finding_id: "f".to_string(),
                gap_id: "gap-1".to_string(),
                base: "HEAD".to_string(),
                uncommitted_files: vec!["src/[ab].py".to_string()],
            };
            let root_text = root.display().to_string();
            let command = route
                .commit_prerequisite(&root_text)
                .ok_or("no commit step")?;
            // The root anchors at the analyzed tree with the shared quoting
            // (`shell_arg`: console quotes on Windows, single quotes on
            // Unix), and the glob-lookalike pathspec stays literal — the
            // quote form is owned by `shell_arg`, not spelled here (#10996).
            let expected_prefix = format!("git -C {}", shell_arg(&root_text));
            assert!(
                command.starts_with(&expected_prefix)
                    && command.contains("--literal-pathspecs commit"),
                "Expected command to start with '{expected_prefix}' and contain '--literal-pathspecs commit'. Actual command: {command}"
            );
            let expected_suffix = format!("-- {}", shell_arg("src/[ab].py"));
            assert!(
                command.ends_with(&expected_suffix),
                "Expected command to end with '{expected_suffix}'. Actual command: {command}"
            );
            let command = command.replace("<message>", "hint");
            let status = std::process::Command::new("sh")
                .arg("-c")
                .arg(&command)
                .current_dir(&decoy)
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@example.invalid")
                .output()
                .map_err(|error| format!("run commit: {error}"))?;
            assert!(
                status.status.success(),
                "{command}: {}",
                String::from_utf8_lossy(&status.stderr)
            );
            let porcelain = std::process::Command::new("git")
                .arg("-C")
                .arg(&root)
                .args(["status", "--porcelain"])
                .output()
                .map_err(|error| format!("status: {error}"))?;
            let text = String::from_utf8_lossy(&porcelain.stdout).into_owned();
            // Only the literal file was committed; the glob siblings stay dirty.
            assert!(!text.contains("[ab].py"), "{text}");
            assert!(
                text.contains("src/a.py") && text.contains("src/b.py"),
                "{text}"
            );
            Ok(())
        })();
        let _ = std::fs::remove_dir_all(&base);
        result
    }

    /// The `--root` value a drill-in prints for a typed root: the resolved
    /// repository, quoted for the shell.
    fn bound(root: &str) -> String {
        shell_arg(&bound_root(root))
    }

    /// The `--diff` / `--from` value a drill-in prints for a typed file.
    fn bound_file(path: &str) -> String {
        shell_arg(&bound_input_file(Path::new(path)))
    }

    #[test]
    fn finding_navigation_preserves_diff_and_quotes_dynamic_values() {
        let input = CheckInput {
            root: PathBuf::from("repo root"),
            diff_file: Some(PathBuf::from("change set.diff")),
            ..CheckInput::default()
        };
        let navigation = finding_navigation(&input, None, false);

        let root = bound("repo root");
        let diff = bound_file("change set.diff");
        assert_eq!(
            navigation.explain_command("probe:src/lib.rs:error_path:abc123"),
            format!("ripr explain --root {root} --diff {diff} probe:src/lib.rs:error_path:abc123")
        );
        assert_eq!(
            navigation.context_command("probe:src/lib.rs:error_path:abc123"),
            format!(
                "ripr context --root {root} --diff {diff} --at probe:src/lib.rs:error_path:abc123"
            )
        );
    }

    #[test]
    fn finding_navigation_prefers_artifact_identity_over_diff_source() {
        let input = CheckInput {
            root: PathBuf::from("repo"),
            diff_file: Some(PathBuf::from("old.diff")),
            mode: Mode::Ready,
            ..CheckInput::default()
        };
        let navigation = finding_navigation(&input, Some(Path::new("saved artifact.json")), false);

        assert_eq!(
            navigation.explain_command("probe:id"),
            format!(
                "ripr explain --root {} --from {} --mode ready probe:id",
                bound("repo"),
                bound_file("saved artifact.json")
            )
        );
    }

    /// `check --worktree` lists findings from uncommitted edits; its drill-in
    /// commands must analyze the same edits, not the committed history.
    #[test]
    fn finding_navigation_carries_worktree_scope_after_the_base() {
        let input = CheckInput {
            base: Some("HEAD".to_string()),
            ..CheckInput::default()
        };
        let navigation = finding_navigation_with_worktree(&input, None, false, true);
        assert_eq!(
            navigation.explain_command("src/calc.py:5"),
            format!(
                "ripr explain --root {} --base HEAD --worktree src/calc.py:5",
                bound(".")
            )
        );
        assert_eq!(
            navigation.context_command("src/calc.py:5"),
            format!(
                "ripr context --root {} --base HEAD --worktree --at src/calc.py:5",
                bound(".")
            )
        );
        // A selector miss lists ids from the same worktree scope.
        assert_eq!(
            navigation.list_command(),
            format!(
                "ripr check --root {} --base HEAD --worktree --json",
                bound(".")
            )
        );
        // An artifact already records the worktree diff; `--from` wins.
        let from_artifact =
            finding_navigation_with_worktree(&input, Some(Path::new("wt.json")), false, true);
        assert_eq!(
            from_artifact.explain_command("probe:id"),
            format!(
                "ripr explain --root {} --from {} probe:id",
                bound("."),
                bound_file("wt.json")
            )
        );
        // `ripr check` has no `--from`: the listing re-runs the worktree scope.
        assert_eq!(
            from_artifact.list_command(),
            format!(
                "ripr check --root {} --base HEAD --worktree --json",
                bound(".")
            )
        );
    }

    /// #8166 Slice C: the repair command names the seam item first and the
    /// bound root after it (`ripr repair [<item>] [--root PATH]`), with no
    /// diff scope carried over from the explain/context prefixes.
    #[test]
    fn finding_navigation_repair_command_names_item_then_bound_root() {
        let input = CheckInput {
            root: PathBuf::from("repo"),
            base: Some("HEAD".to_string()),
            ..CheckInput::default()
        };
        let navigation = finding_navigation_with_worktree(&input, None, false, true);
        assert_eq!(
            navigation.repair_command("67fc764ba37d77bd"),
            format!("ripr repair 67fc764ba37d77bd --root {}", bound("repo"))
        );
        assert_eq!(
            FindingNavigation::legacy().repair_command("67fc764ba37d77bd"),
            "ripr repair 67fc764ba37d77bd"
        );
        // The route only answers for its own finding.
        let routed = navigation.with_repair_route_outcome(CheckRepairRoute::Route(RepairRoute {
            uncommitted_target: None,
            finding_id: "probe:one".to_string(),
            seam_id: "67fc764ba37d77bd".to_string(),
        }));
        assert_eq!(
            routed
                .repair_route_for("probe:one")
                .map(|route| route.seam_id.as_str()),
            Some("67fc764ba37d77bd")
        );
        assert!(routed.repair_route_for("probe:two").is_none());
    }

    /// #8729: a named refusal renders a one-line advisory naming the state
    /// and the full-gate recovery where one helps; the seam-budget arm names its own
    /// recovery because the full gate re-checks the same truncated
    /// authority.
    #[test]
    fn repair_route_refusal_advisory_names_state_and_recovery() {
        let gate = "ripr repair probe:one --root repo";
        // The ineligible arm still routes to the full gate for its answer.
        let ineligible = RepairRouteRefusal::Ineligible {
            reason: "not a headline repair target".to_string(),
            kind: RefusalKind::Other,
            missing: Vec::new(),
            boundary_input_observed: false,
        }
        .advisory(gate);
        assert!(
            ineligible.starts_with("no repair start offered: ")
                && ineligible.contains("not a headline repair target")
                && ineligible.contains(gate)
                && ineligible.contains("for the full gate's answer"),
            "advisory must name the state and the full-gate recovery: {ineligible}"
        );
        // #9244: a not-ready route states what the static evidence still
        // lacks and the rerun, not "not ready" plus a gate pointer.
        let not_ready = RepairRouteRefusal::Ineligible {
            reason: "repair route not ready (missing evidence: boundary_value; activation)"
                .to_string(),
            kind: RefusalKind::RouteNotReady,
            missing: vec!["boundary_value".to_string(), "activation".to_string()],
            boundary_input_observed: false,
        }
        .advisory(gate);
        assert_eq!(
            not_ready,
            "no repair start offered: ripr cannot build a repair packet for this finding yet \
             because it is still missing: boundary_value; activation; follow the Next step \
             above, then rerun `ripr check`"
        );
        // #10622: a boundary input a related test already supplies is not "missing".
        let internal = "producer-owned missing discriminator fact: boundary_value".to_string();
        // Discriminating negative: boundary input not observed keeps the
        // producer-owned wording.
        let unobserved = RepairRouteRefusal::Ineligible {
            reason: "repair route not ready".to_string(),
            kind: RefusalKind::RouteNotReady,
            missing: vec![internal.clone()],
            boundary_input_observed: false,
        }
        .advisory(gate);
        assert!(
            unobserved.contains(
                "still missing: a test assertion that pins the changed boundary value exactly"
            ) && !unobserved.contains("producer-owned missing discriminator fact"),
            "unobserved boundary renders the plain phrase, not the internal code: {unobserved}"
        );
        let observed = RepairRouteRefusal::Ineligible {
            reason: "repair route not ready".to_string(),
            kind: RefusalKind::RouteNotReady,
            missing: vec![internal, "safe test target".to_string()],
            boundary_input_observed: true,
        }
        .advisory(gate);
        assert_eq!(
            observed,
            "no repair start offered: a related test already reaches the changed boundary \
             input, but ripr grades its assertion as a weak check, so ripr cannot build a \
             repair packet for this finding yet. It is also still missing: \
             safe test target. Follow the Next step above, then rerun `ripr check`"
        );
        assert!(
            !not_ready.contains("repair route not ready")
                && !not_ready.contains("ripr repair")
                && !not_ready.contains("full gate's answer"),
            "not-ready must not restate the refusal or route to the gate: {not_ready}"
        );
        let no_missing = RepairRouteRefusal::Ineligible {
            reason: "repair route not ready".to_string(),
            kind: RefusalKind::RouteNotReady,
            missing: Vec::new(),
            boundary_input_observed: false,
        }
        .advisory(gate);
        assert!(
            no_missing.contains("not complete enough to name a safe test target")
                && !no_missing.contains("full gate's answer"),
            "empty missing list must say the evidence is incomplete: {no_missing}"
        );
        // #9244: a refusal that is not a not-ready route (here the gate could
        // not decide) keeps its reason and the gate pointer.
        let unverified = RepairRouteRefusal::Ineligible {
            reason: "the gate could not decide".to_string(),
            kind: RefusalKind::Unverified,
            missing: vec!["ignored".to_string()],
            boundary_input_observed: false,
        }
        .advisory(gate);
        assert!(
            unverified.contains("the gate could not decide")
                && unverified.contains(gate)
                && !unverified.contains("still missing"),
            "non-RouteNotReady kinds keep their reason and gate pointer: {unverified}"
        );
        // A TypeScript/JavaScript preview finding names its packet state, not
        // a Rust seam it can never have.
        let preview = RepairRouteRefusal::PreviewPacketNotComplete {
            language: "TypeScript".to_string(),
            reason: "its repair packet is not complete".to_string(),
        }
        .advisory(gate);
        assert!(
            preview.starts_with("no repair start offered: ripr has no complete repair packet")
                && preview.contains("TypeScript finding")
                && !preview.contains("no seam"),
            "preview refusal must name the packet, not a seam: {preview}"
        );
        // #9244: a finding with no canonical gap would get the same refusal
        // from the full gate, so the advisory names its own recovery.
        let no_gap = RepairRouteRefusal::NoCanonicalGap.advisory(gate);
        assert_eq!(
            no_gap,
            "no repair start offered: this finding has no gap ID, so there is no \
             gap to start a repair from; follow the Next step above"
        );
        assert!(
            !no_gap.contains("ripr repair") && !no_gap.contains("full gate's answer"),
            "no-canonical-gap must not route back to the gate: {no_gap}"
        );
        // #9244: with no binding seam the advisory names the Next step
        // instead of routing back to the gate.
        let unbound = RepairRouteRefusal::NoBindingSeam { seams_in_file: 0 }.advisory(gate);
        assert_eq!(
            unbound,
            "no repair start offered: ripr found no spot in this finding's file to bind it \
             to, so there is no spot to start a repair from; follow the Next step above"
        );
        assert!(
            !unbound.contains("ripr repair") && !unbound.contains("full gate's answer"),
            "no-binding-seam must not route back to the gate: {unbound}"
        );
        // #10702: seams exist in the file, so the advisory must not claim there are none.
        let counted = RepairRouteRefusal::NoBindingSeam { seams_in_file: 3 }.advisory(gate);
        assert!(
            counted.contains("found 3 spots in this finding's file")
                && !counted.contains("no seam in this finding's file")
                && !counted.contains("found no seam"),
            "{counted}"
        );
        let one = RepairRouteRefusal::NoBindingSeam { seams_in_file: 1 }.advisory(gate);
        assert_eq!(
            one,
            "no repair start offered: ripr found 1 spot in this finding's file, but could not \
             tie one to this finding's changed line, so it cannot pick one for you; `ripr \
             pilot` lists them, or follow the Next step above"
        );
        let exposed = RepairRouteRefusal::FindingAlreadyExposed.advisory(gate);
        assert_eq!(
            exposed,
            "no repair start offered: nothing to repair here; the related tests already \
             notice this change (exposed)"
        );
        let disagree = RepairRouteRefusal::SeamGrippedFindingNot.advisory(gate);
        assert!(
            disagree.contains("`ripr check` reads a gap or limit")
                && disagree.contains("conflicting evidence")
                && !disagree.contains("no static gap remains"),
            "{disagree}"
        );
        let config = RepairRouteRefusal::ConfigUnreadable.advisory(gate);
        assert!(
            config.starts_with("no repair start offered: ")
                && config.contains("config could not be read")
                && config.contains("ripr.toml")
                && config.contains("rerun `ripr check`")
                && !config.contains("full gate's answer"),
            "config refusal must name the file and the rerun: {config}"
        );
        let classification = RepairRouteRefusal::ClassificationFailed.advisory(gate);
        assert!(
            classification.contains("classifying the finding's seams failed")
                && classification.contains(&format!("`{gate}` may print the underlying error"))
                && !classification.contains("full gate's answer"),
            "classification refusal must say what the gate yields: {classification}"
        );
        let several = RepairRouteRefusal::SeveralBindingSeams.advisory(gate);
        assert!(
            several.contains("several seams bind")
                && several.contains("needs the ID of the seam you choose")
                && !several.contains("ripr pilot")
                && !several.contains("full gate's answer"),
            "several-seams refusal must not promise a seam listing: {several}"
        );
        // #9102: a non-Rust finding has no seam to bind, so the full gate
        // would repeat the refusal; the advisory names the language, file
        // and manual route instead of routing back to it.
        let terminal = RepairRouteRefusal::NoSeamInventory {
            language: "Python".to_string(),
            file: "pricing/core.py".to_string(),
        }
        .advisory(gate);
        assert!(
            terminal.starts_with("no repair start offered: ")
                && terminal.contains("covers Rust files only")
                && terminal.contains("Python finding in `pricing/core.py`")
                && terminal.contains("manual route")
                && terminal.contains("no spot to start a repair from")
                && terminal.contains("add the missing check to that test")
                && !terminal.contains("seam")
                && !terminal.contains("discriminator")
                && !terminal.contains("ripr repair probe:one"),
            "terminal reason must name language, file and manual route: {terminal}"
        );
        let budget = RepairRouteRefusal::SeamLimit.advisory(gate);
        assert!(
            budget.contains("seam budget") && budget.contains("wider seam budget"),
            "seam-budget refusal must name the budget recovery: {budget}"
        );
        assert!(
            !budget.contains("ripr repair"),
            "seam-budget refusal must not route to the same truncated gate: {budget}"
        );
    }

    #[test]
    fn finding_navigation_preserves_explicit_draft_mode() {
        // #3952: the default carries no base, so name the base explicitly;
        // the test pins mode preservation, not the old default.
        let input = CheckInput {
            base: Some("origin/main".to_string()),
            ..CheckInput::default()
        };
        let navigation = finding_navigation(&input, None, true);

        assert_eq!(
            navigation.explain_command("probe:id"),
            format!(
                "ripr explain --root {} --base origin/main --committed --mode draft probe:id",
                bound(".")
            )
        );
    }

    /// #3948: a relative typed root must not be repeated as typed. Pasted from
    /// another directory it would name whatever sits at that relative path
    /// there, so every drill-in (explain, context, list, stub) carries the
    /// resolved repository.
    #[test]
    fn finding_navigation_binds_a_relative_root_for_every_drill_in() {
        let input = CheckInput {
            root: PathBuf::from("nested/repo"),
            base: Some("HEAD".to_string()),
            ..CheckInput::default()
        };
        let navigation = finding_navigation(&input, None, false);
        let resolved = bound_root("nested/repo");
        assert!(
            Path::new(&resolved).is_absolute(),
            "bound root must be absolute: {resolved}"
        );
        let root_arg = format!("--root {}", shell_arg(&resolved));
        for command in [
            navigation.explain_command("probe:id"),
            navigation.context_command("probe:id"),
            navigation.list_command(),
            navigation.stub_command("src/lib.rs", 5, "predicate"),
        ] {
            assert!(
                command.contains(&root_arg),
                "drill-in must carry the resolved root: {command}"
            );
            assert!(
                !command.contains("--root nested/repo"),
                "drill-in repeated the typed relative root: {command}"
            );
        }
    }

    /// #3948 (Devin on #6759): `--diff`, `--from` and `--perl-facts` resolve
    /// against the process directory, not `--root`, so the drill-in names
    /// them absolute; the stdin sentinel stays `-`.
    #[test]
    fn finding_navigation_binds_relative_input_files_and_keeps_the_stdin_sentinel() {
        let input = CheckInput {
            root: PathBuf::from("repo"),
            diff_file: Some(PathBuf::from("changes.patch")),
            perl_facts_path: Some(PathBuf::from("facts.json")),
            ..CheckInput::default()
        };
        let command = finding_navigation(&input, None, false).explain_command("probe:id");
        let facts = bound_input_file(Path::new("facts.json"));
        assert!(
            Path::new(&facts).is_absolute()
                && command.contains(&format!("--perl-facts {}", shell_arg(&facts))),
            "relative --perl-facts must be bound: {command}"
        );
        let diff = bound_input_file(Path::new("changes.patch"));
        assert!(
            Path::new(&diff).is_absolute()
                && command.contains(&format!("--diff {}", shell_arg(&diff))),
            "relative --diff must be bound: {command}"
        );
        assert!(
            !command.contains("--diff changes.patch"),
            "drill-in repeated the typed relative --diff: {command}"
        );

        let stdin = CheckInput {
            diff_file: Some(PathBuf::from("-")),
            ..CheckInput::default()
        };
        let command = finding_navigation(&stdin, None, false).explain_command("probe:id");
        assert!(
            command.contains("--diff -"),
            "stdin sentinel must stay `-`: {command}"
        );
    }

    /// #7257: selecting provenance from `worktree_explicitly_provided` alone
    /// labels a dirty-default run `CommittedHistory` while analysis used the
    /// working tree. The effective-source helper must disagree with that
    /// flag-only selector on this case.
    #[test]
    fn worktree_flag_alone_misattributes_a_dirty_default_run() {
        let worktree_explicitly_provided = false;
        let worktree_run = true;
        let supplied_scope = false;
        let flag_only = if worktree_explicitly_provided {
            CheckDiffProvenance::Worktree
        } else if supplied_scope {
            CheckDiffProvenance::SuppliedScope
        } else {
            CheckDiffProvenance::CommittedHistory
        };
        assert_eq!(flag_only, CheckDiffProvenance::CommittedHistory);
        assert_eq!(
            CheckDiffProvenance::from_effective_source(worktree_run, supplied_scope),
            CheckDiffProvenance::Worktree
        );
        // Explicit `--committed` and a clean default stay committed.
        assert_eq!(
            CheckDiffProvenance::from_effective_source(false, false),
            CheckDiffProvenance::CommittedHistory
        );
        // `--diff` / `--candidate-tree` cannot become live-tree claims.
        assert_eq!(
            CheckDiffProvenance::from_effective_source(false, true),
            CheckDiffProvenance::SuppliedScope
        );
        // Explicit `--worktree` agrees with the dirty default.
        assert_eq!(
            CheckDiffProvenance::from_effective_source(true, false),
            CheckDiffProvenance::Worktree
        );
    }
}

//! The current change, as pilot sees it.
//!
//! Pilot ranks seams across the whole repository. A developer who just ran
//! `ripr check` on a branch reads pilot's top recommendation as the next step
//! for that change, so pilot names whether the recommendation is part of it
//! and ranks seams on changed lines first. The diff itself comes from the
//! shared diff loaders (selected in the pilot CLI adapter: base against the
//! working tree when it has uncommitted tracked changes, else
//! `<base>...HEAD`); this module only maps its changed new-side lines onto
//! seams.

use crate::analysis::{
    ClassifiedChangeReport, ClassifiedSeam, DiffOnlySource, RepoSeam, SeamLimitInfo,
};
use crate::analysis_outcome::AnalysisLimitation;
use crate::output::path::display_path;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// What pilot knows about the current change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PilotCurrentChange {
    /// The current-change diff is non-empty.
    Changed {
        root: PathBuf,
        /// The base the diff was taken against, when the loader reported one.
        base: Option<String>,
        /// Root-relative path (slash separated) to changed new-side lines.
        lines: BTreeMap<String, BTreeSet<usize>>,
        /// The diff was taken against the live working tree, so `ripr check`
        /// sees it only with `--worktree`; its default reads committed
        /// history (RIPR-SPEC-0112).
        working_tree: bool,
        /// The analyzed seams on changed lines, counted before the pilot
        /// budget cut, which drops the ones pilot cannot recommend.
        seams: ChangeSeams,
        /// Changed files diff analysis covers but the repo inventory leaves
        /// out by design (build scripts, `xtask/`, roots outside `src`),
        /// root-relative and slash separated (#6944).
        diff_only: Vec<(String, DiffOnlySource)>,
        /// Coverage recorded by a successful changed-file supplement; absent
        /// when classification failed or did not run.
        absent_file_limitations: Option<Vec<AnalysisLimitation>>,
    },
    /// The default diff loaded and is empty: there is no current change.
    NoChange { base: Option<String> },
    /// The default diff could not be loaded (not a Git work tree, no default
    /// base, git failed). Pilot keeps its repo-wide ranking; this is not
    /// evidence that nothing changed. `reason` is a short fixed phrase.
    Unavailable { reason: &'static str },
}

impl PilotCurrentChange {
    /// Build from the default diff loader's result. A load failure never
    /// fails pilot; it is recorded as [`PilotCurrentChange::Unavailable`]
    /// with the caller's short reason.
    pub(crate) fn from_diff_load(
        root: &Path,
        loaded: Result<(String, Option<String>), &'static str>,
    ) -> Self {
        match loaded {
            Ok((text, base)) => Self::from_diff_text(root, base, &text),
            Err(reason) => Self::Unavailable { reason },
        }
    }

    /// Why the change could not be loaded, when it could not.
    pub(crate) fn unavailable_reason(&self) -> Option<&'static str> {
        match self {
            Self::Unavailable { reason } => Some(reason),
            Self::Changed { .. } | Self::NoChange { .. } => None,
        }
    }

    /// Build from unified diff text taken relative to `root`.
    pub(crate) fn from_diff_text(root: &Path, base: Option<String>, text: &str) -> Self {
        if text.trim().is_empty() {
            return Self::NoChange { base };
        }
        let mut lines: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
        for file in crate::analysis::parse_unified_diff(text) {
            let entry = lines.entry(normalized(&file.path)).or_default();
            // Added lines name themselves; a removal is anchored at the
            // new-side position where the removed text used to be.
            entry.extend(file.added_lines.iter().map(|line| line.new_side_line));
            entry.extend(file.removed_lines.iter().map(|line| line.new_side_line));
        }
        Self::Changed {
            root: root.to_path_buf(),
            base,
            lines,
            working_tree: false,
            seams: ChangeSeams::default(),
            diff_only: Vec::new(),
            absent_file_limitations: None,
        }
    }

    /// Count the analyzed seams on changed lines before the pilot budget
    /// cut. Without this, a change whose seams pilot withholds reads as a
    /// change with no seams at all (#5309).
    pub(crate) fn with_seams_counted(
        mut self,
        classified: &[ClassifiedSeam],
        inventory_limit: Option<&SeamLimitInfo>,
    ) -> Self {
        let mut counted = ChangeSeams {
            unanalyzed: inventory_limit
                .map(|limit| (limit.analyzed, limit.total))
                .filter(|(analyzed, total)| analyzed < total),
            ..ChangeSeams::default()
        };
        for entry in classified.iter().filter(|entry| self.touches(entry)) {
            counted.touched = counted.touched.saturating_add(1);
            if entry.class.is_static_limitation() {
                counted.withheld = counted.withheld.saturating_add(1);
            }
        }
        if let Self::Changed { seams, .. } = &mut self {
            *seams = counted;
        }
        self
    }

    /// The change's Rust files, root-relative, for classifying the change on
    /// its own when the inventory seam limit cut it (#6943). Empty without a
    /// change.
    pub(crate) fn changed_rust_files(&self) -> Vec<PathBuf> {
        let Self::Changed { lines, .. } = self else {
            return Vec::new();
        };
        // Keys are stable path text (`%` escapes); the inventory compares
        // raw path bytes, so decode them back to the on-disk spelling.
        lines
            .keys()
            .filter(|path| path.ends_with(".rs"))
            .map(|path| crate::analysis::decode_stable_path_text(path))
            .collect()
    }

    /// Fold the change's own classification (run when the inventory seam
    /// limit fired on a change with Rust files) into the ranked population:
    /// the cut seams on changed lines join `classified` and count as
    /// analyzed in `limit`. Returns how many were added and the limit the
    /// change's seam-limit caveat should cite: none after successful scoped
    /// classification (named absence replaces it for missing files), or when
    /// the change has no Rust file the limit could
    /// hide; the inventory's limit when the classification failed or never
    /// ran.
    pub(crate) fn fold_classified_change<'a>(
        &self,
        classified: &mut Vec<ClassifiedSeam>,
        limit: &'a mut Option<SeamLimitInfo>,
        change: Option<Result<ClassifiedChangeReport, String>>,
    ) -> ChangeClassification<'a> {
        match change {
            Some(Ok(scoped)) => {
                let added = self.add_cut_seams(classified, scoped.classified);
                if let Some(info) = limit.as_mut() {
                    info.analyzed = info.analyzed.saturating_add(added);
                    // Every seam the limit cut was on the change: nothing
                    // is left unanalyzed, so no limit applies.
                    if info.analyzed >= info.total {
                        *limit = None;
                    }
                }
                ChangeClassification {
                    added,
                    caveat_limit: None,
                    absent_file_limitations: Some(scoped.absent_file_limitations),
                    error: None,
                }
            }
            Some(Err(error)) => ChangeClassification {
                added: 0,
                caveat_limit: limit.as_ref(),
                absent_file_limitations: None,
                error: Some(error),
            },
            None => ChangeClassification {
                added: 0,
                caveat_limit: limit
                    .as_ref()
                    .filter(|_| !self.is_changed() || !self.changed_rust_files().is_empty()),
                absent_file_limitations: None,
                error: None,
            },
        }
    }

    /// Append the seams in `scoped` that sit on a changed line and are not
    /// already in `classified`, returning how many were added. Seams
    /// elsewhere in a changed file stay out, so the cut repo-wide population
    /// the rest of the ranking sees does not tilt toward the changed files.
    pub(crate) fn add_cut_seams(
        &self,
        classified: &mut Vec<ClassifiedSeam>,
        scoped: Vec<ClassifiedSeam>,
    ) -> usize {
        let known: std::collections::HashSet<_> = classified
            .iter()
            .map(|entry| entry.seam.id().clone())
            .collect();
        let before = classified.len();
        classified.extend(
            scoped
                .into_iter()
                .filter(|entry| self.touches(entry) && !known.contains(entry.seam.id())),
        );
        classified.len() - before
    }

    /// The analyzed seams on changed lines, when there is a change.
    pub(crate) fn seams(&self) -> Option<ChangeSeams> {
        match self {
            Self::Changed { seams, .. } => Some(*seams),
            Self::NoChange { .. } | Self::Unavailable { .. } => None,
        }
    }

    /// Mark a change whose diff came from the live working tree.
    pub(crate) fn with_working_tree(mut self, from_working_tree: bool) -> Self {
        if let Self::Changed { working_tree, .. } = &mut self {
            *working_tree = from_working_tree;
        }
        self
    }

    /// Record the changed files only diff analysis covers (#6944).
    pub(crate) fn with_diff_only_files(mut self, files: Vec<(PathBuf, DiffOnlySource)>) -> Self {
        if let Self::Changed { diff_only, .. } = &mut self {
            *diff_only = files
                .into_iter()
                .map(|(path, source)| (normalized(&path), source))
                .collect();
        }
        self
    }

    /// Replace the supplement's recorded coverage, including an empty recovery.
    pub(crate) fn with_absent_file_limitations(
        mut self,
        limitations: Option<Vec<AnalysisLimitation>>,
    ) -> Self {
        if let Self::Changed {
            absent_file_limitations,
            ..
        } = &mut self
        {
            *absent_file_limitations = limitations;
        }
        self
    }

    /// Canonical absent-file disclosures, or no successful supplement record.
    pub(crate) fn absent_file_limitations(&self) -> Option<&[AnalysisLimitation]> {
        match self {
            Self::Changed {
                absent_file_limitations,
                ..
            } => absent_file_limitations.as_deref(),
            Self::NoChange { .. } | Self::Unavailable { .. } => None,
        }
    }

    /// The changed files only diff analysis covers; empty without a change.
    pub(crate) fn diff_only_files(&self) -> &[(String, DiffOnlySource)] {
        match self {
            Self::Changed { diff_only, .. } => diff_only,
            Self::NoChange { .. } | Self::Unavailable { .. } => &[],
        }
    }

    /// The `ripr check` flags that select this same diff.
    pub(crate) fn check_selector(&self) -> &'static str {
        match self {
            Self::Changed {
                working_tree: true, ..
            } => " --worktree",
            _ => "",
        }
    }

    /// Stable JSON state value.
    pub(crate) fn state(&self) -> &'static str {
        match self {
            Self::Changed { .. } => "changed",
            Self::NoChange { .. } => "no_change",
            Self::Unavailable { .. } => "unavailable",
        }
    }

    pub(crate) fn base(&self) -> Option<&str> {
        match self {
            Self::Changed { base, .. } | Self::NoChange { base } => base.as_deref(),
            Self::Unavailable { .. } => None,
        }
    }

    pub(crate) fn is_changed(&self) -> bool {
        matches!(self, Self::Changed { .. })
    }

    /// Whether the seam budget keeps this seam past its cut: pilot can
    /// recommend it and it is on a changed line. A changed seam pilot cannot
    /// recommend would only displace an actionable one.
    pub(crate) fn keeps_past_budget(&self, entry: &ClassifiedSeam) -> bool {
        super::ranking::is_actionable(entry) && self.touches(entry)
    }

    /// Whether a changed new-side line falls inside the seam's source span
    /// (its display line through the last line of its expression text).
    pub(crate) fn touches(&self, entry: &ClassifiedSeam) -> bool {
        self.touches_seam(&entry.seam)
    }

    /// [`Self::touches`] for a seam not yet classified, so the change's own
    /// classification can skip evidence for seams off the changed lines.
    pub(crate) fn touches_seam(&self, seam: &RepoSeam) -> bool {
        let Self::Changed { root, lines, .. } = self else {
            return false;
        };
        let file = seam.file();
        let relative = file.strip_prefix(root).unwrap_or(file);
        let Some(changed) = lines.get(&normalized(relative)) else {
            return false;
        };
        let start = seam.display_line();
        let end = start + seam.expression().lines().count().saturating_sub(1);
        changed.range(start..=end).next().is_some()
    }
}

/// What folding the change's own classification into the inventory did.
#[derive(Debug)]
pub(crate) struct ChangeClassification<'a> {
    /// Cut seams on changed lines added to the ranked population.
    pub(crate) added: usize,
    /// The seam limit the change's caveat cites, if it still applies.
    pub(crate) caveat_limit: Option<&'a SeamLimitInfo>,
    /// Recorded coverage only after a successful supplement, empty on recovery.
    pub(crate) absent_file_limitations: Option<Vec<AnalysisLimitation>>,
    /// Why the change could not be classified, when it could not.
    pub(crate) error: Option<String>,
}

/// The analyzed seams on a change's lines.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ChangeSeams {
    /// Seams whose span overlaps a changed new-side line.
    pub(crate) touched: usize,
    /// Of those, the ones pilot withholds as static limitations
    /// (`opaque` or an `*_unknown` class).
    pub(crate) withheld: usize,
    /// `(analyzed, total)` when the inventory seam limit left seams
    /// unclassified: the change may have seams among them.
    pub(crate) unanalyzed: Option<(usize, usize)>,
}

/// Slash-separated path without leading `./`, so the seam inventory's paths
/// and the diff's root-relative paths compare equal.
fn normalized(path: &Path) -> String {
    let mut text = display_path(path);
    while let Some(stripped) = text.strip_prefix("./") {
        text = stripped.to_string();
    }
    text
}

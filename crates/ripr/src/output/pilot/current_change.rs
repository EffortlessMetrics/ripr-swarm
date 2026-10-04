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

use crate::analysis::ClassifiedSeam;
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

    /// Whether a changed new-side line falls inside the seam's source span
    /// (its display line through the last line of its expression text).
    pub(crate) fn touches(&self, entry: &ClassifiedSeam) -> bool {
        let Self::Changed { root, lines, .. } = self else {
            return false;
        };
        let file = entry.seam.file();
        let relative = file.strip_prefix(root).unwrap_or(file);
        let Some(changed) = lines.get(&normalized(relative)) else {
            return false;
        };
        let start = entry.seam.display_line();
        let end = start + entry.seam.expression().lines().count().saturating_sub(1);
        changed.range(start..=end).next().is_some()
    }
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

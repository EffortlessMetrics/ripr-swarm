//! One workspace macro-site scan per index (#11509).
//!
//! A single `ripr check` builds several [`OwnerPinSyntax`] instances over the
//! same immutable [`RustIndex`]: the diff pass's candidate index, the seam
//! walk's context, the receiver-pin owner kept in the index memos, and the
//! seam inventory. Each one ran the whole-workspace macro-site scan
//! (`scan_macro_sites`: parse every candidate file, then
//! `macro_binding_ambiguity_sets`, `is_drop_in_assertion` and
//! `binding_free_local_wrapper` per file) for itself, so the same files were
//! parsed up to three times.
//!
//! The scan's output is a pure function of the index: file sources and their
//! stored candidates, the workspace macro scope, the module edges that decide
//! which `mod` declarations resolved, and the target-root recognition derived
//! from the same index. What each instance adds afterwards (withheld crate-root
//! bindings, routing into its own ambiguity sets) happens after the scan and is
//! not shared. This memo therefore holds the raw scan result once, on the index
//! (it dies with the index and is cleared by every index mutation, like the
//! other index-owned memos), and each instance absorbs a clone of it.
//!
//! Fail closed: the memo remembers the exact [`ScanKey`] it was computed under
//! and recomputes when the index no longer matches it, or when the key cannot
//! be taken (a poisoned lock).

use super::{
    MacroScanFile, ScannedMacroSites, TargetRoots, partition_macro_scan_files, scan_macro_sites,
};
use crate::analysis::facts::SourceRoleProvenanceEdge;
use crate::analysis::rust_index::RustIndex;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// Which of the two scan-order partitions a scan covers.
#[derive(Clone, Copy, Debug)]
pub(super) enum ScanPart {
    /// Files that may saturate the ambiguity on their own.
    Likely,
    /// Every other file.
    Rest,
}

/// Everything the scan result depends on, compared by value (no hashing, so
/// no collision can serve a stale result).
#[derive(Debug, PartialEq, Eq)]
struct ScanKey {
    /// Fact storage identity and membership revision.
    storage: (usize, usize, u64),
    /// Per file in path order: path, source allocation, source length and the
    /// stored macro candidates' allocation. Pointer identity of an immutable
    /// `Arc<str>` is exact for "same text"; a different allocation with equal
    /// text only costs a recompute.
    files: Vec<(PathBuf, usize, usize, Option<usize>)>,
    /// Crates whose glob imports the scan treats as workspace-owned.
    macro_scope_crates: BTreeSet<String>,
    /// The module edges that decide which declarations resolved.
    module_parents: Option<BTreeMap<PathBuf, Vec<SourceRoleProvenanceEdge>>>,
    /// Include state that the target-root recognition reads.
    include_limitations: usize,
    include_parents: usize,
    include_targets: usize,
    /// Whether target roots are disabled for the whole workspace.
    roots_disabled: bool,
}

impl ScanKey {
    fn of(index: &RustIndex, roots: &TargetRoots) -> Self {
        Self {
            storage: index.storage_identity(),
            files: index
                .files()
                .iter()
                .map(|(path, facts)| {
                    let data = facts.data();
                    (
                        path.clone(),
                        data.source.as_ptr() as usize,
                        data.source.len(),
                        data.macro_candidates
                            .as_deref()
                            .map(|candidates| std::ptr::from_ref(candidates) as usize),
                    )
                })
                .collect(),
            macro_scope_crates: index.macro_scope_crates().clone(),
            module_parents: index.execution_module_parents.clone(),
            include_limitations: index.include_limitations.len(),
            include_parents: index.include_parents.len(),
            include_targets: index.include_targets.len(),
            roots_disabled: roots.disabled,
        }
    }
}

/// The scans computed under one key.
struct KeyedScans {
    key: ScanKey,
    likely: OnceLock<Arc<ScannedMacroSites>>,
    rest: OnceLock<Arc<ScannedMacroSites>>,
}

impl KeyedScans {
    fn new(key: ScanKey) -> Self {
        Self {
            key,
            likely: OnceLock::new(),
            rest: OnceLock::new(),
        }
    }
}

/// The index-owned, shared macro-site scan.
#[derive(Default)]
pub(in crate::analysis) struct SharedMacroScans {
    slot: Mutex<Option<Arc<KeyedScans>>>,
    /// Test witness: scans that actually parsed files.
    #[cfg(test)]
    runs: std::sync::atomic::AtomicUsize,
    /// Test witness: files handed to those scans.
    #[cfg(test)]
    scanned_files: std::sync::atomic::AtomicUsize,
}

impl SharedMacroScans {
    /// The keyed scans valid for `index` now: the stored ones when their key
    /// still matches, else a fresh empty set that replaces them.
    fn keyed(&self, index: &RustIndex, roots: &TargetRoots) -> Arc<KeyedScans> {
        let key = ScanKey::of(index, roots);
        let Ok(mut slot) = self.slot.lock() else {
            // Not established: compute privately, share nothing.
            return Arc::new(KeyedScans::new(key));
        };
        if let Some(current) = slot.as_ref().filter(|current| current.key == key) {
            return Arc::clone(current);
        }
        let fresh = Arc::new(KeyedScans::new(key));
        *slot = Some(Arc::clone(&fresh));
        fresh
    }

    /// The scan of `part` for `index`, parsed at most once per matching key.
    pub(super) fn scan(
        &self,
        part: ScanPart,
        index: &RustIndex,
        roots: &TargetRoots,
        module_resolved: &(dyn Fn(&Path, usize, &str) -> bool + Sync),
    ) -> Arc<ScannedMacroSites> {
        let keyed = self.keyed(index, roots);
        let cell = match part {
            ScanPart::Likely => &keyed.likely,
            ScanPart::Rest => &keyed.rest,
        };
        Arc::clone(cell.get_or_init(|| {
            let (likely, rest) = partition_macro_scan_files(index);
            let files: Vec<MacroScanFile<'_>> = match part {
                ScanPart::Likely => likely,
                ScanPart::Rest => rest,
            };
            #[cfg(test)]
            {
                use std::sync::atomic::Ordering::Relaxed;
                self.runs.fetch_add(1, Relaxed);
                self.scanned_files.fetch_add(files.len(), Relaxed);
            }
            Arc::new(scan_macro_sites(&files, index, roots, module_resolved))
        }))
    }

    /// How many scans parsed files on this index.
    #[cfg(test)]
    pub(in crate::analysis) fn runs(&self) -> usize {
        self.runs.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// How many files those scans were handed in total.
    #[cfg(test)]
    pub(in crate::analysis) fn scanned_files(&self) -> usize {
        self.scanned_files
            .load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl std::fmt::Debug for SharedMacroScans {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SharedMacroScans")
            .finish_non_exhaustive()
    }
}

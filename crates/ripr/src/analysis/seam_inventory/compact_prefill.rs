//! Complete compact generations derived while the full run's index is live.
//!
//! Admission bounds inputs and retained file/window payloads. These are not
//! allocator RSS or hard elapsed-time guarantees. Cancellation is cooperative.
use super::super::seam_cache::CacheStoreStatus;
use super::*;
use std::io::{Read, Write};

#[derive(Clone, Copy)]
struct Limits {
    files: usize,
    source_bytes: usize,
    file_source_bytes: usize,
    file_shapes: usize,
    indexed_shapes: usize,
    seams: usize,
    tests: usize,
    functions: usize,
    name_bytes: usize,
    retained_bytes: usize,
    window: usize,
}

const LIMITS: Limits = Limits {
    files: 20_000,
    source_bytes: 128 * 1024 * 1024,
    file_source_bytes: 8 * 1024 * 1024,
    file_shapes: 65_536,
    indexed_shapes: 1_250_000,
    seams: 1_000_000,
    tests: 50_000,
    functions: 500_000,
    name_bytes: 8 * 1024 * 1024,
    retained_bytes: 16 * 1024 * 1024,
    window: 64,
};

#[derive(Default)]
struct Retention {
    file_records: usize,
    file_bytes: usize,
    window_records: usize,
    window_bytes: usize,
}

pub(super) struct Failure {
    pub(super) message: String,
    pub(super) inputs_changed: bool,
}

pub(super) fn publish<'index>(
    state: &OwnedWorkspaceState,
    key: &RepoSeamCacheKey,
    files: &[PathBuf],
    index: &'index RustIndex,
    pass: &test_grip_evidence::EvidencePass<'index>,
    total: usize,
    fallback: &[PathBuf],
) -> Result<CacheStoreStatus, Failure> {
    let mut session = Session {
        state,
        key,
        files,
        index,
        pass,
        total,
        fallback,
        limits: LIMITS,
        retention: Retention::default(),
        inputs_changed: std::cell::Cell::new(false),
    };
    let result = session.run();
    pass.clear_window_memos();
    trace_latency_phase(
        "compact_prefill_retained",
        &format!(
            "file_records_{}_file_payload_{}_window_records_{}_window_payload_{}",
            session.retention.file_records,
            session.retention.file_bytes,
            session.retention.window_records,
            session.retention.window_bytes,
        ),
        Duration::ZERO,
    );
    result.map_err(|message| Failure {
        message,
        inputs_changed: session.inputs_changed.get(),
    })
}

struct Session<'a, 'index> {
    state: &'a OwnedWorkspaceState,
    key: &'a RepoSeamCacheKey,
    files: &'a [PathBuf],
    index: &'index RustIndex,
    pass: &'a test_grip_evidence::EvidencePass<'index>,
    total: usize,
    fallback: &'a [PathBuf],
    limits: Limits,
    retention: Retention,
    inputs_changed: std::cell::Cell<bool>,
}

fn skipped(label: &str) -> CacheStoreStatus {
    CacheStoreStatus {
        label: label.into(),
        advisory: None,
    }
}

impl Session<'_, '_> {
    fn admitted(&self) -> Result<bool, String> {
        cancellation::checkpoint()?;
        if self.index.files().values().count() > self.limits.files
            || self.index.tests().len() > self.limits.tests
            || self.index.functions().len() > self.limits.functions
            || self.total > self.limits.seams
            || self.files.len() > self.limits.files
        {
            return Ok(false);
        }
        let mut bytes = 0usize;
        let mut shapes = 0usize;
        for facts in self.index.files().values() {
            cancellation::checkpoint()?;
            // The index removes one leading UTF-8 BOM. Reserve its three
            // raw bytes before admission; lossy UTF-8 decoding otherwise
            // preserves or expands byte length. A tight decoded-only budget
            // declines optional prefill instead of claiming unchanged input
            // moved during raw-byte verification.
            let source_upper = facts.source.len().saturating_add(3);
            if source_upper > self.limits.file_source_bytes
                || facts.probe_shapes.len() > self.limits.file_shapes
                || facts.functions.len() > self.limits.file_shapes
            {
                return Ok(false);
            }
            bytes = bytes.saturating_add(source_upper);
            shapes = shapes.saturating_add(facts.probe_shapes.len());
            if bytes > self.limits.source_bytes || shapes > self.limits.indexed_shapes {
                return Ok(false);
            }
        }
        let mut names = 0usize;
        for function in self.index.functions().iter() {
            cancellation::checkpoint()?;
            names = names.saturating_add(function.name.len());
            if names > self.limits.name_bytes {
                return Ok(false);
            }
        }
        for file in self.files {
            if self.index.files().get(file).is_none() {
                return Err(format!(
                    "compact prefill index is missing {}",
                    file.display()
                ));
            }
        }
        Ok(true)
    }

    fn verify_inputs(&self) -> Result<(), String> {
        self.inputs_changed.set(true);
        // Inputs may grow after admission. Bound the reread itself, rather
        // than allocating the moved file and only then noticing the change.
        let mut hash = FilesContentHashBuilder::new();
        let mut remaining = self.limits.source_bytes;
        for path in &self.state.files {
            cancellation::checkpoint()?;
            if !std::fs::metadata(self.state.workspace_root.join(path))
                .map_err(|error| error.to_string())?
                .is_file()
            {
                return Err(
                    "compact prefill source is no longer a regular file; retry the run".into(),
                );
            }
            let file =
                std::fs::File::open(self.state.workspace_root.join(path)).map_err(|error| {
                    format!("read compact prefill source {}: {error}", path.display())
                })?;
            let mut bytes = Vec::new();
            file.take(
                self.limits
                    .file_source_bytes
                    .min(remaining)
                    .saturating_add(1) as u64,
            )
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
            if bytes.len() > self.limits.file_source_bytes || bytes.len() > remaining {
                return Err(
                    "compact prefill source grew beyond its admitted byte limit; retry the run"
                        .into(),
                );
            }
            remaining -= bytes.len();
            hash.push(path, &bytes);
        }
        if hash.finish() != self.state.files_content_hash {
            return Err("workspace Rust sources changed during compact prefill".into());
        }
        if self.state.cache_key() != *self.key {
            return Err("workspace cache inputs changed during compact prefill".into());
        }
        cancellation::checkpoint()?;
        self.inputs_changed.set(false);
        Ok(())
    }

    fn run(&mut self) -> Result<CacheStoreStatus, String> {
        if !self.admitted()? {
            return Ok(skipped("skipped_compact_prefill_input_limit"));
        }
        if self.pass.compact_evidence_for(&[])?.is_none() {
            return Ok(skipped("skipped_compact_prefill_context"));
        }
        if rust_index::lexical_fallback_files(self.index) != self.fallback {
            return Err("compact prefill fallback provenance does not match the index".into());
        }
        self.verify_inputs()?;
        let window_limit = self
            .limits
            .window
            .min(compact_classified_seam_cache_store_limit()?);
        if window_limit == 0 || self.total.div_ceil(window_limit) > 16_384 {
            return Ok(skipped("skipped_compact_prefill_window_limit"));
        }
        let mut files: Vec<_> = self.files.iter().collect();
        files.sort_unstable();
        files.dedup();
        // The empty store intentionally skips the producer. Enforce the same
        // exhaustion/current-input contract before that publication path.
        if self.total == 0 {
            for file in &files {
                if !file_seams(file, self.index, self.limits, &mut self.retention)?.is_empty() {
                    return Err("compact prefill has undeclared seams for a zero total".into());
                }
            }
            self.verify_inputs()?;
        }
        let mut next_file = 0usize;
        let mut pending = Vec::new().into_iter();
        let mut produced = 0usize;
        let cache = RepoSeamFactCache::at_compact_classified(&self.state.workspace_root);
        cache.store_compact_classified_windows(
            self.key,
            self.total,
            self.fallback,
            window_limit,
            |requested| {
                self.pass.clear_window_memos();
                let mut window = Vec::with_capacity(requested);
                let mut payload = Budget::new(self.limits.retained_bytes);
                for _ in 0..requested {
                    cancellation::checkpoint()?;
                    let seam = loop {
                        if let Some(seam) = pending.next() {
                            break seam;
                        }
                        let Some(file) = files.get(next_file) else {
                            return Err(
                                "compact prefill exhausted before the complete seam count".into()
                            );
                        };
                        next_file += 1;
                        pending = file_seams(file, self.index, self.limits, &mut self.retention)?
                            .into_iter();
                    };
                    let Some(mut evidence) = self
                        .pass
                        .compact_evidence_for(std::slice::from_ref(&seam))?
                    else {
                        return Ok(None);
                    };
                    let evidence = evidence
                        .pop()
                        .ok_or("compact evidence omitted an admitted seam")?;
                    let class = seam_classification::classify_seam(&seam, &evidence);
                    let record = ClassifiedSeam {
                        seam,
                        evidence,
                        class,
                    };
                    if !payload.measure(&record)? {
                        return Ok(None);
                    }
                    window.push(record);
                    self.retention.window_records = self.retention.window_records.max(window.len());
                    self.retention.window_bytes = self.retention.window_bytes.max(payload.used());
                    produced += 1;
                }
                self.pass.clear_window_memos();
                if produced == self.total {
                    if pending.next().is_some() {
                        return Err("compact prefill has more seams than the declared total".into());
                    }
                    for file in &files[next_file..] {
                        if !file_seams(file, self.index, self.limits, &mut self.retention)?
                            .is_empty()
                        {
                            return Err("compact prefill has undeclared trailing seams".into());
                        }
                    }
                    self.verify_inputs()?;
                }
                cancellation::checkpoint()?;
                Ok(Some(window))
            },
        )
    }
}

/// Retain at most one admitted file, sorted/deduplicated by the same identity
/// as the full inventory. One in-flight constructed seam is separate from
/// retained payload, and its source is bounded by file admission.
fn file_seams(
    file: &Path,
    index: &RustIndex,
    limits: Limits,
    retention: &mut Retention,
) -> Result<Vec<RepoSeam>, String> {
    let mut ordered = BTreeMap::new();
    let mut payload = Budget::new(limits.retained_bytes);
    visit_inventory_seams(&[file.to_path_buf()], index, |seam| {
        let key = SeamSortKey::of(&seam);
        if let std::collections::btree_map::Entry::Vacant(entry) = ordered.entry(key) {
            if !payload.measure(&seam)? {
                return Err("compact prefill file payload limit exceeded".into());
            }
            entry.insert(seam);
            retention.file_records = retention.file_records.max(ordered.len());
            retention.file_bytes = retention.file_bytes.max(payload.used());
        }
        Ok(())
    })?;
    Ok(ordered.into_values().collect())
}

struct Budget {
    initial: usize,
    remaining: usize,
}
impl Budget {
    fn new(bytes: usize) -> Self {
        Self {
            initial: bytes,
            remaining: bytes,
        }
    }
    fn used(&self) -> usize {
        self.initial - self.remaining
    }
    fn measure(&mut self, value: &impl serde::Serialize) -> Result<bool, String> {
        match serde_json::to_writer(self, value) {
            Ok(()) => Ok(true),
            Err(error) if error.is_io() => Ok(false),
            Err(error) => Err(format!("measure compact prefill payload: {error}")),
        }
    }
}
impl Write for Budget {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.remaining {
            return Err(std::io::Error::other("compact prefill payload limit"));
        }
        self.remaining -= bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
#[path = "compact_prefill_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "compact_prefill_generation_tests.rs"]
mod generation_tests;

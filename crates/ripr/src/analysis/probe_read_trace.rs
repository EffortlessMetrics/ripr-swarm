//! First #9784 Phase A observation: indexed reads made by one classifier call.
//!
//! This is a bounded diagnostic footprint, never a reuse ledger. Memo, scalar
//! and disk dependencies and content digests are not sealed here; every row
//! therefore reports unknown coverage and cannot authorize cache reuse.

use crate::domain::Probe;
use serde::Serialize;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{self, Write};
use std::marker::PhantomData;
use std::panic::Location;
use std::path::Path;
use std::rc::Rc;
use std::sync::OnceLock;

const TRACE_ENV: &str = "RIPR_PROBE_READ_TRACE";
const PREFIX: &[u8] = b"ripr_probe_read_trace ";
const MAX_READS: usize = 64;
const MAX_TEXT_BYTES: usize = 512;
const MAX_ROW_BYTES: usize = 32 * 1024;
// Distinct subjects remembered per (kind, site); past it the count is a lower bound.
const MAX_DISTINCT: usize = 8192;
const UNKNOWN_INPUTS: &[&str] = &[
    "memo_inputs_not_sealed",
    "scalar_inputs_not_sealed",
    "disk_inputs_not_sealed",
    "nested_worker_reads_not_traced",
    "dependency_digests_not_captured",
];

thread_local! {
    static ACTIVE: RefCell<Option<Pending>> = const { RefCell::new(None) };
    #[cfg(test)]
    static CAPTURE: RefCell<Option<TestCapture>> = const { RefCell::new(None) };
}

#[derive(Clone, Copy)]
pub(in crate::analysis) enum Universe {
    Files,
    Functions,
    Tests,
}

impl Universe {
    fn as_str(self) -> &'static str {
        match self {
            Self::Files => "files",
            Self::Functions => "functions",
            Self::Tests => "tests",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    File,
    Absent,
    Test,
    Universe,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
struct Site {
    file: &'static str,
    line: u32,
    column: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
struct Read {
    kind: Kind,
    subject: String,
    site: Site,
}

#[derive(Default)]
struct SiteTally {
    events: u64,
    distinct: BTreeSet<u64>,
    lower_bound: bool,
}

#[derive(Clone, Debug, Serialize)]
struct SiteSummary {
    kind: Kind,
    site: Site,
    events: u64,
    distinct_subjects: u64,
    distinct_is_lower_bound: bool,
}

struct Pending {
    probe_id: Option<String>,
    reads: BTreeMap<Read, u64>,
    sites: BTreeMap<(Kind, Site), SiteTally>,
    observed_read_events: u64,
    dropped_read_events: u64,
    dropped_site_events: u64,
    universe_observed: bool,
}

#[derive(Clone, Debug, Serialize)]
struct ReadCount {
    #[serde(flatten)]
    read: Read,
    count: u64,
}

#[derive(Clone, Debug, Serialize)]
struct Observation {
    schema_version: &'static str,
    event: &'static str,
    probe_id: Option<String>,
    classification_returned: bool,
    interrupted: bool,
    analysis_completion: &'static str,
    coverage: &'static str,
    unknown_inputs: &'static [&'static str],
    reusable: bool,
    universe_observed: bool,
    observed_read_events: u64,
    dropped_read_events: u64,
    read_detail_omitted: bool,
    reads: Vec<ReadCount>,
    /// Per (kind, site) totals that survive the 64-entry detail cap, so distinct
    /// reads past the cap stay visible. Subject hashes are a diagnostic count only.
    sites: Vec<SiteSummary>,
    dropped_site_events: u64,
}

impl Pending {
    fn observe(&mut self, kind: Kind, subject: Option<&str>, site: &'static Location<'static>) {
        self.observed_read_events = self.observed_read_events.saturating_add(1);
        self.universe_observed |= kind == Kind::Universe;
        self.tally(kind.clone(), subject, site);
        let Some(subject) = subject.filter(|value| value.len() <= MAX_TEXT_BYTES) else {
            self.dropped_read_events = self.dropped_read_events.saturating_add(1);
            return;
        };
        let read = Read {
            kind,
            subject: subject.to_string(),
            site: Site {
                file: site.file(),
                line: site.line(),
                column: site.column(),
            },
        };
        if let Some(count) = self.reads.get_mut(&read) {
            *count = count.saturating_add(1);
        } else if self.reads.len() < MAX_READS {
            self.reads.insert(read, 1);
        } else {
            self.dropped_read_events = self.dropped_read_events.saturating_add(1);
        }
    }

    fn tally(&mut self, kind: Kind, subject: Option<&str>, site: &'static Location<'static>) {
        let key = (
            kind,
            Site {
                file: site.file(),
                line: site.line(),
                column: site.column(),
            },
        );
        if !self.sites.contains_key(&key) && self.sites.len() >= MAX_READS {
            self.dropped_site_events = self.dropped_site_events.saturating_add(1);
            return;
        }
        let tally = self.sites.entry(key).or_default();
        tally.events = tally.events.saturating_add(1);
        let Some(subject) = subject else {
            tally.lower_bound = true;
            return;
        };
        let mut hasher = DefaultHasher::new();
        subject.hash(&mut hasher);
        let digest = hasher.finish();
        if tally.distinct.len() < MAX_DISTINCT || tally.distinct.contains(&digest) {
            tally.distinct.insert(digest);
        } else {
            tally.lower_bound = true;
        }
    }

    fn finish(self, returned: bool) -> Observation {
        Observation {
            schema_version: "1",
            event: "probe_read_observation",
            probe_id: self.probe_id,
            classification_returned: returned,
            interrupted: super::cancellation::current_token()
                .is_some_and(|token| token.abort_kind().is_some()),
            analysis_completion: "not_established",
            coverage: "unknown",
            unknown_inputs: UNKNOWN_INPUTS,
            reusable: false,
            universe_observed: self.universe_observed,
            observed_read_events: self.observed_read_events,
            dropped_read_events: self.dropped_read_events,
            read_detail_omitted: false,
            reads: self
                .reads
                .into_iter()
                .map(|(read, count)| ReadCount { read, count })
                .collect(),
            sites: self
                .sites
                .into_iter()
                .map(|((kind, site), tally)| SiteSummary {
                    kind,
                    site,
                    events: tally.events,
                    distinct_subjects: tally.distinct.len() as u64,
                    distinct_is_lower_bound: tally.lower_bound,
                })
                .collect(),
            dropped_site_events: self.dropped_site_events,
        }
    }
}

/// A lexical scope belongs to an invocation, never a Rayon worker. The marker
/// prevents transferring the guard to a thread whose recording slot it cannot own.
pub(in crate::analysis) struct Scope {
    installed: bool,
    previous: Option<Pending>,
    returned: bool,
    _thread_bound: PhantomData<Rc<()>>,
}

impl Scope {
    pub(in crate::analysis) fn begin(probe: &Probe) -> Self {
        let pending = enabled().then(|| Pending {
            probe_id: (probe.id.0.len() <= MAX_TEXT_BYTES).then(|| probe.id.0.clone()),
            reads: BTreeMap::new(),
            sites: BTreeMap::new(),
            dropped_site_events: 0,
            observed_read_events: 0,
            dropped_read_events: 0,
            universe_observed: false,
        });
        let mut scope = Self {
            installed: false,
            previous: None,
            returned: false,
            _thread_bound: PhantomData,
        };
        if let Some(pending) = pending {
            ACTIVE.with(|slot| {
                if let Ok(mut active) = slot.try_borrow_mut() {
                    scope.previous = active.replace(pending);
                    scope.installed = true;
                }
            });
        }
        scope
    }

    pub(in crate::analysis) fn finish(mut self) {
        self.returned = true;
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        if !self.installed {
            return;
        }
        let pending = ACTIVE.with(|slot| {
            let Ok(mut active) = slot.try_borrow_mut() else {
                return None;
            };
            let pending = active.take();
            *active = self.previous.take();
            pending
        });
        if let Some(pending) = pending {
            let _diagnostic_emitted = emit(pending.finish(self.returned));
        }
    }
}

fn enabled() -> bool {
    #[cfg(test)]
    if let Some(enabled) = CAPTURE.with(|slot| {
        slot.try_borrow()
            .ok()
            .and_then(|capture| capture.as_ref().map(|capture| capture.enabled))
    }) {
        return enabled;
    }
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os(TRACE_ENV).is_some())
}

#[track_caller]
pub(in crate::analysis) fn file(path: &Path, present: bool) {
    record(
        if present { Kind::File } else { Kind::Absent },
        path.to_str(),
        Location::caller(),
    );
}

/// One test fact read by its position. The subject names the fact
/// (`file:start_line:name`), so a keyed read is not a whole-set dependency.
#[track_caller]
pub(in crate::analysis) fn test(file: &Path, start_line: usize, name: &str) {
    // Callers read thousands of candidates per probe; skip the formatting when off.
    if !enabled() {
        return;
    }
    let subject = file
        .to_str()
        .map(|file| format!("{file}:{start_line}:{name}"));
    record(Kind::Test, subject.as_deref(), Location::caller());
}

#[track_caller]
pub(in crate::analysis) fn universe(set: Universe) {
    record(Kind::Universe, Some(set.as_str()), Location::caller());
}

fn record(kind: Kind, subject: Option<&str>, site: &'static Location<'static>) {
    if !enabled() {
        return;
    }
    ACTIVE.with(|slot| {
        if let Ok(mut active) = slot.try_borrow_mut()
            && let Some(pending) = active.as_mut()
        {
            pending.observe(kind, subject, site);
        }
    });
}

fn write_observation(writer: &mut impl Write, mut row: Observation) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(&row).map_err(io::Error::other)?;
    if bytes.len() > MAX_ROW_BYTES {
        row.dropped_read_events = row.observed_read_events;
        row.reads.clear();
        row.read_detail_omitted = true;
        bytes = serde_json::to_vec(&row).map_err(io::Error::other)?;
    }
    if bytes.len() > MAX_ROW_BYTES {
        return Err(io::Error::other(
            "probe read trace metadata exceeds its row bound",
        ));
    }
    writer.write_all(PREFIX)?;
    writer.write_all(&bytes)?;
    writer.write_all(b"\n")
}

fn emit(row: Observation) -> bool {
    #[cfg(test)]
    if CAPTURE.with(|slot| {
        if let Ok(mut capture) = slot.try_borrow_mut()
            && let Some(capture) = capture.as_mut()
        {
            capture.rows.push(row.clone());
            return true;
        }
        false
    }) {
        return true;
    }
    // Diagnostics cannot change a finding or panic when stderr is closed.
    write_observation(&mut io::stderr().lock(), row).is_ok()
}

#[cfg(test)]
struct TestCapture {
    enabled: bool,
    rows: Vec<Observation>,
}

#[cfg(test)]
mod tests;

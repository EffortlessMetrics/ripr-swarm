//! Per-build retention for the content-keyed per-file fact stores (#7047).
//!
//! The Rust, TypeScript and Python file-fact stores all key their entries on
//! the analyzer identity (`build_identity::cache_identity`), so an entry
//! written by another ripr build is never served again. Without pruning,
//! every ripr upgrade, every dirty local rebuild and every schema bump left a
//! full generation of entries behind. This module holds the one retention
//! policy those stores share:
//!
//! - sibling schema directories this build can never read are removed;
//! - current-schema entries of other builds are removed, except the
//!   [`RETAINED_OTHER_BUILDS`] most recently used ones and any build whose
//!   use marker is younger than [`RECENT_BUILD_GRACE`].
//!
//! "Used" is the later of a build's newest entry write and its use marker,
//! which a cache hit refreshes: a warm run never writes, so entry mtimes alone
//! would make an editor server that only hits look abandoned.
//!
//! Every removal is lexically contained under the cache base and refuses
//! symlinked layer, schema and marker directories, so a planted link cannot
//! steer a delete outside the cache.

use super::{cache_path_contained_in_base, fnv1a_64};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Other analyzer identities whose current-schema entries survive the prune:
/// enough for an editor-managed server and a local CLI build to alternate on
/// one cache without evicting each other.
pub(super) const RETAINED_OTHER_BUILDS: usize = 2;

/// An other build whose use marker (a cache hit) is younger than this is
/// never pruned, whatever its rank: a frequently rebuilt local CLI must not
/// evict an editor server that is still in use. Builds that only wrote get no
/// grace, so dirty rebuilds cannot accumulate.
pub(super) const RECENT_BUILD_GRACE: Duration = Duration::from_hours(7 * 24);

/// Directory, beside the schema directories, holding one "last used" marker
/// per build. Its name never parses as a schema version, so the
/// schema-sibling prune keeps it.
pub(super) const BUILD_USE_DIR: &str = ".build-use";

/// A long-running process (the editor server) refreshes its marker this often.
const BUILD_USE_REFRESH: Duration = Duration::from_hours(1);

/// Bytes read from the front of an entry to find its analyzer identity. Every
/// store writes its identity fields before the facts body.
const ENTRY_PREFIX_BYTES: u64 = 8 * 1024;

static BUILD_USE_NOTED: Mutex<BTreeMap<PathBuf, Instant>> = Mutex::new(BTreeMap::new());

/// Layer directories already pruned this process. Mirrors the
/// once-per-directory latch in `atomic_file::sweep_stale_temp_files_once`.
static PRUNED_LAYERS: Mutex<BTreeSet<PathBuf>> = Mutex::new(BTreeSet::new());

/// Whether `path` is a directory that is not a symlink.
pub(super) fn is_real_dir(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .is_ok_and(|meta| meta.is_dir() && !meta.file_type().is_symlink())
}

/// [`is_real_dir`], or nothing exists at `path` yet.
fn is_real_dir_or_absent(path: &Path) -> bool {
    match std::fs::symlink_metadata(path) {
        Ok(meta) => meta.is_dir() && !meta.file_type().is_symlink(),
        Err(err) => err.kind() == std::io::ErrorKind::NotFound,
    }
}

pub(super) fn build_use_marker_name(identity: &str) -> String {
    format!("{:016x}", fnv1a_64(identity.as_bytes()))
}

/// Order of a schema directory that is a single decimal generation (`8`,
/// `19`), as the TypeScript and Python stores use; `None` for any other name.
pub(crate) fn integer_schema_rank(name: &str) -> Option<(u64, u64)> {
    if name.is_empty() || !name.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    name.parse().ok().map(|generation| (generation, 0))
}

/// The analyzer identity of a ripr-written entry, read from its first
/// [`ENTRY_PREFIX_BYTES`]. Accepts a plain JSON envelope (TypeScript) and a
/// framed `<digest>\n<json>` one (Python). The identity field precedes the
/// file path and the facts in every envelope, so the first occurrence of the
/// key is the identity, never user text. `None` for anything unreadable,
/// which the prune then leaves alone.
pub(crate) fn analyzer_version_from_entry_prefix(path: &Path) -> Option<String> {
    use std::io::Read as _;
    let mut prefix = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(ENTRY_PREFIX_BYTES)
        .read_to_end(&mut prefix)
        .ok()?;
    let body = match prefix.first() {
        Some(b'{') => prefix.as_slice(),
        _ => {
            let newline = prefix.iter().position(|byte| *byte == b'\n')?;
            prefix.get(newline + 1..)?
        }
    };
    if body.first() != Some(&b'{') {
        return None;
    }
    const KEY: &[u8] = b"\"analyzer_version\":";
    let start = body.windows(KEY.len()).position(|window| window == KEY)? + KEY.len();
    let rest = body.get(start..)?;
    serde_json::Deserializer::from_slice(rest)
        .into_iter::<String>()
        .next()?
        .ok()
}

/// What `ripr cache status` reports as reclaimable: entries this build never
/// reads because another build or an older schema wrote them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Reclaimable {
    pub(crate) entry_count: usize,
    pub(crate) size_bytes: u64,
}

impl Reclaimable {
    pub(crate) fn add(&mut self, other: Self) {
        self.entry_count = self.entry_count.saturating_add(other.entry_count);
        self.size_bytes = self.size_bytes.saturating_add(other.size_bytes);
    }
}

/// One store's production layout `{base}/{layer}/{schema}` and how to read
/// its entries. Construct it only for the production layout: test roots that
/// write entries directly in an arbitrary directory must never prune.
pub(crate) struct BuildRetention<'a> {
    base: &'a Path,
    layer_dir: PathBuf,
    current_schema: &'a str,
    /// Order of a schema directory name; `None` for anything else (the
    /// marker directory, stray files).
    schema_rank: fn(&str) -> Option<(u64, u64)>,
    /// Analyzer identity of a current-schema entry; `None` leaves it alone.
    read_build: &'a dyn Fn(&Path) -> Option<String>,
}

impl<'a> BuildRetention<'a> {
    pub(crate) fn new(
        base: &'a Path,
        layer_dir: PathBuf,
        current_schema: &'a str,
        schema_rank: fn(&str) -> Option<(u64, u64)>,
        read_build: &'a dyn Fn(&Path) -> Option<String>,
    ) -> Self {
        Self {
            base,
            layer_dir,
            current_schema,
            schema_rank,
            read_build,
        }
    }

    fn schema_dir(&self) -> PathBuf {
        self.layer_dir.join(self.current_schema)
    }

    /// Records that this build served a hit, at most once per
    /// [`BUILD_USE_REFRESH`] per process and layer. The caller checks that the
    /// cache base is usable; this refuses a symlinked layer or marker
    /// directory, writes atomically (a planted symlink at the marker path is
    /// replaced by the rename, never written through) and ignores failures.
    pub(crate) fn note_use(&self) {
        let due = {
            let mut noted = match BUILD_USE_NOTED.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            let now = Instant::now();
            let due = noted
                .get(&self.layer_dir)
                .is_none_or(|last| now.duration_since(*last) >= BUILD_USE_REFRESH);
            if due {
                noted.insert(self.layer_dir.clone(), now);
            }
            due
        };
        if !due {
            return;
        }
        let marker_dir = self.layer_dir.join(BUILD_USE_DIR);
        if !cache_path_contained_in_base(self.base, &marker_dir)
            || !is_real_dir_or_absent(&self.layer_dir)
            || !is_real_dir_or_absent(&marker_dir)
        {
            return;
        }
        let identity = crate::build_identity::cache_identity();
        let marker = marker_dir.join(build_use_marker_name(identity));
        let _ = std::fs::create_dir_all(&marker_dir)
            .map_err(|err| err.to_string())
            .and_then(|()| {
                crate::atomic_file::write_cache(&marker, identity.as_bytes(), "build use marker")
            });
    }

    /// Once per process and layer, best-effort removal of stale schema
    /// siblings and of other builds' entries. Called after a successful store.
    pub(crate) fn prune_once(&self) {
        if !cache_path_contained_in_base(self.base, &self.layer_dir) {
            return;
        }
        let first_visit = {
            let mut pruned = match PRUNED_LAYERS.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            pruned.insert(self.layer_dir.clone())
        };
        if first_visit {
            self.prune_stale_schema_siblings();
            self.prune_stale_builds();
        }
    }

    /// Sibling schema directories other than the current one, each a real
    /// directory contained under the base. With `older_only`, just those
    /// ranked below the current schema: a newer schema belongs to a newer
    /// ripr (an editor server, say) that still reads it and prunes for
    /// itself, so it is reported as reclaimable but never removed here.
    fn stale_schema_dirs(&self, older_only: bool) -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        let Some(current) = (self.schema_rank)(self.current_schema) else {
            return dirs;
        };
        if !is_real_dir(&self.layer_dir) {
            return dirs;
        }
        let Ok(entries) = std::fs::read_dir(&self.layer_dir) else {
            return dirs;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let Some(rank) = (self.schema_rank)(name) else {
                continue;
            };
            if rank == current || (older_only && rank > current) {
                continue;
            }
            let path = self.layer_dir.join(name);
            if cache_path_contained_in_base(self.base, &path) && is_real_dir(&path) {
                dirs.push(path);
            }
        }
        dirs
    }

    fn prune_stale_schema_siblings(&self) {
        for path in self.stale_schema_dirs(true) {
            let _ = std::fs::remove_dir_all(&path);
        }
    }

    /// Current-schema entries of other builds, grouped by build, with each
    /// group's newest write and total bytes.
    fn other_build_entries(&self, identity: &str) -> HashMap<String, BuildEntries> {
        let mut by_build: HashMap<String, BuildEntries> = HashMap::new();
        let schema_dir = self.schema_dir();
        // Both the layer and the schema directory must be real directories:
        // containment is lexical, so a symlinked layer would otherwise let
        // the prune delete JSON outside the cache.
        if !is_real_dir(&self.layer_dir) || !is_real_dir(&schema_dir) {
            return by_build;
        }
        let Ok(entries) = std::fs::read_dir(&schema_dir) else {
            return by_build;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let Ok(metadata) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if !metadata.is_file() {
                continue;
            }
            let Some(build) = (self.read_build)(&path) else {
                continue;
            };
            if build == identity {
                continue;
            }
            let modified = metadata.modified().unwrap_or(UNIX_EPOCH);
            let slot = by_build.entry(build).or_default();
            slot.used = slot.used.max(modified);
            slot.size_bytes = slot.size_bytes.saturating_add(metadata.len());
            slot.paths.push(path);
        }
        by_build
    }

    /// Last-hit time per other build, from the use markers. A marker counts
    /// only when it is a small regular file whose name is the hash of its
    /// content.
    fn other_build_use_markers(&self, identity: &str) -> HashMap<String, (SystemTime, PathBuf)> {
        let mut markers = HashMap::new();
        let marker_dir = self.layer_dir.join(BUILD_USE_DIR);
        if !is_real_dir(&self.layer_dir) || !is_real_dir(&marker_dir) {
            return markers;
        }
        let Ok(entries) = std::fs::read_dir(&marker_dir) else {
            return markers;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(metadata) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if !metadata.is_file() || metadata.len() > 4096 {
                continue;
            }
            let Ok(build) = std::fs::read_to_string(&path) else {
                continue;
            };
            if build == identity
                || entry.file_name().to_str() != Some(build_use_marker_name(&build).as_str())
            {
                continue;
            }
            let modified = metadata.modified().unwrap_or(UNIX_EPOCH);
            markers.insert(build, (modified, path));
        }
        markers
    }

    /// Best-effort removal of current-schema entries written by other builds.
    ///
    /// Kept: the current build; the [`RETAINED_OTHER_BUILDS`] most recently
    /// used other builds; any other build whose use marker is younger than
    /// [`RECENT_BUILD_GRACE`].
    fn prune_stale_builds(&self) {
        let identity = crate::build_identity::cache_identity();
        let by_build = self.other_build_entries(identity);
        let mut markers = self.other_build_use_markers(identity);
        let now = SystemTime::now();
        let hit_recently = |hit: SystemTime| {
            now.duration_since(hit)
                .map_or(true, |age| age < RECENT_BUILD_GRACE)
        };
        let mut builds: Vec<(String, BuildEntries)> = by_build
            .into_iter()
            .map(|(build, mut entries)| {
                if let Some((hit, _)) = markers.get(&build) {
                    entries.used = entries.used.max(*hit);
                }
                (build, entries)
            })
            .collect();
        // Newest first; the identity breaks ties so the choice is deterministic.
        builds.sort_by(|(left_id, left), (right_id, right)| {
            right
                .used
                .cmp(&left.used)
                .then_with(|| left_id.cmp(right_id))
        });
        for (rank, (build, entries)) in builds.into_iter().enumerate() {
            let marker = markers.remove(&build);
            if rank < RETAINED_OTHER_BUILDS
                || marker.as_ref().is_some_and(|(hit, _)| hit_recently(*hit))
            {
                continue;
            }
            for path in entries.paths {
                if cache_path_contained_in_base(self.base, &path) {
                    let _ = std::fs::remove_file(&path);
                }
            }
            if let Some((_, marker_path)) = marker {
                let _ = std::fs::remove_file(marker_path);
            }
        }
        // Markers of builds that no longer have entries, once past the grace.
        for (hit, marker_path) in markers.into_values() {
            if !hit_recently(hit) && cache_path_contained_in_base(self.base, &marker_path) {
                let _ = std::fs::remove_file(marker_path);
            }
        }
    }

    /// Entries and bytes this build never reads: everything under stale
    /// schema siblings plus current-schema entries of other builds. Read-only.
    pub(crate) fn reclaimable(&self) -> Reclaimable {
        let mut total = Reclaimable::default();
        for dir in self.stale_schema_dirs(false) {
            total.add(regular_files_under(&dir));
        }
        for entries in self
            .other_build_entries(crate::build_identity::cache_identity())
            .into_values()
        {
            total.add(Reclaimable {
                entry_count: entries.paths.len(),
                size_bytes: entries.size_bytes,
            });
        }
        total
    }
}

struct BuildEntries {
    used: SystemTime,
    size_bytes: u64,
    paths: Vec<PathBuf>,
}

impl Default for BuildEntries {
    fn default() -> Self {
        Self {
            used: UNIX_EPOCH,
            size_bytes: 0,
            paths: Vec::new(),
        }
    }
}

/// Regular files under `dir`, never following symlinks.
fn regular_files_under(dir: &Path) -> Reclaimable {
    let mut total = Reclaimable::default();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(metadata) = std::fs::symlink_metadata(entry.path()) else {
                continue;
            };
            if metadata.is_dir() {
                stack.push(entry.path());
            } else if metadata.is_file() {
                total.add(Reclaimable {
                    entry_count: 1,
                    size_bytes: metadata.len(),
                });
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> Result<PathBuf, String> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "ripr-build-retention-{name}-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
        Ok(dir)
    }

    #[test]
    fn analyzer_version_reads_plain_and_framed_envelopes_and_refuses_others() -> Result<(), String>
    {
        let dir = scratch("prefix")?;
        let plain = dir.join("plain.json");
        std::fs::write(
            &plain,
            br#"{"schema_version":"8","analyzer_version":"0.11.0 (abc)","file_path":"a.ts"}"#,
        )
        .map_err(|err| err.to_string())?;
        let framed = dir.join("framed.json");
        std::fs::write(
            &framed,
            b"deadbeef\n{\"python_file_fact_schema_version\":\"19\",\"analyzer_version\":\"0.10.0\",\"file_path\":\"a.py\"}",
        )
        .map_err(|err| err.to_string())?;
        let garbage = dir.join("garbage.json");
        std::fs::write(&garbage, b"not json at all").map_err(|err| err.to_string())?;
        let no_key = dir.join("no_key.json");
        std::fs::write(&no_key, br#"{"file_path":"analyzer_version"}"#)
            .map_err(|err| err.to_string())?;
        let results = (
            analyzer_version_from_entry_prefix(&plain),
            analyzer_version_from_entry_prefix(&framed),
            analyzer_version_from_entry_prefix(&garbage),
            analyzer_version_from_entry_prefix(&no_key),
        );
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            results,
            (
                Some("0.11.0 (abc)".to_string()),
                Some("0.10.0".to_string()),
                None,
                None
            )
        );
        Ok(())
    }

    #[test]
    fn integer_schema_ranks_exclude_markers_and_dotted_versions() {
        assert_eq!(integer_schema_rank("8"), Some((8, 0)));
        assert_eq!(integer_schema_rank("19"), Some((19, 0)));
        assert!(integer_schema_rank("19") > integer_schema_rank("8"));
        assert_eq!(integer_schema_rank(""), None);
        assert_eq!(integer_schema_rank(BUILD_USE_DIR), None);
        assert_eq!(integer_schema_rank("1.18"), None);
        assert_eq!(integer_schema_rank("8a"), None);
    }

    #[test]
    fn reclaimable_counts_stale_schemas_and_other_builds_but_not_this_build() -> Result<(), String>
    {
        let base = scratch("reclaim")?;
        let layer = base.join("typescript-file-facts");
        let current = layer.join("8");
        let stale = layer.join("7");
        let markers = layer.join(BUILD_USE_DIR);
        for dir in [&current, &stale, &markers] {
            std::fs::create_dir_all(dir).map_err(|err| err.to_string())?;
        }
        let identity = crate::build_identity::cache_identity();
        let entry =
            |build: &str| format!(r#"{{"schema_version":"8","analyzer_version":"{build}"}}"#);
        std::fs::write(current.join("mine.json"), entry(identity)).map_err(|e| e.to_string())?;
        let other = entry("ripr-other-build");
        std::fs::write(current.join("other.json"), &other).map_err(|e| e.to_string())?;
        std::fs::write(stale.join("old.json"), b"12345").map_err(|e| e.to_string())?;
        std::fs::write(markers.join("0000000000000000"), b"x").map_err(|e| e.to_string())?;
        let read = |path: &Path| analyzer_version_from_entry_prefix(path);
        let retention = BuildRetention::new(&base, layer.clone(), "8", integer_schema_rank, &read);
        let reclaimable = retention.reclaimable();
        let mine_kept = current.join("mine.json").is_file();
        let _ = std::fs::remove_dir_all(&base);
        assert_eq!(
            reclaimable,
            Reclaimable {
                entry_count: 2,
                size_bytes: other.len() as u64 + 5,
            },
            "the other build's entry and the stale schema's file, not this build's entry or the marker"
        );
        assert!(mine_kept, "reclaimable is read-only");
        Ok(())
    }
}

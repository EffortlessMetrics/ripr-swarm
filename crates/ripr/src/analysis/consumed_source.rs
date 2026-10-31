//! Private commitments to raw Rust bytes actually loaded by one analysis.
//!
//! These are per-path observations, not a complete dependency identity. Cached
//! syntax text, decoded geometry and a later filesystem read cannot mint them.

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
enum Commitment {
    Captured(String),
    Unavailable,
    Conflicting,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ConsumedRustSources {
    paths: BTreeMap<PathBuf, Commitment>,
    #[cfg(test)]
    pub(crate) file_fact_cache: crate::analysis::seam_cache::FileFactCacheStats,
}

impl ConsumedRustSources {
    /// Observe the raw loaded buffer before parsing or consulting fact caches.
    pub(crate) fn record(&mut self, path: &Path, bytes: Option<&[u8]>) {
        // An empty, absolute or escaping key cannot be vouched for under the
        // snapshot root. It is kept as a conflicting observation so it forces
        // a refusal instead of being silently dropped.
        let observed = if is_normal_relative_path(path) {
            bytes.map_or(Commitment::Unavailable, |bytes| {
                Commitment::Captured(format!("{:x}", Sha256::digest(bytes)))
            })
        } else {
            Commitment::Conflicting
        };
        self.paths
            .entry(path.to_path_buf())
            .and_modify(|previous| {
                if *previous != observed {
                    *previous = Commitment::Conflicting;
                }
            })
            .or_insert(observed);
    }

    /// Fold another carrier's observations in, path by path, with the same
    /// conflict rule as [`Self::record`].
    pub(crate) fn absorb(&mut self, other: ConsumedRustSources) {
        for (path, observed) in other.paths {
            self.paths
                .entry(path)
                .and_modify(|previous| {
                    if *previous != observed {
                        *previous = Commitment::Conflicting;
                    }
                })
                .or_insert(observed);
        }
    }

    /// Resolve an observed relative key. The consumer owns admission under the
    /// snapshot root; this carrier never reads or canonicalizes the filesystem.
    pub(crate) fn digest(&self, relative: &Path) -> Option<String> {
        if !is_normal_relative_path(relative) {
            return None;
        }
        match self.paths.get(relative)? {
            Commitment::Captured(digest) => Some(digest.clone()),
            Commitment::Unavailable | Commitment::Conflicting => None,
        }
    }

    /// Re-verify, at request time, that the bytes behind `paths` still match
    /// what this carrier captured while the producer ran (#7357 card
    /// currentness). `read` returns the path's current raw bytes, or `None`
    /// when they cannot be read. Only paths with a captured commitment are
    /// checked: an uncommitted path (non-Rust, outside the inventory corpus)
    /// makes no claim and cannot drift. A captured path that is now unreadable
    /// or hashes differently is `Drifted`. Nothing is rescanned or minted: the
    /// caller names the paths, this owner decides.
    pub(crate) fn first_drift<'a>(
        &self,
        paths: impl IntoIterator<Item = &'a Path>,
        mut read: impl FnMut(&Path) -> Option<Vec<u8>>,
    ) -> Option<CustodyRefusal> {
        for path in paths {
            let Some(expected) = self.digest(path) else {
                continue;
            };
            let unchanged =
                read(path).is_some_and(|bytes| format!("{:x}", Sha256::digest(&bytes)) == expected);
            if !unchanged {
                return Some(CustodyRefusal::Drifted(path.to_path_buf()));
            }
        }
        None
    }

    /// Establish that two producers consumed the same Rust bytes (#7357).
    ///
    /// `findings` is the carrier the findings producer observed while it loaded
    /// the Rust sources it analyzed; `inventory` is the carrier the seam
    /// inventory observed over the whole analyzable Rust corpus before its own
    /// parse and cache access. Agreement requires every path the findings
    /// producer consumed to be captured by both with the same digest, and
    /// every inventory observation to be a captured one. A path only the
    /// inventory observed cannot contradict the findings, but it is part of
    /// the returned commitment, because the inventory (and so every card) read
    /// it. Neither carrier is rescanned: this compares what each producer
    /// recorded while its own bytes were live.
    ///
    /// This is a conservative per-path context commitment, not a portable
    /// dependency identity: it is silent about non-Rust inputs and about
    /// anything outside the inventory corpus.
    pub(crate) fn establish_agreement(
        findings: &Self,
        inventory: &Self,
    ) -> Result<ContextCommitment, CustodyRefusal> {
        if findings.paths.is_empty() {
            return Err(CustodyRefusal::FindingsObservedNothing);
        }
        for (path, observed) in &findings.paths {
            let Commitment::Captured(findings_digest) = observed else {
                return Err(CustodyRefusal::FindingsUnobserved(path.clone()));
            };
            match inventory.paths.get(path) {
                Some(Commitment::Captured(inventory_digest))
                    if inventory_digest == findings_digest => {}
                Some(Commitment::Captured(_)) => {
                    return Err(CustodyRefusal::Moved(path.clone()));
                }
                Some(Commitment::Unavailable | Commitment::Conflicting) | None => {
                    return Err(CustodyRefusal::InventoryUnobserved(path.clone()));
                }
            }
        }
        let mut hasher = Sha256::new();
        hasher.update(b"ripr-consumed-rust-context-v1");
        for (path, observed) in &inventory.paths {
            let Commitment::Captured(digest) = observed else {
                return Err(CustodyRefusal::InventoryUnobserved(path.clone()));
            };
            hasher.update([0]);
            hasher.update(path.as_os_str().as_encoded_bytes());
            hasher.update([0]);
            hasher.update(digest.as_bytes());
        }
        Ok(ContextCommitment(format!(
            "rust-context:sha256:{:x}",
            hasher.finalize()
        )))
    }
}

/// A conservative commitment to the Rust context two producers agreed on. It
/// is only minted by [`ConsumedRustSources::establish_agreement`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ContextCommitment(String);

impl ContextCommitment {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// Why agreement between the findings and inventory carriers could not be
/// established. Every variant withholds the claims that need custody.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CustodyRefusal {
    /// The findings producer recorded no Rust source at all.
    FindingsObservedNothing,
    /// The findings producer's own observation of this path is unavailable or
    /// conflicting.
    FindingsUnobserved(PathBuf),
    /// The inventory has no captured observation of a path the findings
    /// consumed (absent from its corpus, unavailable, conflicting, or an
    /// out-of-root key), or holds an uncaptured observation of its own.
    InventoryUnobserved(PathBuf),
    /// The two producers read different bytes for this path: the saved source
    /// moved between them.
    Moved(PathBuf),
    /// A request-time re-read found this path's bytes differ from (or are
    /// missing against) what the snapshot's producer captured.
    Drifted(PathBuf),
}

impl CustodyRefusal {
    pub(crate) fn describe(&self) -> String {
        match self {
            Self::FindingsObservedNothing => {
                "the findings producer recorded no consumed Rust source bytes".to_string()
            }
            Self::FindingsUnobserved(path) => format!(
                "the findings producer has no captured observation of `{}`",
                shown(path)
            ),
            Self::InventoryUnobserved(path) => format!(
                "the seam inventory has no captured observation of `{}`",
                shown(path)
            ),
            Self::Moved(path) => format!(
                "`{}` changed between the findings pass and the seam inventory pass",
                shown(path)
            ),
            Self::Drifted(path) => format!(
                "`{}` changed on disk after the snapshot captured it",
                path.display()
            ),
        }
    }
}

/// Repo-relative paths read with `/` separators in every user-facing reason,
/// on Windows too, matching how the rest of the output names files.
fn shown(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn is_normal_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #11444: a refusal names the changed file with `/` separators on every
    /// host (a Windows path arrives with `\`).
    #[test]
    fn refusal_reasons_name_paths_with_forward_slashes() {
        let path = || PathBuf::from("src\\lib.rs");
        for refusal in [
            CustodyRefusal::FindingsUnobserved(path()),
            CustodyRefusal::InventoryUnobserved(path()),
            CustodyRefusal::Moved(path()),
        ] {
            let text = refusal.describe();
            assert!(text.contains("`src/lib.rs`"), "{text}");
            assert!(!text.contains('\\'), "{text}");
        }
    }

    #[test]
    fn raw_encoding_and_conflicting_observations_remain_distinct() -> Result<(), String> {
        let path = Path::new("src/lib.rs");
        let mut sources = ConsumedRustSources::default();
        let raw = b"\xef\xbb\xbfpub fn f() {}\r\n\xff";
        sources.record(path, Some(raw));
        let expected = format!("{:x}", Sha256::digest(raw));
        if sources.digest(path).as_ref() != Some(&expected) {
            return Err("raw BOM, CRLF and invalid UTF-8 bytes were not retained".into());
        }
        sources.record(path, Some(raw));
        if sources.digest(path).as_ref() != Some(&expected) {
            return Err("identical repeated input became conflicting".into());
        }
        sources.record(path, Some(b"pub fn f() {}\n"));
        sources.record(path, Some(raw));
        if sources.digest(path).is_some() {
            return Err("conflicting observation regained authority".into());
        }
        Ok(())
    }

    #[test]
    fn first_drift_flags_changed_or_unreadable_captured_paths_only() -> Result<(), String> {
        let lib = Path::new("src/lib.rs");
        let other = Path::new("src/other.rs");
        let mut sources = ConsumedRustSources::default();
        sources.record(lib, Some(b"fn a() {}\n"));
        sources.record(Path::new("src/unavailable.rs"), None);
        let same = |_: &Path| Some(b"fn a() {}\n".to_vec());
        if sources.first_drift([lib], same).is_some() {
            return Err("identical bytes must not drift".into());
        }
        let edited = |_: &Path| Some(b"fn a() { 1; }\n".to_vec());
        if sources.first_drift([lib], edited) != Some(CustodyRefusal::Drifted(lib.to_path_buf())) {
            return Err("changed bytes must drift naming the path".into());
        }
        if sources.first_drift([lib], |_: &Path| None)
            != Some(CustodyRefusal::Drifted(lib.to_path_buf()))
        {
            return Err("an unreadable captured path must drift".into());
        }
        // Paths without a captured commitment make no claim and cannot drift.
        let unreadable = |_: &Path| None;
        if sources
            .first_drift([other, Path::new("src/unavailable.rs")], unreadable)
            .is_some()
        {
            return Err("an uncommitted path must not drift".into());
        }
        Ok(())
    }

    #[test]
    fn missing_unknown_and_outside_paths_cannot_mint_commitments() -> Result<(), String> {
        let path = Path::new("src/lib.rs");
        let mut sources = ConsumedRustSources::default();
        sources.record(path, None);
        sources.record(path, Some(b"later bytes"));
        sources.record(Path::new("../outside.rs"), Some(b"outside"));
        for candidate in [
            path,
            Path::new("unknown.rs"),
            Path::new("../outside.rs"),
            Path::new("/outside.rs"),
            Path::new(""),
        ] {
            if sources.digest(candidate).is_some() {
                return Err(format!(
                    "unavailable input acquired authority: {}",
                    candidate.display()
                ));
            }
        }
        Ok(())
    }
    fn carrier(entries: &[(&str, Option<&[u8]>)]) -> ConsumedRustSources {
        let mut sources = ConsumedRustSources::default();
        for (path, bytes) in entries {
            sources.record(Path::new(path), *bytes);
        }
        sources
    }

    #[test]
    fn agreement_requires_every_findings_path_to_match_and_binds_the_inventory_context()
    -> Result<(), String> {
        let findings = carrier(&[("src/lib.rs", Some(b"a")), ("tests/t.rs", Some(b"t"))]);
        let inventory = carrier(&[
            ("src/lib.rs", Some(b"a")),
            ("tests/t.rs", Some(b"t")),
            ("src/other.rs", Some(b"o")),
        ]);
        let agreed = ConsumedRustSources::establish_agreement(&findings, &inventory)
            .map_err(|refusal| refusal.describe())?;
        // Deterministic and independent of observation order.
        let reordered = carrier(&[
            ("src/other.rs", Some(b"o")),
            ("tests/t.rs", Some(b"t")),
            ("src/lib.rs", Some(b"a")),
        ]);
        let again = ConsumedRustSources::establish_agreement(&findings, &reordered)
            .map_err(|refusal| refusal.describe())?;
        if agreed != again {
            return Err("the commitment must not depend on observation order".into());
        }
        // Equal-result movement: an inventory-only helper changes the
        // commitment although every findings path still agrees.
        let moved_helper = carrier(&[
            ("src/lib.rs", Some(b"a")),
            ("tests/t.rs", Some(b"t")),
            ("src/other.rs", Some(b"o2")),
        ]);
        let helper = ConsumedRustSources::establish_agreement(&findings, &moved_helper)
            .map_err(|refusal| refusal.describe())?;
        if helper == agreed {
            return Err("a changed consumed helper must change the commitment".into());
        }
        // A path rename with identical bytes is a different context.
        let renamed = carrier(&[
            ("src/lib.rs", Some(b"a")),
            ("tests/t.rs", Some(b"t")),
            ("src/renamed.rs", Some(b"o")),
        ]);
        let renamed = ConsumedRustSources::establish_agreement(&findings, &renamed)
            .map_err(|refusal| refusal.describe())?;
        if renamed == agreed {
            return Err("the commitment must bind paths, not only byte multisets".into());
        }
        Ok(())
    }

    #[test]
    fn agreement_refuses_moved_missing_unobserved_and_empty_observations() -> Result<(), String> {
        let findings = carrier(&[("src/lib.rs", Some(b"a")), ("tests/t.rs", Some(b"t"))]);
        let refusal_is = |inventory: &ConsumedRustSources,
                          findings: &ConsumedRustSources,
                          want: CustodyRefusal,
                          case: &str|
         -> Result<(), String> {
            match ConsumedRustSources::establish_agreement(findings, inventory) {
                Err(got) if got == want => Ok(()),
                other => Err(format!("{case}: expected {want:?}, got {other:?}")),
            }
        };
        refusal_is(
            &carrier(&[("src/lib.rs", Some(b"a")), ("tests/t.rs", Some(b"t2"))]),
            &findings,
            CustodyRefusal::Moved(PathBuf::from("tests/t.rs")),
            "moved test bytes",
        )?;
        refusal_is(
            &carrier(&[("src/lib.rs", Some(b"a"))]),
            &findings,
            CustodyRefusal::InventoryUnobserved(PathBuf::from("tests/t.rs")),
            "a path the inventory never observed",
        )?;
        refusal_is(
            &carrier(&[("src/lib.rs", Some(b"a")), ("tests/t.rs", None)]),
            &findings,
            CustodyRefusal::InventoryUnobserved(PathBuf::from("tests/t.rs")),
            "an unavailable inventory observation",
        )?;
        let mut conflicting = carrier(&[("src/lib.rs", Some(b"a")), ("tests/t.rs", Some(b"t"))]);
        conflicting.record(Path::new("tests/t.rs"), Some(b"other"));
        refusal_is(
            &conflicting,
            &findings,
            CustodyRefusal::InventoryUnobserved(PathBuf::from("tests/t.rs")),
            "a conflicting inventory observation",
        )?;
        // An uncaptured inventory entry outside the findings still refuses:
        // the card side read it and cannot vouch for it.
        refusal_is(
            &carrier(&[
                ("src/lib.rs", Some(b"a")),
                ("tests/t.rs", Some(b"t")),
                ("src/gone.rs", None),
            ]),
            &findings,
            CustodyRefusal::InventoryUnobserved(PathBuf::from("src/gone.rs")),
            "an unavailable inventory-only observation",
        )?;
        // An escaped or absolute key is not recordable, so it is unobserved.
        let escaped = carrier(&[("src/lib.rs", Some(b"a")), ("../t.rs", Some(b"t"))]);
        refusal_is(
            &escaped,
            &carrier(&[("src/lib.rs", Some(b"a")), ("tests/t.rs", Some(b"t"))]),
            CustodyRefusal::InventoryUnobserved(PathBuf::from("tests/t.rs")),
            "an out-of-root inventory key",
        )?;
        refusal_is(
            &carrier(&[("src/lib.rs", Some(b"a")), ("tests/t.rs", Some(b"t"))]),
            &carrier(&[("src/lib.rs", None), ("tests/t.rs", Some(b"t"))]),
            CustodyRefusal::FindingsUnobserved(PathBuf::from("src/lib.rs")),
            "an unavailable findings observation",
        )?;
        refusal_is(
            &carrier(&[("src/lib.rs", Some(b"a"))]),
            &ConsumedRustSources::default(),
            CustodyRefusal::FindingsObservedNothing,
            "an empty findings carrier",
        )?;
        // A non-normal key is recorded as an uncaptured observation, so it
        // refuses on either side instead of being dropped.
        refusal_is(
            &carrier(&[
                ("src/lib.rs", Some(b"a")),
                ("tests/t.rs", Some(b"t")),
                ("../escaped.rs", Some(b"x")),
            ]),
            &findings,
            CustodyRefusal::InventoryUnobserved(PathBuf::from("../escaped.rs")),
            "a recorded out-of-root inventory key",
        )?;
        refusal_is(
            &findings,
            &carrier(&[("src/lib.rs", Some(b"a")), ("/abs/t.rs", Some(b"t"))]),
            CustodyRefusal::FindingsUnobserved(PathBuf::from("/abs/t.rs")),
            "a recorded absolute findings key",
        )?;
        // Absorbing a conflicting carrier never regains authority.
        let mut folded = carrier(&[("src/lib.rs", Some(b"a")), ("tests/t.rs", Some(b"t"))]);
        folded.absorb(carrier(&[("tests/t.rs", Some(b"other"))]));
        refusal_is(
            &carrier(&[("src/lib.rs", Some(b"a")), ("tests/t.rs", Some(b"t"))]),
            &folded,
            CustodyRefusal::FindingsUnobserved(PathBuf::from("tests/t.rs")),
            "a conflicting findings observation",
        )?;
        Ok(())
    }
}

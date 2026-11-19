//! #15103: a patch git wrote under `diff.mnemonicPrefix=true` names its two
//! sides `i/`, `w/`, `c/` or `o/` instead of `a/` and `b/`. Read with the
//! prefix as part of the path, every changed file looks absent from the
//! working tree. This module recognizes such a section from its `diff --git`
//! header and respells its header and `---`/`+++` markers with the canonical
//! prefixes, so the rest of the parser reads one spelling.
//!
//! The reading is taken only when the header carries two *different* mnemonic
//! prefixes over the *same* remaining path. A `--no-prefix` patch for a real
//! top-level directory named `i/` repeats one token on both sides, and a
//! `--no-prefix` rename between `i/x` and `w/x` announces itself with rename
//! metadata, which withdraws the reading (see [`MnemonicSides::raw_header`]).
//! Arbitrary `--src-prefix`/`--dst-prefix` values are not guessed.

use std::borrow::Cow;

/// The prefix pairs git writes under `diff.mnemonicPrefix` (index, working
/// tree, commit, object), in both orders because `-R` swaps the sides.
const PAIRS: [(u8, u8); 8] = [
    (b'i', b'w'),
    (b'w', b'i'),
    (b'c', b'w'),
    (b'w', b'c'),
    (b'c', b'i'),
    (b'i', b'c'),
    (b'o', b'w'),
    (b'w', b'o'),
];

/// One `diff --git` section whose sides use a mnemonic prefix pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MnemonicSides {
    old: u8,
    new: u8,
    /// The shared path as the header spells it, with the closing quote of a
    /// quoted token.
    path: String,
    raw_header: String,
    canonical_header: String,
}

impl MnemonicSides {
    /// Read `raw` as a `diff --git` header with mnemonic side prefixes.
    pub(super) fn of_header(raw: &str) -> Option<Self> {
        let line = raw.trim_end_matches('\r');
        let marker = line.strip_prefix("diff --git ")?;
        // Both sides spell the same path behind a one-letter prefix, so the
        // separating space sits exactly in the middle. A space is one byte
        // and never part of a multi-byte sequence, so both halves are text.
        let middle = marker.len() / 2;
        if marker.len() % 2 == 0 || marker.as_bytes().get(middle) != Some(&b' ') {
            return None;
        }
        let (old_token, new_token) = (marker.get(..middle)?, marker.get(middle + 1..)?);
        // Git quotes each side independently, but equal paths quote alike.
        let lead = usize::from(old_token.starts_with('"'));
        let side = |token: &str| {
            let bytes = token.as_bytes();
            (lead == usize::from(token.starts_with('"')) && bytes.get(lead + 1) == Some(&b'/'))
                .then(|| bytes.get(lead).copied())
                .flatten()
        };
        let (old, new) = (side(old_token)?, side(new_token)?);
        let (old_path, new_path) = (old_token.get(lead + 2..)?, new_token.get(lead + 2..)?);
        if !PAIRS.contains(&(old, new)) || old_path.is_empty() || old_path != new_path {
            return None;
        }
        let quote = &old_token[..lead];
        let ending = &raw[line.len()..];
        Some(Self {
            old,
            new,
            path: old_path.to_owned(),
            raw_header: raw.to_owned(),
            canonical_header: format!("diff --git {quote}a/{old_path} {quote}b/{new_path}{ending}"),
        })
    }

    /// The header as git wrote it, for a section that turns out to carry
    /// rename metadata: its two tokens are then two real paths.
    pub(super) fn raw_header(&self) -> &str {
        &self.raw_header
    }

    /// The header with `a/` and `b/` in place of the mnemonic prefixes.
    pub(super) fn canonical_header(&self) -> &str {
        &self.canonical_header
    }

    /// A `--- ` or `+++ ` marker of this section with its mnemonic prefix
    /// respelled as `a/` or `b/`; any other line is returned unchanged. The
    /// marker must name the header's own path (git may add a tab and a
    /// timestamp, or a carriage return, behind it), so a later plain section
    /// for a real `w/` directory is not this section's marker.
    pub(super) fn canonical_marker<'a>(&self, raw: &'a str) -> Cow<'a, str> {
        for (marker, side, canonical) in [("--- ", self.old, 'a'), ("+++ ", self.new, 'b')] {
            let Some(token) = raw.strip_prefix(marker) else {
                continue;
            };
            let lead = usize::from(token.starts_with('"'));
            let bytes = token.as_bytes();
            let names_this_path = token
                .get(lead + 2..)
                .and_then(|rest| rest.strip_prefix(self.path.as_str()))
                .is_some_and(|tail| {
                    tail.is_empty() || tail.starts_with('\t') || tail.starts_with('\r')
                });
            if bytes.get(lead) == Some(&side)
                && bytes.get(lead + 1) == Some(&b'/')
                && names_this_path
            {
                return Cow::Owned(format!(
                    "{marker}{}{canonical}{}",
                    &token[..lead],
                    &token[lead + 1..]
                ));
            }
        }
        Cow::Borrowed(raw)
    }
}

#[cfg(test)]
mod tests {
    use super::MnemonicSides;
    use crate::analysis::diff::{parse_unified_diff, parse_unified_diff_bounded_with_metadata};

    /// A plain section (no `diff --git` header) after a mnemonic section
    /// names a real `w/` directory; it must not inherit the earlier section's
    /// prefix reading.
    #[test]
    fn a_later_plain_section_keeps_its_real_directory() -> Result<(), String> {
        let input = "diff --git i/src/first.rs w/src/first.rs\n--- i/src/first.rs\n+++ w/src/first.rs\n@@ -1 +1 @@\n-old\n+new\n--- w/lib.rs\n+++ w/lib.rs\n@@ -2 +2 @@\n-before\n+after\n--- w/src/first.rs\n+++ w/src/first.rs\n@@ -3 +3 @@\n-x\n+y\n";
        let bounded = parse_unified_diff_bounded_with_metadata(input)?;
        for (route, files) in [
            ("unbounded", parse_unified_diff(input)),
            ("bounded", bounded.changed_files),
        ] {
            let seen: Vec<(String, Vec<usize>)> = files
                .iter()
                .map(|file| {
                    (
                        file.path.to_string_lossy().replace('\\', "/"),
                        file.added_lines.iter().map(|line| line.line).collect(),
                    )
                })
                .collect();
            assert_eq!(
                seen,
                [
                    ("src/first.rs".to_string(), vec![1]),
                    ("w/lib.rs".to_string(), vec![2]),
                    ("w/src/first.rs".to_string(), vec![3]),
                ],
                "{route}"
            );
        }
        Ok(())
    }

    #[test]
    fn mnemonic_pairs_over_one_path_are_respelled() {
        for (raw, canonical) in [
            (
                "diff --git i/src/lib.rs w/src/lib.rs",
                "diff --git a/src/lib.rs b/src/lib.rs",
            ),
            (
                "diff --git w/src/lib.rs c/src/lib.rs\r",
                "diff --git a/src/lib.rs b/src/lib.rs\r",
            ),
            (
                "diff --git c/my file.rs i/my file.rs",
                "diff --git a/my file.rs b/my file.rs",
            ),
            (
                "diff --git \"o/s\\303\\251.rs\" \"w/s\\303\\251.rs\"",
                "diff --git \"a/s\\303\\251.rs\" \"b/s\\303\\251.rs\"",
            ),
            ("diff --git i/é.rs w/é.rs", "diff --git a/é.rs b/é.rs"),
        ] {
            let sides = MnemonicSides::of_header(raw);
            assert_eq!(
                sides.as_ref().map(MnemonicSides::canonical_header),
                Some(canonical),
                "{raw}"
            );
            assert_eq!(sides.as_ref().map(MnemonicSides::raw_header), Some(raw));
        }
    }

    #[test]
    fn other_headers_keep_their_reading() {
        for raw in [
            // Canonical and prefix-free patches.
            "diff --git a/src/lib.rs b/src/lib.rs",
            "diff --git src/lib.rs src/lib.rs",
            // A real `i/` directory under `--no-prefix`: one token twice.
            "diff --git i/lib.rs i/lib.rs",
            // Different paths: a rename or two real directories.
            "diff --git i/old.rs w/new.rs",
            "diff --git i/lib.rs w/lib.rs.bak",
            // Not a mnemonic pair.
            "diff --git w/lib.rs w/lib.rs",
            "diff --git x/lib.rs y/lib.rs",
            "diff --git 1/lib.rs 2/lib.rs",
            "diff --git old/lib.rs new/lib.rs",
            // Mixed quoting, an empty path, other header kinds.
            "diff --git \"i/lib.rs\" w/lib.rs",
            "diff --git i/ w/",
            "diff --cc i/lib.rs w/lib.rs",
            "--- i/lib.rs",
        ] {
            assert_eq!(MnemonicSides::of_header(raw), None, "{raw}");
        }
    }

    #[test]
    fn only_this_sections_own_markers_are_respelled() -> Result<(), String> {
        let sides = MnemonicSides::of_header("diff --git i/src/lib.rs w/src/lib.rs")
            .ok_or("header not recognized")?;
        for (raw, canonical) in [
            ("--- i/src/lib.rs", "--- a/src/lib.rs"),
            ("+++ w/src/lib.rs", "+++ b/src/lib.rs"),
            (
                "+++ w/src/lib.rs\t2026-01-01",
                "+++ b/src/lib.rs\t2026-01-01",
            ),
            // The other side's letter, the null device and other lines stay.
            ("--- w/src/lib.rs", "--- w/src/lib.rs"),
            ("+++ i/src/lib.rs", "+++ i/src/lib.rs"),
            ("--- /dev/null", "--- /dev/null"),
            // Another path under the same letter is not this section's.
            ("--- i/lib.rs", "--- i/lib.rs"),
            ("+++ w/src/lib.rs.orig", "+++ w/src/lib.rs.orig"),
            ("+++ w/src/lib.rs\r", "+++ b/src/lib.rs\r"),
            ("+++ b/src/lib.rs", "+++ b/src/lib.rs"),
            ("rename from i/src/lib.rs", "rename from i/src/lib.rs"),
        ] {
            assert_eq!(sides.canonical_marker(raw), canonical, "{raw}");
        }
        // Git quotes a path the same way in the header and its markers.
        let quoted =
            MnemonicSides::of_header("diff --git \"i/s\\303\\251.rs\" \"w/s\\303\\251.rs\"")
                .ok_or("quoted header not recognized")?;
        assert_eq!(
            quoted.canonical_marker("+++ \"w/s\\303\\251.rs\""),
            "+++ \"b/s\\303\\251.rs\""
        );
        assert_eq!(
            quoted.canonical_marker("+++ w/s\\303\\251.rs"),
            "+++ w/s\\303\\251.rs"
        );
        Ok(())
    }
}

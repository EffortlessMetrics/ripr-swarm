//! Shared native path identity; presentation stays in the facade.

use std::path::Path;

/// Whether this host's default filesystem compares path names without regard
/// to case: Windows (NTFS) and macOS (default APFS). Linux stays
/// case-sensitive. Assumes the default case-insensitive APFS volume on macOS.
/// This is the single host predicate for case-folding path
/// identity; separator and verbatim-prefix handling stays keyed on Windows.
pub const fn host_paths_case_insensitive() -> bool {
    cfg!(any(windows, target_os = "macos"))
}

/// Whether this host's default filesystem also treats canonically equivalent
/// Unicode spellings (NFC/NFD) of a name as one file: macOS APFS. Windows
/// NTFS and Linux preserve the exact code points, so they stay distinct.
pub const fn host_paths_normalization_insensitive() -> bool {
    cfg!(target_os = "macos")
}

/// Shared owner of path-identity keys: two paths name the same file for a
/// path-string comparator exactly when their keys are equal.
/// `backslash_is_separator` rewrites `\` to `/` and is `true` only where `\` is
/// a separator (Windows, or a drive/UNC spelling); on Unix `\` is an ordinary
/// filename byte, so `a\b.rs` and `a/b.rs` stay distinct. `case_insensitive`
/// folds case per code point, conservatively: a character folds to its
/// lowercase only when that is a single code point that upper-cases straight
/// back to it, so one-way folds never merge names NTFS keeps distinct: the
/// Kelvin, Ohm and Angstrom signs, and `U+1E9E` (it lowercases to `ß`, but `ß`
/// uppercases to `SS`, so the round trip fails). Case tables are the current
/// Rust Unicode tables, so newer letters (Georgian Mtavruli, Cherokee small
/// letters) fold and could over-merge on an old NTFS volume whose `$UpCase`
/// predates them (accepted; very narrow). This
/// can still miss equivalences the filesystem makes (e.g. `ς` vs `σ`), which
/// only keeps files apart. On a host that also ignores normalization
/// ([`host_paths_normalization_insensitive`], macOS) it additionally folds
/// canonically equivalent NFC/NFD spellings through the partial table in
/// `path_unicode`. A caller that passes `false` for both never folds.
/// A path that is not valid UTF-8 keys on its exact encoded bytes
/// behind a NUL marker (a path cannot contain NUL, so it cannot collide with a
/// UTF-8 path's key) and is never folded or rewritten, so two distinct
/// non-UTF-8 paths never collapse.
pub fn path_identity_key_on(
    path: &Path,
    backslash_is_separator: bool,
    case_insensitive: bool,
) -> String {
    path_identity_key_with(
        path,
        backslash_is_separator,
        case_insensitive,
        case_insensitive && host_paths_normalization_insensitive(),
    )
}

/// Lowercase `ch` only when the fold is a reversible single-code-point pair.
/// `U+0130` (multi-code-point lowercase) and one-way folds (`U+212A` Kelvin,
/// `U+1E9E`) keep their own spelling so they cannot merge with another name.
fn conservative_lowercase(ch: char) -> char {
    let mut lower = ch.to_lowercase();
    let (Some(single), None) = (lower.next(), lower.next()) else {
        return ch;
    };
    let mut upper = single.to_uppercase();
    match (upper.next(), upper.next()) {
        (Some(back), None) if back == ch => single,
        _ => ch,
    }
}

/// [`path_identity_key_on`] with canonical-equivalence folding explicit, so
/// the macOS rule is testable on every host. `normalization_insensitive` is
/// valid on its own (`case_insensitive == false`): it then folds only
/// canonical equivalence. The fold is one pass of per-character NFD from the
/// partial `path_unicode` table with no canonical reordering of combining
/// marks, so Hangul, Indic scripts and out-of-order mark sequences are
/// intentionally not folded and stay distinct (fail closed).
pub fn path_identity_key_with(
    path: &Path,
    backslash_is_separator: bool,
    case_insensitive: bool,
    normalization_insensitive: bool,
) -> String {
    let Some(text) = path.to_str() else {
        let mut key = String::from("\0");
        for byte in path.as_os_str().as_encoded_bytes() {
            key.push_str(&format!("{byte:02x}"));
        }
        return key;
    };
    let text = if backslash_is_separator {
        text.replace('\\', "/")
    } else {
        text.to_string()
    };
    if !case_insensitive && !normalization_insensitive {
        return text;
    }
    let mut key = String::with_capacity(text.len());
    for ch in text.chars() {
        let mut piece = String::new();
        if normalization_insensitive {
            crate::path_unicode::decompose_into(ch, &mut piece);
        } else {
            piece.push(ch);
        }
        for part in piece.chars() {
            key.push(if case_insensitive {
                conservative_lowercase(part)
            } else {
                part
            });
        }
    }
    key
}

/// [`path_identity_key_on`] with the host rules: `\` separates only on Windows
/// and case folds per [`host_paths_case_insensitive`].
pub fn path_identity_key(path: &Path) -> String {
    path_identity_key_on(path, cfg!(windows), host_paths_case_insensitive())
}

#[cfg(test)]
mod tests {
    use super::{path_identity_key_on, path_identity_key_with};
    use std::path::Path;

    #[test]
    fn path_identity_key_is_one_rule_for_separator_case_and_bytes() {
        let upper = Path::new("Src/A\\B.rs");
        let lower = Path::new("src/a/b.rs");
        // Windows-flagged seam: separator rewrite plus ASCII fold.
        assert_eq!(
            path_identity_key_on(upper, true, true),
            path_identity_key_on(lower, true, true)
        );
        // Unix seam: `\` is a filename byte, case is significant.
        assert_ne!(
            path_identity_key_on(upper, false, false),
            path_identity_key_on(lower, false, false)
        );
        assert_ne!(
            path_identity_key_on(Path::new("a\\b.rs"), false, true),
            path_identity_key_on(Path::new("a/b.rs"), false, true)
        );
        // Non-ASCII case variants fold only where the host is case-insensitive.
        assert_eq!(
            path_identity_key_on(Path::new("\u{c9}.rs"), true, true),
            path_identity_key_on(Path::new("\u{e9}.rs"), true, true)
        );
        assert_ne!(
            path_identity_key_on(Path::new("\u{c9}.rs"), false, false),
            path_identity_key_on(Path::new("\u{e9}.rs"), false, false)
        );
    }

    #[test]
    fn nfc_nfd_fold_only_when_normalization_insensitive() {
        let nfc = Path::new("caf\u{e9}/\u{c5}.rs");
        let nfd = Path::new("cafe\u{301}/A\u{30a}.rs");
        // macOS rule (explicit seam): equivalent spellings share a key,
        // including combined with case folding.
        assert_eq!(
            path_identity_key_with(nfc, false, true, true),
            path_identity_key_with(nfd, false, true, true)
        );
        assert_eq!(
            path_identity_key_with(Path::new("CAF\u{c9}.rs"), false, true, true),
            path_identity_key_with(Path::new("cafe\u{301}.rs"), false, true, true)
        );
        // Linux/Windows rule: distinct files keep distinct keys.
        assert_ne!(
            path_identity_key_with(nfc, false, false, false),
            path_identity_key_with(nfd, false, false, false)
        );
        assert_ne!(
            path_identity_key_with(nfc, true, true, false),
            path_identity_key_with(nfd, true, true, false)
        );
        // Genuinely different names never merge under the macOS rule.
        assert_ne!(
            path_identity_key_with(Path::new("\u{e9}.rs"), false, true, true),
            path_identity_key_with(Path::new("e.rs"), false, true, true)
        );
        // U+0130 has a multi-code-point lowercase: not folded to plain `i`.
        assert_ne!(
            path_identity_key_with(Path::new("\u{130}.rs"), false, true, false),
            path_identity_key_with(Path::new("i.rs"), false, true, false)
        );
        assert!(crate::path_unicode::table_is_sorted_and_unique());
    }

    #[test]
    fn one_way_case_folds_never_merge_distinct_windows_names() {
        // NTFS keeps these pairs distinct; folding them would merge files.
        for (left, right) in [
            ("\u{212a}.rs", "k.rs"),
            ("\u{2126}.rs", "\u{3c9}.rs"),
            ("\u{212b}.rs", "\u{e5}.rs"),
            ("\u{1e9e}.rs", "\u{df}.rs"),
        ] {
            for norm in [false, true] {
                assert_ne!(
                    path_identity_key_with(Path::new(left), true, true, norm),
                    path_identity_key_with(Path::new(right), true, true, norm),
                    "{left} vs {right} norm={norm}"
                );
            }
        }
        // U+0130 has a multi-code-point lowercase: without normalization it
        // stays itself and never merges with `i` + combining dot; with the
        // canonical rule it is `I` + dot, which is a true equivalence.
        let dotted = Path::new("\u{130}.rs");
        let spelled = Path::new("i\u{307}.rs");
        assert_ne!(
            path_identity_key_with(dotted, true, true, false),
            path_identity_key_with(spelled, true, true, false)
        );
        assert_eq!(
            path_identity_key_with(dotted, true, true, true),
            path_identity_key_with(spelled, true, true, true)
        );
        // Known conservative miss: NTFS `$UpCase` treats sigma and final
        // sigma as one name; we keep them apart (a missed merge, never a
        // wrong one).
        assert_ne!(
            path_identity_key_with(Path::new("\u{3c3}.rs"), true, true, false),
            path_identity_key_with(Path::new("\u{3c2}.rs"), true, true, false)
        );
    }

    /// The host-default entry point must pick the macOS rule through
    /// `host_paths_normalization_insensitive`; only macOS CI can observe it.
    #[cfg(target_os = "macos")]
    #[test]
    fn host_default_key_folds_nfc_nfd_on_macos() {
        assert!(super::host_paths_normalization_insensitive());
        assert_eq!(
            path_identity_key_on(Path::new("caf\u{e9}"), false, true),
            path_identity_key_on(Path::new("cafe\u{301}"), false, true)
        );
    }

    #[test]
    fn reversible_non_ascii_case_pairs_fold() {
        for (upper, lower) in [
            ("\u{393}.rs", "\u{3b3}.rs"),
            ("\u{3a3}.rs", "\u{3c3}.rs"),
            ("\u{416}.rs", "\u{436}.rs"),
            ("\u{c9}.rs", "\u{e9}.rs"),
        ] {
            assert_eq!(
                path_identity_key_with(Path::new(upper), false, true, false),
                path_identity_key_with(Path::new(lower), false, true, false)
            );
            assert_ne!(
                path_identity_key_with(Path::new(upper), false, false, false),
                path_identity_key_with(Path::new(lower), false, false, false)
            );
        }
    }

    #[test]
    fn normalization_folding_is_independent_of_case_and_stays_partial() {
        // case=false + norm=true: canonical equivalence folds, case does not.
        assert_eq!(
            path_identity_key_with(Path::new("\u{e9}.rs"), false, false, true),
            path_identity_key_with(Path::new("e\u{301}.rs"), false, false, true)
        );
        assert_ne!(
            path_identity_key_with(Path::new("\u{c9}.rs"), false, false, true),
            path_identity_key_with(Path::new("\u{e9}.rs"), false, false, true)
        );
        // Deep decomposition: U+1E69 = s + dot below + dot above.
        assert_eq!(
            path_identity_key_with(Path::new("\u{1e69}"), false, true, true),
            path_identity_key_with(Path::new("s\u{323}\u{307}"), false, true, true)
        );
        // Greek tonos and Cyrillic short i.
        assert_eq!(
            path_identity_key_with(Path::new("\u{3ac}"), false, true, true),
            path_identity_key_with(Path::new("\u{3b1}\u{301}"), false, true, true)
        );
        assert_eq!(
            path_identity_key_with(Path::new("\u{419}"), false, true, true),
            path_identity_key_with(Path::new("\u{438}\u{306}"), false, true, true)
        );
        // Singleton canonical decomposition (U+0374 -> U+02B9) is folded.
        assert_eq!(
            path_identity_key_with(Path::new("\u{374}"), false, false, true),
            path_identity_key_with(Path::new("\u{2b9}"), false, false, true)
        );
        // Outside the table (Hangul syllable vs jamo, Devanagari composition
        // exclusion, Angstrom sign) stays distinct: partial coverage fails
        // closed.
        for (left, right) in [
            ("\u{ac00}", "\u{1100}\u{1161}"),
            ("\u{958}", "\u{915}\u{93c}"),
            ("\u{212b}", "A\u{30a}"),
        ] {
            assert_ne!(
                path_identity_key_with(Path::new(left), false, true, true),
                path_identity_key_with(Path::new(right), false, true, true),
                "{left} vs {right}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn path_identity_key_keeps_distinct_non_utf8_paths_distinct() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        let first = Path::new(OsStr::from_bytes(b"/w/a\xFF"));
        let second = Path::new(OsStr::from_bytes(b"/w/a\xFE"));
        assert_ne!(
            path_identity_key_on(first, false, false),
            path_identity_key_on(second, false, false)
        );
        assert_ne!(
            path_identity_key_on(first, true, true),
            path_identity_key_on(Path::new("/w/a\u{fffd}"), true, true)
        );
    }

    #[test]
    fn host_predicate_matches_target_platform() {
        assert_eq!(
            super::host_paths_case_insensitive(),
            cfg!(any(windows, target_os = "macos"))
        );
        if cfg!(target_os = "linux") {
            assert!(!super::host_paths_case_insensitive());
        }
    }
}

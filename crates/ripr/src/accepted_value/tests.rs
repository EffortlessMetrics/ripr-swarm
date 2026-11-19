use super::*;

#[test]
fn a_flag_abbreviated_word_by_word_is_suggested() {
    // #15162: `first-pr` spells it `--out-dir`; `check` spells it
    // `--output-dir`. Edit distance 3 is over the 7-character budget.
    let candidates = ["--output-dir", "--format", "--base"];
    assert_eq!(
        closest("--out-dir", candidates.iter().copied()),
        Some("--output-dir")
    );
}

#[test]
fn word_by_word_abbreviation_needs_matching_words() {
    // Different word count, a non-prefix word, an empty word and a
    // single word each fall through to the other tiers, which reject
    // these inputs.
    let candidates = ["--output-dir"];
    for input in [
        "--out-dir-x",
        "--out",
        "--put-dir",
        "--out--dir",
        "--outdir",
    ] {
        assert!(
            !abbreviates_word_by_word(input.trim_start_matches('-'), "output-dir"),
            "{input}"
        );
    }
    assert_eq!(closest("--put-dir", candidates.iter().copied()), None);
    assert_eq!(closest("--outdir", candidates.iter().copied()), None);
}

#[test]
fn whole_prefix_still_beats_word_by_word_and_values_do_not_use_it() {
    // `--out-m` is a whole-name prefix of `--out-mode` and a word-by-word
    // abbreviation of `--output-mode`; the whole-name prefix wins.
    let flags = ["--output-mode", "--out-mode"];
    assert_eq!(
        closest("--out-m", flags.iter().copied()),
        Some("--out-mode")
    );
    // Enum values are not flags: the word-by-word rule is flag-only.
    assert_eq!(closest("p-s", ["pr-summary"].iter().copied()), None);
    assert_eq!(
        closest("--p-s", ["--pr-summary"].iter().copied()),
        Some("--pr-summary")
    );
}

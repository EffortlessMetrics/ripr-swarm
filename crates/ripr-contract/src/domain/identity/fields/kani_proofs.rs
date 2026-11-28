//! Kani bounded-model-checking proofs for identity-field surface matching.
//!
//! Compiled only under `cfg(kani)`; normal builds never see this module.
//! Run with `cargo kani -p ripr --harness <name>`.

use super::surface_matches;

const ALPHABET: [char; 4] = ['a', '.', '/', '\\'];

/// Symbolic ASCII path text of up to `N` bytes held in `buf`.
fn any_path<const N: usize>(buf: &mut [u8; N]) -> usize {
    let len: usize = kani::any();
    kani::assume(len <= N);
    let mut index = 0;
    while index < N {
        let pick: usize = kani::any();
        kani::assume(pick < ALPHABET.len());
        buf[index] = ALPHABET[pick] as u8;
        index += 1;
    }
    len
}

/// A surface matches only when it is identical or a path-separator-anchored
/// suffix: a truncated suffix (`schema.json` vs `check.schema.json`) never
/// inherits a disposition. Both directions are verified, so the matcher accepts
/// exactly that language.
///
/// Bounds: registered up to 2 characters, observed up to 4, alphabet
/// `{a, ., /, \}`.
#[kani::proof]
#[kani::unwind(8)]
fn surface_matches_accepts_exactly_identical_or_separator_anchored_suffix() {
    let mut registered_buf = [0u8; 2];
    let mut observed_buf = [0u8; 4];
    let registered_len = any_path(&mut registered_buf);
    let observed_len = any_path(&mut observed_buf);
    let (Ok(registered), Ok(observed)) = (
        std::str::from_utf8(&registered_buf[..registered_len]),
        std::str::from_utf8(&observed_buf[..observed_len]),
    ) else {
        return;
    };
    let matched = surface_matches(registered, observed);

    let anchored = observed.len() > registered.len()
        && observed.ends_with(registered)
        && matches!(
            observed.as_bytes()[observed.len() - registered.len() - 1],
            b'/' | b'\\'
        );
    kani::cover!(
        matched && observed != registered,
        "anchored suffix match taken"
    );
    kani::cover!(
        !matched && observed.ends_with(registered) && !registered.is_empty(),
        "unanchored suffix rejected"
    );
    assert_eq!(matched, observed == registered || anchored);
}

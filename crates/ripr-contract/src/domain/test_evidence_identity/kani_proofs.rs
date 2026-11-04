//! Kani bounded-model-checking proofs for missing-discriminator identity.
//!
//! Compiled only under `cfg(kani)`; normal builds never see this module.
//! Run with `cargo kani -p ripr --harness <name>`.
//!
//! Strings are built from a small symbolic alphabet that contains the
//! characters the framing treats specially (digits and `:`). The bounds are
//! small to stay tractable: the injectivity proof covers lists of exactly two
//! items, each at most one character, so every frame is a single-digit length
//! plus a body of at most one character. A body that itself looks like a frame
//! (for example `1:a`), a multi-digit length, and lists of other lengths are
//! NOT covered; this is a bounded check, not a general collision-freedom proof.

use super::encode_missing_discriminators;

const ENCODING_ALPHABET: [char; 4] = ['1', '2', ':', 'a'];

/// Symbolic text of exactly `N` slots; a slot is either empty or one
/// character of `alphabet`. The text holds `0..=N` characters and every
/// loop here has a concrete bound.
fn any_text<const A: usize, const N: usize>(alphabet: &[char; A]) -> String {
    let mut value = String::new();
    let mut ended = false;
    let mut slot = 0;
    while slot < N {
        let pick: usize = kani::any();
        kani::assume(pick <= A);
        if pick == A {
            ended = true;
        }
        if !ended {
            value.push(alphabet[pick]);
        }
        slot += 1;
    }
    value
}

/// Length-prefix framing is injective: two lists encode to the same string
/// only when they are the same list, so no concatenation of items (including
/// items that contain digits or `:`) can collide with a different partition.
///
/// Bounds: exactly 2 items per list, each of up to 1 character from the
/// alphabet `{1, 2, :, a}`. Items may be empty here, which is stricter than production
/// (canonical items are never empty).
#[kani::proof]
#[kani::unwind(12)]
fn encode_missing_discriminators_is_injective() {
    let left = [
        any_text::<4, 1>(&ENCODING_ALPHABET),
        any_text::<4, 1>(&ENCODING_ALPHABET),
    ];
    let right = [
        any_text::<4, 1>(&ENCODING_ALPHABET),
        any_text::<4, 1>(&ENCODING_ALPHABET),
    ];
    kani::cover!(
        left != right && left[0].is_empty() != right[0].is_empty(),
        "lists differing in item boundaries compared"
    );
    kani::cover!(left == right && !left[0].is_empty(), "equal lists compared");
    if encode_missing_discriminators(&left) == encode_missing_discriminators(&right) {
        assert_eq!(left, right);
    }
}

// Not verified here (not tractable with Kani 0.68 / CBMC 6.11 at useful bounds):
// `canonicalize_missing_discriminators` (stable `sort` + `dedup` over
// `Vec<String>` on top of the normalizer) and `normalize_discriminator_text`
// idempotence (`Peekable<Chars>` clones and UTF-8 decoding). Both exceeded a
// 25-40 minute budget at 2-3 characters of input without terminating, so no
// harness is committed for them; the unit tests above remain their evidence.

//! A related assertion that still pins the old raised type after a raised-type
//! replacement (#13622).
//!
//! The Python analyzer records the fact as advisory finding evidence. It never
//! changes the exposure class: a pre-existing `pytest.raises(Old, ...)` does
//! notice `raise Old(...)` -> `raise New(...)`, because it fails on the edit
//! (RIPR-SPEC-0238 example 14). The gap-bound repair closure
//! (RIPR-SPEC-0310) reads the fact back, so a test written during the repair
//! that fails against the new raise is not reported as closing the gap.
//!
//! The evidence line is the single wire form; [`StaleRaisedTypePin::parse`]
//! accepts exactly what [`StaleRaisedTypePin::evidence_line`] writes.

/// Evidence-line prefix of the fact.
pub(crate) const STALE_RAISED_TYPE_PIN_PREFIX: &str = "stale_raised_type_pin: ";

const PINS_OLD: &str = "` pins the old raised type `";
const NOW_RAISES: &str = "`; the changed raise now raises `";

/// One related assertion that expects `old_type` exactly while the changed
/// line now raises `new_type`, which is neither `old_type` nor a statically
/// known subclass of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StaleRaisedTypePin {
    /// The assertion source text, for example `pytest.raises(ValueError, match="m")`.
    pub(crate) assertion: String,
    pub(crate) old_type: String,
    pub(crate) new_type: String,
    /// The related test that carries the assertion.
    pub(crate) test: String,
}

impl StaleRaisedTypePin {
    /// The finding evidence line for this fact.
    pub(crate) fn evidence_line(&self) -> String {
        format!(
            "{STALE_RAISED_TYPE_PIN_PREFIX}`{}{PINS_OLD}{}{NOW_RAISES}{}` ({})",
            self.assertion, self.old_type, self.new_type, self.test
        )
    }

    /// Reads one evidence line back; `None` for any other line or shape.
    pub(crate) fn parse(line: &str) -> Option<Self> {
        let rest = line.strip_prefix(STALE_RAISED_TYPE_PIN_PREFIX)?;
        let (rest, test) = rest.strip_suffix(')')?.rsplit_once("` (")?;
        let (rest, new_type) = rest.rsplit_once(NOW_RAISES)?;
        let (assertion, old_type) = rest.rsplit_once(PINS_OLD)?;
        let assertion = assertion.strip_prefix('`')?;
        let identifier = |text: &str| {
            !text.is_empty()
                && !text.starts_with(|ch: char| ch.is_ascii_digit())
                && text.chars().all(|ch| ch.is_alphanumeric() || ch == '_')
        };
        (identifier(old_type)
            && identifier(new_type)
            && old_type != new_type
            && identifier(test)
            && !assertion.trim().is_empty())
        .then(|| Self {
            assertion: assertion.to_string(),
            old_type: old_type.to_string(),
            new_type: new_type.to_string(),
            test: test.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::StaleRaisedTypePin;

    #[test]
    fn evidence_line_round_trips_and_rejects_other_shapes() -> Result<(), String> {
        let pin = StaleRaisedTypePin {
            assertion: "pytest.raises(ValueError, match=\"neg (qty)\")".to_string(),
            old_type: "ValueError".to_string(),
            new_type: "TypeError".to_string(),
            test: "test_neg".to_string(),
        };
        let line = pin.evidence_line();
        if line
            != "stale_raised_type_pin: `pytest.raises(ValueError, match=\"neg (qty)\")` pins the old raised type `ValueError`; the changed raise now raises `TypeError` (test_neg)"
        {
            return Err(format!("unexpected evidence line: {line}"));
        }
        if StaleRaisedTypePin::parse(&line).as_ref() != Some(&pin) {
            return Err(format!("evidence line did not round-trip: {line}"));
        }
        for other in [
            "exception_variant_read: `pytest.raises(TypeError)` rejects `ValueError` and accepts `TypeError` at the changed raise (test_neg)",
            "stale_raised_type_pin: `x` pins the old raised type `A`; the changed raise now raises `A` (t)",
            "stale_raised_type_pin: `x` pins the old raised type `A.B`; the changed raise now raises `C` (t)",
            "stale_raised_type_pin: `` pins the old raised type `A`; the changed raise now raises `C` (t)",
            "stale_raised_type_pin: `x` pins the old raised type `A`; the changed raise now raises `C`",
        ] {
            if StaleRaisedTypePin::parse(other).is_some() {
                return Err(format!("parsed a line it must refuse: {other}"));
            }
        }
        Ok(())
    }
}

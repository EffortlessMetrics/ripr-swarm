pub(crate) const RECEIPT_MISSING: &str = "receipt_missing";
pub(crate) const RECEIPT_FOUND: &str = "receipt_found";
pub(crate) const RECEIPT_STALE: &str = "receipt_stale";
pub(crate) const RECEIPT_GAP_MISMATCH: &str = "receipt_gap_mismatch";
pub(crate) const RECEIPT_MOVEMENT_IMPROVED: &str = "receipt_movement_improved";
pub(crate) const RECEIPT_MOVEMENT_UNCHANGED: &str = "receipt_movement_unchanged";
pub(crate) const RECEIPT_NOT_APPLICABLE: &str = "receipt_not_applicable";

pub(crate) fn canonical_receipt_state(state: Option<&str>, movement: Option<&str>) -> &'static str {
    let state = normalize_receipt_state(state);
    if state == Some(RECEIPT_FOUND)
        && let Some(movement) = normalize_receipt_movement(movement)
    {
        return movement;
    }
    if let Some(state) = state {
        return state;
    }
    if let Some(movement) = normalize_receipt_movement(movement) {
        return movement;
    }
    RECEIPT_MISSING
}

pub(crate) fn canonical_receipt_state_for_presence(
    present: bool,
    state: Option<&str>,
    movement: Option<&str>,
    missing_state: &str,
) -> &'static str {
    let state = normalize_receipt_state(state);
    if state == Some(RECEIPT_FOUND)
        && let Some(movement) = normalize_receipt_movement(movement)
    {
        return movement;
    }
    if let Some(state) = state {
        return state;
    }
    if let Some(movement) = normalize_receipt_movement(movement) {
        return movement;
    }
    if present {
        RECEIPT_FOUND
    } else {
        normalize_receipt_state(Some(missing_state)).unwrap_or(RECEIPT_MISSING)
    }
}

fn normalize_receipt_state(state: Option<&str>) -> Option<&'static str> {
    let normalized = state?.trim();
    match normalized {
        "" => None,
        "missing" | "missing_artifact" | "missing_receipt" | "receipt_missing" => {
            Some(RECEIPT_MISSING)
        }
        "found" | "present" | "receipt_found" | "receipt_present" => Some(RECEIPT_FOUND),
        "stale" | "stale_artifact" | "receipt_stale" => Some(RECEIPT_STALE),
        "gap_mismatch" | "mismatch" | "receipt_gap_mismatch" => Some(RECEIPT_GAP_MISMATCH),
        "improved" | "resolved" | "receipt_improved" | "receipt_movement_improved" => {
            Some(RECEIPT_MOVEMENT_IMPROVED)
        }
        "unchanged"
        | "unchanged_after_attempt"
        | "receipt_unchanged"
        | "receipt_movement_unchanged" => Some(RECEIPT_MOVEMENT_UNCHANGED),
        "not_available" | "not_applicable" | "not_projected" | "receipt_not_applicable" => {
            Some(RECEIPT_NOT_APPLICABLE)
        }
        _ => None,
    }
}

fn normalize_receipt_movement(movement: Option<&str>) -> Option<&'static str> {
    let normalized = movement?.trim();
    match normalized {
        "" => None,
        "improved" | "resolved" => Some(RECEIPT_MOVEMENT_IMPROVED),
        "unchanged" | "unchanged_after_attempt" => Some(RECEIPT_MOVEMENT_UNCHANGED),
        "missing" | "missing_receipt" => Some(RECEIPT_MISSING),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_receipt_state_covers_surface_lifecycle_labels() {
        assert_eq!(
            canonical_receipt_state(Some("missing"), None),
            RECEIPT_MISSING
        );
        assert_eq!(
            canonical_receipt_state(Some("present"), None),
            RECEIPT_FOUND
        );
        assert_eq!(
            canonical_receipt_state(Some("present"), Some("improved")),
            RECEIPT_MOVEMENT_IMPROVED
        );
        assert_eq!(
            canonical_receipt_state(Some("stale_artifact"), None),
            RECEIPT_STALE
        );
        assert_eq!(
            canonical_receipt_state(Some("gap_mismatch"), None),
            RECEIPT_GAP_MISMATCH
        );
        assert_eq!(
            canonical_receipt_state(None, Some("improved")),
            RECEIPT_MOVEMENT_IMPROVED
        );
        assert_eq!(
            canonical_receipt_state(None, Some("unchanged")),
            RECEIPT_MOVEMENT_UNCHANGED
        );
        assert_eq!(
            canonical_receipt_state(Some("not_projected"), None),
            RECEIPT_NOT_APPLICABLE
        );
    }
}

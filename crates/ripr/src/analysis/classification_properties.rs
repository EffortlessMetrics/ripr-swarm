//! Shared, test-only projections for #12927. No total ordering of unknowns.
use crate::domain::{ExposureClass, Finding, LanguageId, ProbeFamily, StageState};

pub(super) fn exposure_matches(class: &ExposureClass, expected: bool) -> Result<(), String> {
    if (*class == ExposureClass::Exposed) != expected {
        return Err(format!("expected exposed={expected}, got {class:?}"));
    }
    Ok(())
}

/// Only for a fixed, modeled return seam with an unchanged direct relation.
/// Unknown classes are outside this partial order, rather than ranked as gaps.
pub(super) fn oracle_upgrade(before: &ExposureClass, after: &ExposureClass) -> Result<(), String> {
    let rank = |class: &ExposureClass| match class {
        ExposureClass::ReachableUnrevealed => Some(0),
        ExposureClass::WeaklyExposed => Some(1),
        ExposureClass::Exposed => Some(2),
        _ => None,
    };
    match (rank(before), rank(after)) {
        (Some(a), Some(b)) if b >= a => Ok(()),
        _ => Err(format!(
            "modeled oracle upgrade lowered/left the chain: {before:?} -> {after:?}"
        )),
    }
}

pub(super) fn preview_finding(finding: &Finding) -> Result<(), String> {
    if finding.probe.family != ProbeFamily::ReturnValue {
        return Err("the property projection is bounded to return-value seams".to_string());
    }
    if finding.class.requires_stop_reason() && finding.stop_reasons.is_empty() {
        return Err(format!("unknown finding has no stop reason: {finding:?}"));
    }
    if finding.class == ExposureClass::Exposed {
        // Explicit stage-exception allowlist for this bounded projection.
        // Do not silently waive a newly introduced state or another language.
        let flow_allowed = match finding.language {
            Some(LanguageId::TypeScript) => {
                matches!(
                    finding.ripr.infect.state,
                    StageState::Yes | StageState::Unknown
                ) && finding.ripr.propagate.state == StageState::Unknown
            }
            Some(LanguageId::Python) => {
                finding.ripr.infect.state == StageState::Yes
                    && finding.ripr.propagate.state == StageState::Yes
            }
            _ => false,
        };
        if !flow_allowed {
            return Err(format!(
                "exposed outside the flow exception allowlist: {finding:?}"
            ));
        }
        for state in [
            &finding.ripr.reach.state,
            &finding.ripr.reveal.observe.state,
            &finding.ripr.reveal.discriminate.state,
        ] {
            if *state != StageState::Yes {
                return Err(format!(
                    "exposed without a confirmed preview gate: {finding:?}"
                ));
            }
        }
    } else if finding.stop_reasons.is_empty()
        && finding.missing.iter().all(|text| text.trim().is_empty())
        && finding.activation.missing_discriminators.is_empty()
    {
        return Err(format!(
            "non-exposed finding has no explanation: {finding:?}"
        ));
    }
    Ok(())
}

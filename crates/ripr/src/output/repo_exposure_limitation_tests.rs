//! Scope and empty-inventory behavior of evidence limitations (#13624).

use super::repo_exposure::{
    evidence_limitation_json_object, evidence_limitation_scope, render_repo_exposure_md_with_limit,
};
use crate::analysis_outcome::{
    AnalysisLimitation, AnalysisLimitationKind, AnalysisRecovery, AnalysisRecoveryKind,
    AnalysisStage,
};

fn limitation(path: Option<&str>) -> Result<AnalysisLimitation, String> {
    let base = AnalysisLimitation::new(
        AnalysisLimitationKind::LanguageScopeUnsupported,
        AnalysisStage::LanguageAdapter,
        AnalysisRecovery::new(AnalysisRecoveryKind::Retry, "restore read access")?,
    );
    match path {
        Some(path) => base.with_path(path),
        None => Ok(base),
    }
}

#[test]
fn path_limitation_scope_is_path_in_json_and_sarif_and_pass_wide_without_path() -> Result<(), String>
{
    let named = limitation(Some("src/locked"))?;
    let bare = limitation(None)?;
    assert_eq!(evidence_limitation_scope(&named), "path");
    assert_eq!(evidence_limitation_scope(&bare), "pass_wide");
    let named_json: serde_json::Value =
        serde_json::from_str(&evidence_limitation_json_object(&named, 0))
            .map_err(|error| error.to_string())?;
    assert_eq!(named_json["scope"], "path");
    assert_eq!(named_json["path"], "src/locked");
    let bare_json: serde_json::Value =
        serde_json::from_str(&evidence_limitation_json_object(&bare, 0))
            .map_err(|error| error.to_string())?;
    assert_eq!(bare_json["scope"], "pass_wide");
    assert!(bare_json.get("path").is_none());
    assert_eq!(
        super::sarif::evidence_limitation_value(&named)["scope"],
        "path"
    );
    assert_eq!(
        super::sarif::evidence_limitation_value(&bare)["scope"],
        "pass_wide"
    );
    Ok(())
}

#[test]
fn pathless_markdown_text_is_unchanged_and_path_markdown_suppresses_empty_next_step()
-> Result<(), String> {
    let bare = limitation(None)?;
    let md = render_repo_exposure_md_with_limit(&[], None, None, None, None, &[bare]);
    assert!(
        md.contains(
            "scope: pass_wide): the shared test-evidence pass failed, so the counts above are \
             not a complete inventory result and every seam carries opaque stand-in evidence."
        ),
        "{md}"
    );
    assert!(md.contains("Nothing was found to analyze"), "{md}");
    let named = limitation(Some("src/locked"))?;
    let md = render_repo_exposure_md_with_limit(&[], None, None, None, None, &[named]);
    assert!(md.contains("scope: path"), "{md}");
    assert!(!md.contains("Nothing was found to analyze"), "{md}");
    Ok(())
}

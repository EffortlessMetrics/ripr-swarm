//! Compatibility delegates to the single lightweight production owner.
use crate::work_portfolio::workspace_path;

pub(crate) fn check_product_route_parity(args: &[String]) -> Result<(), String> {
    repo_policy::product_route_parity::check_product_route_parity(args, &workspace_path(""))
}

pub(crate) fn validate_product_route_parity_fixture_corpus(violations: &mut Vec<String>) {
    repo_policy::product_route_parity::validate_product_route_parity_fixture_corpus(
        &workspace_path(""),
        violations,
    );
}

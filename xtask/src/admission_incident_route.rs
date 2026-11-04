//! xtask front door for #5076 admission-incident route checks.

pub(crate) fn check() -> Result<(), String> {
    repo_policy::enter_workspace_root()?;
    repo_policy::admission_incident_route::check_committed_route_corpus()
}

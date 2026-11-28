//! #5076: committed admission-incident route corpus must stay executable.

#[test]
fn committed_admission_incident_route_corpus() -> Result<(), String> {
    repo_policy::enter_workspace_root()?;
    repo_policy::admission_incident_route::check_committed_route_corpus()
}

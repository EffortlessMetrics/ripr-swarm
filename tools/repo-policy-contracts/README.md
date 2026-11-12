# Workflow contract tests

Unpublished development-only sibling of `repo-policy` under ADR 0027.
Neither package depends on the other. Pure workflow
leaves share `workflow_contracts`; incident-route stays in the production package because it
changes the process working directory. Grouped suites retain child-only
process environment/directory changes and distinct temporary namespaces.

Run all: `cargo test --locked -p repo-policy -p repo-policy-contracts`.
Run one suite: `cargo test --locked -p repo-policy-contracts --test workflow_contracts rust_only_routing`.
Workspace nextest includes the workflow bundle and production targets automatically. The frozen inventory map
records exact baseline cases, platform gates and ignored states. Adding or
removing a case requires updating that map deliberately, preserving its controls.
See `docs/repo-policy-compilation.md` for resource identity and qualification.

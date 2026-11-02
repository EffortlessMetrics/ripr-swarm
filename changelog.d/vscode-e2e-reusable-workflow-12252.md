<!-- section: Internal / CI -->
- Moved the VS Code extension e2e into the reusable `vscode-e2e.yml`, which the routed `merge-ready` run calls as its own job and the Result requires through `needs` (#12252). The cross-workflow check-run correlation and the `CI merge-ready request` run title are retired; `CI` keeps running the same workflow on `main` pushes and manual dispatch.

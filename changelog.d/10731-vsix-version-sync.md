<!-- section: Fixed -->
- `cargo xtask vscode-verify-vsix` now fails when a `ripr-<version>.vsix` carries a different extension `package.json` or `extension.vsixmanifest` version, and the policy checks fail when `editors/vscode` (manifest and lockfile) or the npm launcher version differs from the workspace version, so a stale extension can no longer ship under a newer release name (#10731).

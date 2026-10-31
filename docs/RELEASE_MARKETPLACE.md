# Marketplace Release

This document covers the editor extension release surfaces:

```text
VS Code Marketplace:
  EffortlessMetrics.ripr

Open VSX:
  EffortlessMetrics.ripr
```

The Rust crate release is documented separately in [RELEASE.md](RELEASE.md).

## Versioning

Keep versions aligned:

```text
ripr crate:      0.8.x
VS extension:    0.8.x
Open VSX:        0.8.x
```

For `0.8.x`, the universal extension can download the matching `ripr` server
from GitHub Releases. `cargo install ripr` remains a manual fallback for
offline, pinned, or controlled environments.

The marketplace release story should lead with the editor and CI evidence loop:
install the extension, inspect saved-workspace diagnostics, copy the targeted
brief or agent commands, add one focused test, and verify through the copied
command chain or generated CI artifacts.

## Required Files

Before publishing, confirm `editors/vscode` contains:

```text
README.md
CHANGELOG.md
LICENSE
icon.png
package.json
package-lock.json
```

Use `icon.png`, not SVG.

The extension icon should be regenerated from the canonical brand asset at
`assets/logo/ripr-icon-dark.svg`. The committed marketplace PNG derivative is
kept at `assets/logo/ripr-icon-dark.png` and copied to `editors/vscode/icon.png`.

Before triggering `publish-extension.yml`, run the
[Release copy checklist](RELEASE_COPY_CHECKLIST.md). It covers the VSIX
package metadata (`displayName`, `description`), the marketplace README
opener, badge freshness disclosure, and the VSIX-rebuild step that prevents a
stale package attached to an earlier tag from being published with old copy.

## Local Package Gates

```bash
cd editors/vscode
npm ci
npm run compile
npm run package
code --install-extension dist/ripr-0.8.0.vsix --force
```

Also run the Rust gates from the repository root:

```bash
cargo fmt --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Before publishing a self-provisioning extension, confirm the matching server
binary assets and manifest exist on the GitHub Release. See
[RELEASE_BINARIES.md](RELEASE_BINARIES.md).

## Secrets

Environment secrets, each scoped to the one publish environment that reads it
(see [REPO_SETTINGS.md](REPO_SETTINGS.md#release-environments)):

```text
VSCE_PAT   in environment vs-marketplace-publish
OVSX_PAT   in environment open-vsx-publish
```

The Open VSX namespace must exist before publish:

```bash
npx ovsx create-namespace EffortlessMetrics -p "$OVSX_PAT"
```

## Manual Registry Publish

```bash
cd editors/vscode
npm ci
npm run compile
npm run package
npx @vscode/vsce publish --packagePath dist/ripr-0.8.0.vsix --pat "$VSCE_PAT"
npx ovsx publish dist/ripr-0.8.0.vsix -p "$OVSX_PAT" --skip-duplicate
```

## CI Publish

Use:

```text
.github/workflows/publish-extension.yml
```

The workflow has no tag trigger; it runs only by `workflow_dispatch`, at the
receipted release head. Each dispatch publishes to exactly one channel
(`publish_vs_marketplace` or `publish_open_vsx`; zero or two is refused).
The first dispatch packages the VSIX and attaches it to the existing
`v<version>` GitHub Release (it never creates the Release). The second
channel's dispatch reuses those attached bytes and must pass their digest as
`vsix_sha256`. Each publish job waits for approval on its protected
environment.

Every dispatch requires `version`, `source_sha` (the 40-hex head the release
tag peels to; dispatch with `--ref` at that commit) and `manifest_sha256` (raw
bytes of the published `ripr-server-manifest-v<version>.json`). The workflow
normalizes tag names like `v0.8.0` to the package version `0.8.0` for the VSIX
filename and duplicate-version checks.

The exact commands for both channels, including the Open VSX dispatch with
`--ref`, `source_sha`, `manifest_sha256` and `vsix_sha256`, are in
[RELEASE_TRANSACTION.md](RELEASE_TRANSACTION.md) section 6. Do not dispatch
from a hand-written sample.

Publish the server binary assets before publishing the marketplace VSIX so the
extension can download a matching `ripr` server on first activation. For
defaults-first releases, the matching server must also pass the
[Installation verification](INSTALLATION_VERIFICATION.md) server smoke.

## Post-Publish Verification

```text
VS Code Marketplace listing is live.
Open VSX listing is live.
Both listings show the same version.
VS Marketplace manual install badge count was refreshed from publisher
metrics or explicitly disclosed as static/deferred.
Open VSX download badge renders.
GitHub Release has the VSIX attached.
Installing from each registry works.
The extension starts `ripr lsp`.
Missing `ripr` executable shows the install/settings message.
```

## Marketplace Badge Maintenance

VS Marketplace install-count badges are manually maintained.

Do not use live VS Marketplace Shields routes for install, download, or version
counts. They are intentionally not treated as a reliable source of truth for
this repo.

Use static VS Marketplace badges instead:

```text
https://img.shields.io/badge/VS%20Marketplace-<count>%20installs-0078D4
```

Open VSX download badges may use live Shields routes:

```text
https://img.shields.io/open-vsx/dt/EffortlessMetrics/ripr
```

After each extension release, either refresh the static VS Marketplace count
from publisher metrics or record why the static count was intentionally left
unchanged in the release proof. Do not infer a new count from public badge
rendering or hand-edit the number without the publisher metrics source.

When refreshing:

1. Open the VS Marketplace publisher metrics page.
2. Record the current install count.
3. Update the manual badge count in `README.md` and
   `editors/vscode/README.md`.
4. Update the hidden `Last checked: YYYY-MM-DD` comment near each manual badge.
5. Leave Open VSX download badges as live Shields badges.

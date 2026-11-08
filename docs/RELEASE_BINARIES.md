# Server Binary Rehearsal

The VS Code/Open VSX extension can self-provision only when GitHub Releases has
native `ripr` server archives and a manifest.

## Workflow

Use:

```text
.github/workflows/release-server-binaries.yml
```

Dispatch from the selected swarm branch:

```bash
gh workflow run release-server-binaries.yml --ref <candidate-branch> -f version=<candidate-package-version>
```

This read-only entrypoint calls `server-archive-qualification.yml` with the
dispatch commit's exact SHA. It builds and verifies:

```text
x86_64-pc-windows-msvc
x86_64-apple-darwin
aarch64-apple-darwin
x86_64-unknown-linux-gnu
aarch64-unknown-linux-gnu
```

Download the `ripr-server-qualification-*` Actions artifacts to inspect all
five archives, per-target checksums, manifest, `SHA256SUMS`, and the
JSON/Markdown receipt. The machine-readable receipt records the exact swarm
SHA, version, each target archive digest and resolved Rust/Cargo toolchain,
plus the Cargo.lock, workflow, manifest, and `SHA256SUMS` digests for the
source-repo handoff. Actions artifacts expire and are not public release
assets. The prospective URLs in the rehearsal manifest name the swarm repo;
the source release must generate its own manifest for its selected source
commit and independently verify the promoted assets. Public release creation
and asset upload belong only to `EffortlessMetrics/ripr`.

### Final native payload identity

Each target job stages the bytes that packaging is allowed to consume under:

```text
target/ripr/distribution/v<VERSION>/<target>/payload/
```

Beside that directory it writes `payload-identity.json` and
`payload-identity.md`. The strict identity binds the product, native version,
target, executable, source commit and tree, `Cargo.lock`, selected features,
Rust/Cargo toolchain, bounded runner identity, ordered payload file
paths/roles/sizes/digests, and one aggregate payload digest. Runtime
compatibility evidence is copied from `policy/distribution.toml`; it remains
`unqualified` until #4489 records measured compatibility.

`release-server-archive` archives only this staged directory. The qualification
workflow extracts the resulting archive and runs the same Rust verifier over
the extracted bytes before recording `archive_readback_verified: true` in that
target's build identity; the aggregate receipt repeats the field only after an
all-targets gate asserts every per-target readback flag. It
retains the exact JSON/Markdown identities with each target's build identity,
then requires five selected, five executed, zero failed, and zero not-run
targets before writing the aggregate receipt. This proves payload and archive
shape for the exact candidate; it does not establish a public release, wheel,
npm package, or native runtime compatibility floor.

### Native npm packages

From the same clean committed checkout, the npm producer can wrap any of the
five contracted targets' staged final payload and its producer build receipt
without publishing:

```bash
cargo xtask release-server-archive --npm-payload-dir <payload-directory> \
  --payload-identity <payload-identity.json> --producer-receipt <producer-build.json> \
  --build-identity <selected-target-build-identity.json> --qualified-archive <ripr-server-vVERSION-TARGET.tar.gz|zip> \
  --out-dir <new-output-directory>
```

The command requires the successful selected-target build identity from the
qualification workflow and its matching archive. It binds their candidate,
version, target, readback flag, workflow, archive and payload hashes to the
current checkout and producer receipt. The package name, `os`, `cpu`, `libc`
(Linux only) and executable path come from the target's row in
`policy/distribution.toml` and must match the launcher's platform map. The
command maps the flat `ripr` (or `ripr.exe`) file to the launcher's
`bin/ripr` (or `bin/ripr.exe`), writes a deterministic npm tarball,
and checks its extracted metadata, inventory, executable mode and payload
digests. It records the payload aggregate, identity and producer receipt hashes
separately from the tarball hash in `npm-native-receipt-<target>.json`. Existing
output files are not overwritten. A Windows payload carries no POSIX mode, so
only the other targets require the source execute bit. The package alone does
not establish installed channel parity, a compatibility floor or publication.
The build identity must be retained with the successful workflow run and
artifact evidence; its JSON fields alone do not authenticate their origin. If
the process is killed between the package and receipt writes, remove the
incomplete output package before retrying; a package without its receipt is not
a successful producer result.

### Native Python wheel

From the same clean committed checkout, any of the five contracted targets can
wrap its staged final payload into a platform-tagged `ripr-rs` wheel without
rebuilding or publishing. The inputs and identity binding are those of the npm
producer above, for the target named by the payload identity:

```bash
cargo xtask release-server-archive --wheel-payload-dir <payload-directory> \
  --payload-identity <payload-identity.json> --producer-receipt <producer-build.json> \
  --build-identity <selected-target-build-identity.json> \
  --qualified-archive <ripr-server-vVERSION-TARGET.tar.gz|zip> \
  --out-dir <new-output-directory>
```

The qualified archive must be named `ripr-server-v<version>-<target>.<archive>`
with the archive extension from the target's row in `policy/distribution.toml`.
The command writes `ripr_rs-<pep440>-py3-none-<wheel_platform_tag>.whl` using
the row's `wheel_platform_tag`. The wheel holds the payload executable under
`.data/scripts/` (mode 0755), the two licenses under `dist-info/licenses/`,
`WHEEL`, a `METADATA` generated from `packaging/python/pyproject.toml` and
`packaging/python/README.md`, and `RECORD`, all with fixed timestamps in a fixed
order so the same inputs give byte-identical output. `README-server.txt` is
verified as a payload notice but is not packaged. After writing, the command
re-opens the wheel and checks the exact entry set and order, RECORD digests, the
executable and license digests against the payload identity, regenerated
`WHEEL` and `METADATA`, and the script's execute mode, then writes
`wheel-native-receipt-<target>.json` with the wheel digest next to the payload,
producer, build-identity and archive digests. Existing output files are not
overwritten. This is a local wheel artifact only: it does not establish
installed-channel parity, a compatibility floor or publication.

### Whole package family in the release rehearsal

`release-server-binaries.yml` runs both producers for every target after the
five archives qualify, so one dispatch yields the complete PyPI and npm family
for the exact commit (#5378):

```bash
cargo xtask release-server-archive --packages-from <downloaded-qualification-artifacts> \
  --out-dir <new-output-directory>
```

The command reads each target's `ripr-server-qualification-<target>`,
`ripr-final-native-payload-<target>` and `ripr-server-build-identity-<target>`
artifacts, extracts the flat archive (refusing nested, hidden, duplicate or
non-regular members), and writes `wheels/` and `npm/`. Every target must
succeed; one missing, ambiguous or failing target fails the command rather than
producing a subset. The workflow then packs the launcher next to the native
packages and uploads everything as the `ripr-platform-packages` artifact.

The `package-smoke` matrix installs that artifact on each native runner
(Linux x64 on Ubuntu 22.04 for the glibc 2.34 floor, Linux ARM64, macOS Intel,
macOS ARM64, Windows x64), with no ripr on `PATH`:

- `pip install --no-index --pre --find-links wheels ripr-rs` from the full five-wheel
  directory, so pip must select the wheel whose tag matches the host; the job
  fails if it picks any other tag;
- `npm install --offline` of the launcher tarball plus the host's own native
  package, with scripts disabled and an empty cache;
- both installed commands must print `ripr <version> (<commit>)` and run
  `ripr doctor` in a fresh Cargo project;
- on macOS, the installed binary's Mach-O minimum OS must not exceed the
  wheel tag (10.12 for Intel, 11.0 for ARM64; the rustc defaults, since the
  archive build sets no deployment target).

The wheel tags live in `wheel_platform_tag` in `policy/distribution.toml` and
are pinned by `check-release-targets`. Nothing here uploads to a registry.
Publication stays with the release operator in `EffortlessMetrics/ripr`: upload
the five wheels to PyPI, and publish the five native npm packages before the
launcher that references them, all with a non-`latest` tag for a prerelease.

## Exact-candidate qualification (read-only)

Use `.github/workflows/server-archive-qualification.yml` when archive shape
must be checked before any publication decision. It requires an immutable
40-character `candidate_sha`; an optional `candidate_ref` must be exactly
`refs/tags/ripr-release-<version>-<candidate_sha>` (the grammar the Windows
packaged lane also requires) and may be lightweight or annotated,
but must resolve to the same commit. Every matrix job fetches that
SHA, builds the existing five-target server matrix, verifies the archive
checksum, extracts the flat package, and checks both the archive label and the
candidate-built binary's `--version` command, including that the requested
qualification version matches the candidate package version and that the
binary names the candidate commit. The manifest job verifies
`SHA256SUMS` and emits a
machine-readable and Markdown qualification receipt.

Dispatch with the already selected candidate identity:

```bash
gh workflow run server-archive-qualification.yml \
  -f candidate_sha=<40-character-candidate-sha> \
  -f candidate_ref=<optional-refs/tags/ripr-release-version-sha> \
  -f version=<version>
```

This workflow has `contents: read`, does not call `release-upload-assets`, and
does not use `GH_TOKEN`, `github.token`, or repository secrets. When a tag is
supplied, it verifies the fixed public detail endpoint
`/repos/EffortlessMetrics/ripr-swarm/rulesets/20661783` without credentials;
the response must contain the expected ruleset id, active tag target, singleton
`refs/tags/ripr-release-*` include, empty exclude, and both update/deletion
rules. Bounded HTTP retries report status, rate-limit, and response-digest
diagnostics on every failure, then fail closed if the endpoint or shape is
unavailable. Its only writes are scoped
GitHub Actions artifacts containing the archives, manifest, checksums, and
qualification receipt. An Actions artifact is rehearsal evidence, not a
GitHub Release asset and not publication proof. The swarm
`release-server-binaries.yml` entrypoint forwards its optional `candidate_ref` input and uses this same qualification receipt;
the source repository's separate release workflow owns publication.


Packaging and manifest assembly intentionally live in Rust-first automation:

```bash
cargo xtask release-server-archive --version <VERSION> --target <target> --executable <ripr-or-ripr.exe> --archive <zip-or-tar.gz>
cargo xtask release-server-manifest --version <VERSION> --repository <owner/repo>
```

The qualification workflow orchestrates packaging and checksums. A later,
separately authorized source-repo release may publish assets named:

```text
ripr-server-v<VERSION>-<target>.zip
ripr-server-v<VERSION>-<target>.tar.gz
ripr-server-manifest-v<VERSION>.json
SHA256SUMS
```

The `SHA256SUMS` sidecar is `sha256sum -c SHA256SUMS`-compatible (one
`<sha256>  <file_name>` line per asset). Releases through `v0.7.0` published the
same manifest under the legacy name `checksums.txt`; the content format is
unchanged.

### RC placement and the embedded manifest digest (#3798)

The server manifest is placement-neutral per distribution generation: the
generation's manifest bytes (and archive identities) are identical on the
exact stable release and on the one public RC release of the same generation,
so one raw-byte manifest SHA-256 admits both placements. Build the RC release
from the generation's version (`release-server-manifest --version 0.11.0`),
upload those bytes to the RC tag, and upload the identical bytes to the stable
tag when it is cut.

`cargo xtask release-server-manifest` also regenerates
`editors/vscode/src/serverDescriptor.ts` with that digest, which is useful for
local rehearsals. The descriptor's authoritative source at release is the
publish workflow: `publish-extension.yml` runs
`cargo xtask release-bind-server-descriptor --version <V> --manifest <file>
--manifest-sha256 <receipted digest>` before compiling, so the packaged
extension embeds the digest of the manifest actually published (the command
refuses bytes that do not match the digest, a wrong generation, non-https
asset URLs and malformed archive digests). The committed descriptor stays
empty on `main`; the digest is not derivable before the server run because
archive digests depend on the commit that would contain the descriptor.
A generation without an embedded descriptor has no fallback row: a
prerelease request's stable absence stays terminal.

With the descriptor embedded, a clean RC extension requests its own version
(for example `0.11.0-rc.1`) and the downloader resolves:

```text
fetch the exact stable placement v<GENERATION>/ripr-server-manifest-v<GENERATION>.json
  accepted      -> selected_stable_exact
  direct 404    -> fetch the one predeclared RC placement
                   v<REQUESTED>/ripr-server-manifest-v<GENERATION>.json
                   admitted by the same digest
                -> selected_rc_exact_after_stable_absent
  anything else -> fail closed with the original typed reason
```

A redirected 404, DNS/connect/TLS failure, timeout, 401/403, 5xx, redirect
loop, malformed or wrong-digest manifest, or a target-set identity conflict is
never absence and never falls back. No other RC, `latest` query, version
range, or mutable lookup is ever derived.

Each server archive contains:

```text
ripr(.exe)
LICENSE-MIT
LICENSE-APACHE
README-server.txt
```

## Release Proof

The last release whose proof is recorded here is `v0.7.0`, published on
May 20, 2026; 0.8.0 through 0.10.0 were published later without a proof
recorded in this file. The proof ran:

- The GitHub Release has `ripr-0.7.0.vsix`.
- The release has `ripr-server-manifest-v0.7.0.json`.
- The release has `checksums.txt`.
- The release has server archives and `.sha256` files for each supported
  target:
  - `x86_64-pc-windows-msvc`;
  - `x86_64-apple-darwin`;
  - `aarch64-apple-darwin`;
  - `x86_64-unknown-linux-gnu`;
  - `aarch64-unknown-linux-gnu`.
- The installed public loop was verified for `doctor`, `pilot`, `outcome`,
  `agent verify`, and `agent receipt`; see
  [Installation verification](INSTALLATION_VERIFICATION.md).
- Future releases must refresh the same VSIX, manifest, checksum, and
  per-target server-archive asset family before publication.

The historical `v0.3.1` release was verified on May 7, 2026:

- `ripr v0.3.1` was the public GitHub Release at that time.
- The release has `ripr-0.3.1.vsix`.
- The release has `ripr-server-manifest-v0.3.1.json`.
- The release has server archives and `.sha256` files for each supported
  target.
- The Windows archive checksum matched the manifest entry for
  `x86_64-pc-windows-msvc`.
- The extracted Windows server ran `ripr --version`, `ripr lsp --version`,
  `ripr pilot`, and `ripr outcome`.

That proof covered server archive shape for the then-current public release and
the defaults-first `ripr pilot` and `ripr outcome` public-install smoke; see
[Installation verification](INSTALLATION_VERIFICATION.md).

The historical `v0.4.0` release was verified on May 7, 2026:

- `ripr-server-manifest-v0.4.0.json`, `checksums.txt`, per-target server
  archives, per-target `.sha256` files, and `ripr-0.4.0.vsix` were present on
  the GitHub Release.
- The Windows archive checksum matched the manifest entry for
  `x86_64-pc-windows-msvc`.
- The extracted Windows server ran `ripr --version`, `ripr lsp --version`,
  `ripr pilot`, `ripr outcome`, `ripr agent verify`, and
  `ripr agent receipt`.

## Local Verification

After downloading a release asset for the current platform:

```bash
ripr --version
ripr lsp --version
```

Then install the local VSIX and open a Rust workspace, which exercises
`ripr lsp --stdio` through proper LSP framing:

```bash
cd editors/vscode
npm ci
npm run compile
npm run package
code --install-extension dist/ripr-<VERSION>.vsix --force
```

For the defaults-first release line, also run the server archive smoke from
[Installation verification](INSTALLATION_VERIFICATION.md): the extracted server
binary must report the release version and run `ripr pilot` against the checked
boundary-gap fixture.

## Notes

The extension verifies archive SHA-256 before extraction. It still keeps
`ripr.server.path` and PATH fallback for offline installs, pinned binaries, and
enterprise-managed environments.

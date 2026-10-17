# Install channels

Status of every way a developer can get `ripr`, observed 2026-10-04 against
public registries, plus the reversible preparation for 0.11. Nothing here
publishes, tags or submits anything; each outward step names who must act.

## Upgrading from 0.10 to 0.11

This is the **development upgrade guide**, not an announcement that 0.11 has
been published. Use the installation instructions for the exact release or
candidate you selected; the dated channel inventory below is not an instruction
to install an unavailable version. Installing a registry package does not
install the current development checkout.

### Confirm which binary and repository you are using

Record the old version before replacing it, then check the new installation:

```bash
ripr --version
```

Check the resolved executable with `command -v ripr` in Bash, or
`Get-Command ripr -All` in PowerShell. Multiple Cargo, npm, Python, or manual
installations can leave an older executable first on PATH. An alpha or RC
installation is not the stable release merely because it belongs to 0.11.

In the repository being upgraded, preserve existing work and configuration
before regenerating anything:

```bash
git status --short
ripr doctor
ripr check --help
```

Keep a copy of custom workflow/configuration files, including untracked files;
Git cannot recover an untracked file it never stored. `doctor` is a setup and
migration diagnostic, not proof that tests pass or that all findings are sound.
Do not remove the repository's toolchain pin to install RIPR; source-install
compiler requirements and the project's own verification toolchain are separate.

### Refresh generated CI without resetting configuration

For a repository already using generated GitHub Actions, first inspect the
0.11 generator's replacement plan:

```bash
ripr init --ci github --force --dry-run
```

After checking that plan and preserving workflow customizations, refresh:

```bash
ripr init --ci github --force
git diff -- ripr.toml .github/workflows
```

With `--ci`, this refresh keeps an existing `ripr.toml` and replaces the
generated workflow. **Do not substitute bare `ripr init --force`: that can
reset configuration customizations.** Reapply intentional workflow edits and
review the diff before committing. An unreleased generator may deliberately
pin the latest published RIPR instead of its own development version; read its
warning and inspect the emitted install/version steps. A successful refresh
alone does not prove CI is running 0.11. See [CI](CI.md).

### Re-select the diff and review suppressions

Use the installed `check --help` (`check --help --all` lists every option) and
[quickstart](QUICKSTART.md) to select the
intended working-tree, staged, or committed comparison. Select `--base REF`
when an explicit existing Git ref is required; do not assume an old invocation
and the new default cover the same edits. Check the reported scope and files
before interpreting findings. An empty or limited analysis is not a clean bill
of health.

Run a fresh analysis before changing old suppressions or baselines. A 0.10
`finding_id` may no longer identify a current finding. Where the migration
warning supplies a replacement, inspect the current finding and suppression
reason before adopting that ID. Unmatched old selectors do not suppress a new
finding; do not copy IDs from examples, mass-replace IDs, or broaden the policy
just to reproduce an old count. Stricter evidence checks can legitimately
change classifications. Review any adopted gate baseline and policy against
the new evidence. See [configuration](CONFIGURATION.md).

### Regenerate packets and retain old receipts as history

Regenerate the start-here/first-PR packet with the new binary, using
`ripr first-pr --help` and its current printed instructions. Follow the new
packet's root, scope, selected ID and continuation commands; do not reuse a
0.10 command merely because its output path still exists.

When a receipt is rejected for predating HEAD binding, follow the diagnostic's
fresh verification/receipt route. **Do not add a HEAD field or change a version
string in the old JSON to make it pass.** Retain the original as historical
evidence, and create a current receipt from current inputs. A repair attempt
from another build may need a new before/edit/after transaction rather than a
resume. Use the [command guide](COMMAND_HIERARCHY.md) and the printed recovery
route; finding IDs, seam IDs and attempt IDs are not interchangeable.

Keep actual project test execution separate from static before/after movement.
A receipt saying static evidence improved does not, by itself, say the focused
test ran or passed. Preserve failed, not-run and unsupported states rather than
rewriting them as successful verification.

### Let incompatible caches miss; check editor overrides

The first analysis after a build/cache identity change can be cold. Let RIPR
reject incompatible cache entries and recompute them; do not copy old cache
identity fields into a new build or delete the whole `.ripr` directory, which
can also contain policy and retained evidence. For a cache-specific diagnostic,
inspect `ripr cache --help` and use only the scoped recovery it documents.
No universal install-time or warm-analysis speed is promised by this guide.

In VS Code, check the installed extension and the server version/path shown in
RIPR's status/output. Updating the CLI does not update the extension, and an
explicit `ripr.server.path` can keep selecting an older server. Check explicit
user, workspace and folder settings when `[lsp]` configuration appears ignored;
an explicit editor setting can override repository configuration. Preserve
Workspace Trust and digest verification rather than disabling them to force an
installation. See [editor setup](EDITOR_EXTENSION.md).

The upgrade is ready for use when the intended binary and scope are selected,
custom configuration is preserved, regenerated artifacts describe current
inputs, and any selected focused verification has an honest recorded outcome.
This guide does not itself establish the final installed 0.10-to-0.11 replay,
all-platform acceptance, or public channel availability.

## Observed state

| Channel | Public today | 0.11 state | Gap |
| --- | --- | --- | --- |
| `cargo install ripr` | crates.io serves 0.10.0 | Publishes with the release | Compiles from source (needs Rust 1.95; minutes, not seconds). |
| GitHub Release server archives | 0.10.0 has all five targets plus `.sha256` | Rehearsed by `release-server-binaries.yml`; 0.11.0 assets do not exist yet | Release operator publishes from `EffortlessMetrics/ripr`. |
| `cargo binstall ripr` | Falls back to a source build (0.10.0 carries no binstall metadata) | Metadata added in this change; works once 0.11.0 assets exist | Verify with a real `cargo binstall ripr` after the release. |
| CI (`ripr init --ci github`) | Compiles ripr each run | Downloads the release archive and verifies its checksum (#5236) | Merge #5236. |
| PyPI `ripr-rs` | Only `0.11.0a1`, Linux x86-64 wheel | Wheel qualification exists | Single platform; no macOS, Windows or ARM wheel is public. |
| npm `@effortlessmetrics/ripr` | `0.11.0-alpha.2` on `next`, Linux x86-64 only, README says so | Multi-platform launcher is checked in, not published | Platform payload packages (`ripr-darwin-arm64` and the others) do not exist on npm. |
| VS Code Marketplace / Open VSX | Marketplace lists 0.10.0 (read back in a browser 2026-10-04; Open VSX not checked) | `publish-extension.yml` is source-owned | The listing's two continuation links omit `/blob/main/` and render "Not Found" (`github.com/EffortlessMetrics/ripr/docs/EDITOR_FIRST_RUN_TO_FIRST_RECEIPT.md` and `.../EDITOR_FIRST_PR_BRIDGE_WORKFLOW.md`); its description still says 0.8.x. A source docs merge does not change a published listing; the next authorized extension publication does. Installed-snippet and continuation work is owned by #4629. |
| Homebrew | None | Formula draft below | Needs a tap repository and Steven's go-ahead. |

## Install time by route

Measured on one Linux container with a cold target directory (#5311). The
last two rows were re-measured on a 4-core container in October 2026 from the
same tree:

| route | seconds |
| --- | --- |
| prebuilt 0.10.0 archive (download, checksum, extract) | about 1 |
| `cargo install ripr --locked` (0.10.0 from crates.io) | 128 |
| `cargo install --locked --path crates/ripr` (0.11.0 development build, workspace profile) | 640 |
| `cargo build --release --locked -p ripr` in the workspace (same profile, 4 cores) | 751 |
| `cargo build --release --locked` on the packaged 0.11.0 crate (what `cargo install ripr --locked` builds, 4 cores) | 275 |

The first three rows did not record a core count and the 640 s figure ran beside
light unrelated work, so read them as rough, not an exact ratio. The 751 s and 275 s rows came from `cargo build`,
not `cargo install`, and the packaged-crate row is a local build of the output of
`cargo package`, not a download from crates.io, which carries no 0.11.0 yet.

What dominates: the `ripr` crate itself, about 700 of 907 CPU-seconds in the
workspace build (library 462 s, binary 234 s), not its dependencies (about 210
s, the largest being `rmcp`, `oxc_parser`, `ra_ap_syntax` and
`rustpython-parser` at 9 to 24 s each). The workspace `[profile.release]` sets
`lto = true` and `codegen-units = 1`, and the published crate carries no
`[profile]` section, so a crates.io install compiles with cargo's default release
profile and takes about 2.7 times as little time as a workspace build (275 s against 751 s) such as the
git install in the README. As an experiment, thin LTO with 16 codegen units
built the workspace in 266 s; on a tiny crate `ripr check` took the same 7 ms,
and on the `ripr` workspace itself, a large Rust workspace, it took about 5% longer (10.5 s against
10.0 s) with an 11% larger binary. The release profile was not changed, since it
also builds the prebuilt archives. `cargo xtask first-run --install-published`
times the source route and the scoreboard records it as
`first_run.install_seconds`. The nightly `first-run-install.yml` workflow runs
that walk and ingests it, but the metric has no committed baseline sample yet,
so nothing fails on a slower install until one is committed from that
workflow's report; the prebuilt route is not timed by the walk.

## What blocks a developer who is not us

1. **Every fast path hangs on one event:** the 0.11.0 GitHub Release with its
   server archives. Binstall, the VS Code server download, the CI install and a
   Homebrew formula all read those assets.
2. **macOS and Windows users have no non-source path** in the package registries
   until npm and PyPI publish more than Linux x86-64.
3. **First-run trust:** the archive is unsigned beyond its SHA-256. Binstall
   verifies nothing further, and Homebrew verifies the pinned hash.

## cargo-binstall

`crates/ripr/Cargo.toml` now carries `[package.metadata.binstall]` pointing at
`ripr-server-v{version}-{target}.tar.gz` (`.zip` on `x86_64-pc-windows-msvc`).
The archive holds `ripr` at its root. The xtask test
`release_server::binstall_metadata` renders the templates for all five targets
and compares them with `server_asset_name`, the function the release archive
command names its assets with; a renamed asset fails the test.

Only releases published after this change contain the metadata. After 0.11.0
ships, check:

```bash
cargo binstall ripr --no-confirm   # expect a download, not a compile
ripr --version
```

## Homebrew formula (draft, not submitted)

Target: a third-party tap (`EffortlessMetrics/homebrew-tap`, which does not
exist yet), not homebrew-core. The formula installs the prebuilt archives, so
`brew install` takes seconds and needs no Rust. Fill each `sha256` from the
release's `.sha256` assets once 0.11.0 is published; do not guess them.

```ruby
class Ripr < Formula
  desc "Static mutation-exposure analyzer for Rust workspaces"
  homepage "https://github.com/EffortlessMetrics/ripr"
  version "0.11.0"
  license any_of: ["MIT", "Apache-2.0"]

  on_macos do
    on_arm do
      url "https://github.com/EffortlessMetrics/ripr/releases/download/v#{version}/ripr-server-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "FILL_FROM_RELEASE_SHA256_ASSET"
    end
    on_intel do
      url "https://github.com/EffortlessMetrics/ripr/releases/download/v#{version}/ripr-server-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "FILL_FROM_RELEASE_SHA256_ASSET"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/EffortlessMetrics/ripr/releases/download/v#{version}/ripr-server-v#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "FILL_FROM_RELEASE_SHA256_ASSET"
    end
    on_intel do
      url "https://github.com/EffortlessMetrics/ripr/releases/download/v#{version}/ripr-server-v#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "FILL_FROM_RELEASE_SHA256_ASSET"
    end
  end

  def install
    bin.install "ripr"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/ripr --version")
  end
end
```

Open points before this is real: the Linux archives are built on `ubuntu-22.04`
(glibc 2.35), and release qualification caps the required glibc symbol version at
2.34. Linux compatibility is therefore established only for glibc 2.34+ hosts;
older Homebrew Linux systems are unverified. The Windows archive has no
Homebrew analogue (Scoop or winget would be the equivalent, not drafted).

## Decisions that need the owner

- Publish 0.11.0 (unblocks binstall, the extension download, CI install).
- Create the tap repository and approve the formula.
- Decide whether npm and PyPI ship 0.11.0 on all five targets or stay
  Linux-only alphas; today's README text is honest about the alpha scope.

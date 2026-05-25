# STRIDE Security Threat Model — ripr-swarm

**Repository:** `/mnt/actions/_work/ripr-swarm/ripr-swarm`
**Generated:** 2026-05-25
**Last review:** db34454 ("ci: run swarm workflows on self-hosted runners only")
**Maintainer:** ripr project security policy (SECURITY.md)

---

## 1. Overview

`ripr` is a static mutation-exposure analyzer. It reads a git diff and a Cargo workspace, then reports which code paths lack discriminating test coverage. It does not run mutants or execute arbitrary code from diffs — its analysis surface is a Rust parser that produces structured findings.

The toolchain consists of:
- **Core binary** (`crates/ripr/`) — Rust library + CLI + LSP sidecar
- **VS Code extension** (`editors/vscode/`) — TypeScript host wrapping the LSP
- **Repo automation** (`xtask/`) — `cargo xtask` commands for CI and releases
- **GitHub Actions** (`.github/workflows/`) — self-hosted runners on Linux, macOS, Windows, ARM64
- **Release distribution** — archives and checksums published to GitHub Releases

---

## 2. Key Assets

| Asset | Type | Sensitivity |
|---|---|---|
| `crates/ripr/src/` | Rust source (forbid unsafe_code) | Core product |
| `xtask/src/` | Rust repo automation | Integrity of CI gates |
| `editors/vscode/` | TypeScript VS Code extension | User trust, distribution |
| `.github/workflows/` | CI/CD YAML | Supply chain integrity |
| `target/` build artifacts | Derived | None (not committed) |
| `policy/` allowlists | TOML policy files | Controls what is permitted |
| `.ripr/` | In-repo state, traceability | Evidence integrity |
| Release archives (`dist/`) | Binary distributions | User-facing integrity |
| GitHub Release manifests | JSON + checksums | Distribution integrity |
| Self-hosted runner VMs | CI infrastructure | Secrets, build provenance |

---

## 3. Entry Points

### 3.1 CLI Input Surface
- `cargo run -p ripr -- check --diff <file>` — user-supplied diff file
- `cargo run -p ripr -- explain --diff <file> --at <probe>` — user-supplied probe selector
- `cargo run -p ripr -- context --diff <file> --at <probe>` — same
- `cargo run -p ripr -- lsp --stdio` — stdio LSP server (VS Code extension)
- `cargo run -p ripr -- doctor` — workspace self-check

**Threat surface:** Diff file is untrusted text. Probe selectors are strings from the user. Both are parsed by trusted code but could trigger panics in parser errors (policy: no panic family).

### 3.2 LSP Interface
- `tower-lsp-server` sidecar listening on stdio
- `backend.rs` — LSP session state including workspace path and open documents
- `diagnostics.rs`, `hover.rs`, `actions.rs` — LSP method handlers

**Threat surface:** A malicious LSP client sending malformed requests could interact with the file system through workspace operations.

### 3.3 VS Code Extension Host
- `editors/vscode/src/` — TypeScript code that invokes the LSP server
- Server resolution order: `ripr.server.path` setting → bundled binary → cached download → verified first-run GitHub Release download → `ripr` on `PATH`

**Threat surface:** If a user sets a custom `ripr.server.path` to a malicious binary, VS Code will execute it with the user's permissions.

### 3.4 GitHub Actions (Self-Hosted Runners)
- CI workflow runs on self-hosted VMs: Linux X64/ARM64, macOS X64/ARM64, Windows X64
- Workflows check out repository code and run `cargo test`, `cargo xtask`, `npm run test:e2e`
- Release workflow builds and publishes binaries to GitHub Releases

**Threat surface:** Self-hosted runners have access to GitHub tokens with `contents: write` permission (for release uploads). A compromised workflow could exfiltrate tokens or tamper with release artifacts.

### 3.5 Release Artifact Distribution
- `cargo xtask release-server-archive` — produces `.tar.gz` or `.zip` archives per target
- `cargo xtask release-server-manifest` — generates `manifest.json` and SHA256 checksums
- `cargo xtask release-upload-assets` — uploads artifacts to GitHub Release

**Threat surface:** Checksums are only as reliable as the build pipeline. If build artifacts are tampered with before archiving, the checksums will cover the tampered files.

### 3.6 xtask Automation
- `xtask/src/` — Rust binary with policy checks, fixture runners, report generation
- `cargo xtask` commands are invoked from CI and locally
- Some commands write to `target/` and create reports

**Threat surface:** xtask is not distributed to users, but CI trust chain depends on it. A tampered xtask could bypass policy checks.

---

## 4. STRIDE Threat Analysis

### 4.1 Spoofing

**Threat:** An attacker impersonates a legitimate user, process, or system component to gain unauthorized access.

| Scenario | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Malicious diff file causes ripr to emit false findings framing innocent code as weak | Low | Medium | Findings are static evidence, not runtime mutations. No exec of diff content. |
| VS Code extension downloads aTrojaned ripr binary from a fake release | Medium | High | Server resolution checksums against manifest. `ripr.server.path` bypass is user opt-in. |
| Self-hosted runner is shared with a hostile workflow that steals GitHub tokens | Medium | High | Self-hosted runners should be dedicated. CI jobs run with minimal `contents: read`. Release upload tokens scoped to one repo. |
| Attacker forks the repo and submits a PR with a malicious workflow | Low | Medium | PRs from forks run with `contents: read` only. Release steps gated by `release-check` / `full-ci` labels or push to main. |

**Existing controls:**
- `cargo-deny` checks licenses, bans, advisories
- `dependency-review-action` blocks high-severity license diffs
- `actions/checkout@v6` with `fetch-depth: 0` for proper git history
- Release tokens scoped to repo (`GH_TOKEN: ${{ github.token }}`)

---

### 4.2 Tampering

**Threat:** Unauthorized modification of data, code, configuration, or build artifacts.

| Scenario | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Diff file contains specially crafted syntax that corrupts internal state | Low | Medium | No unsafe code. Parser errors return Result/Error not panics (enforced by `cargo xtask check-no-panic-family`). |
| Fixture files under `fixtures/` are modified to pass tests without real coverage | Low | Medium | Fixture contracts checked by `cargo xtask check-fixture-contracts`. Goldens checked by `cargo xtask goldens check`. |
| Policy allowlist (`policy/`) is updated to permit malicious files | Low | Medium | Policy changes reviewed in PRs. `cargo xtask check-file-policy` enforced in CI. |
| `xtask` source is tampered to bypass policy gates | Low | High | `xtask` not distributed. CI runs `cargo xtask check-*` which reads xtask source; a tampered xtask could skip its own checks. Mitigated by isolation between xtask command execution and policy evaluation. |
| Release archive is modified after build but before upload | Low | High | Archive created and checksummed in same step (`release-server-archive`). Manifest generated from those files. Upload step only transfers pre-made artifacts. |
| Git history is rebased or rewritten to hide malicious commits | Low | High | CI uses `fetch-depth: 0` to get full history. `cargo xtask check-traceability` enforces spec→test→code map. |
| Self-hosted runner disk is tampered with between workflow runs | Low | High | Runner hygiene is operational responsibility of the host. No automated cleanup that could remove evidence. |

**Existing controls:**
- `#![forbid(unsafe_code)]` workspace-wide
- No `panic`, `unwrap`, `expect`, `todo`, `unimplemented` in production (enforced)
- `cargo xtask check-generated` and `check-generated-clean` detect generated file drift
- `cargo xtask check-badge-diff-policy` prevents badge format abuse

---

### 4.3 Repudiation

**Threat:** An entity denies having performed an action that others cannot prove otherwise.

| Scenario | Likelihood | Impact | Mitigation |
|---|---|---|---|
| User claims ripr produced incorrect findings | Medium | Low | Output includes probe ID, file path, line number, exposure class, and evidence traces. JSON output is versioned. |
| Developer denies pushing a malicious commit | Low | Medium | Git history is immutable when properly used. CI `fetch-depth: 0` captures full history. |
| Attacker denies exfiltrating secrets from a compromised runner | High | High | Self-hosted runners lack hardware attestation. Depends on operational security. |
| CI workflow fails to run policy checks but is not caught | Low | Medium | `cargo xtask check-pr` is a required CI gate. `cargo xtask precommit` locally mirrors CI gates. |

**Existing controls:**
- `cargo xtask receipts` generates machine-readable evidence receipts for findings
- `cargo xtask pr-summary` documents the PR surface
- JSON output includes version, timestamp, git commit hash (when available)

---

### 4.4 Information Disclosure

**Threat:** Unauthorized exposure of sensitive data to an unauthorized party.

| Scenario | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Diff file contains credentials or secrets that ripr logs or emits in findings | Medium | High | ripr is a static analyzer; it parses text but does not execute it. Secrets in diffs are part of the input text and could appear in probe IDs or file paths in output. |
| Workspace contains sensitive files (`.env`, tokens) that ripr includes in analysis | Medium | Medium | `cargo xtask check-local-context` validates context boundaries. Workspace discovery is configurable. |
| LSP server exposes workspace file contents through diagnostics or hover | Low | Medium | LSP diagnostics only surface exposure findings, not raw file content. |
| VS Code extension logs server communications including file paths | Low | Low | Extension logs are local to the user's editor instance. |
| CI artifacts from failed runs contain workspace data | Low | Medium | `actions/upload-artifact` on failure includes `target/ripr/reports/`, `receipts/`, `pr/`, `review/`. These contain analysis outputs, not raw source. |
| `cargo xtask` reports include file paths or code snippets from the workspace | Medium | Medium | Reports are reviewer artifacts, not distributed. They are scoped to the diff surface. |
| Dependency on `rust-unic` (LGPL-3.0-only) leaks into supply chain | Low | Medium | `deny.toml` suppresses RUSTSEC advisories with explanation. Dependencies are MIT/Apache-2.0 only at the ripr distribution layer. |

**Existing controls:**
- `deny.toml` enforces license allowlist with explicit rationale for suppressions
- `cargo xtask check-public-api` tracks public symbol surface
- `cargo xtask check-local-context` validates context boundaries
- `cargo xtask check-network-policy` / `check-process-policy` document allowed operations

---

### 4.5 Denial of Service

**Threat:** An attacker prevents legitimate users from accessing or using the system.

| Scenario | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Maliciously large diff file causes ripr to consume excessive CPU/memory | Medium | Medium | ripr runs in-process with tokio async runtime. No resource limits are enforced. Large diffs may cause timeouts in CI. |
| Corrupt diff file causes infinite loop in parser | Low | Medium | No unsafe code, but parser bugs could cause resource exhaustion. `cargo xtask check-no-panic-family` prevents crashes but not hangs. |
| CI workflow is cancelled by a concurrent push, delaying review | Low | Low | CI uses `concurrency` group with `cancel-in-progress` only on PR synchronize events. Label toggles do not cancel. |
| LSP server hangs on a malformed workspace, blocking VS Code | Low | Medium | LSP backend has no documented timeout on workspace operations. |
| Self-hosted runner is taken offline, blocking all CI | Medium | Medium | Multiple runners per platform (Linux X64, macOS X64/ARM64, Windows X64, Linux ARM64). |

**Existing controls:**
- `cargo xtask` fixture tests have timeouts enforced via test harness
- CI runs `cargo xtask test-efficiency-report` to flag slow tests
- `cargo xtask check-pr` does not run fixtures (only `cargo xtask fixtures` does)
- `cancel-in-progress` prevents duplicate runs on same PR

---

### 4.6 Elevation of Privilege

**Threat:** An attacker gains capabilities without proper authorization.

| Scenario | Likelihood | Impact | Mitigation |
|---|---|---|---|
| VS Code extension `ripr.server.path` setting points to a malicious binary | Medium | High | User explicitly opts into custom path. README documents the risk. No mitigation in code — user responsibility. |
| Malicious PR workflow escalates from `contents: read` to `contents: write` | Low | High | Release steps are gated by labels or push to main. `release-upload-assets` job only runs on tags or manual dispatch with explicit version input. |
| LSP client sends crafted requests that read arbitrary files outside workspace | Low | Medium | LSP workspace operations are scoped to the discovered Cargo workspace root. No documented sandbox. |
| `cargo xtask` command is injected via environment variables | Low | Medium | xtask commands are defined in `xtask/src/main.rs` and dispatched via `cargo xtask <subcommand>`. No user-controlled command injection. |
| Attacker compromises self-hosted runner and uses it to access other repos or secrets | Medium | High | Runner hygiene is operational. GitHub tokens are scoped to the repository. Release uploads use `GH_TOKEN` scoped to one repo. |

**Existing controls:**
- `cargo xtask check-process-policy` documents allowed shell commands
- `cargo xtask check-network-policy` documents allowed network operations
- `cargo xtask check-supply-chain` validates dependency graph integrity
- No `sudo` in CI workflows
- Self-hosted runners isolated per platform

---

## 5. Existing Security Controls Summary

| Control | Type | Coverage |
|---|---|---|
| `#![forbid(unsafe_code)]` | Compiler | Rust codebase |
| `cargo xtask check-no-panic-family` | Policy gate | No panic/unwrap/expect in production |
| `cargo xtask check-static-language` | Policy gate | No "killed"/"survived" in static output |
| `cargo xtask check-allow-attributes` | Policy gate | All `#[allow(...)]` tracked |
| `cargo xtask check-file-policy` | Policy gate | Non-Rust file allowlist |
| `cargo xtask check-dependencies` | Policy gate | deny.toml enforced |
| `cargo xtask check-supply-chain` | Policy gate | Dependency graph integrity |
| `cargo xtask check-process-policy` | Policy gate | Allowed shell commands |
| `cargo xtask check-network-policy` | Policy gate | Allowed network operations |
| `cargo xtask check-workflows` | Policy gate | Workflow YAML validation |
| `cargo xtask check-generated` | Policy gate | Generated file drift detection |
| `cargo xtask check-public-api` | Policy gate | Public symbol allowlist |
| `cargo xtask check-output-contracts` | Policy gate | JSON output schema versioning |
| `cargo xtask check-traceability` | Policy gate | spec→test→code map |
| `cargo-deny` (GitHub Action) | Dependency audit | License, advisory, ban, source |
| `dependency-review-action@v4` | License audit | High-severity license diffs |
| `cargo xtask check-pr` | CI gate | Pre-release review gate |
| `cargo xtask precommit` | Local gate | Non-mutating guardrail |
| `deny.toml` suppressions | Explicit allowlist | rust-unic LGPL suppression documented |
| SECURITY.md | Policy document | Disclosure and response expectations |

---

## 6. Recommendations

### High Priority

1. **Self-hosted runner isolation** — Runners should be dedicated to this repo and not shared with untrusted workflows. Consider adding runner-level secrets rotation. Document the operational security expectations in `docs/`.

2. **LSP resource limits** — The LSP backend (`lsp/backend.rs`) should enforce timeouts and memory limits on workspace operations to prevent DoS from malformed workspaces.

3. **Diff file size limits** — The diff parser (`analysis/diff/parse.rs`) should reject or truncate diffs exceeding a reasonable size threshold (e.g., 10MB uncompressed) to prevent resource exhaustion.

4. **Server path integrity for VS Code** — The extension's server resolution logic should display a checksum verification prompt when using a custom `ripr.server.path`. Currently it silently executes whatever binary the user points to.

### Medium Priority

5. **xtask integrity** — `cargo xtask check-*` commands read xtask source. If xtask is tampered, policy checks could be bypassed. Consider adding a separate CI job that runs policy checks from a known-good xtask, or sign xtask binaries.

6. **Release artifact provenance** — The release workflow builds and archives in one step, then uploads in another. Add a provenance attestation (e.g., Sigstore cosign) to bind the archive to the Git commit and runner identity.

7. **Git history integrity** — Add a CI step that verifies the HEAD commit is reachable from the default branch and that the branch is not force-pushed after merge.

8. **Secrets in diffs** — Document clearly that users should not run `ripr check` on diffs containing credentials. Consider adding an optional redaction mode that strips strings resembling tokens from probe identifiers before output.

9. **Evidence receipt integrity** — `cargo xtask receipts` generates machine-readable receipts. Add a signing step so receipts can be verified as coming from the official `ripr` binary.

### Low Priority

10. **LSP hover diagnostic** — `hover.rs` should sanitize all file paths and code snippets before returning them to the LSP client to prevent information disclosure through hover.

11. **Extension telemetry** — Review `editors/vscode/` for any telemetry or crash reporting. If present, it should be documented and opt-in.

12. **Fixture mutation detection** — `cargo xtask check-fixture-contracts` validates fixture structure but does not detect semantic changes to fixture inputs. Consider adding a hash of fixture inputs to detect tampering.

---

## 7. Threat Model Maintenance

This threat model should be reviewed:
- On every major architecture change (new entry point, new distribution channel, new language adapter)
- After a security incident or near-miss
- Annually, as part of the security audit cycle
- After any change to the CI/CD infrastructure (new runner, new cloud provider, new secrets management)

Updates should be committed alongside the change that prompted them, with a note in `docs/LEARNINGS.md`.

---

*This document was generated as part of a STRIDE-based threat modeling exercise. It does not represent a penetration test or a code audit. It should be used as a starting point for security-sensitive changes and as a checklist for security review of PRs.*

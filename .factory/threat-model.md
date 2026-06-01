# STRIDE Threat Model for ripr-swarm

## Overview

**Repository**: https://github.com/EffortlessMetrics/ripr
**Product**: Static RIPR (Reach-Infect-Propagate-Observe-Discriminate) mutation-exposure analyzer for Rust/Cargo workspaces.
**Core Question**: "For the behavior changed in this diff, do the current tests appear to contain a discriminator that would notice if that behavior were wrong?"

## System Components

| Component | Description |
|-----------|-------------|
| `crates/ripr/src/` | Main library and binary (domain, app, analysis, output, cli, lsp modules) |
| `xtask/` | Repo automation (`cargo xtask` commands) |
| `editors/vscode/` | VS Code extension with LSP client |
| `.github/workflows/` | CI/CD pipelines (14 workflow files) |

---

## 1. Spoofing

**Threat**: How could someone impersonate the tool or its outputs?

### Rust Library/Binary (`crates/ripr/`)
- **Binary substitution**: An attacker with write access to `PATH` or configuration could replace the `ripr` binary with a malicious variant that reports false exposure estimates.
- **Library embedding spoofing**: A downstream crate embedding `ripr` as a library could wrap `check_workspace` to return falsified `CheckOutput` with manipulated `ExposureClass` values.

### VS Code Extension (`editors/vscode/`)
- **Server path hijacking**: The extension resolves the LSP server in order: configured path → bundled binary → cached download → verified first-run GitHub Release download → `PATH`. A compromised `ripr.server.path` or man-in-the-middle during download could deliver a malicious binary.
- **Extension impersonation**: A malicious VSIX package with the same publisher name could impersonate the official extension.

### GitHub Actions Workflows
- **Workflow impersonation**: External actors cannot directly impersonate workflows since GitHub Actions run in the context of the repository. However, a compromised runner could report false CI statuses.
- **Action pin drift**: Workflows use pinned commit SHAs for third-party actions (e.g., `actions/checkout@v6`, `dtolnay/rust-toolchain@stable`), but `droid-action-safe` and some droid workflows use full SHA pins that should be verified.

### xtask Automation
- **xtask command spoofing**: If a developer confuses a malicious `xtask` binary with the legitimate one, it could execute arbitrary code during `cargo xtask` invocations.

---

## 2. Tampering

**Threat**: How could code or data be modified maliciously?

### Code Tampering
- **`unsafe_code = "forbid"` policy**: The workspace enforces `forbid(unsafe_code)` at compile time. This prevents introduction of unsafe Rust code that could lead to memory corruption, but does not prevent all undefined behavior (e.g., logic bugs, algorithmic complexities).
- **Config suppression paths** (`crates/ripr/src/config.rs`): The config allows suppression paths that could theoretically be exploited if an attacker controls config files; tests verify config rejects unsafe suppression paths.
- **Policy allowlists**: Files under `policy/` (e.g., `dependency_allowlist.txt`) control what dependencies and files are permitted. Tampering with these could allow introduction of malicious dependencies.

### Data Tampering
- **Diff file manipulation**: `CheckInput.diff_file` accepts an optional path to a unified diff. If an attacker can control this path, they could provide a crafted diff that triggers unexpected behavior in the parser.
- **LSP URI handling** (`lsp/uri.rs`): Path encoding/decoding logic handles percent-encoded URIs and Windows drive letters. A malicious URI could potentially cause path traversal if not properly validated.
- **Output/report files**: `target/ripr/` directories contain generated reports that could be tampered with by processes with filesystem access.

### Workflow Tampering
- **Workflow injection**: GitHub Actions workflows that process PR labels, titles, or other PR content use those values in shell commands. While workflows use proper quoting, malformed input could potentially affect outputs.
- **Artifact tampering**: Uploaded artifacts (e.g., `ripr-reports`, VSIX packages) could be modified after upload if storage is compromised.

---

## 3. Repudiation

**Threat**: Can actions be taken that can't be traced?

### Audit Trail
- **Git history**: The repository uses git for all source code history. Blobs are content-addressed, providing integrity verification.
- **Workflow runs**: GitHub Actions provides audit logs for workflow executions.
- **Droid operations**: The droid-security-scan and droid-review workflows write to PR comments and issues, providing traceability.

### Non-Repudiation Gaps
- **Local execution**: `cargo xtask` commands executed locally leave no centralized audit trail unless explicitly captured.
- **Report artifacts**: Reports written to `target/ripr/` are local filesystem artifacts with no inherent provenance tracking beyond git.
- **VS Code extension telemetry**: The extension may collect usage telemetry depending on user settings; this is not covered in the codebase.

### Evidence/Receipt System
- **RIPR receipts** (`cargo xtask receipts`): Machine-readable evidence receipts exist but their security depends on the integrity of the underlying git state and workflow artifacts.

---

## 4. Information Disclosure

**Threat**: Could sensitive data be exposed?

### Source Code Exposure
- **Rust source parsing**: `ripr` parses Rust source files to generate probes. This involves reading source code from the filesystem. If the tool is run against repositories with secrets in source code, those secrets could be:
  - Included in probe IDs or finding descriptions
  - Logged in debug output
  - Exposed through LSP hover information
- **Diff content**: Unified diffs may contain context lines from source files, potentially exposing sensitive code sections in reports.

### Configuration Secrets
- **GitHub Secrets**: Workflows use several secrets:
  - `VSCE_PAT`: VS Code Marketplace publish token
  - `OVSX_PAT`: Open VSX publish token
  - `CODECOV_TOKEN`: Codecov upload token
  - `MINIMAX_API_KEY`: AI API key for droid operations
  - `FACTORY_API_KEY`: Factory Droid API key
- **Secret exposure risk**: Workflow files show secrets are used directly in env vars. If workflows are modified or if there are workflow injection vulnerabilities, secrets could be exfiltrated.
- **Droid workflow config files**: `droid.yml`, `droid-review.yml`, `droid-security-scan.yml` write MiniMax API keys to `~/.factory/settings.local.json` during workflow runs.

### Path/URI Information
- **File path exposure**: Findings include `PathBuf` for source locations and test file paths. Reports generated from `cargo xtask` commands may expose internal directory structures.
- **LSP URI handling**: File URIs are encoded/decoded; improper handling could leak path information.

### Supply Chain Information
- **Dependency tree**: The `Cargo.lock` contains all transitive dependencies with exact versions. This information is public but could reveal:
  - Use of specific libraries that might have vulnerabilities
  - Version pinning that reveals security awareness (or lack thereof)

---

## 5. Denial of Service

**Threat**: Could the tool be made unavailable?

### Local DoS
- **Resource exhaustion**: Large diffs or deeply nested Rust source trees could cause memory or CPU exhaustion during analysis. The `analysis` module uses `ra_ap_syntax` for Rust parsing, which could be resource-intensive on large codebases.
- **Filesystem exhaustion**: Writing many reports to `target/ripr/` could fill disk space.
- **Temp directory exhaustion**: `agent/provenance.rs` creates temp directories with unique names based on timestamps; repeated rapid calls could exhaust inodes or temp space.

### LSP Server DoS
- **Protocol exhaustion**: The LSP server (`lsp/`) handles multiple concurrent requests. A malicious editor could send rapid requests that exhaust server resources.
- **Analysis cache corruption**: If `target/ripr/cache` is corrupted, analysis may fail until cache is cleared.

### CI/CD DoS
- **Workflow cancellation**: The `ci.yml` workflow cancels in-progress runs on PR synchronization. An attacker with PR access could continuously push commits to cancel CI runs.
- **Artifact upload flooding**: If an attacker gains ability to trigger workflows, they could upload large artifacts repeatedly.
- **Concurrency limits**: GitHub's workflow concurrency limits could be exploited by creating many PRs or branches.

### Dependency DoS
- **crates.io outage**: If `ra_ap_syntax` or other dependencies become unavailable on crates.io, builds would fail.
- **GitHub Release outage**: VS Code extension auto-download functionality relies on GitHub Releases being available.

---

## 6. Elevation of Privilege

**Threat**: Could an attacker gain additional capabilities?

### Code Execution
- **`cargo xtask` execution**: xtask runs as part of the build process with the same privileges as the developer. A compromised xtask command could:
  - Execute arbitrary shell commands during build
  - Modify source files
  - Exfiltrate environment variables (including credentials in some CI contexts)
- **Build artifact execution**: Released binaries (`ripr`, VSIX packages) are executed by users. If a binary is compromised, it runs with user privileges.

### CI/CD Privilege Escalation
- **Workflow permissions**: Workflows use varying permission levels:
  - `ci.yml`: `contents: read` (least privilege)
  - `publish-extension.yml`: `contents: write` (needed for release uploads)
  - `droid-*` workflows: `contents: write`, `pull-requests: write`, `issues: write`, `id-token: write`, `actions: read`
- **`id-token: write`**: The droid workflows request `id-token: write` which allows OIDC token generation. This is used for Factory Droid authentication but could theoretically be misused.
- **Self-hosted runners**: The repository uses self-hosted runners which have more privileges than GitHub-hosted runners. A runner compromise could affect the entire repository.

### VS Code Extension Privilege Escalation
- **Extension capabilities**: The VS Code extension requests:
  - `onLanguage:rust` activation on any Rust file
  - Access to workspace files via LSP
  - Command execution via `ripr.*` commands
- **Server path override**: Users can configure `ripr.server.path` to point to any executable, effectively allowing arbitrary code execution in the extension's context.

### LSP Server Privilege Escalation
- **File system access**: The LSP server reads source files for analysis. It should only access files within the workspace root, but path traversal vulnerabilities could allow access outside.
- **Command execution**: The LSP server exposes commands like `ripr.restartServer` and diagnostic refresh; these are limited to server management but could be exploited.

### Rust-Specific EoP Concerns
- **No `unsafe_code`**: The forbid policy prevents direct memory corruption via unsafe code, but logic bugs can still cause security issues.
- **Dependency trust**: The tool trusts its dependencies (particularly `ra_ap_syntax`) to correctly parse Rust code. A compromised parser could produce incorrect analysis or crash.
- **Denial of service leading to crash**: A malformed diff or source file could cause the analyzer to panic (though `panic` is denied in lints and checked by `cargo xtask check-no-panic-family`).

---

## Risk Summary by STRIDE Category

| Category | Risk Level | Primary Concerns |
|----------|------------|------------------|
| **Spoofing** | Medium | Binary substitution, VS Code server download, xtask confusion |
| **Tampering** | Low-Medium | Code integrity enforced by forbid policy; config and policy allowlists mitigate supply chain risks |
| **Repudiation** | Low | Git history provides traceability; local xtask runs lack centralized audit |
| **Information Disclosure** | Medium | Source code parsing exposes content; GitHub secrets in workflows; path leakage in reports |
| **Denial of Service** | Low-Medium | Resource exhaustion on large repos; workflow cancellation; self-hosted runner availability |
| **Elevation of Privilege** | Medium | `cargo xtask` executes arbitrary commands; self-hosted runners; VS Code server path override |

---

## Existing Mitigations

1. **`unsafe_code = "forbid"`**: Prevents memory safety issues in Rust code
2. **Lint enforcement**: Comprehensive Clippy lints prevent many bug patterns
3. **Policy allowlists**: `policy/dependency_allowlist.txt`, `policy/non_rust_allowlist.txt` control supply chain
4. **cargo-deny**: `security.yml` workflow runs `cargo-deny` for advisories, licenses, and banned sources
5. **Dependency review**: `security.yml` runs GitHub's dependency-review action
6. **Pinned action SHAs**: Most third-party actions use pinned commit SHAs
7. **Secret management**: GitHub secrets used for all sensitive credentials
8. **Workflow concurrency**: Limits prevent duplicate workflow runs
9. **Output schema versioning**: `CHECK_OUTPUT_SCHEMA_VERSION = "0.1"` provides stability for consumers

---

## Recommendations

1. **Verify action pins**: Some workflows use full SHA pins that should be audited against known-good versions
2. **Consider signing releases**: Add code signing for released binaries to prevent binary substitution
3. **Review droid workflow permissions**: The `id-token: write` permission should be audited for minimum necessary scope
4. **Add path traversal testing**: Fuzz test the LSP URI and diff file parsing logic
5. **Document secret rotation**: Establish and document secret rotation procedures for `VSCE_PAT`, `OVSX_PAT`, `MINIMAX_API_KEY`, `FACTORY_API_KEY`
6. **Consider SBOM generation**: Generate Software Bill of Materials for released artifacts
7. **Audit self-hosted runners**: Ensure runners are properly isolated and secured
8. **Review VS Code extension auto-download**: The manifest download verification should be documented and audited

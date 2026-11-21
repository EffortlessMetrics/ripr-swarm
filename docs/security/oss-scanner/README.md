# OSS Scanner preparation

This is the maintainer entry point for preparing RIPR for
[Anthropic OSS Scanner](https://github.com/anthropics/oss-scanner).
The execution owner is [#14053](https://github.com/EffortlessMetrics/ripr-swarm/issues/14053).
The [offline environment](https://github.com/EffortlessMetrics/ripr-swarm/issues/14054)
and [maintainer/contact/submission](https://github.com/EffortlessMetrics/ripr-swarm/issues/14055)
issues carry the remaining work and its evidence.

This documentation is preparation, not enrollment, acceptance, a successful
scan, a security certification or a release-readiness claim. It does not supply
a qualified Dockerfile or an approved security contact. Native image and
patch/retest qualification remain required before submitting the proposed
configuration below.

## One project, an explicit source target

The requested preparation target is `EffortlessMetrics/ripr-swarm`: RIPR's
active product-development repository. `EffortlessMetrics/ripr` owns public
releases. Present that relationship as one project, not two independently
adopted products. The enrollment must name the repository actually copied into
the scanner image. Do not replace swarm source with a public release binary,
clone sibling repositories into the target, or silently change the scan target.

A maintainer may instead select the canonical public repository after upstream
feedback. In that case, use the existing [source-promotion route](../../SOURCE_PROMOTION.md)
and requalify the actual public source and image. A swarm receipt is not proof
of the public checkout merely because the product has the same name.

Anthropic makes its own eligibility decision. Its
[eligibility guidance](https://red.anthropic.com/oss-scanner/) considers project
maturity, security relevance and genuine use or dependents. A public repository
and a permissive license are helpful but do not establish eligibility. A new
release, a large test count, internal agent activity or a Docker build cannot
substitute for external impact evidence. Do not invent a star/download target
or make completion of every RIPR release/optimization issue a scanner gate.

## Scanner-facing files

These repo-relative paths can be referenced directly from `project.yaml`:

| File | Purpose |
| --- | --- |
| [threat_model.md](threat_model.md) | Project boundaries, attacker-controlled inputs, intended protections and severity reasoning. |
| [report_guidelines.md](report_guidelines.md) | Concise report format with reproducible evidence. |
| [patch_guidelines.md](patch_guidelines.md) | Narrow patches and discriminating regression tests. |

The guidelines do not redefine scope or suppress findings; scope belongs in
the threat model. Upstream currently limits each guidelines file to 16 KiB.
Read the current upstream validator before submission because the accepted
schema can change.

The future image recipe belongs at `.oss-scanner/Dockerfile`, with a narrow
repository file-policy allowance and its own review. Keeping the documents
under `docs/` uses the existing documentation surface; `.oss-scanner/` is an
upstream recommendation, not a required location for the documents.

## Build online, investigate and retest offline

The inspected upstream harness on November 10, 2026 defines:

| Phase | Observed limits |
| --- | --- |
| Clone through completed image | 45 minutes; build machine: 16 CPUs, 64 GB memory, 120 GB disk. |
| Offline audit agent | 2 CPUs, 8 GB memory. |

Source: upstream
[check_build.py](https://github.com/anthropics/oss-scanner/blob/main/tools/check_build.py).
These are a dated observation, not a permanent service guarantee. Record the
upstream commit and actual resource settings during qualification. QEMU can
scale the machine down to fit its host; a smaller local run is not the same
configuration as the advertised envelope.

The image must retain the enrolled source in `/src`, its compiler, dependency
sources/caches, fixtures and the test harnesses an investigator needs. A
runtime-only final image, discarded Cargo cache or prebuilt CLI alone is
insufficient for testing a patch. Do not rely on a BuildKit cache mount as the
only copy of an offline-required dependency.

Read `rust-toolchain.toml`, manifests and lockfiles from the selected source.
Do not follow floating stable or change the product's default features merely
to make the scanner build smaller. Include the default Rust/Python/TypeScript
surface and explicitly account for optional Perl ingestion. Keep editor,
provisioning and packaging sources visible; document the executable coverage
and missing platform/tool dependencies instead of silently excluding them from
security review.

Prefetch the dependencies for an audited, named set of test targets. Root
`cargo fetch --locked` does not by itself prove that independently locked or
dynamically constructed fixture projects work offline. Conversely, walking
and building every `Cargo.toml` can execute intentionally invalid test inputs.
Inventory the selected consumers and their nested manifests first.

Use the existing [development validation](../../agent-context/validation.md)
and [testing](../../TESTING.md) procedures. Resolve effective Cargo target,
intermediate and temporary directories; an inherited environment variable may
be overridden by repository Cargo configuration. Retain one owned build tree
where safe rather than repeatedly rebuilding identical profiles. Never merge
concurrent writable candidate targets to obtain apparent cache savings.

### Qualification that is not just a warm-binary demonstration

The environment owner must retain evidence for all of these dimensions:

1. Build the selected source and scanner tools layer online within the actual
   build envelope; record source/tree identity, image digest, compiler,
   features/profile, locks, elapsed time and disk growth.
2. In the finished image, disable network access and constrain the container
   to the audit-agent CPU/memory envelope, with swap explicitly accounted for.
   Run the declared native test targets. Record actual discovery and execution
   counts, exits and logs; zero selected tests is not a passing suite.
3. In a disposable container, make a small but nontrivial change to a
   representative first-party security boundary. Rebuild the affected
   production and regression-test target and rerun it offline under the same
   limits. Retain original-pass, controlled-wrong-behavior-fail and
   restored-pass evidence. A timestamp touch or an unchanged cached executable
   is not this proof.
4. Challenge the receipt with a stale/wrong binary, wrong root, absent cached
   dependency and nonexistent test selector. None may become success-shaped
   evidence. Retain observed failures and omitted dimensions.
5. Check process cleanup and repeated-session scratch/memory growth. Report
   aggregate process-tree or cgroup memory separately from the maximum RSS of
   one compiler process.

`CARGO_NET_OFFLINE=true` prevents Cargo network use; it is not an operating-system
network boundary. A precompiled lib-test binary can execute within 8 GB even
when recompiling it cannot. Filtering its test name does not make rustc compile
only that test. Use existing bounded integration targets and measured compiler
work before proposing an architectural split.

The upstream [tools/check](https://github.com/anthropics/oss-scanner/blob/main/tools/check)
exit status describes image-build success. Retain the native statuses and
nonempty test inventories from the shell; exiting that shell successfully is
not an offline-test verdict. `tools/check --no-shell` is build-only proof.
`tools/check --qemu` is useful VM-parity evidence, but its availability and
actual host limits must remain explicit.

Do not add a new expensive PR workflow just to stage this recipe. Develop and
measure it on an isolated builder, then reuse existing validation/CI owners for
any proven regression. A below-20-minute internal CI result is not an
Anthropic enrollment requirement.

## Maintainer and disclosure prerequisites

Follow [SECURITY.md](../../../SECURITY.md). Before enrollment, the maintainer
must choose and verify a monitored public `primary_contact`, publish that
exact contact in the project's security policy, and approve its public
visibility. Do not infer a mailbox from a Git author, invent a security alias,
or use a GitHub noreply address as a receiving inbox.

Prepare evidence that the applicant maintains RIPR and that the supplied
address is controlled by its maintainers. Add dated, attributable external
usage/dependency evidence. Separate automated downloads, internal dogfood and
independent users; unavailable metrics remain unknown.

The [OSS Scanner agreement](https://red.anthropic.com/oss-scanner/terms/) and
[contributor agreement process](https://github.com/anthropics/oss-scanner/blob/main/CONTRIBUTING.md)
require the maintainer's own decisions. General repository-development
approval is not an agreement signature. Keep findings in a maintainer-only
channel, independently reproduce them and review proposed patches. Do not
copy unpatched reports into public issues or general bot-review prompts.

Email is the baseline delivery route. GitHub advisory delivery is optional
and requires enabled private vulnerability reporting plus the upstream
`oss-scanner` trusted reporter. Those settings need their own authorization
and verification; do not read or modify branch protection to prepare this
packet. An OpenPGP public key can encrypt email to the primary contact, but
cannot be combined with `auto_ccs` under the inspected upstream contract.

## Non-submittable configuration example

The null contact below is intentional and must fail validation. It is not a
contact-selection shortcut or a checked-in enrollment. The Dockerfile path
names planned work until the environment issue has delivered it.

```yaml
repo: https://github.com/EffortlessMetrics/ripr-swarm#main
primary_contact: null # Replace only with an approved, verified public mailbox.
homepage: https://github.com/EffortlessMetrics/ripr
dockerfile: .oss-scanner/Dockerfile
threat_model: docs/security/oss-scanner/threat_model.md
report_guidelines: docs/security/oss-scanner/report_guidelines.md
patch_guidelines: docs/security/oss-scanner/patch_guidelines.md
output_format:
  email:
    min_severity: low
  github_security_advisory:
    min_severity: none
```

After the prerequisites are satisfied, prepare one project directory in an
upstream checkout, use its current validator and `tools/check`, retain the
exact subjects and offline-test evidence, and submit one explicitly authorized
enrollment PR. `#main` selects a branch, not an immutable reviewed snapshot;
record the actual cloned commit and reconcile changes before submission.
If the Dockerfile lives beside upstream `project.yaml`, omit `dockerfile`
instead of defining it twice. Do not add operator-only budget, cadence,
priority or scheduling fields.

Keep these outcomes separate: documentation prepared; image implemented;
offline environment qualified; maintainer/contact verified; submission
authorized; enrollment submitted; accepted; first scan completed. The execution
issues record those states without treating any earlier one as completion.

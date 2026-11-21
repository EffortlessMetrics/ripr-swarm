# RIPR security scope and threat model

## Project and trust context

RIPR statically analyzes changed code and the available test evidence. It
provides a CLI/library, an experimental LSP sidecar, a bounded read-only MCP
adapter, editor integration, and artifact/repair workflows. Static exposure
classifications are advisory evidence, not proofs of correctness, runtime
mutation adequacy or merge safety.

`ripr-swarm` is the active development repository for the same RIPR project
whose public releases are issued from `EffortlessMetrics/ripr`. Review the
first-party code present in the enrolled checkout. Third-party dependency
sources may be necessary for building and reproducing a first-party boundary
failure; they are not independent scan targets. The development repository
does not own public release credentials or authorize publication.

Relevant security context includes a maintainer inspecting an untrusted pull
request, a developer opening a downloaded repository, and an editor/agent/CI
consumer reading produced evidence. The LSP and MCP processes are not claimed
to be publicly exposed network services. Explain the concrete integration and
attacker foothold for any finding rather than assuming remote access.

## Assets and attacker-controlled inputs

Assets include the operator's files and credentials, processes and computing
resources, selected source identities, integrity of analysis/repair receipts,
and trusted executable/download identity.

Potentially hostile inputs include source files; Git patches, paths and
repository metadata; configuration; symlinks; serialized facts/caches;
coverage or semantic-index inputs; retained reports and repair receipts;
LSP/MCP messages and URIs; and metadata or archives consumed by provisioning
code. An attacker may be able to contribute these inputs without possessing
the operator's ambient privileges. Distinguish this from a trusted operator
explicitly choosing an executable or test command.

## Boundaries to examine

| Boundary | Intended security property and useful counterexample |
| --- | --- |
| Parsing and ingestion | Malformed/truncated/oversized inputs produce bounded errors or explicit limitations, not memory corruption, uncontrolled allocation, pathological recursion or persistent service failure. Exercise actual entrypoints and supported input budgets. |
| Filesystem and source selection | Caller-selected roots and admitted artifact paths remain authoritative. Review traversal, symlink replacement, path normalization, cross-checkout substitution, rename/race behavior and atomic writes. Demonstrate any out-of-root read/write on harmless temporary files. |
| Subprocess and verification | Repository data cannot silently become arbitrary commands or shell arguments. Review argument boundaries, environment/cwd handling, executable selection, timeouts, descendant cleanup and interruption. Explicitly requested native tests are execution, not a sandbox guarantee. |
| Artifact identity and state | Cached facts, evidence, receipts and continuation state are bound to the intended source/configuration/tool identities. Missing, corrupt, stale, substituted or cross-root inputs must not acquire success or execution authority. Show the downstream decision affected by a forged state. |
| Protocol and editor integration | Message/URI validation, request size, cancellation, lifecycle and workspace boundaries remain enforced. The read-only MCP adapter must not gain edit or execution capabilities through input data. Trace editor-to-server actions as well as the transport parser. |
| Reports and agent handoff | Untrusted source text, names, paths and evidence remain data when rendered into Markdown, annotations, terminal output or agent packets. Identify any unintended capability or action actually induced; do not equate arbitrary text appearing in a report with demonstrated code execution. |
| Provisioning and distribution code | When present in the enrolled tree, archive extraction, manifests, checksums, version/platform matching and fallback behavior must not allow wrong or unverified executables to become trusted. Keep package identity and credential boundaries explicit. |
| Repository automation | Review first-party CI/admission/publication logic present in this checkout for candidate-to-privileged-execution or artifact-substitution boundaries. Demonstrate locally without real secrets, protected-ref changes or registry writes. |

The use of safe Rust does not exclude logic errors, denial of service,
filesystem/process errors or faults reached through dependencies. Conversely,
a resource-intensive valid input is not automatically a security vulnerability:
show attacker control, affected budgets, operational impact and recovery.

## Execution versus ambient trust

Ordinary static inspection should not execute analyzed application source as
a side effect of parsing it. Some explicit verification or operator-selected
workflows intentionally execute native tools/tests. An authorized Cargo build
can execute project build scripts, and a selected test may run arbitrary
project code. Do not present this expected execution as a RIPR sandbox escape.
Report an unintended transition into that authority, a broken documented
boundary, or failure to terminate the processes RIPR owns.

Process containment and an edit boundary are not a claim that an arbitrary
consumer repository is safe to execute with sensitive credentials. A malicious
local user who already controls the installed RIPR executable or the operator's
entire account is a different threat from a contributor-controlled repository.
State any stronger assumed attacker capability.

## Severity guidance

Assess severity from reachable impact, not from the subsystem name. Consider
unintended code execution, credential disclosure, or material file overwrite
from contributor-controlled input high impact when the ordinary supported
workflow makes it reachable. Critical severity requires a correspondingly
broad or privileged, realistically reachable compromise; an unproven chain
must remain an uncertainty rather than being asserted as impact.

For availability findings, distinguish a bounded command failure from a
repeatable persistent sidecar failure, escaped process tree or resource drain
that defeats the intended budget. Include size/cost and recovery observations.

A wrong static classification, missed test connection or inaccurate repair
suggestion is normally a product-correctness defect. It becomes security
relevant only with a concrete adversarial effect on a security boundary or
consumer decision. Do not claim RIPR proves test adequacy or security, and do
not dismiss a real boundary failure merely because the product is advisory.

## Reproduction safety and coverage limits

Use synthetic local repositories, harmless canary files, isolated processes
and disposable image state. No production credentials, customer data, real
publication, network attacks, or changes to third-party systems are needed.
Keep findings private under the project's security-reporting policy until
maintainers have reviewed the disclosure path.

The scanner environment's tested feature/platform inventory must be explicit.
A Linux-only reproduction is not Windows/macOS execution evidence. Unavailable
editor-host assets, optional producers or a rebuild that exceeds the audit
machine's memory budget are coverage limitations, not reasons to suppress
potential findings in those sources. Preserve the complete first-party source
for review and name the unavailable execution dimension.

The maintainer guide owns environment commands and qualification. The separate
report and patch guidelines describe presentation and patch style only. This
threat model does not authorize enrollment, release operations or expansion
of the scan to another project.

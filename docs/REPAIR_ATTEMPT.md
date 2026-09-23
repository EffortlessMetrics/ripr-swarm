# Repair attempt identity

`ripr agent repair` has three phases: `before`, `after`, and `verify`. The before phase prepares bounded evidence; a human or external coding agent makes one focused test edit; the after phase checks the exact prepared transaction and writes its static review receipt. For a trust-bound Python attempt, the separately authorized verify phase executes the retained packet's bounded verification route and records execution and static movement separately. The ordinary unbound before/edit/after path remains available; it does not authorize execution.

The durable object is a **repair attempt**, not a seam lookup and not the repository-global workflow directory.

## Ordinary sequence

```text
ripr agent repair --root . --seam-id <seam-id> --phase before
# make the focused test edit outside RIPR
ripr agent repair --root . --attempt <repair-attempt-id> --phase after
```

The before phase prints the attempt manifest path and the exact `--attempt` command to run next. Preserve that command across agent sessions, process restarts, and concurrent work.

`--seam-id <id> --phase after` remains a compatibility route. It succeeds only when exactly one awaiting attempt has that seam. Zero or multiple matches fail closed; RIPR does not guess which attempt is newest or intended.

## Governed Python sequence

Start with an accepted repair-trust selection manifest and its selected row ID
([RIPR-SPEC-0176](specs/RIPR-SPEC-0176-python-repair-trust-attempts.md)).
The selection row must match the current repository HEAD and the packet's exact
test-only target. This command consumes the selection; it does not create or
approve a cohort. The manifest must be inside the selected repository root;
relative paths resolve from that root. Keep its bytes unchanged throughout the
attempt. Finish verification before applying another trust-bound attempt in the
same repository: the compatibility apply record is repository-global, and a
later apply prevents verification of the earlier attempt.

```text
ripr agent repair --root . --seam-id <seam-id> --phase before --python-repair-trust-manifest <selection.json> --python-repair-trust-attempt <selection-attempt-id> --edit-authorized --edit-authority <operator-id>
# preserve the printed repair-attempt ID; make the focused test edit outside RIPR
ripr agent repair --root . --attempt <repair-attempt-id> --phase after --edit-authorized --edit-authority <operator-id>
ripr agent repair --root . --attempt <repair-attempt-id> --phase verify --verify-authorized --verify-authority <operator-id>
```

The selection-attempt ID identifies a manifest row; the repair-attempt ID is the
new durable transaction printed by `before`. They are not interchangeable.

- Pass both `--python-repair-trust-manifest` and
  `--python-repair-trust-attempt` on `before` only. Later phases consume and
  revalidate the retained binding, not a replacement manifest argument.
- Pass `--edit-authorized` and `--edit-authority` together on the trust-bound
  `before` and `after` phases. Use the same operator or agent identity.
- `verify` accepts only `--attempt`, never `--seam-id`. It requires an applied,
  trust-bound attempt and the separate `--verify-authorized` /
  `--verify-authority` pair, reaffirming the identity that authorized the edit.
  The edit flags cannot substitute for verification authorization.
- Keep HEAD, the applied patch, configuration, and retained artifacts unchanged
  between `after` and `verify`. Identity drift refuses execution; do not repair
  a stale attempt by rewriting its retained evidence.

### Verification, rollback, and receipt

The verify phase revalidates the retained identities before execution. It runs
only the packet's producer-owned typed verification route through bounded
execution controls, then reruns analysis and compares the intended native Python
behavior by identity. A missing canonical route records execution as
`unavailable`; it does not authorize a guessed command.

To request restoration of the applied test edit after the observations, append
`--verify-rollback` to the authorized verify command. This is optional and valid
only on `verify`. The rollback touches only the allowed changed edit surface,
not the workflow artifacts. It refuses a restore that cannot recover the
baseline bytes (including pre-attempt dirty content or a moved index), and
checks the pinned HEAD and remaining edit residue. Inspect the receipt's
`rollback` disposition: `proved`, `blocked`, or `not_run`. A request is not a
guarantee of restoration, and an error before the rollback step does not undo
the edit automatically.

The candidate receipt is written to
`target/ripr/workflow/python-repair-driver-verification.json`; the bounded
execution record is
`target/ripr/workflow/python-repair-driver-verification-execution.json`, and the
fresh analysis is
`target/ripr/workflow/after-verification.repo-exposure.json`. Preserve these
artifacts with the attempt's evidence. The receipt path is repository-global:
verification refuses to overwrite an existing receipt, even for another attempt.
Archive that evidence before explicitly removing the receipt for a new run.

**Execution and static movement are independent observations.** A passed command
does not imply improved static exposure, and improved exposure does not imply a
passed command. Inspect both axes and their limitations, not just the command's
exit status. The candidate receipt grants no lifecycle, acceptance, closure,
repair-correctness, support-tier, gate, badge, or promotion claim. Cohort review
and acceptance remain separate work.

## Durable location

Each before phase reserves an immutable directory:

```text
target/ripr/repair-attempts/<repair-attempt-id>/
├── attempt.json
├── before-commitment.sha256
└── artifacts/
    ├── workflow.json
    ├── commands.md
    ├── agent-brief.json
    ├── before.repo-exposure.json
    ├── agent-packet.json
    └── attempt-baseline.json
```

The exact filenames follow the command-owned source artifacts. `attempt.json` identifies them by semantic role and binds each retained file by path, byte count, and SHA-256 digest.

Repository-global files under `target/ripr/workflow/` remain compatibility projections for existing cockpit and review consumers. They are not repair-attempt identity.

## Manifest contract

The manifest schema is `schemas/ripr/repair-attempt.schema.json` (`schema_version: "0.1"`). A prepared manifest records:

- the closed-form `repair_attempt_id`;
- canonical repository root and concrete Git `HEAD`;
- producer version and selected seam ID;
- creation time;
- retained before artifacts and content commitments;
- the exact next command;
- limitations and explicit non-claims.

The before commitment is derived from the prepared manifest. Terminal updates may add after-phase evidence, but they cannot silently rewrite the retained before identity or artifacts.

## After-phase authority

`--attempt <id>` resolves one manifest directly. Before producing a receipt, RIPR verifies that:

1. the manifest path is bound to the selected attempt ID and repository root;
2. the manifest is still in `awaiting_edit`;
3. the retained before snapshot, packet, and edit-cage baseline still match their recorded byte counts and digests;
4. the after phase uses that attempt's retained packet rather than a repository-global or another attempt's packet;
5. repository `HEAD` still matches the prepared head;
6. the observed edit delta is compliant with the retained packet's allowed, forbidden, and expected operational-write surfaces;
7. verify output, packet digest, delta digest, and receipt all bind to the same attempt.

A different attempt for the same seam is a different transaction. Its packet, snapshot, baseline, and terminal state cannot be substituted.

## Terminal state

The after phase records one of these states in `attempt.json`:

| State | Meaning |
| --- | --- |
| `ready_to_finish` | Current, comparable, and edit-cage compliant; receipt admission may proceed. |
| `stale` | Repository `HEAD` changed after the attempt was prepared. |
| `incomparable` | The retained and current evidence cannot support a valid comparison. |
| `failed` | The edit-cage or another terminal invariant failed. |

Only `ready_to_finish` with a current, compliant after verdict can authorize the attempt-bound receipt.

## Compatibility outputs

The composed after command still writes the established projections:

```text
target/ripr/workflow/after.repo-exposure.json
target/ripr/workflow/analysis-outcome.json
target/ripr/workflow/agent-verify.json
target/ripr/reports/agent-receipt.json
target/ripr/workflow/            # status input
```

Those paths keep existing review and cockpit integrations working. Their evidence is admitted only after the exact attempt's retained before snapshot and packet have been resolved and validated.

## Failure behavior

Repair attempts fail closed:

- a packet whose selected edit target is not a test surface (a `tests` or
  `test` path component, or a `*_test.rs`, `*_tests.rs`, `test_*.py`,
  `*_test.py`, or `*_tests.py` file name) is refused before any attempt is
  created; inline `#[cfg(test)]` modules in production files are not valid edit
  targets;
- malformed or unknown attempt IDs are rejected;
- missing, moved, modified, or digest-mismatched retained artifacts are rejected;
- a cross-attempt packet is rejected;
- ambiguous seam-selected after phases are rejected with an instruction to pass `--attempt`;
- stale `HEAD`, incomparable evidence, and edit-cage violations do not produce a receipt-ready state;
- unrelated repository changes outside the trusted edit surface block receipt admission.

RIPR does not select “the latest” attempt, reconstruct an attempt from mutable global files, or continue on partial evidence.

## Boundary

A repair attempt prepares and verifies evidence. RIPR does not author or apply the focused test edit, call an external model provider, run mutation testing, prove test adequacy or correctness, authorize merge, or turn static evidence into runtime proof.

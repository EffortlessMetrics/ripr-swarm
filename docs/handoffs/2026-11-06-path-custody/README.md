# Bounded #4006 custody handoff

Owner: delegated documentation/path lane; central integration remains with thread
01a1a959-f983-763a-96e6-7dbe1cced019. No integration, public activation, release,
settings or hosted-CI request is performed by this handoff.

## Source and contract

Input: swarm b51aebc7e4735529befd677ebf14071c7f09e477,
tree d3142dd59d4dcfea5e6b4981732f61c8516f3fa0. Separate docs commit:
497f97817 (roadmap corrections). Custody patch:
6d949fcc9091074ea16f446d2bf3d256a0dd0a45,
tree 188fbcaf26b73fd8abb8143f420e85dd6aea0333.

Reuse open swarm PR #13320, verified e9ce6779b3d4072285547b3b57073ae19d77ea71.
It owns the ledger/manifest leg. This patch requires that producer; it does not
reimplement its capture or project a substitute candidate. Its accepted role is
range_touched (union of per-record paths across the fixed release base..cut),
not net source_changed/swarm_changed/swarm_only or K active paths.

The bundle carries role, exact repository/base/cut/tree, decoded count,
NUL-framed digest, retained recipe and raw denominator-packet digest. The public
reader derives expected values from the independently selected manifest and raw
#2768 bytes, compares the bundle and recomputes the visible receipt comparison.
Absent input is rejected, not treated as an empty inventory. Valid empty input
has its explicit zero count and SHA-256 empty-stream digest.

Fetched public ripr d731a19bf13e5a5c2257d3c320f734f65ccd9894,
tree 57c0f03ff224d5eb56cd897672858188ddd359ce. Its original reader and material
blobs exactly equal the inspected swarm input (Git blobs 682b887964d83a83130cdb2ad05577a9684b8d74
and 234c4aa2738e6ea4afa2fb3a8d047a883cd8f0ba, respectively). This proves patch
surface applicability; it does not prove effective trusted public execution.
Public #1893 and draft #1889 holds remain. No attribution history was changed.

## Executed proof and limitations

Linux x86_64, rustc 1.95.0 (59807616e 2026-04-14). Reader/shared authority proof:
15 passed, 0 failed, 0 ignored; harness Clippy --lib --tests -- -D warnings passed.
The harness compiles byte-identical copies of the actual reader modules, imports
the actual path_inventory/path_custody modules and runs their actual tests.
source-sha256.json binds those bytes. Native subprocess transport is deliberately
unavailable; no network response or Git tree observation is manufactured.
Native API/tree transport, the complete handoff producer module, full xtask
compilation, hosted gates and public activation were NOT established.
The producer call site and fixture were reviewed and formatted, not executed.
Overall review: REVIEW_INCOMPLETE for combined #13320 integration and those
remaining dimensions. Do not treat this branch as merge/release readiness.

The old reader admits a fixture with no path custody: the deliberately failing
reader-red.txt is the intended missing-custody witness. Removing the new equality
check lets a same-subject altered inventory through even after the attacker
refreshes the claimed comparison observation and outer qualification digest;
reader-removal-control.txt fails with "accepted path substitution". Source was
restored before the green run. An earlier removal attempt was caught by the
receipt-comparison check; the negative was strengthened to update that claimed
observation as well. Corpus-audit is an independent Python check of eight fixed
pins, not execution of the PR-evidence producers.

Initial harness setup failed because a #[path] module resolved child files in
the wrong directory; staging byte-identical modules as mod.rs repaired that
instrument error. An initial unrestricted cargo test also attempted empty
doctests with a missing repository-configured TMPDIR. The retained passing
command explicitly uses --lib; no doctest pass is claimed.

The denied gh API read for PR #13325 was not retried or substituted. That
metadata was unnecessary for this isolated repair.

## Reproduce without building ripr

Run from the repository containing the patch. Use an external disposable
directory; it will hold only a small crate and selected module copies.

```bash
python3 - <<'PY'
from pathlib import Path
import shutil
root=Path.cwd()
out=Path('/tmp/ripr-path-reader-proof')
assert not out.exists(), 'choose a fresh proof directory'
(out/'src/source_promotion_acceptance').mkdir(parents=True)
(out/'Cargo.toml').write_text('''[package]
name="ripr-path-custody-reader-proof"
version="0.1.0"
edition="2024"
[dependencies]
serde={version="1",features=["derive"]}
serde_json="1"
sha2="0.10"
''')
shutil.copyfile(root/'docs/handoffs/2026-11-06-path-custody/harness-Cargo.lock',out/'Cargo.lock')
src=root/'xtask/src/reports'
shutil.copyfile(src/'source_promotion_acceptance.rs',out/'src/source_promotion_acceptance/mod.rs')
for p in (src/'source_promotion_acceptance').glob('*.rs'):
    shutil.copyfile(p,out/'src/source_promotion_acceptance'/p.name)
(out/'src/lib.rs').write_text('''#![allow(dead_code)]
mod run {
 pub struct Output {pub timed_out:bool,pub stdout_truncated:bool,pub stderr_truncated:bool,
 pub status:Option<std::process::ExitStatus>,pub stdout:String,pub stderr:String}
 pub fn capture_output_in_dir_with_timeout_bounded(_: &std::path::Path,_:&[String],
 _:&[(String,String)],_:&std::path::Path,_:std::time::Duration,_:usize,_:&str)->Result<Output,String>{
 Err("transport is outside the isolated reader proof".into())}
}
mod reports {
 #[path="'''+str(src/'path_inventory.rs')+'''"] pub(crate) mod path_inventory;
 #[path="'''+str(src/'path_custody.rs')+'''"] pub(crate) mod path_custody;
}
mod source_promotion_acceptance;
''')
PY
cargo test --manifest-path /tmp/ripr-path-reader-proof/Cargo.toml --locked --lib
cargo clippy --manifest-path /tmp/ripr-path-reader-proof/Cargo.toml --locked --lib --tests -- -D warnings
```

Full commands used this same harness with local CARGO_HOME/RUSTUP_HOME under
/workspace and --manifest-path /workspace/ripr-docs-evidence/reader-harness/Cargo.toml.
No full library build was repeated.

## Storage and continuation

Owned isolated checkout: /workspace/ripr-docs-011; its Git common directory is
/workspace/ripr-docs-011/.git. Retained witnesses and harness are under
/workspace/ripr-docs-evidence. Toolchain/cache use /workspace/ripr-rustup and
/workspace/ripr-cargo. Observed free space before generation: 30 GiB; bounded
additional compiler/cache/harness budget: 1 GiB, plus source checkout. Actual
sizes: toolchain 614 MiB, cargo cache 34 MiB, evidence/build 153 MiB. No full ripr
build admitted. Toolchain/build state retained for possible focused follow-up;
source witnesses retained pending receiver verification, not deleted.

Next owner action: reconcile #13320, compile/run the combined handoff producer
and scoped reader tests in the owning qualification environment, then refresh
applicable source review. Public trusted deployment/activation remains with its
existing owner and holds. scope_pending and inactive policy bytes are unchanged.

## Coordinated schema-lane release-note correction

The schema lane reported exact-b51aebc7e five-producer replay and 104 scoped
passes (rows digest b1ecc45e0da4b1fcdf855123944b7ca29c634aa602743b404048b68796856c92).
Its raw archive was not fetched or replayed here; those are attributed lane
results. Local source inspection independently confirms producer constants in
output/repo_exposure.rs: 0.5, output/repo_seams.rs: 0.3, and
output/mutation_calibration.rs: 0.3. A separate docs commit corrects those three
0.11 target versions and says "including additive" span fields. The 0.10
baseline versions and all other release-note text are preserved. No schema
replay was duplicated or promoted to this patch's executed proof.

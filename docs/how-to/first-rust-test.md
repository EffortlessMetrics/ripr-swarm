# Your first focused Rust test

Work through one small change from a testing gap to a precise assertion. You
will create a dependency-free Rust crate, inspect a changed comparison, add one
test, run that test, and ask RIPR to analyze the same production change again.

The example uses the same function and inputs as RIPR's checked
[boundary-gap fixture](../../fixtures/boundary_gap/SPEC.md) and
[strong boundary-oracle fixture](../../fixtures/strong_boundary_oracle/SPEC.md).
It is small enough to understand the expected result without trusting an
analyzer's recommendation.

## Before you start

Use a new directory for this exercise. You need:

- Git, with an identity configured for a local commit.
- An installed `ripr` on PATH.
- Rust and Cargo to execute the example's tests. Installing a prebuilt RIPR
  archive supplies the analyzer, not the Rust toolchain used by your tests.

[Installation](../QUICKSTART.md#installation) covers Cargo and checksummed
prebuilt archives, including Bash/zsh and PowerShell commands. Pick the build
before following its instructions:

| Build | Installation and expectations |
| --- | --- |
| Current development build | Use the development Cargo route in the installation guide. The expected static classifications below come from this source tree's checked fixtures. |
| Published release | Use its Cargo package or matching prebuilt archive, and read that binary's help. The expected classification change below is a current-source example; it is not a promise of identical behavior on the published 0.10.0 release. |

From the shell you will use for the exercise, check:

```text
ripr --version
git --version
cargo --version
```

If an older RIPR stays first on PATH, locate it with `command -v ripr` in Bash
or `Get-Command ripr -All` in PowerShell. The
[install-channel guide](../INSTALL_CHANNELS.md#confirm-which-binary-and-repository-you-are-using)
explains how to distinguish an installed release from a development build.

If Cargo is unavailable, you can inspect the static example after creating the
files below, but record the native test result as **not run**. A static
classification cannot supply the missing test execution.

## 1. Create the starting project

Choose the block for your shell. These commands create a new directory; if it
already exists, choose a different name before continuing.

Bash or zsh:

```bash
mkdir ripr-first-test &&
  cd ripr-first-test &&
  git init &&
  mkdir src tests
```

PowerShell:

```powershell
New-Item -ItemType Directory -Path ripr-first-test -ErrorAction Stop | Out-Null
Set-Location ripr-first-test -ErrorAction Stop
git init
if ($LASTEXITCODE -ne 0) { throw "git init failed with exit $LASTEXITCODE" }
New-Item -ItemType Directory -Path src, tests -ErrorAction Stop | Out-Null
```

Create these four files in your editor. Keep their paths and names as shown so
the commands later in the exercise select the intended test.

**`Cargo.toml`**

```toml
[package]
name = "ripr-first-test"
version = "0.1.0"
edition = "2024"

[workspace]
```

The empty `[workspace]` keeps this example independent of a surrounding Cargo
workspace.

**`.gitignore`**

```gitignore
/target/
```

**`src/lib.rs`**

```rust
pub fn discounted_total(amount: i32, discount_threshold: i32) -> i32 {
    if amount > discount_threshold {
        amount - 10
    } else {
        amount
    }
}
```

**`tests/pricing.rs`**

```rust
use ripr_first_test::discounted_total;

#[test]
fn below_threshold_has_no_discount() {
    assert_eq!(discounted_total(50, 100), 50);
}

#[test]
fn far_above_threshold_discounts() {
    assert_eq!(discounted_total(10_000, 100), 9_990);
}
```

Run the existing integration tests:

```text
cargo test --test pricing
```

Confirm that Cargo actually runs both named tests and reports `2 passed`, with
no failures or ignored tests. A compile error or zero selected tests is a setup
problem to resolve before relying on that result.

Create the baseline commit. These commands work in both shells:

```text
git add Cargo.toml .gitignore src/lib.rs tests/pricing.rs
git commit -m "Baseline before equality change"
```

If Git asks for an author identity, configure your name and email for this
local example and retry the commit. Confirm the commit succeeds before moving
on: `HEAD` is the baseline used below. No remote or `origin/main` is needed.

## 2. Make a behavior change the existing tests miss

The new requirement is: **apply the 10-unit discount when the amount is at or
above the threshold**.

In `src/lib.rs`, change only `>` to `>=`:

```diff
-    if amount > discount_threshold {
+    if amount >= discount_threshold {
```

The two existing tests are still compatible with both implementations:

| Input | Old `>` implementation | New `>=` implementation |
| --- | --- | --- |
| `discounted_total(50, 100)` | `50` | `50` |
| `discounted_total(10_000, 100)` | `9_990` | `9_990` |
| `discounted_total(100, 100)` | `100` | `90` |

The equality case is the discriminator: it is the input that separates the
intended new behavior from the old behavior. The expected `90` follows from
the stated requirement and a 10-unit discount; it is not copied from a value
observed by running the implementation.

## 3. Analyze that production change

Save a patch of **only the production file** and analyze it. Using an explicit
patch makes the comparison visible and keeps it fixed during the test-only
steps that follow.

Bash or zsh:

```bash
mkdir -p target/ripr &&
  git diff --no-ext-diff --submodule=short --output=target/ripr/production.diff HEAD -- src/lib.rs &&
  ripr check --root . --diff target/ripr/production.diff --mode fast
```

PowerShell, including Windows PowerShell 5.1:

```powershell
New-Item -ItemType Directory -Force -Path target/ripr -ErrorAction Stop | Out-Null
git diff --no-ext-diff --submodule=short --output=target/ripr/production.diff HEAD -- src/lib.rs
if ($LASTEXITCODE -ne 0) { throw "git diff failed with exit $LASTEXITCODE" }
ripr check --root . --diff target/ripr/production.diff --mode fast
if ($LASTEXITCODE -ne 0) { throw "ripr check did not complete; read its diagnostic" }
```

Git writes the patch itself, avoiding shell-dependent text encoding. The Bash
`&&` and PowerShell status check prevent analysis from proceeding when the
patch command fails. Keep `production.diff` for the rest of this exercise;
regenerate it if you change production code or choose a different baseline.

Read the finding for `src/lib.rs` and `discounted_total`. The checked example
has one predicate finding with `weakly_exposed` evidence: tests call the
function and compare results, but neither uses `amount == discount_threshold`.
A newer human renderer may call this “partly noticed.” Use JSON to inspect the
classification without depending on the display wording:

```text
ripr check --root . --diff target/ripr/production.diff --mode fast --json
```

Look for these facts in your own report:

| Fact | Why it matters |
| --- | --- |
| The finding points at the changed comparison in `src/lib.rs`. | You are inspecting the intended production change. |
| The related tests are the below-threshold and far-above-threshold cases. | Existing tests reach the function. |
| The missing discriminator is the equality boundary. | Existing assertions do not distinguish `>` from `>=`. |
| The analysis completed without a partial or unsupported state. | Missing analysis must not be interpreted as missing test evidence. |

For more detail, use the report's printed `explain` or `context` command when
available. Preserve its root and diff arguments, and copy the ID from your
run. Example IDs from other runs are not selectors for your project.

A nonzero exit, no finding for the intended change, or a limitation is a reason
to inspect the version, patch and reported scope. Do not change the expected
value or remove an assertion just to reproduce an example classification.

## 4. Add and run the equality test

Append one test to `tests/pricing.rs`; leave the two existing tests and
production function unchanged:

```rust
#[test]
fn equality_boundary_discounts() {
    assert_eq!(discounted_total(100, 100), 90);
}
```

Run exactly that integration test:

```text
cargo test --test pricing equality_boundary_discounts -- --exact
```

Confirm the output names `equality_boundary_discounts` and reports **one test
passed**. Cargo can exit successfully with zero tests selected when a filter
matches nothing; check the count as well as the exit status. In PowerShell,
read `$LASTEXITCODE` immediately after Cargo. In Bash, read `$?` immediately.

RIPR does not run this Cargo command for you. Retain an actual native failure,
compile error, timeout or not-run result even if RIPR later reports stronger
static evidence.

### Check the assertion's value

In this disposable example, an optional wrong-behavior control is easy to
understand: temporarily put the comparison back to `>` and run the same
focused test. The test should fail because it observes `100` where the
requirement expects `90`. Restore `>=` and rerun it; require the intended test
to execute and pass before continuing.

This is a manual control for this one assertion. It is not a claim that RIPR
ran a mutation engine or tested every possible faulty implementation. Restore
the intended production source before the next RIPR check so it matches the
saved patch.

## 5. Recheck the same production change

Run the same analysis command again, from the same project root:

```text
ripr check --root . --diff target/ripr/production.diff --mode fast
```

Only the tests changed since the first analysis. The patch still describes the
original `>` to `>=` change, and RIPR can now read the equality assertion in
the current test sources.

For the current checked fixtures, the predicate moves from `weakly_exposed` to
`exposed`. The finding can remain in the report: stronger evidence does not
require the total finding count to fall to zero. Match the production file,
function and predicate when comparing results, and inspect the current JSON
if the human summary no longer leads with the now-exposed finding.

Keep the two observations separate:

| Observation | What it establishes |
| --- | --- |
| The exact Cargo test executed and passed. | This test produced the expected result for the exercised input on the tested implementation. |
| The same production predicate is now `exposed`. | RIPR found a static path and assertion that discriminate the modeled comparison change. |

Neither observation establishes general correctness, exhaustive testing, a
runtime mutation score, or permission to merge.

If the classification stays weak or unknown, inspect the remaining
explanation. An older build may lack the equality-boundary producer fix; a
partial result may not have analyzed the relevant evidence. Preserve the
actual result rather than treating an expected classification as proof that it
occurred. If the original subject disappears entirely, first check that the
patch is non-empty and still describes the production change.

## Take the loop to your project

Choose one intended behavior change and keep its comparison fixed while you
improve the test. Read the missing discriminator, justify the expected value
from the requirement, execute the exact native test, then refresh RIPR's
analysis of the original production change.

- [Daily check/test/recheck loop](../QUICKSTART.md#check-test-and-refresh-the-same-change)
  explains base selection when the production change is already committed.
- [Finding triage](triage-a-finding.md) explains weak, unknown and limited
  findings in more complex projects.
- [Targeted test workflow](../TARGETED_TEST_WORKFLOW.md) adds before/after
  snapshots and a reviewable receipt.
- [Boundary-gap case study](../case-studies/TARGETED_TEST_BOUNDARY_GAP.md) shows
  the repository's executable fixture and its retained static evidence.

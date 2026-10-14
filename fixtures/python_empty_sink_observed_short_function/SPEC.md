# Fixture: python_empty_sink_observed_short_function

Spec: RIPR-SPEC-0238

## Given

The exact assertion f()==1 observes the changed short-function output. Current return1 passes and mutant return9 is rejected. Empty name/sink token sets must fall through the existing module-identified call branch: exposed/direct with strong_oracle_observes_owner_call_through_module.

The Python preview configuration, current production bytes, test assertions and
Git-derived diff are copied without rewriting from independently authored native
controls for #7526. A fresh Python process invoked the exact test function; its
assertions and import values were observed before and after a non-equivalent
mutant. Ordinary module import links remain conservative controls elsewhere.

## When

`ripr check` analyzes the real diff against these current Python sources.

## Then

The exact assertion f()==1 observes the changed short-function output. Current return1 passes and mutant return9 is rejected. Empty name/sink token sets must fall through the existing module-identified call branch: exposed/direct with strong_oracle_observes_owner_call_through_module.

Runtime rejection is an independent control, not a static mutation outcome or
proof of test adequacy. No new owner identity or shell command is invented.

## Must Not

- Drop the changed finding or replace the original source, assertion, import or diff identity.
- Credit an unrelated assertion merely because no useful sink token remains.
- Downgrade the genuine short-function output control by stopping before its existing credit branch.
- Claim broader module-read, relation-scanner, runtime mutation or test-adequacy support.

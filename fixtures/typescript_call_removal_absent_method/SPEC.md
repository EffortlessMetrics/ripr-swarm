# Fixture: typescript_call_removal_absent_method

Spec: RIPR-SPEC-0243 counted removal receiver witness (#10157).

## Given

An ordered node:test spy/install/owner/count sequence has a stable receiver,
but `missing_selected_method_prevents_spy_installation` prevents a usable count discriminator. Native Node rejects
both original and removal; the safe counterpart passes before removal and
fails afterward.

## When

The CLI analyzes the removal in `diff.patch`.

## Then

Exactly one finding at `src/audit.ts:2` remains `weakly_exposed`. The
promotion-honesty corpus pins the nonempty denominator and refuses `exposed`.

## Must Not

- Grant call-deletion credit based on receiver identity and a nearby count alone.
- Change the retained receiver/order controls or the corpus's runtime truth.

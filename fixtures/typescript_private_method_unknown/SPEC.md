# Fixture: typescript_private_method_unknown

Spec: RIPR-SPEC-0243

## Given

An ordinary private class method changes its threshold. No test exists, so there is no evidence of a candidate test path.

## When

`cargo xtask fixtures typescript_private_method_unknown` analyzes the changed threshold through the real CLI.

## Then

Exactly one static_unknown finding retains the #limit owner spelling, unknown reach and advisory private_member_reach_unknown actionability. Output does not invent a test path or suggest an invalid direct private call.

## Must Not

- Credit unsupported indirect reach from a public caller.
- Report unsupported extraction as complete or silently clean.
- Treat a private member as a public call target.

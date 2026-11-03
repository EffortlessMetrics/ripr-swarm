# Fixture: typescript_nested_private_method_partial

Spec: RIPR-SPEC-0243

## Given

A public function contains a nested class whose private method contributes to its return. The real wrapper assertion discriminates the changed threshold, but the preview does not extract nested private owners or model that call chain.

## When

`cargo xtask fixtures typescript_nested_private_method_partial` analyzes the changed threshold through the real CLI.

## Then

No finding borrows the enclosing public function or its related test. Analysis is partial and the extraction limitation counts the file once.

## Must Not

- Credit unsupported indirect reach from a public caller.
- Report unsupported extraction as complete or silently clean.
- Treat a private member as a public call target.

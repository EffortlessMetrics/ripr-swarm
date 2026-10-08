# Fixture: rust_adversarial_free_function_receiver (false-exposed guard — free-function receiver identity)

Spec: RIPR-SPEC-0108

Corpus case: `rust_free_function_receiver_qualified_call` in
`fixtures/evidence-promotion-honesty-corpus/corpus.json` (issue #7006).

## Given

An adversarial **same-name, receiver-qualified-only** over-credit trap. The
changed owner is the free function `parse`, flipping
`(0, None)` → `(0, Some(input.len()))`. The only related test
never calls the free function: it calls the *method* `Config::parse` on a
`Config` receiver under a strong exact-value oracle:

```rust
// changed owner: free function parse
pub fn parse(input: &str) -> (usize, Option<usize>) {
    (0, Some(input.len()))
}

// the ONLY related test — a method call, never the free function
let config = Config { strict: true };
let input = "hey";
assert_eq!(config.parse(input).1, Some(3));
```

The test linked to the owner only because the name-only `calls_owner`
match credits `config.parse(` against the free function `parse`.
Method-call syntax can never resolve to a free function, so the right
string sits on the wrong receiver. Bare `parse(` and path-qualified
`path::parse(` spellings keep `direct_owner_call`; method owners are
untouched (#3047).

## When

```bash
ripr check \
  --root fixtures/rust_adversarial_free_function_receiver/input \
  --diff fixtures/rust_adversarial_free_function_receiver/diff.patch
```

## Then

ripr classifies the change **below `exposed`**. The receiver-qualified-only
relation is `weak_token_substring`, not `direct_owner_call`: the name-only
match keeps reach `weak` and the finding `weakly_exposed`.

**This fixture must NEVER read `exposed`.** Before the free-function
receiver gate it read `exposed` — a silent over-credit: a test that only
calls `Config::parse` was reported as discriminating the free function
`parse`'s changed return. The shape mirrors
`rust_adversarial_same_method_other_type` (#4760): the differ is free-vs-method
identity, not which impl type holds the receiver.

## Must Not

- Credit `exposed` or `direct_owner_call` from a receiver-qualified
  `other.parse(` call when the owner is a free function.
- Demote bare `parse(` or path-qualified `path::parse(` free-function
  calls, or any method-owner receiver call.
- Absorb #3727 (CallFact receiver fields) or #4760 (method-owner
  receiver resolution).
- Use mutation-runtime outcome vocabulary.

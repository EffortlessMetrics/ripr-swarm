# Macro-generated collection constructor refusal

RIPR-SPEC-0094 / #7966. The owner module declares `Bag(Vec<u32>)`, but a parameterized macro emits the called test-module `Bag` factory returning `Payload(Numbers)`. `Numbers` coerces to `Vec<u32>` and compares only lengths.

The independent literal expectation is `weakly_exposed` for the changed `v.push(x * 1)` call-deletion and side-effect findings. The neutral and wrong-value subjects compile and pass the named assertion; deletion compiles and fails it. The public and repository tests retain the actual owner/test target. Constructor-generating expansion establishes no standard field equality. The macro binds its function name through `$name`, so a lexical `fn Bag` scan cannot establish the actual constructor.

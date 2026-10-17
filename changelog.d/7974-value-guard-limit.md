<!-- section: Fixed -->
- A test whose only `return` is a value guard (`if call() <op> k { return; }`) that spares both the expected value and some wrong value reads as `static_unknown`, not a gap; a guard that holds at the expected value, or a `!=` guard, keeps the gap (RIPR-SPEC-0240, #7974).

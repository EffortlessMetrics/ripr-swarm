# Procedural constructor refusal

RIPR-SPEC-0094 / #7966. A derive on `Trigger` emits a `Bag` factory returning a custom length-comparing field; a sibling module declares the unrelated standard-vector `Bag`. Expansion does not establish the asserted field type.

Independent literal expectation: the changed push call-deletion and side-effect findings stay `weakly_exposed`. The neutral and wrong-value subjects compile and pass; deletion compiles and fails the named equality. The runtime harness first compiles the local procedural macro companion, then links each actual test subject against it.

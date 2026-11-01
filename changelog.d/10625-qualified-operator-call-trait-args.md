<!-- section: Fixed -->
- A qualified operator call that spells the trait's generic arguments (`<Money as Add<Money>>::add(..)`) now credits the impl whose header spells the same arguments, where it was read as an unrelated call and stayed withheld; a call spelling different arguments is still not attributed (#10625).

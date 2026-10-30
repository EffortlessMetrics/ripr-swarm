<!-- section: Fixed -->
- A changed return expression is no longer read as `exposed` when the related tests pair an equality assertion at an input where both versions agree with a relational assertion (`assert!(f(3) > f(1))`) whose truth value is the same before and after; a relational assertion that flips between the versions keeps the credit (#10916).

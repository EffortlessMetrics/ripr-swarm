<!-- section: Fixed -->
- A pushed element that embeds a type defined outside the scanned workspace (a dependency whose `PartialEq` the index cannot read) no longer reads `exposed` from a whole-collection comparison: the embedded type fails closed like a hand-written `eq`, while workspace and std types keep the credit (#10684).
